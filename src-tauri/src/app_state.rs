use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, MutexGuard,
};

use crate::options::OptionsStore;

pub struct AppState {
    options: Mutex<OptionsStore>,
    is_injecting: AtomicBool,
}

pub struct InjectionGuard<'a> {
    flag: &'a AtomicBool,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            options: Mutex::new(OptionsStore::load()?),
            is_injecting: AtomicBool::new(false),
        })
    }

    pub fn try_begin_injection(&self) -> Result<InjectionGuard<'_>, String> {
        self.is_injecting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                "Injection is already in progress. Please wait until Status: Idle".to_string()
            })?;

        Ok(InjectionGuard {
            flag: &self.is_injecting,
        })
    }

    pub fn get_bool_option(&self, id: &str) -> Result<bool, String> {
        let options = self.lock_options()?;
        options.get_bool(id)
    }

    pub fn update_bool_option(&self, id: &str, value: bool) -> Result<(), String> {
        let mut options = self.lock_options()?;
        options.set_bool(id, value)?;
        options.save()
    }

    pub fn get_last_used_version(&self) -> Result<Option<String>, String> {
        let options = self.lock_options()?;
        Ok(options.last_used_version().map(str::to_owned))
    }

    pub fn set_last_used_version(&self, version: Option<String>) -> Result<(), String> {
        let mut options = self.lock_options()?;
        options.set_last_used_version(version);
        options.save()
    }

    fn lock_options(&self) -> Result<MutexGuard<'_, OptionsStore>, String> {
        self.options
            .lock()
            .map_err(|_| "Launcher options state is unavailable.".to_string())
    }
}

impl Drop for InjectionGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
