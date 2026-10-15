//! The monitors tab of the picker.
//!
//! This is the only place previews are used: each monitor gets a card with a
//! live thumbnail produced by a throwaway screen cast. The thumbnails are
//! intentionally low-framerate and downscaled.

use std::rc::Rc;

use gtk::prelude::*;
use tracing::warn;

use crate::icons;
use crate::preview::{self, Frame, PreviewHandle};
use crate::sources::{MonitorSource, Selection};

/// The tab's widget plus the preview that keep it alive.
pub struct MonitorTab {
    pub widget: gtk::Widget,
    pub previews: Vec<PreviewHandle>,
}

/// Build the monitors tab.
pub fn build(monitors: &[MonitorSource], on_select: Rc<dyn Fn(Option<Selection>)>) -> MonitorTab {
    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .homogeneous(true)
        .row_spacing(12)
        .column_spacing(12)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .valign(gtk::Align::Start)
        .build();

    let monitors = Rc::new(monitors.to_vec());
    let mut previews = Vec::new();

    for monitor in monitors.iter() {
        let (card, picture, placeholder) = build_card(monitor);
        flow.append(&card);

        // If the UI cannot keep up we drop frames.
        let (frames_tx, frames_rx) = async_channel::bounded::<Frame>(2);
        let selection = Selection::Monitor(monitor.clone());
        match preview::spawn(selection, move |frame| {
            let _ = frames_tx.try_send(frame);
        }) {
            Ok(handle) => previews.push(handle),
            Err(err) => warn!("no preview for {}: {err:#}", monitor.connector),
        }

        // Pump frames into the picture on the main thread.
        glib::spawn_future_local(async move {
            let mut first_frame = true;
            while let Ok(frame) = frames_rx.recv().await {
                if first_frame {
                    // The icon is a stand-in for "no frame yet".
                    placeholder.set_visible(false);
                    first_frame = false;
                }
                let stride = frame.width as usize * 4;
                let bytes = glib::Bytes::from_owned(frame.rgba);
                let texture = gtk::gdk::MemoryTexture::new(
                    frame.width as i32,
                    frame.height as i32,
                    gtk::gdk::MemoryFormat::R8g8b8a8,
                    &bytes,
                    stride,
                );
                picture.set_paintable(Some(&texture));
            }
        });
    }

    {
        let monitors = monitors.clone();
        let on_select = on_select.clone();
        flow.connect_selected_children_changed(move |flow: &gtk::FlowBox| {
            let index = flow
                .selected_children()
                .first()
                .map(|child| child.index())
                .and_then(|index| usize::try_from(index).ok());
            let selection = index
                .and_then(|index| monitors.get(index))
                .map(|monitor| Selection::Monitor(monitor.clone()));
            on_select(selection);
        });
    }

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&flow)
        .build();

    MonitorTab {
        widget: scroller.upcast(),
        previews,
    }
}

/// The preview area above the connector name and geometry..
fn build_card(monitor: &MonitorSource) -> (gtk::Widget, gtk::Picture, gtk::Image) {
    let picture = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .build();

    // A placeholder so cards have their final size before the first frame.
    let placeholder = gtk::Image::from_icon_name(icons::DISPLAY);
    placeholder.set_pixel_size(48);
    placeholder.set_halign(gtk::Align::Center);
    placeholder.set_valign(gtk::Align::Center);
    placeholder.add_css_class("dim-label");

    let preview_area = gtk::Overlay::builder()
        .width_request(280)
        .height_request(158)
        .child(&picture)
        .build();
    preview_area.add_overlay(&placeholder);

    let preview_frame = gtk::Frame::builder()
        .child(&preview_area)
        .css_classes(["card"])
        .build();

    let name = gtk::Label::builder()
        .label(&monitor.connector)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .xalign(0.0)
        .build();

    let geometry = gtk::Label::builder()
        .label(monitor.geometry_label())
        .xalign(0.0)
        .css_classes(["dim-label", "caption"])
        .build();

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.append(&name);
    text.append(&geometry);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.append(&preview_frame);
    card.append(&text);

    (card.upcast(), picture, placeholder)
}
