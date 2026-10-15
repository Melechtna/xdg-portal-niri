//! The source picker UI.
//!
//! The picker is the only part of this backend that owns GTK. It lives on the
//! main thread and talks to the portal half over two async channel`s:
//!
//! There is at most one picker on screen at a time: a new request dismisses any
//! previous one as cancelled.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use libadwaita as adw;
use tokio::sync::oneshot;
use tracing::debug;

use crate::config;
use crate::icons;
use crate::monitor_tab;
use crate::preview::PreviewHandle;
use crate::sources::{AvailableSources, Selection};
use crate::window_tab;

/// A request from the portal half to show the picker.
pub struct PickerRequest {
    pub id: String,
    pub app_id: String,
    pub sources: AvailableSources,
    pub reply: oneshot::Sender<Option<Vec<Selection>>>,
}

/// A picker that is currently on screen.
struct Active {
    id: String,
    window: adw::ApplicationWindow,
    reply: Option<oneshot::Sender<Option<Vec<Selection>>>>,
    _previews: Vec<PreviewHandle>,
}

/// Picker state owned by the main thread.
#[derive(Default)]
struct PickerState {
    active: RefCell<Option<Active>>,
}

impl PickerState {
    fn present(self: &Rc<Self>, app: &adw::Application, request: PickerRequest) {
        self.finish(None);
        icons::install();
        let has_monitors = !request.sources.monitors.is_empty();
        let has_windows = !request.sources.windows.is_empty();
        if !has_monitors && !has_windows {
            let _ = request.reply.send(None);
            return;
        }

        debug!(
            "picker: offering {} monitor(s) and {} window(s) to {:?}",
            request.sources.monitors.len(),
            request.sources.windows.len(),
            request.app_id,
        );

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(config::APP_NAME)
            .default_width(860)
            .default_height(620)
            .build();

        let done: Rc<dyn Fn(Option<Vec<Selection>>)> = {
            let state = Rc::downgrade(self);
            Rc::new(move |selection| {
                if let Some(state) = state.upgrade() {
                    state.finish(selection);
                }
            })
        };

        let selected: Rc<RefCell<Option<Selection>>> = Rc::new(RefCell::new(None));

        let share = gtk::Button::with_label("Share");
        share.add_css_class("suggested-action");
        share.set_sensitive(false);

        let on_select: Rc<dyn Fn(Option<Selection>)> = {
            let selected = selected.clone();
            let share = share.clone();
            Rc::new(move |picked| {
                share.set_sensitive(picked.is_some());
                *selected.borrow_mut() = picked;
            })
        };

        let header = adw::HeaderBar::new();
        let cancel = gtk::Button::with_label("Cancel");
        header.pack_start(&cancel);
        header.set_title_widget(Some(&adw::WindowTitle::new(
            config::APP_NAME,
            &request.app_id,
        )));
        header.pack_end(&share);

        let stack = adw::ViewStack::new();
        stack.set_vexpand(true);
        let switcher = adw::ViewSwitcher::builder()
            .policy(adw::ViewSwitcherPolicy::Wide)
            .stack(&stack)
            .halign(gtk::Align::Center)
            .margin_top(12)
            .margin_bottom(12)
            .build();

        let mut previews = Vec::new();
        if has_monitors {
            let tab = monitor_tab::build(&request.sources.monitors, on_select.clone());
            stack.add_titled_with_icon(&tab.widget, Some("monitors"), "Monitors", icons::DISPLAY);
            previews = tab.previews;
        }
        if has_windows {
            let tab = window_tab::build(&request.sources.windows, on_select.clone());
            stack.add_titled_with_icon(&tab, Some("windows"), "Windows", icons::WINDOW);
        }

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        if has_monitors && has_windows {
            content.append(&switcher);
        }
        content.append(&stack);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));

        window.set_content(Some(&toolbar));

        {
            let done = done.clone();
            cancel.connect_clicked(move |_| done(None));
        }
        {
            let done = done.clone();
            let selected = selected.clone();
            share.connect_clicked(move |_| {
                let picked = selected.borrow().clone().map(|selection| vec![selection]);
                done(picked);
            });
        }
        {
            let done = done.clone();
            window.connect_close_request(move |_| {
                done(None);
                glib::Propagation::Proceed
            });
        }

        window.present();

        self.active.borrow_mut().replace(Active {
            id: request.id,
            window,
            reply: Some(request.reply),
            _previews: previews,
        });
    }

    fn finish(&self, selection: Option<Vec<Selection>>) {
        let active = self.active.borrow_mut().take();
        if let Some(mut active) = active {
            active.window.close();
            if let Some(reply) = active.reply.take() {
                let _ = reply.send(selection);
            }
        }
    }

    fn cancel(&self, id: &str) {
        let is_current = self
            .active
            .borrow()
            .as_ref()
            .is_some_and(|active| active.id == id);
        if is_current {
            self.finish(None);
        }
    }
}

/// Run the GTK picker until the process exits.
pub fn run(
    picker_rx: async_channel::Receiver<PickerRequest>,
    cancel_rx: async_channel::Receiver<String>,
) -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(config::APPLICATION_ID)
        .flags(gtk::gio::ApplicationFlags::IS_SERVICE)
        .build();

    let _hold = app.hold();

    let state: Rc<PickerState> = Rc::new(PickerState::default());

    app.connect_startup(move |app| {
        {
            let state = state.clone();
            let app = app.clone();
            let picker_rx = picker_rx.clone();
            glib::spawn_future_local(async move {
                while let Ok(request) = picker_rx.recv().await {
                    state.present(&app, request);
                }
            });
        }

        {
            let state = state.clone();
            let cancel_rx = cancel_rx.clone();
            glib::spawn_future_local(async move {
                while let Ok(id) = cancel_rx.recv().await {
                    state.cancel(&id);
                }
            });
        }
    });

    app.run()
}
