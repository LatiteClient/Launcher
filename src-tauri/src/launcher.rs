use std::{
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    app_state::AppState, inject as injector, launch_request::InjectRequest, paths, release,
};
use tauri::{AppHandle, Manager};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
const STATUS_EVENT: &str = "inject_status";
const STATUS_IDLE: &str = "Idle";
const PROCESS_LOOKUP_ATTEMPTS: usize = 100;
const PROCESS_LOOKUP_DELAY: Duration = Duration::from_millis(50);
const STATUS_ANIMATION_DELAY: Duration = Duration::from_millis(300);

const FAILURE_STATUS_TIME: Duration = Duration::from_secs(5);
const POST_INJECTION_MONITOR_DURATION: Duration = Duration::from_secs(7);
const POST_INJECTION_MONITOR_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub struct LaunchError {
    message: String,
    dialog_already_shown: bool,
}

impl LaunchError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            dialog_already_shown: false,
        }
    }

    fn mark_dialog_shown(mut self) -> Self {
        self.dialog_already_shown = true;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn dialog_already_shown(&self) -> bool {
        self.dialog_already_shown
    }

    pub fn into_message(self) -> String {
        self.message
    }
}

impl From<String> for LaunchError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

#[derive(Clone)]
struct StatusEmitter {
    app_handle: AppHandle,
}

impl StatusEmitter {
    fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }

    fn emit(&self, status: &str) {
        if let Err(error) = self.app_handle.emit_all(STATUS_EVENT, status) {
            eprintln!("Failed to emit injection status '{status}': {error}");
        }
    }

    fn show_then_idle(&self, status: &str, delay: Duration) {
        self.emit(status);
        thread::sleep(delay);
        self.emit(STATUS_IDLE);
    }
}

struct StatusAnimation {
    should_run: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl StatusAnimation {
    fn start(status: StatusEmitter, label: &'static str) -> Self {
        let should_run = Arc::new(AtomicBool::new(true));
        let animation_flag = Arc::clone(&should_run);

        let handle = thread::spawn(move || {
            let dots = ["", ".", "..", "..."];
            let mut dot_index = 0;

            while animation_flag.load(Ordering::Relaxed) {
                let current_status = format!("{label}{}", dots[dot_index]);
                status.emit(&current_status);
                dot_index = (dot_index + 1) % dots.len();
                thread::sleep(STATUS_ANIMATION_DELAY);
            }
        });

        Self {
            should_run,
            handle: Some(handle),
        }
    }
}

impl Drop for StatusAnimation {
    fn drop(&mut self) {
        self.should_run.store(false, Ordering::Relaxed);

        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                eprintln!("Status animation thread panicked.");
            }
        }
    }
}

enum ProcessMonitorOutcome {
    Running,
    Exited,
}

pub async fn inject(
    state: &AppState,
    request: InjectRequest,
    app_handle: &AppHandle,
) -> Result<(), LaunchError> {
    let _guard = state.try_begin_injection().map_err(LaunchError::from)?;
    println!("Injecting...");

    let dll_path = resolve_dll_path(state, request)
        .await
        .map_err(LaunchError::from)?;
    let app_handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || inject_with_status(dll_path, app_handle))
        .await
        .map_err(|error| LaunchError::new(format!("Injection task failed: {error}")))?
}

