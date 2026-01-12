use std::env;
use std::path::Path;
use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub struct EnvGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvGuard {
    pub fn set(key: &'static str, value: &Path) -> Self {
        let original = env::var(key).ok();
        env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            env::set_var(self.key, value);
        } else {
            env::remove_var(self.key);
        }
    }
}
