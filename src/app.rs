use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use eframe::egui;

use crate::app_state::{AppCommand, AppEvent, AppState};
use crate::worker;

pub struct RouterApp {
    state: AppState,
    cmd_tx: Sender<AppCommand>,
    evt_rx: Receiver<AppEvent>,
    worker_handle: Option<JoinHandle<()>>,
}

impl RouterApp {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let worker_handle = worker::start_worker(cmd_rx, evt_tx);

        let state = AppState::default();
        let _ = cmd_tx.send(AppCommand::LoadProfiles);

        Self {
            state,
            cmd_tx,
            evt_rx,
            worker_handle: Some(worker_handle),
        }
    }
}

impl eframe::App for RouterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.evt_rx.try_recv() {
            self.state.apply_event(event);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Codex Router");

            if ui.button("Refresh Profiles").clicked() {
                let _ = self.cmd_tx.send(AppCommand::LoadProfiles);
            }

            if let Some(error) = &self.state.error {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.separator();

            for profile in &self.state.profiles {
                let name = if profile.is_current {
                    format!("* {}", profile.name)
                } else {
                    profile.name.clone()
                };
                ui.horizontal(|ui| {
                    ui.label(name);
                    if let Some(email) = &profile.email {
                        ui.label(email);
                    }
                });
            }
        });
    }
}

impl Drop for RouterApp {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(AppCommand::Shutdown);
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_app_initializes_state() {
        let app = RouterApp::new();
        assert_eq!(app.state.refresh_interval_seconds, 600);
    }
}
