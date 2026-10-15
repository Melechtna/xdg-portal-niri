//! A session is created by CreateSession, configured by SelectSources, and
//! activated by Start. It then outlives the Start call for as long as the
//! application is sharing.

use std::sync::Mutex;

use crate::niri::NiriCastSession;

/// Options accumulated from SelectSources.
#[derive(Debug, Clone, Copy)]
pub struct SessionOptions {
    pub multiple: bool,
    pub cursor_mode: u32,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            multiple: false,
            cursor_mode: 1,
        }
    }
}

/// The mutable guts of a session.
pub struct SessionState {
    pub app_id: String,
    pub options: Mutex<SessionOptions>,
    pub cast: Mutex<Option<NiriCastSession>>,
}

impl SessionState {
    pub fn new(app_id: String) -> Self {
        Self {
            app_id,
            options: Mutex::new(SessionOptions::default()),
            cast: Mutex::new(None),
        }
    }

    pub async fn stop_cast(&self) {
        let cast = self.cast.lock().unwrap().take();
        if let Some(cast) = cast {
            cast.stop().await;
        }
    }
}

/// Exported at the session object path.
pub struct Session {
    pub state: std::sync::Arc<SessionState>,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Session")]
impl Session {
    async fn close(&self, #[zbus(signal_context)] ctxt: zbus::object_server::SignalEmitter<'_>) {
        self.state.stop_cast().await;
        let _ = Session::closed(&ctxt).await;
    }

    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    #[zbus(signal)]
    async fn closed(ctxt: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}
