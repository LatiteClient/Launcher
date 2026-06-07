use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use crate::{
    app_state::AppState,
    inject as injector,
    latite_dll::{self, LatiteDllMetadata},
    launch_request::{BuildKind, InjectRequest},
    paths, release,
    ui::{self, UiDialog, UiMessage},
    version_info,
};
use tauri::{AppHandle, Manager};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::RPC_E_CHANGED_MODE,
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_LOCAL_SERVER,
            COINIT_MULTITHREADED,
        },
        UI::Shell::{ApplicationActivationManager, IApplicationActivationManager, AO_NONE},
    },
};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
const MC_AUMID: PCWSTR = w!("MICROSOFT.MINECRAFTUWP_8wekyb3d8bbwe!Game");
const STATUS_EVENT: &str = "inject_status";
const STATUS_IDLE: &str = "launcher.status.idle.name";
const STATUS_DOWNLOADING_ASSETS: &str = "launcher.status.downloadingAssets.name";
const STATUS_INJECTING: &str = "launcher.status.injecting.name";
const STATUS_OPENING_MINECRAFT: &str = "launcher.status.openingMinecraft.name";
const STATUS_LOADING_MINECRAFT: &str = "launcher.status.loadingMinecraft.name";
const STATUS_SUCCESS: &str = "launcher.status.success.name";
const STATUS_INJECT_FAILED: &str = "launcher.status.injectFailed.name";
const STATUS_LAUNCH_FAILED: &str = "launcher.status.launchFailed.name";
const STATUS_MINECRAFT_NOT_FOUND: &str = "launcher.status.minecraftNotFound.name";
const STATUS_VERIFY_FAILED: &str = "launcher.status.verifyFailed.name";
const STATUS_PREPARING_DLL: &str = "launcher.status.preparingDll.name";
const STATUS_INJECTION_ERROR: &str = "launcher.status.injectionError.name";
const STATUS_INVALID_DLL_PATH: &str = "launcher.status.invalidDllPath.name";
const STATUS_UNSUPPORTED_MINECRAFT: &str = "launcher.status.unsupportedMinecraft.name";

const PROCESS_LOOKUP_ATTEMPTS: usize = 100;
const PROCESS_LOOKUP_DELAY: Duration = Duration::from_millis(50);
const STATUS_ANIMATION_DELAY: Duration = Duration::from_millis(300);
const INJECTION_MIN_STATUS_TIME: Duration = Duration::from_secs(5);
const FAILURE_STATUS_TIME: Duration = Duration::from_secs(3);
const MINECRAFT_LOADING_DELAY: Duration = Duration::from_secs(6);

static MONITORED_PROCESS_IDS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();

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
            crate::log_error!("Failed to emit injection status '{}': {error}", message.key);
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
                let current_status = UiMessage::new(label_key).with_arg(dots[dot_index]);
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
                crate::log_error!("Status animation thread panicked.");
            }
        }
    }
}

struct ResolvedDll {
    path: PathBuf,
    metadata: Option<LatiteDllMetadata>,
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
        crate::log_error!("{}", error.message());
        error.mark_dialog_shown()
    })?;
    crate::log_info!("Injecting...");

    let resolved_dll = resolve_dll(state, request, &status)
        .await
        .map_err(|error| {
            // Distinguish between invalid DLL path and DLL preparation errors
            if error.contains("does not exist")
                || error.contains("is not a file")
                || error.contains("is not a DLL")
                || error.contains("Failed to resolve DLL path")
            {
                report_failure(
                    &status,
                    STATUS_INVALID_DLL_PATH,
                    UiDialog::error("launcher.error.prepareDll.name").with_arg(&error),
                    error,
                )
            } else {
                report_failure(
                    &status,
                    STATUS_PREPARING_DLL,
                    UiDialog::error("launcher.error.prepareDll.name").with_arg(&error),
                    error,
                )
            }
        })?;
    let worker_app_handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        inject_with_status(resolved_dll, worker_app_handle)
    })
    .await
    .map_err(|error| {
        let message = format!("Injection task failed: {error}");
        report_failure(
            &status,
            STATUS_INJECTION_ERROR,
            UiDialog::error("launcher.error.launchTaskFailed.name").with_arg(&message),
            message,
        )
    })?
}

