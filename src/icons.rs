//! The two icons this picker uses, baked into the binary.
//!
//! Rather than depending on a Font Awesome package being installed, the two
//! glyphs we need are compiled into a GResource.

use std::sync::Once;

use tracing::warn;

/// The display glyph, for anything that stands for a monitor.
pub const DISPLAY: &str = "fa-display-symbolic";

/// The window glyph, for anything that stands for a window.
pub const WINDOW: &str = "fa-window-maximize-symbolic";

/// Where the icons live inside the binary, as an icon theme directory.
const RESOURCE_PATH: &str = "/dev/melechtna/XdgPortalNiri/icons";

/// Register the bundled icons with the icon theme.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(register);
}

fn register() {
    gtk::gio::resources_register_include!("resources.gresource")
        .expect("registering the bundled icons");

    let Some(display) = gtk::gdk::Display::default() else {
        warn!("no display; the bundled icons will not be available");
        return;
    };

    let theme = gtk::IconTheme::for_display(&display);
    theme.add_resource_path(RESOURCE_PATH);

    for name in [DISPLAY, WINDOW] {
        if !theme.has_icon(name) {
            warn!("bundled icon {name} is not resolvable");
        }
    }
}
