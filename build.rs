//! Compiles the bundled icons into a GResource.
//!
//! The two Font Awesome glyphs under `assets/` are baked into the binary so the
//! picker never depends on Font Awesome being installed. `icons.rs` registers
//! the result and points the icon theme at it.

fn main() {
    glib_build_tools::compile_resources(
        &["assets"],
        "assets/resources.gresource.xml",
        "resources.gresource",
    );
}
