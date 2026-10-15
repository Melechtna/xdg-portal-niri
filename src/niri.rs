//! Everything about talking to the compositor.
//!
//! niri does not implement the `org.freedesktop.impl.portal` interfaces that
//! xdg-desktop-portal dispatches to. What it *does* implement is the
//! Mutter-compatible trio that GNOME's own portal is built on:
//!
//! - `org.gnome.Mutter.ScreenCast` for capture,
//! - `org.gnome.Shell.Introspect` for the window list,
//! - `org.gnome.Mutter.DisplayConfig` for outputs.
//!
//! This module is the thin client for those services. It is the only place in
//! the crate that knows niri's D-Bus surface.

use std::collections::HashMap;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use tracing::{debug, warn};
use zbus::zvariant::{OwnedObjectPath, Value};

use crate::config;
use crate::sources::{AvailableSources, MonitorSource, Selection, WindowSource};

/// Mutter-compatible proxy declarations
#[zbus::proxy(
    interface = "org.gnome.Mutter.ScreenCast",
    default_service = "org.gnome.Mutter.ScreenCast",
    default_path = "/org/gnome/Mutter/ScreenCast"
)]
trait MutterScreenCast {
    fn create_session(&self, properties: HashMap<&str, Value<'_>>)
        -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.gnome.Mutter.ScreenCast.Session",
    default_service = "org.gnome.Mutter.ScreenCast"
)]
trait MutterScreenCastSession {
    fn record_monitor(
        &self,
        connector: &str,
        properties: HashMap<&str, Value<'_>>,
    ) -> zbus::Result<OwnedObjectPath>;

