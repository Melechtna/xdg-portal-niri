//! The `org.freedesktop.impl.portal.ScreenCast` backend interface, and the
//! process-wide state that backs it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};
use zbus::object_server::ObjectServer;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::config;
use crate::error::Response;
use crate::niri::{CastStream, Niri};
use crate::picker::PickerRequest;
use crate::request::Request;
use crate::session::{Session, SessionState};

/// Portal ScreenCast source type flags.
pub const SOURCE_TYPE_MONITOR: u32 = 1;
pub const SOURCE_TYPE_WINDOW: u32 = 2;

/// Portal ScreenCast cursor mode flags.
pub const CURSOR_MODE_HIDDEN: u32 = 1;
pub const CURSOR_MODE_EMBEDDED: u32 = 2;
pub const CURSOR_MODE_METADATA: u32 = 4;

/// State shared by every object this backend exports.
pub struct PortalState {
    pub niri: Niri,
    pub picker_tx: async_channel::Sender<PickerRequest>,
    pub cancel_tx: async_channel::Sender<String>,
    pub sessions: Mutex<HashMap<OwnedObjectPath, Arc<SessionState>>>,
}

impl PortalState {
    pub fn new(
        niri: Niri,
        picker_tx: async_channel::Sender<PickerRequest>,
        cancel_tx: async_channel::Sender<String>,
    ) -> Self {
        Self {
            niri,
            picker_tx,
            cancel_tx,
            sessions: Mutex::new(HashMap::new()),
        }
    }

    fn session(&self, path: &OwnedObjectPath) -> Option<Arc<SessionState>> {
        self.sessions.lock().unwrap().get(path).cloned()
    }
}

/// The backend object.
pub struct ScreenCast {
    state: Arc<PortalState>,
}

