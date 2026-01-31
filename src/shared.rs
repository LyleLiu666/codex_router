use crate::profile::ProfileSummary;
use crate::usage::UsageManager;
use std::sync::{Arc, RwLock};

#[derive(Debug)] // removed Clone as UsageManager might not be Clone (it has Mutex) - actually SharedState is wrapped in Arc so it doesn't need Clone itself?
                 // SharedState was derived Clone. Arc<RwLock> is Clone. Mutex is not Clone.
                 // I should wrap UsageManager in Arc.

pub struct SharedState {
    pub profiles: Arc<RwLock<Vec<ProfileSummary>>>,
    pub usage: Arc<UsageManager>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(Vec::new())),
            usage: Arc::new(UsageManager::new().expect("Failed to initialize UsageManager")), // Panic if fails? Or handle gracefully? usage logging failing shouldn't crash app?
                                                                                              // simpler to expect for now.
        }
    }

    pub fn update_profiles(&self, profiles: Vec<ProfileSummary>) {
        if let Ok(mut lock) = self.profiles.write() {
            *lock = profiles;
        }
    }
}
