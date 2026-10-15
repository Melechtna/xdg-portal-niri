//! The org.freedesktop.impl.portal.Request object.
//!
//! Every backend method call comes with a `handle` object path, the frontend
//! creates a proxy to that path BEFORE invoking us so it can cancel the
//! interaction.

use async_channel::Sender;

/// Exported at each method's handle path.
pub struct Request {
    id: String,
    cancel: Sender<String>,
}

impl Request {
    pub fn new(id: String, cancel: Sender<String>) -> Self {
        Self { id, cancel }
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl Request {
    async fn close(&self) {
        if self.cancel.send(self.id.clone()).await.is_err() {}
    }
}