    fn record_window(&self, properties: HashMap<&str, Value<'_>>) -> zbus::Result<OwnedObjectPath>;

    fn start(&self) -> zbus::Result<()>;

    fn stop(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn closed(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.gnome.Mutter.ScreenCast.Stream",
    default_service = "org.gnome.Mutter.ScreenCast"
)]
trait MutterScreenCastStream {
    /// The PipeWire node id for this stream.
    #[zbus(signal)]
    fn pipe_wire_stream_added(&self, node_id: u32) -> zbus::Result<()>;
}

/// A live screen cast session on the compositor's side.
pub struct NiriCastSession {
    connection: zbus::Connection,
    session_path: OwnedObjectPath,
    /// The PipeWire streams niri handed us.
    pub streams: Vec<CastStream>,
}

/// One PipeWire stream produced by niri, described the way the portal expects.
#[derive(Debug, Clone, Copy)]
pub struct CastStream {
    pub node_id: u32,
    /// Logical position of the captured region.
    pub position: (i32, i32),
    /// Logical size of the captured region.
    pub size: (i32, i32),
    /// Portal source type, 1 = monitor and 2 = window.
    pub source_type: u32,
}

impl NiriCastSession {
    /// Ask niri to stop the session.
    pub async fn stop(&self) {
        let builder = match MutterScreenCastSessionProxy::builder(&self.connection)
            .path(self.session_path.clone())
        {
            Ok(builder) => builder,
            Err(err) => {
                warn!("could not build session proxy for stop: {err}");
                return;
            }
        };
        match builder.build().await {
            Ok(proxy) => {
                if let Err(err) = proxy.stop().await {
                    warn!("failed to stop niri screen cast session: {err}");
                }
            }
            Err(err) => warn!("could not connect session proxy for stop: {err}"),
        }
    }
}

/// A handle to the compositor's Mutter-compatible services.
#[derive(Clone)]
pub struct Niri {
    connection: zbus::Connection,
}

impl Niri {
    pub fn new(connection: zbus::Connection) -> Self {
        Self { connection }
    }

    /// Enumerate every shareable source.
    pub async fn available_sources(&self) -> AvailableSources {
        let (monitors, windows) = tokio::join!(self.monitors(), self.windows());
        AvailableSources {
            monitors: monitors.unwrap_or_else(|err| {
                warn!("could not enumerate monitors: {err:#}");
                Vec::new()
            }),
            windows: windows.unwrap_or_else(|err| {
                warn!("could not enumerate windows: {err:#}");
                Vec::new()
            }),
        }
    }

    /// List monitors.
    async fn monitors(&self) -> Result<Vec<MonitorSource>> {
        let outputs = tokio::task::spawn_blocking(|| -> Result<Vec<niri_ipc::Output>> {
            let mut socket =
                niri_ipc::socket::Socket::connect().context("connecting to niri's IPC socket")?;
            match socket
                .send(niri_ipc::Request::Outputs)
                .context("requesting outputs from niri")?
            {
                Ok(niri_ipc::Response::Outputs(outputs)) => Ok(outputs.into_values().collect()),
                Ok(other) => anyhow::bail!("unexpected niri reply to Outputs: {other:?}"),
                Err(message) => anyhow::bail!("niri refused Outputs: {message}"),
            }
        })
        .await
        .context("niri outputs task panicked")??;

        let mut monitors: Vec<MonitorSource> = outputs
            .into_iter()
            .filter_map(|output| {
                // logical is none for disabled outputs, which cannot be cast.
                let logical = output.logical?;
                Some(MonitorSource {
                    connector: output.name,
                    width: logical.width as i32,
                    height: logical.height as i32,
                    x: logical.x,
                    y: logical.y,
                    scale: logical.scale,
                })
            })
            .collect();

        // This keeps the order stable from one request to the next.
        monitors.sort_by_key(|monitor| (monitor.x, monitor.y));

        Ok(monitors)
    }

    /// List toplevel windows.
    async fn windows(&self) -> Result<Vec<WindowSource>> {
        let windows = tokio::task::spawn_blocking(|| -> Result<Vec<niri_ipc::Window>> {
            let mut socket =
                niri_ipc::socket::Socket::connect().context("connecting to niri's IPC socket")?;
            match socket
                .send(niri_ipc::Request::Windows)
                .context("requesting windows from niri")?
            {
                Ok(niri_ipc::Response::Windows(windows)) => Ok(windows),
                Ok(other) => anyhow::bail!("unexpected niri reply to Windows: {other:?}"),
                Err(message) => anyhow::bail!("niri refused Windows: {message}"),
            }
        })
        .await
        .context("niri windows task panicked")??;

        Ok(windows
            .into_iter()
            .map(|window| WindowSource {
                id: window.id,
                title: window.title.unwrap_or_default(),
                app_id: window.app_id.unwrap_or_default(),
                width: window.layout.window_size.0,
                height: window.layout.window_size.1,
            })
            .collect())
    }

    /// Start capturing selections returning niri's PipeWire node ids.
    pub async fn start_cast(
        &self,
        selections: &[Selection],
        cursor_mode: u32,
    ) -> Result<NiriCastSession> {
        let screencast = MutterScreenCastProxy::new(&self.connection)
            .await
            .context("connecting to org.gnome.Mutter.ScreenCast")?;

        let session_path = screencast
            .create_session(HashMap::new())
            .await
            .context("creating a Mutter screen cast session")?;
        debug!("created Mutter screen cast session at {session_path}");

        let session = MutterScreenCastSessionProxy::builder(&self.connection)
            .path(session_path.clone())
            .with_context(|| format!("invalid session path {session_path}"))?
            .build()
            .await
            .context("connecting to the Mutter screen cast session")?;

        let niri_cursor = niri_cursor_mode(cursor_mode);

        // Register one stream per selected source.
        let mut stream_proxies = Vec::with_capacity(selections.len());
        for selection in selections {
            let stream_path = match selection {
                Selection::Monitor(monitor) => {
                    let mut props: HashMap<&str, Value<'_>> = HashMap::new();
                    props.insert("cursor-mode", Value::from(niri_cursor));
                    session
                        .record_monitor(&monitor.connector, props)
                        .await
                        .with_context(|| format!("recording monitor {}", monitor.connector))?
                }
                Selection::Window(window) => {
                    let mut props: HashMap<&str, Value<'_>> = HashMap::new();
                    props.insert("window-id", Value::from(window.id));
                    props.insert("cursor-mode", Value::from(niri_cursor));
                    session
                        .record_window(props)
                        .await
                        .with_context(|| format!("recording window {}", window.id))?
                }
            };

            let proxy = MutterScreenCastStreamProxy::builder(&self.connection)
                .path(stream_path.clone())
                .with_context(|| format!("invalid stream path {stream_path}"))?
                .build()
                .await
                .context("connecting to a Mutter screen cast stream")?;
            stream_proxies.push(proxy);
        }

        // Subscribe BEFORE start so we cannot miss the signal.
        let mut signals = Vec::with_capacity(stream_proxies.len());
        for proxy in &stream_proxies {
            signals.push(
                proxy
                    .receive_pipe_wire_stream_added()
                    .await
                    .context("subscribing to PipeWireStreamAdded")?,
            );
        }

        // 3. Start session.
        session
            .start()
            .await
            .context("starting the Mutter screen cast session")?;

        // 4. Collect the node ids.
        let mut streams = Vec::with_capacity(selections.len());
        for (index, signal) in signals.iter_mut().enumerate() {
            let message = tokio::time::timeout(config::STREAM_START_TIMEOUT, signal.next())
                .await
                .with_context(|| format!("timed out waiting for stream #{index}"))?
                .with_context(|| {
                    format!("stream #{index} closed before providing a PipeWire node")
                })?;
            let node_id = message
                .args()
                .with_context(|| format!("decoding PipeWireStreamAdded for stream #{index}"))?
                .node_id;
            let (position, size, source_type) = describe(selections[index].clone());
            streams.push(CastStream {
                node_id,
                position,
                size,
                source_type,
            });
        }

        Ok(NiriCastSession {
            connection: self.connection.clone(),
            session_path,
            streams,
        })
    }
}

/// Geometry + portal source type for a selection.
fn describe(selection: Selection) -> ((i32, i32), (i32, i32), u32) {
    match selection {
        Selection::Monitor(monitor) => ((monitor.x, monitor.y), (monitor.width, monitor.height), 1),
        Selection::Window(window) => ((0, 0), (window.width, window.height), 2),
    }
}

/// Translate a portal cursor mode into niri's `CursorMode`.
fn niri_cursor_mode(portal: u32) -> u32 {
    match portal {
        2 => 1,
        4 => 2,
        _ => 0,
    }
}