fn inject_with_status(resolved_dll: ResolvedDll, app_handle: AppHandle) -> Result<(), LaunchError> {
    let status = StatusEmitter::new(app_handle);
    let (pid, was_already_running) = find_or_launch_minecraft(&status)?;

    // If Minecraft was just launched, show loading animation before injecting
    // If it was already running, skip straight to injection
    if !was_already_running {
        let _loading_animation = StatusAnimation::start(status.clone(), STATUS_LOADING_MINECRAFT);
        thread::sleep(MINECRAFT_LOADING_DELAY);
    }

    if let Some(metadata) = &resolved_dll.metadata {
        verify_minecraft_supported(pid, metadata, &status)?;
    }

    let injection_started_at = Instant::now();
    let injection_animation = StatusAnimation::start(status.clone(), STATUS_INJECTING);
    let injection_result = injector::open_target_process(pid).and_then(|target_process| {
        injector::inject_dll(&target_process, &resolved_dll.path)?;
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
                UiDialog::error("launcher.error.injectFailed.name").with_arg(&error),
                error,
            ));
        }
    };

    status.show_then_idle(UiMessage::new(STATUS_SUCCESS), FAILURE_STATUS_TIME);
    start_process_lifetime_monitor(target_process, status);
    Ok(())
}

fn start_process_lifetime_monitor(target_process: injector::TargetProcess, status: StatusEmitter) {
    let target_pid = target_process.pid();
    let monitored_process_ids = MONITORED_PROCESS_IDS.get_or_init(|| Mutex::new(HashSet::new()));

    {
        let mut monitored_process_ids = monitored_process_ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !monitored_process_ids.insert(target_pid) {
            crate::log_info!("Minecraft process {target_pid} is already being monitored.");
            return;
        }
    }

    thread::spawn(move || {
        crate::log_info!("Monitoring Minecraft process {target_pid} for its remaining lifetime.");

        match injector::wait_for_process_exit_forever(&target_process) {
            Ok(0) => {
                crate::log_info!("Minecraft process {target_pid} closed normally.");
            }
            Ok(exit_code) => {
                let message = format!(
                    "Minecraft process {target_pid} closed after DLL injection with exit code {exit_code:#x}. The DLL may be incompatible with your Minecraft version."
                );
                let _ = report_failure(
                    &status,
                    STATUS_INJECT_FAILED,
                    UiDialog::error("launcher.error.injectedProcessExited.name")
                        .with_arg(target_pid)
                        .with_arg(format!("{exit_code:#x}")),
                    message,
                );
            }
            Err(error) => {
                crate::log_error!(
                    "Failed to monitor Minecraft process {target_pid} for its lifetime: {error}"
                );
            }
        }

        let mut monitored_process_ids = MONITORED_PROCESS_IDS
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        monitored_process_ids.remove(&target_pid);
    });
}

fn find_or_launch_minecraft(status: &StatusEmitter) -> Result<(u32, bool), LaunchError> {
    if let Some(pid) = injector::find_process_id(MC_PROCESS_NAME).map_err(|error| {
        report_failure(
            status,
            STATUS_INJECT_FAILED,
            UiDialog::error("launcher.error.injectFailed.name").with_arg(&error),
            error,
        )
    })? {
        crate::log_info!("Minecraft process found with PID: {pid} - Minecraft already running");
        return Ok((pid, true));
    }

    crate::log_info!("Minecraft process not found - launching Minecraft");
    status.emit_key(STATUS_OPENING_MINECRAFT);

    if let Err(error) = launch_minecraft() {
        return Err(report_failure(
            status,
            STATUS_LAUNCH_FAILED,
            UiDialog::error("launcher.error.openMinecraft.name").with_arg(&error),
            error,
        ));
    }

    let pid = wait_for_process(MC_PROCESS_NAME).map_err(|error| {
        report_failure(
            status,
            STATUS_MINECRAFT_NOT_FOUND,
            UiDialog::error("launcher.error.minecraftNotFound.name").with_arg(&error),
            error,
        )
    })?;

    Ok((pid, false))
}

