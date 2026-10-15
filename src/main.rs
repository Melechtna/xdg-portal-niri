//! [`main`] is the orchestrator: it wires the channels together, starts the
//! portal thread, and then hands the main thread to GTK. This is done for
//! the sake of modularity, as with most of my code bases.

mod config;
mod error;
mod icons;
mod monitor_tab;
mod niri;
mod picker;
mod preview;
mod request;
mod screencast;
mod session;
mod sources;
mod window_tab;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

fn main() -> glib::ExitCode {
    init_tracing();

    // Portal half -> picker half.
    let (picker_tx, picker_rx) = async_channel::unbounded::<picker::PickerRequest>();
    // Frontend cancellation -> picker half.
    let (cancel_tx, cancel_rx) = async_channel::unbounded::<String>();

    spawn_portal_thread(picker_tx, cancel_tx);

    // GTK must own the main thread.
    picker::run(picker_rx, cancel_rx)
}

/// Start the D-Bus half on its own thread and runtime.
fn spawn_portal_thread(
    picker_tx: async_channel::Sender<picker::PickerRequest>,
    cancel_tx: async_channel::Sender<String>,
) {
    let result = std::thread::Builder::new()
        .name("xdg-portal-niri-portal".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    tracing::error!("could not build the portal runtime: {err:#}");
                    std::process::exit(1);
                }
            };

            runtime.block_on(async move {
                if let Err(err) = screencast::serve(picker_tx, cancel_tx).await {
                    tracing::error!("the portal backend stopped: {err:#}");
                    std::process::exit(1);
                }
            });
        })
        .context("spawning the portal thread");

    if let Err(err) = result {
        tracing::error!("{err:#}");
        std::process::exit(1);
    }
}

/// Set up logging, honouring `RUST_LOG` (or `XDG_PORTAL_NIRI_LOG`).
fn init_tracing() {
    let filter = std::env::var("XDG_PORTAL_NIRI_LOG")
        .ok()
        .or_else(|| std::env::var("RUST_LOG").ok())
        .unwrap_or_else(|| "xdg_portal_niri=debug,info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .init();
}
