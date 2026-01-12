use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use eframe::egui;

use crate::app_state::{AppCommand, AppEvent, AppState};
use crate::tray::{self, TrayEvent, TrayHandle};
use crate::worker;

pub struct RouterApp {
    state: AppState,
    cmd_tx: Sender<AppCommand>,
    evt_rx: Receiver<AppEvent>,
    tray_rx: Receiver<TrayEvent>,
    tray_handle: Option<TrayHandle>,
    worker_handle: Option<JoinHandle<()>>,
}

impl RouterApp {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let (tray_tx, tray_rx) = std::sync::mpsc::channel();
        let worker_handle = worker::start_worker(cmd_rx, evt_tx);
        let tray_handle = if cfg!(test) {
            None
        } else {
            Some(tray::start_tray(tray_tx))
        };

        let state = AppState::default();
        let _ = cmd_tx.send(AppCommand::LoadProfiles);

        Self {
            state,
            cmd_tx,
            evt_rx,
            tray_rx,
            tray_handle,
            worker_handle: Some(worker_handle),
        }
    }
}

fn command_for_tray_event(event: &TrayEvent) -> Option<AppCommand> {
    match event {
        TrayEvent::RefreshProfiles => Some(AppCommand::LoadProfiles),
        TrayEvent::SwitchProfile(name) => Some(AppCommand::SwitchProfile(name.clone())),
        _ => None,
    }
}

impl eframe::App for RouterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.evt_rx.try_recv() {
            if let AppEvent::ProfilesLoaded(ref profiles) = event {
                if let Some(tray_handle) = &self.tray_handle {
                    tray_handle.update_profiles(profiles);
                }
            }
            self.state.apply_event(event);
        }

        while let Ok(event) = self.tray_rx.try_recv() {
            if let Some(command) = command_for_tray_event(&event) {
                let _ = self.cmd_tx.send(command);
            }
            match event {
                TrayEvent::OpenWindow => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
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

    #[test]
    fn tray_event_maps_to_app_command() {
        let refresh = command_for_tray_event(&TrayEvent::RefreshProfiles);
        assert!(matches!(refresh, Some(AppCommand::LoadProfiles)));

        let switch = command_for_tray_event(&TrayEvent::SwitchProfile("alpha".to_string()));
        match switch {
            Some(AppCommand::SwitchProfile(name)) => assert_eq!(name, "alpha"),
            _ => panic!("expected switch profile command"),
        }

        let open = command_for_tray_event(&TrayEvent::OpenWindow);
        assert!(open.is_none());
    }
}
