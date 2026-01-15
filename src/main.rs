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
mod icon;
#[cfg(test)]
mod test_support;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    
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