impl ScreenCast {
    pub fn new(state: Arc<PortalState>) -> Self {
        Self { state }
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.ScreenCast")]
impl ScreenCast {
    async fn create_session(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
        handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: String,
        _options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        debug!(%app_id, %session_handle, "CreateSession");

        export_request(server, &handle, &self.state).await;

        let session_state = Arc::new(SessionState::new(app_id));
        let session = Session {
            state: session_state.clone(),
        };
        if let Err(err) = server.at(&session_handle, session).await {
            warn!("could not export session {session_handle}: {err}");
            unexport_request(server, &handle).await;
            return (Response::OtherError.code(), HashMap::new());
        }

        self.state
            .sessions
            .lock()
            .unwrap()
            .insert(session_handle, session_state);

        (Response::Success.code(), HashMap::new())
    }

    async fn select_sources(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
        handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        _app_id: String,
        options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        debug!(%session_handle, "SelectSources");
        export_request(server, &handle, &self.state).await;

        let Some(session) = self.state.session(&session_handle) else {
            warn!("SelectSources for unknown session {session_handle}");
            unexport_request(server, &handle).await;
            return (Response::OtherError.code(), HashMap::new());
        };

        // We deliberately do not honor the apps types mask. A fair number
        // of apps ask for monitors only yet are perfectly happy to be handed a
        // window, and vise versa, so the picker always offers everything the
        // compositor can cast.
        let requested_types = read_u32(&options, "types").unwrap_or(SOURCE_TYPE_MONITOR);

        {
            let mut opts = session.options.lock().unwrap();
            opts.multiple = read_bool(&options, "multiple").unwrap_or(false);
            opts.cursor_mode = read_u32(&options, "cursor_mode").unwrap_or(CURSOR_MODE_HIDDEN);
            debug!(
                "SelectSources: app asked for types={requested_types:#b} (offering all), \
                 multiple={}, cursor_mode={}",
                opts.multiple, opts.cursor_mode
            );
        }

        unexport_request(server, &handle).await;
        (Response::Success.code(), HashMap::new())
    }

    async fn start(
        &self,
        #[zbus(object_server)] server: &ObjectServer,
        handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: String,
        _parent_window: String,
        _options: HashMap<String, OwnedValue>,
    ) -> (u32, HashMap<String, OwnedValue>) {
        debug!(%session_handle, "Start");
        export_request(server, &handle, &self.state).await;

        let result = self.start_inner(&handle, &session_handle, app_id).await;

        unexport_request(server, &handle).await;
        result
    }

    #[zbus(property)]
    fn available_source_types(&self) -> u32 {
        SOURCE_TYPE_MONITOR | SOURCE_TYPE_WINDOW
    }

    #[zbus(property)]
    fn available_cursor_modes(&self) -> u32 {
        CURSOR_MODE_HIDDEN | CURSOR_MODE_EMBEDDED | CURSOR_MODE_METADATA
    }

    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        4
    }
}

impl ScreenCast {
    async fn start_inner(
        &self,
        handle: &OwnedObjectPath,
        session_handle: &OwnedObjectPath,
        app_id: String,
    ) -> (u32, HashMap<String, OwnedValue>) {
        let Some(session) = self.state.session(session_handle) else {
            warn!("Start for unknown session {session_handle}");
            return (Response::OtherError.code(), HashMap::new());
        };

        let options = *session.options.lock().unwrap();

        let sources = self.state.niri.available_sources().await;
        if sources.monitors.is_empty() && sources.windows.is_empty() {
            warn!("no shareable sources available");
            return (Response::OtherError.code(), HashMap::new());
        }

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        let request = PickerRequest {
            id: handle.to_string(),
            app_id,
            sources,
            reply: reply_tx,
        };
        if let Err(err) = self.state.picker_tx.send(request).await {
            warn!("picker is unavailable: {err}");
            return (Response::OtherError.code(), HashMap::new());
        }

        let selection = match reply_rx.await {
            Ok(Some(selection)) if !selection.is_empty() => selection,
            Ok(_) => {
                debug!("user cancelled the picker");
                return (Response::Cancelled.code(), HashMap::new());
            }
            Err(_) => {
                debug!("picker went away");
                return (Response::Cancelled.code(), HashMap::new());
            }
        };

        let cast = match self
            .state
            .niri
            .start_cast(&selection, options.cursor_mode)
            .await
        {
            Ok(cast) => cast,
            Err(err) => {
                warn!("could not start screen cast: {err:#}");
                return (Response::OtherError.code(), HashMap::new());
            }
        };

        let results = build_results(&cast.streams);
        *session.cast.lock().unwrap() = Some(cast);

        info!(
            "screen cast started for {} ({} stream(s))",
            session.app_id,
            results.get("streams").map(count_streams).unwrap_or(0)
        );
        (Response::Success.code(), results)
    }
}

/// Export the request object the frontend expects at handle.
async fn export_request(server: &ObjectServer, handle: &OwnedObjectPath, state: &PortalState) {
    let request = Request::new(handle.to_string(), state.cancel_tx.clone());
    if let Err(err) = server.at(handle, request).await {
        warn!("could not export request object {handle}: {err}");
    }
}

/// Removal of a request object.
async fn unexport_request(server: &ObjectServer, handle: &OwnedObjectPath) {
    let _ = server.remove::<Request, _>(handle).await;
}

fn build_results(streams: &[CastStream]) -> HashMap<String, OwnedValue> {
    let entries: Vec<(u32, HashMap<&str, Value<'_>>)> = streams
        .iter()
        .map(|stream| {
            let mut props: HashMap<&str, Value<'_>> = HashMap::new();
            props.insert("position", Value::from(stream.position));
            props.insert("size", Value::from(stream.size));
            props.insert("source_type", Value::from(stream.source_type));
            (stream.node_id, props)
        })
        .collect();

    let mut results = HashMap::new();
    match OwnedValue::try_from(Value::from(entries)) {
        Ok(streams) => {
            results.insert("streams".to_string(), streams);
        }
        Err(err) => warn!("could not encode stream list: {err}"),
    }
    results
}

fn count_streams(value: &OwnedValue) -> usize {
    match &**value {
        Value::Array(array) => array.len(),
        _ => 0,
    }
}

fn read_u32(options: &HashMap<String, OwnedValue>, key: &str) -> Option<u32> {
    options
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| u32::try_from(value).ok())
}

fn read_bool(options: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    options
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| bool::try_from(value).ok())
}

/// Bring the backend up, own the bus name and export the ScreenCast object.
pub async fn serve(
    picker_tx: async_channel::Sender<PickerRequest>,
    cancel_tx: async_channel::Sender<String>,
) -> anyhow::Result<()> {
    let connection = zbus::Connection::session()
        .await
        .map_err(|err| anyhow::anyhow!("connecting to the session bus: {err}"))?;

    let state = Arc::new(PortalState::new(
        Niri::new(connection.clone()),
        picker_tx,
        cancel_tx,
    ));

    connection
        .object_server()
        .at(config::DESKTOP_PATH, ScreenCast::new(state.clone()))
        .await
        .map_err(|err| anyhow::anyhow!("exporting the ScreenCast object: {err}"))?;

    connection
        .request_name(config::BUS_NAME)
        .await
        .map_err(|err| anyhow::anyhow!("requesting {}: {err}", config::BUS_NAME))?;

    info!(
        "backend '{}' ready on {} ({})",
        config::BACKEND_NAME,
        config::BUS_NAME,
        config::DESKTOP_PATH
    );

    std::future::pending::<()>().await;
    Ok(())
}
