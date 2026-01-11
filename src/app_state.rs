use chrono::{DateTime, Utc};

use crate::api::QuotaInfo;
use crate::profile::ProfileSummary;

#[derive(Debug, Clone)]
pub enum AppCommand {
    LoadProfiles,
    SwitchProfile(String),
    SaveProfile(String),
    DeleteProfile(String),
    FetchQuota,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    ProfilesLoaded(Vec<ProfileSummary>),
    QuotaLoaded(QuotaInfo),
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
            AppEvent::Error(message) => {
                self.error = Some(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_profile() -> ProfileSummary {
        ProfileSummary {
            name: "work".to_string(),
            email: None,
            is_current: true,
        }
    }

    #[test]
    fn applies_profiles_loaded_event() {
        let mut state = AppState::default();
        state.apply_event(AppEvent::ProfilesLoaded(vec![sample_profile()]));
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.current_profile.as_deref(), Some("work"));
    }
}
