use std::{path::PathBuf, process::Command, thread, time::Duration};

use crate::{
    app_state::AppState, inject as injector, launch_request::InjectRequest, paths, release,
};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
const PROCESS_LOOKUP_ATTEMPTS: usize = 100;
const PROCESS_LOOKUP_DELAY: Duration = Duration::from_millis(50);

pub async fn inject(state: &AppState, request: InjectRequest) -> Result<(), String> {
    let _guard = state.try_begin_injection()?;
    println!("Injecting...");

    let dll_path = resolve_dll_path(state, request).await?;

    let pid = if let Some(pid) = injector::find_process_id(MC_PROCESS_NAME)? {
        println!("Minecraft process found with PID: {pid}");
        pid
    } else {
        launch_minecraft()?;
        wait_for_process(MC_PROCESS_NAME)?
    };

    injector::inject_dll(pid, &dll_path)
}

async fn resolve_dll_path(state: &AppState, request: InjectRequest) -> Result<PathBuf, String> {
    match request.dll_path {
        Some(dll_path) => validate_custom_dll_path(dll_path),
        None => prepare_latite_dll(state).await,
    }
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
    let dll_path = PathBuf::from(dll_path);

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
        "{process_name} was not found after launching. Please try again."
    ))
}
