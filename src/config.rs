//! Static configuration: D-Bus names, object paths, and tunables.
//! Everything that identifies this backend to the outside world lives here.

/// Backend name. This is simultaneously:
/// - the basename of our `.portal` file (`<NAME>.portal`),
/// - the value used in `xdg-desktop-portal`'s `portals.conf`,
/// - and the last component of [`BUS_NAME`].
pub const BACKEND_NAME: &str = "niriportal";

/// The well-known name we own on the session bus.
pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.niriportal";

/// Object path that the frontend invokes.
pub const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";

/// GTK application id used by the picker UI.
pub const APPLICATION_ID: &str = "dev.melechtna.XdgPortalNiri";

/// Human-readable name shown in the picker window / logs.
pub const APP_NAME: &str = "Screen Share";

/// How long we wait for niri to hand us PipeWire node ids after start.
pub const STREAM_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Cursor mode used for the live monitor previews.
pub const PREVIEW_CURSOR_HIDDEN: u32 = 1;

/// Upper bound for a preview's longest edge, in pixels. Keeping previews small
/// keeps the PipeWire to GTK hop cheap.
pub const PREVIEW_MAX_EDGE: u32 = 480;
