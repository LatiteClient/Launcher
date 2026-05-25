use std::{
    fs,
    sync::{Mutex, MutexGuard},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const INSTANCE_MUTEX_NAME: &str = "Local\\LatiteClientLauncher.SingleInstance";
const DUPLICATE_SIGNAL_FILE_NAME: &str = "duplicate-instance-attempt.signal";
const DUPLICATE_SIGNAL_POLL_DELAY: Duration = Duration::from_millis(250);

pub struct InstanceManager {
    guard: Mutex<Option<InstanceGuard>>,
}

impl InstanceManager {
    pub fn new(guard: Option<InstanceGuard>) -> Self {
        Self {
            guard: Mutex::new(guard),
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut guard = self.lock_guard()?;

        if enabled {
            if guard.is_none() {
                *guard =
                    Some(try_acquire()?.ok_or_else(|| {
                        "Another launcher instance is already running.".to_string()
                    })?);
            }

            return Ok(());
        }

        *guard = None;
        Ok(())
    }

    pub fn is_enabled(&self) -> Result<bool, String> {
        Ok(self.lock_guard()?.is_some())
    }

    fn lock_guard(&self) -> Result<MutexGuard<'_, Option<InstanceGuard>>, String> {
        self.guard
            .lock()
            .map_err(|_| "Launcher instance guard is unavailable.".to_string())
    }
}

pub fn try_acquire() -> Result<Option<InstanceGuard>, String> {
    platform::try_acquire(INSTANCE_MUTEX_NAME)
}

pub fn signal_duplicate_instance_attempt() -> Result<(), String> {
    let signal_path = signal_path()?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("System clock is before Unix epoch: {error}"))?
        .as_nanos();

    fs::write(
        &signal_path,
        format!("pid={};timestamp={timestamp}", std::process::id()),
    )
    .map_err(|error| {
        format!(
            "Failed to notify the existing launcher instance at {}: {error}",
            signal_path.display()
        )
    })
}

pub fn start_duplicate_attempt_monitor<F>(on_attempt: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    let signal_path = signal_path()?;
    let mut last_payload = fs::read_to_string(&signal_path).ok();

    thread::spawn(move || loop {
        thread::sleep(DUPLICATE_SIGNAL_POLL_DELAY);

        let Ok(payload) = fs::read_to_string(&signal_path) else {
            continue;
        };

        if last_payload.as_deref() == Some(payload.as_str()) {
            continue;
        }

        last_payload = Some(payload);
        on_attempt();
    });

    Ok(())
}

fn signal_path() -> Result<std::path::PathBuf, String> {
    Ok(crate::paths::get_launcher_path()?.join(DUPLICATE_SIGNAL_FILE_NAME))
}

#[cfg(windows)]
mod platform {
    use std::os::windows::ffi::OsStrExt;

    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
            System::Threading::CreateMutexW,
        },
    };

    pub struct InstanceGuard(HANDLE);

    unsafe impl Send for InstanceGuard {}

    pub fn try_acquire(name: &str) -> Result<Option<InstanceGuard>, String> {
        let name_wide = to_wide_null(name);
        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name_wide.as_ptr())) }
            .map_err(|error| format!("Failed to create launcher instance guard: {error}"))?;

        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            unsafe {
                let _ = CloseHandle(handle);
            }

            return Ok(None);
        }

        Ok(Some(InstanceGuard(handle)))
    }

    fn to_wide_null(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    impl Drop for InstanceGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub struct InstanceGuard;

    pub fn try_acquire(_name: &str) -> Result<Option<InstanceGuard>, String> {
        Ok(Some(InstanceGuard))
    }
}

pub use platform::InstanceGuard;
