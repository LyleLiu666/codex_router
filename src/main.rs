mod api;
mod app;
mod app_state;
mod auth;
mod config;
mod dock;
mod icon;
mod login_output;
mod oauth;
mod profile;
mod refresh;
mod server;
mod shared;
mod state;
#[cfg(test)]
mod test_support;
mod tray;
mod worker;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();

    // Initialize Tokio runtime
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Unable to create Runtime");

    // Enter the runtime context so that tokio::spawn works within the app initialization
    let _enter = rt.enter();

    let (rgba, width, height) = icon::load_icon_data();
    let icon_data = eframe::egui::IconData {
        rgba,
        width,
        height,
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_icon(icon_data),
        ..Default::default()
    };

    eframe::run_native(
        "Codex Router",
        options,
        Box::new(|_cc| Ok(Box::new(app::RouterApp::new()))),
    )
}