fn verify_minecraft_supported(
    pid: u32,
    metadata: &LatiteDllMetadata,
    status: &StatusEmitter,
) -> Result<(), LaunchError> {
    let minecraft_path = injector::get_process_image_path(pid).map_err(|error| {
        report_failure(
            status,
            STATUS_VERIFY_FAILED,
            UiDialog::error("launcher.error.minecraftVersionCheckFailed.name").with_arg(&error),
            error,
        )
    })?;
    let minecraft_version = match version_info::get_file_version(&minecraft_path) {
        Ok(version) => {
            crate::log_info!(
                "Detected Minecraft version {version} from {}.",
                minecraft_path.display()
            );
            version
        }
        Err(error) if error.is_missing_version_info() => {
            crate::log_info!("{error} Trying package-version fallback.");

            match injector::get_process_package_version(pid) {
                Ok(Some(package_version)) => {
                    let matched_version = version_info::match_supported_package_version(
                        package_version.major,
                        package_version.minor,
                        package_version.build,
                        metadata.supported_minecraft_versions(),
                    );

                    crate::log_info!(
                        "Detected Minecraft package version {}.{}.{}.{}.",
                        package_version.major,
                        package_version.minor,
                        package_version.build,
                        package_version.revision,
                    );

                    if let Some(version) = matched_version {
                        crate::log_info!(
                            "Latite DLL version {} supports Minecraft package version {}.{}.{}.{} via supported version {version}.",
                            metadata.version(),
                            package_version.major,
                            package_version.minor,
                            package_version.build,
                            package_version.revision,
                        );
                        return Ok(());
                    }

                    let package_version = format!(
                        "{}.{}.{}.{}",
                        package_version.major,
                        package_version.minor,
                        package_version.build,
                        package_version.revision,
                    );
                    let supported_versions =
                        format_supported_versions(metadata.supported_minecraft_versions());

                    return Err(report_failure(
                        status,
                        STATUS_UNSUPPORTED_MINECRAFT,
                        UiDialog::error("launcher.error.unsupportedMinecraftVersion.name")
                            .with_arg(&package_version)
                            .with_arg(metadata.version())
                            .with_arg(&supported_versions),
                        format!(
                            "Latite DLL version {} does not support Minecraft package version {package_version}. Supported Minecraft versions: {supported_versions}",
                            metadata.version()
                        ),
                    ));
                }
                Ok(None) => {
                    crate::log_info!(
                        "{} Process {pid} has no package identity. Skipping Minecraft compatibility precheck and continuing injection.",
                        error
                    );
                    return Ok(());
                }
                Err(package_error) => {
                    crate::log_error!(
                        "{} Package-version fallback also failed: {package_error}. Skipping Minecraft compatibility precheck and continuing injection.",
                        error
                    );
                    return Ok(());
                }
            }
        }
        Err(error) => {
            let error = error.to_string();
            return Err(report_failure(
                status,
                STATUS_VERIFY_FAILED,
                UiDialog::error("launcher.error.minecraftVersionCheckFailed.name").with_arg(&error),
                error,
            ));
        }
    };

    if metadata.supports_minecraft_version(&minecraft_version) {
        crate::log_info!(
            "Latite DLL version {} supports Minecraft version {minecraft_version}.",
            metadata.version()
        );
        return Ok(());
    }

    let supported_versions = format_supported_versions(metadata.supported_minecraft_versions());
    Err(report_failure(
        status,
        STATUS_UNSUPPORTED_MINECRAFT,
        UiDialog::error("launcher.error.unsupportedMinecraftVersion.name")
            .with_arg(&minecraft_version)
            .with_arg(metadata.version())
            .with_arg(&supported_versions),
        format!(
            "Latite DLL version {} does not support Minecraft version {minecraft_version}. Supported Minecraft versions: {supported_versions}",
            metadata.version()
        ),
    ))
}

fn format_supported_versions(versions: &[String]) -> String {
    if versions.is_empty() {
        "none reported".to_string()
    } else {
        versions.join(", ")
    }
}