fn inject_with_status(dll_path: PathBuf, app_handle: AppHandle) -> Result<(), LaunchError> {
    let status = StatusEmitter::new(app_handle);
    
    // Check if Minecraft is already running without starting animation
    let minecraft_pid = injector::find_process_id(MC_PROCESS_NAME).map_err(LaunchError::from)?;
    
    let (pid, _was_just_launched) = if let Some(pid) = minecraft_pid {
        // Minecraft already running - skip "Opening Minecraft" animation entirely
        println!("Minecraft process found with PID: {pid}");
        (pid, false)
    } else {
        // Minecraft not running - start animation and launch it
        let opening_animation = StatusAnimation::start(status.clone(), "Opening Minecraft");
        
        // Try to launch Minecraft and wait for it to appear
        if let Err(_) = launch_minecraft() {
            drop(opening_animation);  // Stop animation before showing error
            return Err(report_failure(
                &status,
                "Failed to open Minecraft",
                "Minecraft does not seem to be installed.",
                FAILURE_STATUS_TIME,
            ));
        }
        
        match wait_for_process(MC_PROCESS_NAME) {
            Ok(pid) => {
                println!("Minecraft process found with PID: {pid}");
                // Keep the Opening Minecraft animation running while we wait for game initialization
                println!("Minecraft was just launched, waiting 5 seconds for initialization...");
                thread::sleep(Duration::from_secs(5));
                drop(opening_animation);  // Stop animation before injection
                (pid, true)
            }
            Err(err_msg) => {
                drop(opening_animation);  // Stop animation before showing error
                return Err(report_failure(
                    &status,
                    "Failed to open Minecraft",
                    err_msg,
                    FAILURE_STATUS_TIME,
                ));
            }
        }
    };
    
    let injection_started_at = Instant::now();
    let injection_animation = StatusAnimation::start(status.clone(), "Injecting");
    let injection_result = injector::inject_dll(pid, &dll_path).map_err(LaunchError::from);

    // Ensure Injecting status shows for at least 5 seconds
    wait_for_minimum_duration(injection_started_at, Duration::from_secs(5));
    drop(injection_animation);

    if let Err(error) = injection_result {
        return Err(report_failure(
            &status,
            "Failed to inject",
            error.into_message(),
            FAILURE_STATUS_TIME,
        ));
    }

    match monitor_process_after_injection(MC_PROCESS_NAME, &status)? {
        ProcessMonitorOutcome::Running => {
            status.show_then_idle("Successfully injected", FAILURE_STATUS_TIME);
            Ok(())
        }
        ProcessMonitorOutcome::Exited => Err(report_failure(
            &status,
            "Failed to inject",
            "Minecraft process closed after DLL injection. The DLL may be incompatible with your Minecraft version.",
            FAILURE_STATUS_TIME,
        )),
    }
}



fn report_failure(
    status: &StatusEmitter,
    status_message: &str,
    error: impl Into<String>,
    _delay: Duration,
) -> LaunchError {
    let error = LaunchError::new(error.into());
    status.emit(status_message);
    // TODO: Append directions to report the bug with Latite Debug to all error messages
    // TODO: not related to report_failure specifically but while I'm here I might as well add
    // that we should be logging all these print's to a file, similar to how the old latiteinjector
    // does it.
    println!("{}", error.message());
    crate::dialogs::show_error("Latite Client", error.message());
    // Don't sleep here or emit Idle - return immediately so guard can be released quickly
    // Main thread will emit Idle after guard is released
    error.mark_dialog_shown()
}

fn wait_for_minimum_duration(started_at: Instant, minimum_duration: Duration) {
    let elapsed = started_at.elapsed();

    if elapsed < minimum_duration {
        thread::sleep(minimum_duration - elapsed);
    }
}

fn monitor_process_after_injection(
    process_name: &str,
    status: &StatusEmitter,
) -> Result<ProcessMonitorOutcome, LaunchError> {
    let _animation: StatusAnimation = StatusAnimation::start(status.clone(), "Finalizing");
    let monitor_iterations =
        POST_INJECTION_MONITOR_DURATION.as_millis() / POST_INJECTION_MONITOR_INTERVAL.as_millis();

    for attempt in 1..=monitor_iterations {
        thread::sleep(POST_INJECTION_MONITOR_INTERVAL);
        let elapsed_ms = attempt * POST_INJECTION_MONITOR_INTERVAL.as_millis();

        match injector::find_process_id(process_name) {
            Ok(Some(_)) => println!("Process alive at {elapsed_ms}ms"),
            Ok(None) => {
                println!("Process died after {elapsed_ms}ms");
                return Ok(ProcessMonitorOutcome::Exited);
            }
            Err(error) => {
                return Err(report_failure(
                    status,
                    "Failed to verify injection",
                    format!("Failed to confirm Minecraft stayed open after injection: {error}"),
                    FAILURE_STATUS_TIME,
                ));
            }
        }
    }

    println!(
        "Process survived {}ms monitoring period",
        POST_INJECTION_MONITOR_DURATION.as_millis()
    );
    Ok(ProcessMonitorOutcome::Running)
}

