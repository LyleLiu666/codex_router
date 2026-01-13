mod auth;
mod config;
mod profile;
mod api;
mod state;
mod refresh;
mod app_state;
mod worker;
mod app;
mod tray;
mod login_output;
#[cfg(test)]
mod test_support;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Codex Router",
        options,
        Box::new(|_cc| Ok(Box::new(app::RouterApp::new()))),
    )
}
