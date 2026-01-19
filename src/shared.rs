use std::sync::{Arc, RwLock};
use crate::profile::ProfileSummary;

#[derive(Debug, Clone)]
pub struct SharedState {
    pub profiles: Arc<RwLock<Vec<ProfileSummary>>>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn update_profiles(&self, profiles: Vec<ProfileSummary>) {
        if let Ok(mut lock) = self.profiles.write() {
            *lock = profiles;
        }
    }
}
