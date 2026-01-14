use chrono::{DateTime, Utc};

use crate::api::QuotaInfo;
use crate::login_output::LoginOutput;
use crate::profile::{ProfileSummary, SaveProfileOutcome};

#[derive(Debug, Clone)]
pub enum AppCommand {
    LoadProfiles,
    SwitchProfile(String),
    SaveProfile(String),
    DeleteProfile(String),
    RunLogin,
    OpenLoginUrl(String),
    FetchQuota,
    FetchProfileQuota(String),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    ProfilesLoaded(Vec<ProfileSummary>),
    ProfileSaved(SaveProfileOutcome),
    LoginOutput {
        output: String,
        parsed: LoginOutput,
        running: bool,
    },
    LoginFinished {
        success: bool,
        message: String,
    },
    QuotaLoaded(QuotaInfo),
    ProfileQuotaLoaded { name: String, quota: QuotaInfo },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub profiles: Vec<ProfileSummary>,
    pub current_profile: Option<String>,
    pub quota: Option<QuotaInfo>,
    pub refresh_interval_seconds: u64,
    pub auto_refresh_enabled: bool,
    pub last_updated: Option<DateTime<Utc>>,
    pub profile_message: Option<String>,
    pub profile_name_input: String,
    pub login_output: String,
    pub login_url: Option<String>,
    pub login_code: Option<String>,
    pub login_running: bool,
    pub error: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            profiles: Vec::new(),
            current_profile: None,
            quota: None,
            refresh_interval_seconds: 600,
            auto_refresh_enabled: true,
            last_updated: None,
            profile_message: None,
            profile_name_input: String::new(),
            login_output: String::new(),
            login_url: None,
            login_code: None,
            login_running: false,
            error: None,
        }
    }
}

impl AppState {
    pub fn apply_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::ProfilesLoaded(profiles) => {
                self.current_profile = profiles
                    .iter()
                    .find(|profile| profile.is_current)
                    .map(|profile| profile.name.clone());
                self.profiles = profiles;
            }
            AppEvent::QuotaLoaded(quota) => {
                self.quota = Some(quota);
                self.last_updated = Some(Utc::now());
            }
            AppEvent::ProfileQuotaLoaded { name, quota } => {
                if let Some(profile) = self.profiles.iter_mut().find(|p| p.name == name) {
                    profile.quota = Some(quota);
                }
            }
            AppEvent::ProfileSaved(outcome) => {
                self.profile_message = Some(match outcome {
                    SaveProfileOutcome::Created { name } => format!("Saved profile: {name}"),
                    SaveProfileOutcome::Updated { name } => format!("Updated profile: {name}"),
                    SaveProfileOutcome::AlreadyExists { name } => {
                        format!("Profile already saved: {name}")
                    }
                });
            }
            AppEvent::LoginOutput {
                output,
                parsed,
                running,
            } => {
                if !output.is_empty() {
                    self.login_output.push_str(&output);
                }
                if parsed.url.is_some() {
                    self.login_url = parsed.url;
                }
                if parsed.code.is_some() {
                    self.login_code = parsed.code;
                }
                self.login_running = running;
            }
            AppEvent::LoginFinished { success, message } => {
                self.login_running = false;
                if !message.is_empty() {
                    if !self.login_output.is_empty() && !self.login_output.ends_with('\n') {
                        self.login_output.push('\n');
                    }
                    self.login_output.push_str(&message);
                    self.login_output.push('\n');
                }
                if success {
                    self.error = None;
                } else {
                    self.error = Some(message);
                }
            }
            AppEvent::Error(message) => {
                self.error = Some(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::login_output::LoginOutput;
    use crate::profile::SaveProfileOutcome;

    fn sample_profile() -> ProfileSummary {
        ProfileSummary {
            name: "work".to_string(),
            email: None,
            is_current: true,
            quota: None,
        }
    }

    #[test]
    fn applies_profiles_loaded_event() {
        let mut state = AppState::default();
        state.apply_event(AppEvent::ProfilesLoaded(vec![sample_profile()]));
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.current_profile.as_deref(), Some("work"));
    }

    #[test]
    fn applies_profile_saved_event() {
        let mut state = AppState::default();
        state.apply_event(AppEvent::ProfileSaved(SaveProfileOutcome::Created {
            name: "work".to_string(),
        }));
        assert_eq!(state.profile_message.as_deref(), Some("Saved profile: work"));
    }

    #[test]
    fn applies_login_output_event() {
        let mut state = AppState::default();
        state.apply_event(AppEvent::LoginOutput {
            output: "hello".to_string(),
            parsed: LoginOutput {
                url: Some("http://localhost".to_string()),
                code: None,
            },
            running: true,
        });
        assert!(state.login_running);
        assert!(state.login_output.contains("hello"));
        assert_eq!(state.login_url.as_deref(), Some("http://localhost"));
    }

    #[test]
    fn applies_save_input_change() {
        let mut state = AppState::default();
        state.profile_name_input = "work".to_string();
        assert_eq!(state.profile_name_input, "work");
    }
}
