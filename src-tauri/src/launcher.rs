use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    app_state::AppState,
    inject as injector,
    launch_request::{BuildKind, InjectRequest},
    paths, release,
    ui::{self, UiDialog, UiMessage},
};
use tauri::{AppHandle, Manager};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
const STATUS_EVENT: &str = "inject_status";
const STATUS_IDLE: &str = "launcher.status.idle.name";
const STATUS_INJECTING: &str = "launcher.status.injecting.name";
const STATUS_OPENING_MINECRAFT: &str = "launcher.status.openingMinecraft.name";
const STATUS_LOADING_MINECRAFT: &str = "launcher.status.loadingMinecraft.name";
const STATUS_FINALIZING: &str = "launcher.status.finalizing.name";
const STATUS_SUCCESS: &str = "launcher.status.success.name";
const STATUS_INJECT_FAILED: &str = "launcher.status.injectFailed.name";
const STATUS_LAUNCH_FAILED: &str = "launcher.status.launchFailed.name";
const STATUS_MINECRAFT_NOT_FOUND: &str = "launcher.status.minecraftNotFound.name";
const STATUS_VERIFY_FAILED: &str = "launcher.status.verifyFailed.name";
const STATUS_PREPARING_DLL: &str = "launcher.status.preparingDll.name";
const STATUS_INJECTION_ERROR: &str = "launcher.status.injectionError.name";
const STATUS_INVALID_DLL_PATH: &str = "launcher.status.invalidDllPath.name";
const PROCESS_LOOKUP_ATTEMPTS: usize = 100;
const PROCESS_LOOKUP_DELAY: Duration = Duration::from_millis(50);
const STATUS_ANIMATION_DELAY: Duration = Duration::from_millis(300);
const INJECTION_MIN_STATUS_TIME: Duration = Duration::from_secs(5);
const FAILURE_STATUS_TIME: Duration = Duration::from_secs(3);
const POST_INJECTION_MONITOR_DURATION: Duration = Duration::from_secs(8);
const MINECRAFT_LOADING_DELAY: Duration = Duration::from_secs(6);

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

    fn emit(&self, message: UiMessage) {
        if let Err(error) = self.app_handle.emit_all(STATUS_EVENT, &message) {
            eprintln!("Failed to emit injection status '{}': {error}", message.key);
        }
    }

    fn emit_key(&self, key: &str) {
        self.emit(UiMessage::new(key));
    }

    fn emit_dialog(&self, dialog: UiDialog) {
        ui::emit_dialog(&self.app_handle, &dialog);
    }

    fn show_then_idle(&self, status: UiMessage, delay: Duration) {
        self.emit(status);
        thread::sleep(delay);
        self.emit(UiMessage::new(STATUS_IDLE));
    }
}

