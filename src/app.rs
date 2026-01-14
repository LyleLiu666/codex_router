use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use eframe::egui;

use crate::app_state::{AppCommand, AppEvent, AppState};
use crate::refresh::RefreshSchedule;
use crate::state::{self, RouterState};
use crate::tray::{self, TrayEvent, TrayHandle};
use crate::worker;

pub struct RouterApp {
    state: AppState,
    router_state: RouterState,
    quota_refresh: RefreshSchedule,
    allow_close: bool,
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

        let mut state = AppState::default();
        let router_state = match state::load_state() {
            Ok(router_state) => {
                apply_router_state(&mut state, &router_state);
                router_state
            }
            Err(err) => {
                state.error = Some(err.to_string());
                RouterState::default()
            }
        };
        let _ = cmd_tx.send(AppCommand::LoadProfiles);

        Self {
            state,
            router_state,
            quota_refresh: RefreshSchedule::new(),
            allow_close: false,
            cmd_tx,
            evt_rx,
            tray_rx,
            tray_handle,
            worker_handle: Some(worker_handle),
        }
    }

    fn persist_router_state(&mut self) {
        if let Err(err) = state::save_state(&self.router_state) {
            self.state.error = Some(err.to_string());
        }
    }
}

fn command_for_tray_event(event: &TrayEvent) -> Option<AppCommand> {
    match event {
        TrayEvent::SwitchProfile(name) => Some(AppCommand::SwitchProfile(name.clone())),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseAction {
    Hide,
    Close,
}

fn close_action(allow_close: bool) -> CloseAction {
    if allow_close {
        CloseAction::Close
    } else {
        CloseAction::Hide
    }
}

fn should_fetch_on_profile_change(prev: Option<&str>, next: Option<&str>) -> bool {
    match (prev, next) {
        (None, None) => false,
        (Some(prev), Some(next)) => prev != next,
        (None, Some(_)) => true,
        (Some(_), None) => false,
    }
}

fn format_reset_time(utc_str: Option<&str>) -> String {
    let Some(utc_str) = utc_str else {
        return "-".to_string();
    };

    // Parse the UTC time string
    let Ok(utc_time) = DateTime::parse_from_rfc3339(utc_str) else {
        return utc_str.to_string();
    };

    // Convert to local time
    let local_time = utc_time.with_timezone(&Local);
    
    // Format as "YYYY-MM-DD HH:MM (local)"
    local_time.format("%Y-%m-%d %H:%M (local)").to_string()
}

fn apply_router_state(app_state: &mut AppState, router_state: &RouterState) {
    app_state.refresh_interval_seconds = router_state.refresh_interval_seconds;
    app_state.auto_refresh_enabled = router_state.auto_refresh_enabled;
}

fn update_router_state_settings(
    router_state: &mut RouterState,
    app_state: &mut AppState,
    interval_seconds: u64,
    auto_refresh_enabled: bool,
) -> bool {
    let mut changed = false;
    if router_state.refresh_interval_seconds != interval_seconds {
        router_state.refresh_interval_seconds = interval_seconds;
        app_state.refresh_interval_seconds = interval_seconds;
        changed = true;
    }
    if router_state.auto_refresh_enabled != auto_refresh_enabled {
        router_state.auto_refresh_enabled = auto_refresh_enabled;
        app_state.auto_refresh_enabled = auto_refresh_enabled;
        changed = true;
    }
    changed
}

fn auto_refresh_tick(
    enabled: bool,
    schedule: &mut RefreshSchedule,
    now: Instant,
    interval: Duration,
) -> bool {
    if !enabled {
        return false;
    }
    schedule.tick(now, interval)
}

impl eframe::App for RouterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            match close_action(self.allow_close) {
                CloseAction::Hide => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                CloseAction::Close => {}
            }
        }

        while let Ok(event) = self.evt_rx.try_recv() {
            let prev_profile = self.state.current_profile.clone();
            let is_profiles_loaded = matches!(event, AppEvent::ProfilesLoaded(_));
            if let AppEvent::ProfilesLoaded(ref profiles) = event {
                if let Some(tray_handle) = &self.tray_handle {
                    tray_handle.update_profiles(profiles);
                }
            }
            self.state.apply_event(event);
            if is_profiles_loaded {
                let next_profile = self.state.current_profile.clone();
                if should_fetch_on_profile_change(
                    prev_profile.as_deref(),
                    next_profile.as_deref(),
                ) {
                    self.router_state.last_selected_profile = next_profile;
                    self.persist_router_state();
                    let _ = self.cmd_tx.send(AppCommand::FetchQuota);
                }
            }
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
                    self.allow_close = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Codex Router");

            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label("Profiles");
                    if ui.button("Refresh").clicked() {
                        let _ = self.cmd_tx.send(AppCommand::FetchQuota);
                        self.quota_refresh.clear();
                    }

                    let mut auto_refresh_enabled = self.state.auto_refresh_enabled;
                    if ui
                        .checkbox(&mut auto_refresh_enabled, "Auto refresh")
                        .changed()
                    {
                        let interval_seconds = self.state.refresh_interval_seconds;
                        let changed = update_router_state_settings(
                            &mut self.router_state,
                            &mut self.state,
                            interval_seconds,
                            auto_refresh_enabled,
                        );
                        if changed {
                            self.persist_router_state();
                        }
                        self.quota_refresh.clear();
                        if auto_refresh_enabled {
                            let _ = self.cmd_tx.send(AppCommand::FetchQuota);
                        }
                    }

                    let mut interval_minutes = (self.state.refresh_interval_seconds / 60).max(1);
                    let response = ui.add(
                        egui::DragValue::new(&mut interval_minutes)
                            .range(1..=120)
                            .suffix(" min"),
                    );
                    if response.changed() {
                        let interval_seconds = interval_minutes.saturating_mul(60);
                        let auto_refresh_enabled = self.state.auto_refresh_enabled;
                        let changed = update_router_state_settings(
                            &mut self.router_state,
                            &mut self.state,
                            interval_seconds,
                            auto_refresh_enabled,
                        );
                        if changed {
                            self.persist_router_state();
                            self.quota_refresh.clear();
                        }
                    }
                });

                if self.state.profiles.is_empty() {
                    ui.label("No profiles yet. Save current login or run codex login.");
                }

                for profile in &self.state.profiles {
                    ui.separator();
                    ui.horizontal(|ui| {
                        let profile_label = match &profile.email {
                            Some(email) => format!("{} ({})", profile.name, email),
                            None => profile.name.clone(),
                        };
                        ui.strong(&profile_label);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("⟳").clicked() {
                                let _ = self.cmd_tx.send(AppCommand::FetchProfileQuota(profile.name.clone()));
                            }
                            if profile.is_current {
                                ui.add_enabled(false, egui::Button::new("Current"));
                            } else if ui.button("Switch").clicked() {
                                let _ = self
                                    .cmd_tx
                                    .send(AppCommand::SwitchProfile(profile.name.clone()));
                            }
                        });
                    });

                    if let Some(quota) = &profile.quota {
                        egui::Grid::new(format!("quota_grid_{}", profile.name))
                            .num_columns(2)
                            .spacing([10.0, 2.0])
                            .show(ui, |ui| {
                                ui.label("Plan");
                                ui.label(&quota.plan_type);
                                ui.end_row();

                                ui.label("Primary");
                                ui.label(match quota.used_requests {
                                    Some(used) => {
                                        let total = quota.total_requests.unwrap_or(100);
                                        let left = total.saturating_sub(used.min(total));
                                        format!("{}% used ({}% left)", used.min(total), left)
                                    }
                                    None => "-".to_string(),
                                });
                                ui.end_row();

                                ui.label("Secondary");
                                ui.label(match quota.used_tokens {
                                    Some(used) => {
                                        let total = quota.total_tokens.unwrap_or(100);
                                        let left = total.saturating_sub(used.min(total));
                                        format!("{}% used ({}% left)", used.min(total), left)
                                    }
                                    None => "-".to_string(),
                                });
                                ui.end_row();

                                ui.label("Reset (Primary)");
                                ui.label(format_reset_time(quota.reset_date.as_deref()));
                                ui.end_row();

                                ui.label("Reset (Secondary)");
                                ui.label(format_reset_time(quota.secondary_reset_date.as_deref()));
                                ui.end_row();
                            });
                    } else {
                        ui.label("Loading quota...");
                    }
                }
            });

            if let Some(error) = &self.state.error {
                ui.colored_label(egui::Color32::RED, error);
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Save as");
                ui.text_edit_singleline(&mut self.state.profile_name_input);
                let trimmed = self.state.profile_name_input.trim().to_string();
                if ui
                    .add_enabled(!trimmed.is_empty(), egui::Button::new("Save Current Profile"))
                    .clicked()
                {
                    let _ = self.cmd_tx.send(AppCommand::SaveProfile(trimmed));
                    self.state.profile_name_input.clear();
                }
            });

            if let Some(message) = &self.state.profile_message {
                ui.label(message);
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Login");
                let run_button = ui.add_enabled(
                    !self.state.login_running,
                    egui::Button::new("Run codex login"),
                );
                if run_button.clicked() {
                    let _ = self.cmd_tx.send(AppCommand::RunLogin);
                }
                if self.state.login_running {
                    ui.label("Running…");
                }
            });

            if let Some(url) = &self.state.login_url {
                ui.horizontal(|ui| {
                    ui.label(url);
                    if ui.button("Open URL").clicked() {
                        let _ = self.cmd_tx.send(AppCommand::OpenLoginUrl(url.clone()));
                    }
                });
            }
            if let Some(code) = &self.state.login_code {
                ui.label(format!("Code: {code}"));
            }
            if !self.state.login_output.is_empty() {
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    ui.monospace(&self.state.login_output);
                });
            }
        });

        let interval = Duration::from_secs(self.state.refresh_interval_seconds.max(60));
        if auto_refresh_tick(
            self.state.auto_refresh_enabled,
            &mut self.quota_refresh,
            Instant::now(),
            interval,
        ) {
            let _ = self.cmd_tx.send(AppCommand::FetchQuota);
        }
        if self.state.auto_refresh_enabled {
            ctx.request_repaint_after(interval);
        }

        // Always request periodic repaint to handle tray events even when window is hidden
        ctx.request_repaint_after(Duration::from_millis(500));
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
    use crate::refresh::RefreshSchedule;
    use std::time::{Duration, Instant};

    fn switchable_profiles(profiles: &[crate::profile::ProfileSummary]) -> Vec<crate::profile::ProfileSummary> {
        profiles
            .iter()
            .filter(|profile| !profile.is_current)
            .cloned()
            .collect()
    }

    #[test]
    fn router_app_initializes_state() {
        let app = RouterApp::new();
        assert_eq!(app.state.refresh_interval_seconds, 600);
    }

    #[test]
    fn tray_event_maps_to_app_command() {
        let switch = command_for_tray_event(&TrayEvent::SwitchProfile("alpha".to_string()));
        match switch {
            Some(AppCommand::SwitchProfile(name)) => assert_eq!(name, "alpha"),
            _ => panic!("expected switch profile command"),
        }

        let open = command_for_tray_event(&TrayEvent::OpenWindow);
        assert!(open.is_none());
    }

    #[test]
    fn close_action_hides_unless_allowed() {
        assert!(matches!(close_action(false), CloseAction::Hide));
        assert!(matches!(close_action(true), CloseAction::Close));
    }

    #[test]
    fn profile_change_triggers_refresh() {
        assert!(should_fetch_on_profile_change(None, Some("a")));
        assert!(should_fetch_on_profile_change(Some("a"), Some("b")));
        assert!(!should_fetch_on_profile_change(Some("a"), Some("a")));
        assert!(!should_fetch_on_profile_change(None, None));
    }

    #[test]
    fn switchable_profiles_excludes_current() {
        use crate::profile::ProfileSummary;

        let profiles = vec![
            ProfileSummary {
                name: "work".to_string(),
                email: Some("work@example.com".to_string()),
                is_current: true,
                quota: None,
            },
            ProfileSummary {
                name: "personal".to_string(),
                email: Some("personal@example.com".to_string()),
                is_current: false,
                quota: None,
            },
        ];

        let switchable = switchable_profiles(&profiles);
        assert_eq!(switchable.len(), 1);
        assert_eq!(switchable[0].name, "personal");
    }

    #[test]
    fn applies_router_state_to_app_state() {
        let mut app_state = AppState::default();
        let router_state = RouterState {
            refresh_interval_seconds: 300,
            auto_refresh_enabled: false,
            last_selected_profile: Some("work".to_string()),
        };

        apply_router_state(&mut app_state, &router_state);

        assert_eq!(app_state.refresh_interval_seconds, 300);
        assert!(!app_state.auto_refresh_enabled);
    }

    #[test]
    fn update_router_state_settings_returns_change() {
        let mut app_state = AppState::default();
        let mut router_state = RouterState::default();

        let changed = update_router_state_settings(&mut router_state, &mut app_state, 900, false);

        assert!(changed);
        assert_eq!(router_state.refresh_interval_seconds, 900);
        assert!(!router_state.auto_refresh_enabled);
    }

    #[test]
    fn auto_refresh_disabled_never_triggers() {
        let mut schedule = RefreshSchedule::new();
        let now = Instant::now();
        let interval = Duration::from_secs(60);

        let triggered = auto_refresh_tick(false, &mut schedule, now, interval);

        assert!(!triggered);
        assert!(schedule.next_due().is_none());
    }
}