fn report_failure(
    status: &StatusEmitter,
    status_key: &str,
    dialog: UiDialog,
    error: impl Into<String>,
) -> LaunchError {
    let error = LaunchError::new(error.into());
    status.emit(UiMessage::new(status_key));
    // TODO: Append directions to report the bug with Latite Debug to all error messages
    crate::log_error!("{}", error.message());
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

async fn resolve_dll(
    state: &AppState,
    request: InjectRequest,
    status: &StatusEmitter,
) -> Result<ResolvedDll, String> {
    let InjectRequest { dll_path, build } = request;

    match dll_path {
        Some(dll_path) => resolve_custom_dll(dll_path).await,
        None => prepare_latite_dll(state, resolve_latite_build(state, build)?, status).await,
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

async fn prepare_latite_dll(
    state: &AppState,
    build: BuildKind,
    status: &StatusEmitter,
) -> Result<ResolvedDll, String> {
    let build_path = paths::get_latite_build_path(build)?;
    let dll_path = build_path.join(release::latite_dll_file_name(build));

    match build {
        BuildKind::Release => prepare_release_dll(state, &build_path, &dll_path, status).await?,
        BuildKind::Nightly | BuildKind::Debug => {
            prepare_mutable_build(build, &build_path, status).await?
        }
    }

    if !release::has_required_assets(build, &build_path) {
        return Err(format!(
            "{} files are missing and could not be downloaded.",
            release::build_display_name(build)
        ));
    }

    let metadata = latite_dll::read_metadata(&dll_path)?;
    crate::log_info!(
        "Prepared {} DLL version {}. Supported Minecraft versions: {}.",
        release::build_display_name(build),
        metadata.version(),
        format_supported_versions(metadata.supported_minecraft_versions())
    );

    if build == BuildKind::Release {
        state.set_last_used_version(Some(metadata.version().to_string()))?;
    }

    Ok(ResolvedDll {
        path: dll_path,
        metadata: Some(metadata),
    })
}

async fn prepare_release_dll(
    state: &AppState,
    build_path: &Path,
    dll_path: &Path,
    status: &StatusEmitter,
) -> Result<(), String> {
    let previous_version = state.get_last_used_version()?;
    let cached_metadata = read_cached_latite_metadata(dll_path);
    let cached_version = cached_metadata
        .as_ref()
        .map(|metadata| metadata.version())
        .or(previous_version.as_deref());
    let latest_dll_version = match release::fetch_latest_release_name(release::LATITE_REPO).await {
        Ok(version) => {
            crate::log_info!("Latest release version: {version}");
            Some(version)
        }
        Err(error) => {
            crate::log_error!("{error}");
            None
        }
    };

    let release_cached = release::has_required_assets(BuildKind::Release, build_path);
    let has_newer_release = latest_dll_version.as_deref().is_some_and(|version| {
        cached_version.map_or(true, |cached_version| {
            !latite_dll::versions_equivalent(cached_version, version)
        })
    });
    let needs_download = !release_cached || cached_metadata.is_none() || has_newer_release;

    if needs_download {
        let _download_animation = StatusAnimation::start(status.clone(), STATUS_DOWNLOADING_ASSETS);
        release::download_build(BuildKind::Release, build_path).await?;
    }

    Ok(())
}

fn read_cached_latite_metadata(dll_path: &Path) -> Option<LatiteDllMetadata> {
    if !dll_path.is_file() {
        return None;
    }

    match latite_dll::read_metadata(dll_path) {
        Ok(metadata) => {
            crate::log_info!(
                "Cached Latite DLL version {} found at {}.",
                metadata.version(),
                dll_path.display()
            );
            Some(metadata)
        }
        Err(error) => {
            crate::log_error!(
                "Failed to read cached Latite DLL metadata from {}: {error}",
                dll_path.display()
            );
            None
        }
    }
}

async fn prepare_mutable_build(
    build: BuildKind,
    build_path: &Path,
    status: &StatusEmitter,
) -> Result<(), String> {
    let cached_assets_exist = release::has_required_assets(build, build_path);
    let _download_animation = StatusAnimation::start(status.clone(), STATUS_DOWNLOADING_ASSETS);

    // Nightly and debug are mutable tags, so refresh them when possible.
    match release::download_build(build, build_path).await {
        Ok(_) => Ok(()),
        Err(error) if cached_assets_exist => {
            crate::log_error!("{error}");
            crate::log_info!(
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

async fn resolve_custom_dll(dll_path: String) -> Result<ResolvedDll, String> {
    let dll_path = dll_path.trim().to_string();

    let path = if is_custom_dll_url(&dll_path) {
        validate_custom_dll_url(&dll_path)?;
        download_custom_dll(&dll_path).await?
    } else {
        validate_custom_dll_path(dll_path)?
    };

    Ok(ResolvedDll {
        path,
        metadata: None,
    })
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
    struct ComInitializationGuard(bool);

    impl Drop for ComInitializationGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    unsafe {
        let initialization_result = CoInitializeEx(None, COINIT_MULTITHREADED);
        let _com_guard = ComInitializationGuard(initialization_result.is_ok());

        if initialization_result.is_err() && initialization_result != RPC_E_CHANGED_MODE {
            return Err(format!(
                "Failed to initialize COM while launching Minecraft: {}",
                windows::core::Error::from_hresult(initialization_result)
            ));
        }

        let activation_manager: IApplicationActivationManager =
            CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER).map_err(
                |error| format!("Failed to create the application activation manager: {error}"),
            )?;

        activation_manager
            .ActivateApplication(MC_AUMID, PCWSTR::null(), AO_NONE)
            .map_err(|_| "Minecraft does not seem to be installed.".to_string())?;
    }

    Ok(())
}

fn wait_for_process(process_name: &str) -> Result<u32, String> {
    for attempt in 0..PROCESS_LOOKUP_ATTEMPTS {
        crate::log_info!(
            "Waiting for {process_name}... ({}/{PROCESS_LOOKUP_ATTEMPTS})",
            attempt + 1
        );

        thread::sleep(PROCESS_LOOKUP_DELAY);

        if let Some(pid) = injector::find_process_id(process_name)? {
            crate::log_info!("{process_name} found with PID: {pid}");
            return Ok(pid);
        }
    }

    Err(format!(
        "{process_name} was not found after launching. Please try launching again, or please make sure Minecraft is installed."
    ))
}