struct StatusAnimation {
    should_run: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl StatusAnimation {
    fn start(status: StatusEmitter, label_key: &'static str) -> Self {
        let should_run = Arc::new(AtomicBool::new(true));
        let animation_flag = Arc::clone(&should_run);

        let handle = thread::spawn(move || {
            let dots = ["", ".", "..", "..."];
            let mut dot_index = 0;

            while animation_flag.load(Ordering::Relaxed) {
                let current_status = UiMessage::new(label_key).with_var("dots", dots[dot_index]);
                status.emit(current_status);
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
    Exited(u32),
}

pub async fn inject(
    state: &AppState,
    request: InjectRequest,
    app_handle: &AppHandle,
) -> Result<(), LaunchError> {
    let status = StatusEmitter::new(app_handle.clone());
    
    let _guard = state.try_begin_injection().map_err(|error| {
        // Injection in progress - show dialog only, no status
        let error = LaunchError::new(error);
        status.emit_dialog(UiDialog::error("launcher.error.injectionInProgress.name"));
        println!("{}", error.message());
        error.mark_dialog_shown()
    })?;
    println!("Injecting...");

    let dll_path = resolve_dll_path(state, request).await.map_err(|error| {
        // Distinguish between invalid DLL path and DLL preparation errors
        if error.contains("does not exist") 
            || error.contains("is not a file")
            || error.contains("is not a DLL")
            || error.contains("Failed to resolve DLL path") {
            report_failure(
                &status,
                STATUS_INVALID_DLL_PATH,
                UiDialog::error("launcher.error.prepareDll.name").with_var("detail", &error),
                error,
            )
        } else {
            report_failure(
                &status,
                STATUS_PREPARING_DLL,
                UiDialog::error("launcher.error.prepareDll.name").with_var("detail", &error),
                error,
            )
        }
    })?;
    let worker_app_handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || inject_with_status(dll_path, worker_app_handle))
        .await
        .map_err(|error| {
            let message = format!("Injection task failed: {error}");
            report_failure(
                &status,
                STATUS_INJECTION_ERROR,
                UiDialog::error("launcher.error.launchTaskFailed.name")
                    .with_var("detail", &message),
                message,
            )
        })?
}

fn inject_with_status(dll_path: PathBuf, app_handle: AppHandle) -> Result<(), LaunchError> {
    let status = StatusEmitter::new(app_handle);
    let (pid, was_already_running) = find_or_launch_minecraft(&status)?;

    // If Minecraft was just launched, show loading animation before injecting
    // If it was already running, skip straight to injection
    if !was_already_running {
        let _loading_animation = StatusAnimation::start(status.clone(), STATUS_LOADING_MINECRAFT);
        thread::sleep(MINECRAFT_LOADING_DELAY);
    }

    let injection_started_at = Instant::now();
    let injection_animation = StatusAnimation::start(status.clone(), STATUS_INJECTING);
    let injection_result = injector::open_target_process(pid).and_then(|target_process| {
        injector::inject_dll(&target_process, &dll_path)?;
        Ok(target_process)
    });

    wait_for_minimum_duration(injection_started_at, INJECTION_MIN_STATUS_TIME);
    drop(injection_animation);

    let target_process = match injection_result {
        Ok(target_process) => target_process,
        Err(error) => {
            return Err(report_failure(
                &status,
                STATUS_INJECT_FAILED,
                UiDialog::error("launcher.error.injectFailed.name").with_var("detail", &error),
                error,
            ));
        }
    };

    match monitor_process_after_injection(&target_process, &status)? {
        ProcessMonitorOutcome::Running => {
            status.show_then_idle(UiMessage::new(STATUS_SUCCESS), FAILURE_STATUS_TIME);
            Ok(())
        }
        ProcessMonitorOutcome::Exited(exit_code) if exit_code != 0 => {
            Err(report_failure(
                &status,
                STATUS_INJECT_FAILED,
                UiDialog::error("launcher.error.injectedProcessExited.name")
                    .with_var("pid", pid)
                    .with_var("exitCode", format!("{exit_code:#x}")),
                format!(
                    "Minecraft process {pid} closed after DLL injection with exit code {exit_code:#x}. The DLL may be incompatible with your Minecraft version."
                ),
            ))
        }
        ProcessMonitorOutcome::Exited(exit_code) => {
            println!(
                "Process {pid} exited after injection with exit code {exit_code:#x}; treating injection as successful."
            );
            status.show_then_idle(UiMessage::new(STATUS_SUCCESS), FAILURE_STATUS_TIME);
            Ok(())
        }
    }
}

fn find_or_launch_minecraft(status: &StatusEmitter) -> Result<(u32, bool), LaunchError> {
    if let Some(pid) = injector::find_process_id(MC_PROCESS_NAME).map_err(|error| {
        report_failure(
            status,
            STATUS_INJECT_FAILED,
            UiDialog::error("launcher.error.injectFailed.name").with_var("detail", &error),
            error,
        )
    })? {
        println!("Minecraft process found with PID: {pid} - Minecraft already running");
        return Ok((pid, true));
    }

    println!("Minecraft process not found - launching Minecraft");
    status.emit_key(STATUS_OPENING_MINECRAFT);

    if let Err(error) = launch_minecraft() {
        return Err(report_failure(
            status,
            STATUS_LAUNCH_FAILED,
            UiDialog::error("launcher.error.openMinecraft.name").with_var("detail", &error),
            error,
        ));
    }

    let pid = wait_for_process(MC_PROCESS_NAME).map_err(|error| {
        report_failure(
            status,
            STATUS_MINECRAFT_NOT_FOUND,
            UiDialog::error("launcher.error.minecraftNotFound.name").with_var("detail", &error),
            error,
        )
    })?;

    Ok((pid, false))
}

fn report_failure(
    status: &StatusEmitter,
    status_key: &str,
    dialog: UiDialog,
    error: impl Into<String>,
) -> LaunchError {
    let error = LaunchError::new(error.into());
    status.emit(UiMessage::new(status_key));
    println!("{}", error.message());
    status.emit_dialog(dialog);
    // Keep status visible while user reads dialog, then revert to idle
    // Typical dialog dismissal time is 2-5 seconds, use 5 second timeout
    thread::sleep(Duration::from_secs(6));
    status.emit(UiMessage::new(STATUS_IDLE));
    error.mark_dialog_shown()
}

fn wait_for_minimum_duration(started_at: Instant, minimum_duration: Duration) {
    let elapsed = started_at.elapsed();

    if elapsed < minimum_duration {
        thread::sleep(minimum_duration - elapsed);
    }
}

fn monitor_process_after_injection(
    target_process: &injector::TargetProcess,
    status: &StatusEmitter,
) -> Result<ProcessMonitorOutcome, LaunchError> {
    let _animation: StatusAnimation = StatusAnimation::start(status.clone(), STATUS_FINALIZING);
    let monitor_started_at = Instant::now();
    let target_pid = target_process.pid();

    match injector::wait_for_process_exit(target_process, POST_INJECTION_MONITOR_DURATION) {
        Ok(injector::ProcessWaitOutcome::Running) => {
            println!(
                "Process {target_pid} survived {}ms monitoring period - injection successful",
                POST_INJECTION_MONITOR_DURATION.as_millis()
            );
            Ok(ProcessMonitorOutcome::Running)
        }
        Ok(injector::ProcessWaitOutcome::Exited(exit_code)) => {
            let elapsed_ms = monitor_started_at.elapsed().as_millis();
            println!(
                "Process {target_pid} exited after {elapsed_ms}ms with exit code {exit_code:#x} - DLL may be incompatible or crashed"
            );
            Ok(ProcessMonitorOutcome::Exited(exit_code))
        }
        Err(error) => {
            eprintln!("Failed to monitor process {target_pid}: {error}");
            Err(report_failure(
                status,
                STATUS_VERIFY_FAILED,
                UiDialog::error("launcher.error.verifyInjection.name")
                    .with_var("detail", &error)
                    .with_var("pid", target_pid),
                format!(
                    "Failed to confirm Minecraft process {target_pid} stayed open after injection: {error}"
                ),
            ))
        }
    }
}

pub async fn check_for_updates(
    current_version: &str,
    app_handle: &AppHandle,
) -> Result<(), String> {
    match release::fetch_latest_release_name(release::LAUNCHER_REPO).await {
        Ok(latest_version) => {
            println!("Latest launcher version: {latest_version}, Current launcher version: {current_version}");

            if current_version != latest_version {
                ui::emit_dialog(
                    app_handle,
                    &UiDialog::info("launcher.dialog.updateAvailable.name")
                        .with_var("latestVersion", latest_version),
                );
            }

            Ok(())
        }
        Err(error) => Err(format!("Failed to check for launcher updates: {error}")),
    }
}

async fn resolve_dll_path(state: &AppState, request: InjectRequest) -> Result<PathBuf, String> {
    let InjectRequest { dll_path, build } = request;

    match dll_path {
        Some(dll_path) => resolve_custom_dll_path(dll_path).await,
        None => prepare_latite_dll(state, resolve_latite_build(state, build)?).await,
    }
}

fn resolve_latite_build(
    state: &AppState,
    requested_build: Option<BuildKind>,
) -> Result<BuildKind, String> {
    match requested_build {
        Some(build) => Ok(build),
        None => state.get_latite_build(),
    }
}

async fn prepare_latite_dll(state: &AppState, build: BuildKind) -> Result<PathBuf, String> {
    let build_path = paths::get_latite_build_path(build)?;
    let dll_path = build_path.join(release::latite_dll_file_name(build));

    match build {
        BuildKind::Release => prepare_release_dll(state, &build_path).await?,
        BuildKind::Nightly | BuildKind::Debug => prepare_mutable_build(build, &build_path).await?,
    }

    if !release::has_required_assets(build, &build_path) {
        return Err(format!(
            "{} files are missing and could not be downloaded.",
            release::build_display_name(build)
        ));
    }

    Ok(dll_path)
}

async fn prepare_release_dll(state: &AppState, build_path: &Path) -> Result<(), String> {
    let previous_version = state.get_last_used_version()?;
    let latest_dll_version = match release::fetch_latest_release_name(release::RELEASE_REPO).await {
        Ok(version) => {
            println!("Latest release version: {version}");
            Some(version)
        }
        Err(error) => {
            eprintln!("{error}");
            None
        }
    };

    let release_cached = release::has_required_assets(BuildKind::Release, build_path);
    let has_newer_release = latest_dll_version
        .as_deref()
        .is_some_and(|version| previous_version.as_deref() != Some(version));
    let needs_download = !release_cached || has_newer_release;

    if needs_download {
        release::download_build(BuildKind::Release, build_path).await?;

        if let Some(version) = latest_dll_version {
            state.set_last_used_version(Some(version))?;
        }
    }

    Ok(())
}

async fn prepare_mutable_build(build: BuildKind, build_path: &Path) -> Result<(), String> {
    let cached_assets_exist = release::has_required_assets(build, build_path);

    // Nightly and debug are mutable tags, so refresh them when possible.
    match release::download_build(build, build_path).await {
        Ok(_) => Ok(()),
        Err(error) if cached_assets_exist => {
            eprintln!("{error}");
            println!(
                "Using cached {} files from {}.",
                release::build_display_name(build),
                build_path.display()
            );
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn validate_custom_dll_path(dll_path: String) -> Result<PathBuf, String> {
    let dll_path = PathBuf::from(dll_path.trim());

    if !dll_path.exists() {
        return Err(format!(
            "The selected DLL does not exist: {}",
            dll_path.display()
        ));
    }

    if !dll_path.is_file() {
        return Err(format!(
            "The selected path is not a file: {}",
            dll_path.display()
        ));
    }

    let is_dll = dll_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"));

    if !is_dll {
        return Err(format!(
            "The selected file is not a DLL: {}",
            dll_path.display()
        ));
    }

    std::fs::canonicalize(&dll_path)
        .map_err(|error| format!("Failed to resolve DLL path {}: {error}", dll_path.display()))
}

async fn resolve_custom_dll_path(dll_path: String) -> Result<PathBuf, String> {
    let dll_path = dll_path.trim().to_string();

    if is_custom_dll_url(&dll_path) {
        validate_custom_dll_url(&dll_path)?;
        return download_custom_dll(&dll_path).await;
    }

    validate_custom_dll_path(dll_path)
}

fn is_custom_dll_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn validate_custom_dll_url(url: &str) -> Result<(), String> {
    let parsed_url =
        reqwest::Url::parse(url).map_err(|error| format!("Invalid DLL URL: {error}"))?;

    match parsed_url.scheme() {
        "http" | "https" => Ok(()),
        _ => Err("Custom DLL URLs must use http or https.".to_string()),
    }
}

async fn download_custom_dll(url: &str) -> Result<PathBuf, String> {
    let custom_dll_cache = std::env::temp_dir().join("Latite").join("CustomDLLs");
    std::fs::create_dir_all(&custom_dll_cache).map_err(|error| {
        format!(
            "Failed to create the custom DLL cache at {}: {error}",
            custom_dll_cache.display()
        )
    })?;

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let download_id = hasher.finish();
    let downloaded_dll_path = custom_dll_cache.join(format!("custom-{download_id:016x}.dll"));

    let response = reqwest::get(url)
        .await
        .map_err(|error| format!("Failed to download DLL from {url}: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Failed to download DLL from {url}: HTTP {}",
            status
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read the downloaded DLL from {url}: {error}"))?;

    std::fs::write(&downloaded_dll_path, &bytes).map_err(|error| {
        format!(
            "Failed to save the downloaded DLL to {}: {error}",
            downloaded_dll_path.display()
        )
    })?;

    Ok(downloaded_dll_path)
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