async fn resolve_dll_path(state: &AppState, request: InjectRequest) -> Result<PathBuf, String> {
    match request.dll_path {
        Some(dll_path) => {
            // Check if it's a URL
            if dll_path.starts_with("http://") || dll_path.starts_with("https://") {
                download_custom_dll(&dll_path).await
            } else {
                validate_custom_dll_path(dll_path)
            }
        }
        None => prepare_latite_dll(state).await,
    }
}

async fn download_custom_dll(url: &str) -> Result<PathBuf, String> {
    // Create temp directory for custom DLLs
    let temp_dir = std::env::temp_dir().join("LatiteCustomDLLs");
    std::fs::create_dir_all(&temp_dir)
        .map_err(|_e| "Failed to create DLL directory".to_string())?;

    // Generate a unique filename based on URL hash
    let hash = format!("{:x}", fxhash::hash64(&url));
    let temp_dll_path = temp_dir.join(format!("custom_{}.dll", hash));

    // Download the DLL
    let response = reqwest::get(url)
        .await
        .map_err(|_error| "Failed to download DLL".to_string())?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Failed to download DLL: HTTP {}", status.as_u16()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|_error| "Failed to read DLL file".to_string())?;

    std::fs::write(&temp_dll_path, &bytes)
        .map_err(|_error| "Failed to save DLL file. Please manually end the Minecraft.Windows.exe task in Task Manager and try again.".to_string())?;

    println!("Custom DLL downloaded to {}.", temp_dll_path.display());
    Ok(temp_dll_path)
}

async fn prepare_latite_dll(state: &AppState) -> Result<PathBuf, String> {
    let dll_path = paths::get_dll_path()?;
    let previous_version = state.get_last_used_version()?;
    let latest_version = match release::fetch_latest_release_name().await {
        Ok(version) => {
            println!("Latest release version: {version}");
            Some(version)
        }
        Err(error) => {
            eprintln!("{error}");
            None
        }
    };

    let dll_missing = !dll_path.exists();
    let has_newer_release = latest_version
        .as_deref()
        .is_some_and(|version| previous_version.as_deref() != Some(version));
    let needs_download = dll_missing || has_newer_release;

    if needs_download {
        release::download_latest_dll(&dll_path).await?;

        if let Some(version) = latest_version {
            state.set_last_used_version(Some(version))?;
        }
    }

    if !dll_path.exists() {
        return Err("Latite.dll is missing and could not be downloaded.".to_string());
    }

    Ok(dll_path)
}

fn validate_custom_dll_path(dll_path: String) -> Result<PathBuf, String> {
    // Check if it's a URL
    if dll_path.starts_with("http://") || dll_path.starts_with("https://") {
        // Validate URL format
        if let Err(_e) = url::Url::parse(&dll_path) {
            return Err("Invalid DLL URL format".to_string());
        }
        
        // For URLs, we need to download them asynchronously
        // This is a blocking function, so we'll handle this in resolve_dll_path instead
        return Ok(PathBuf::from(dll_path));
    }

    let dll_path_buf = PathBuf::from(&dll_path);

    if !dll_path_buf.exists() {
        return Err("DLL file does not exist".to_string());
    }

    if !dll_path_buf.is_file() {
        return Err("DLL path is not a file".to_string());
    }

    let is_dll = dll_path_buf
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"));

    if !is_dll {
        return Err("Selected file is not a DLL".to_string());
    }

    std::fs::canonicalize(&dll_path_buf)
        .map_err(|_error| "Failed to resolve DLL path".to_string())
}

fn launch_minecraft() -> Result<(), String> {
    let mut process = Command::new("explorer")
        .arg("minecraft:")
        .spawn()
        .map_err(|_| "Minecraft does not seem to be installed.".to_string())?;

    process
        .wait()
        .map_err(|error| format!("Failed while launching Minecraft: {error}"))?;

    Ok(())
}

fn wait_for_process(process_name: &str) -> Result<u32, String> {
    for attempt in 0..PROCESS_LOOKUP_ATTEMPTS {
        println!(
            "Waiting for {process_name}... ({}/{PROCESS_LOOKUP_ATTEMPTS})",
            attempt + 1
        );

        thread::sleep(PROCESS_LOOKUP_DELAY);

        if let Some(pid) = injector::find_process_id(process_name)? {
            println!("{process_name} found with PID: {pid}");
            return Ok(pid);
        }
    }

    Err(format!(
        "{process_name} was not found after launching. Please try launching again, or please make sure Minecraft is installed."
    ))
}
