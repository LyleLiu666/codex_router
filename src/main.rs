mod auth;
mod config;
mod profile;
mod api;
mod state;
mod app_state;
mod worker;
mod app;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Codex Router",
        options,
        Box::new(|_cc| Box::new(app::RouterApp::new())),
    )
}
