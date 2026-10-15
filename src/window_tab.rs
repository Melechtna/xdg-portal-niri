//! The window tab of the picker.
//!
//! Deliberately preview-free, this never ends well if we try.

use std::rc::Rc;

use gtk::prelude::*;

use crate::icons;
use crate::sources::{Selection, WindowSource};

/// Build the window tab.
pub fn build(windows: &[WindowSource], on_select: Rc<dyn Fn(Option<Selection>)>) -> gtk::Widget {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .show_separators(true)
        .css_classes(["boxed-list"])
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();

    let windows = Rc::new(windows.to_vec());

    for window in windows.iter() {
        list.append(&build_row(window));
    }

    list.connect_row_selected(move |_: &gtk::ListBox, row: Option<&gtk::ListBoxRow>| {
        let selection = row
            .and_then(|row| usize::try_from(row.index()).ok())
            .and_then(|index| windows.get(index))
            .map(|window| Selection::Window(window.clone()));
        on_select(selection);
    });

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&list)
        .build();
    scroller.upcast()
}

/// One window row, an icon, the title, and the application id.
fn build_row(window: &WindowSource) -> gtk::ListBoxRow {
    let icon = gtk::Image::from_icon_name(icons::WINDOW);
    icon.set_pixel_size(32);

    let title = gtk::Label::builder()
        .label(window.display_title())
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .xalign(0.0)
        .build();

    let app_id = gtk::Label::builder()
        .label(&window.app_id)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    text.append(&title);
    text.append(&app_id);

    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_box.set_margin_top(8);
    row_box.set_margin_bottom(8);
    row_box.set_margin_start(8);
    row_box.set_margin_end(8);
    row_box.append(&icon);
    row_box.append(&text);

    gtk::ListBoxRow::builder().child(&row_box).build()
}
