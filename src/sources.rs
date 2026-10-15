//! The data model shared between the portals D-Bus side and the pickers GTK
//! side of the process.

/// An output the user can share.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorSource {
    pub connector: String,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
}

impl MonitorSource {
    pub fn geometry_label(&self) -> String {
        if (self.scale - 1.0).abs() < f64::EPSILON {
            format!("{}x{}", self.width, self.height)
        } else {
            format!("{}x{} ({:.2}x)", self.width, self.height, self.scale)
        }
    }
}

/// A toplevel window the user can share.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowSource {
    pub id: u64,
    pub title: String,
    pub app_id: String,
    pub width: i32,
    pub height: i32,
}

impl WindowSource {
    pub fn display_title(&self) -> &str {
        if !self.title.is_empty() {
            &self.title
        } else if !self.app_id.is_empty() {
            &self.app_id
        } else {
            "Untitled window"
        }
    }
}

/// Everything the picker can offer.
#[derive(Debug, Clone, Default)]
pub struct AvailableSources {
    pub monitors: Vec<MonitorSource>,
    pub windows: Vec<WindowSource>,
}

/// The user's choice of what to share.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    Monitor(MonitorSource),
    Window(WindowSource),
}
