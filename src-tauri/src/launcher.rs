use std::{path::Path, process::Command, thread, time::Duration};

use crate::{app_state::AppState, inject as injector, paths, release};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
const PROCESS_LOOKUP_ATTEMPTS: usize = 100;
const PROCESS_LOOKUP_DELAY: Duration = Duration::from_millis(50);

pub async fn inject(state: &AppState) -> Result<(), String> {
    let _guard = state.try_begin_injection()?;
    println!("Injecting...");

    let dll_path = paths::get_dll_path()?;
    prepare_dll(state, &dll_path).await?;

    let pid = if let Some(pid) = injector::find_process_id(MC_PROCESS_NAME)? {
        println!("Minecraft process found with PID: {pid}");
        pid
    } else {
        launch_minecraft()?;
        wait_for_process(MC_PROCESS_NAME)?
    };

    injector::inject_dll(pid, &dll_path)
}

async fn prepare_dll(state: &AppState, dll_path: &Path) -> Result<(), String> {
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
        release::download_latest_dll(dll_path).await?;

        if let Some(version) = latest_version {
            state.set_last_used_version(Some(version))?;
        }
    }

    if !dll_path.exists() {
        return Err("Latite.dll is missing and could not be downloaded.".to_string());
    }

    Ok(())
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
