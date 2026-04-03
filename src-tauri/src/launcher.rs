use std::{path::PathBuf, process::Command, thread, time::Duration};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::{
    app_state::AppState, inject as injector, launch_request::InjectRequest, paths, release,
};
use tauri::{AppHandle, Manager};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
const PROCESS_LOOKUP_ATTEMPTS: usize = 100;
const PROCESS_LOOKUP_DELAY: Duration = Duration::from_millis(50);

pub async fn inject(state: &AppState, request: InjectRequest, app_handle: &AppHandle) -> Result<(), String> {
    let _guard = state.try_begin_injection()?;
    println!("Injecting...");

    let dll_path = resolve_dll_path(state, request).await?;

    let pid = if let Some(pid) = injector::find_process_id(MC_PROCESS_NAME)? {
        println!("Minecraft process found with PID: {pid}");
        app_handle.emit_all("inject_status", "Injecting").unwrap();
        pid
    } else {
        app_handle.emit_all("inject_status", "Opening Minecraft").unwrap();
        match launch_minecraft() {
            Ok(_) => {},
            Err(err) => {
                app_handle.emit_all("inject_status", "Cannot open Minecraft").unwrap();
                // Give the user time to see the message before it could reset
                thread::sleep(Duration::from_secs(3));
                app_handle.emit_all("inject_status", "Idle").unwrap();
                return Err(err);
            }
        }
        // Start animated injecting status while waiting for process
        match wait_for_process_with_animation(MC_PROCESS_NAME, app_handle) {
            Ok(pid) => pid,
            Err(err) => {
                app_handle.emit_all("inject_status", "Minecraft not found").unwrap();
                crate::dialogs::show_error("Latite Client", &err);
                // Give the user time to see the message before resetting to idle
                thread::sleep(Duration::from_secs(7));
                app_handle.emit_all("inject_status", "Idle").unwrap();
                return Err(format!("{} [DIALOG_SHOWN]", err));
            }
        }
    };

    // Start animation thread for continuous injecting animation
    let should_animate = Arc::new(AtomicBool::new(true));
    let should_animate_clone = should_animate.clone();
    let app_handle_clone = app_handle.clone();
    
    let animation_start_time = std::time::Instant::now();
    
    let animation_thread = thread::spawn(move || {
        let dots = ["", ".", "..", "..."];
        let mut dot_index = 0;
        
        while should_animate_clone.load(Ordering::Relaxed) {
            let status = format!("Injecting{}", dots[dot_index]);
            app_handle_clone.emit_all("inject_status", &status).unwrap();
            dot_index = (dot_index + 1) % dots.len();
            
            thread::sleep(Duration::from_millis(300));
        }
    });

    let result = injector::inject_dll(pid, &dll_path);
    if result.is_ok() {
        // Ensure injecting animation displays for at least 5 seconds
        let elapsed = animation_start_time.elapsed();
        let min_duration = Duration::from_secs(5);
        if elapsed < min_duration {
            let remaining = min_duration - elapsed;
            thread::sleep(remaining);
        }
        
        // Stop injecting animation
        should_animate.store(false, Ordering::Relaxed);
        let _ = animation_thread.join();
        
        // Switch to finalizing status with its own animation
        // Monitor the process to ensure it doesn't crash after injection
        // Minecraft Bedrock can take several seconds to fully load
        // If the DLL is incompatible, it will crash within this window
        let crashed = monitor_process_after_injection(MC_PROCESS_NAME, app_handle);
        
        if crashed {
            app_handle.emit_all("inject_status", "Failed to inject").unwrap();
            crate::dialogs::show_error("Latite Client", "Minecraft process closed after DLL injection. The DLL may be incompatible with your Minecraft version.");
            // Give the user time to see the message before resetting to idle
            thread::sleep(Duration::from_secs(7));
            app_handle.emit_all("inject_status", "Idle").unwrap();
            return Err("Minecraft process closed after DLL injection. The DLL may be incompatible with your Minecraft version. [DIALOG_SHOWN]".to_string());
        }
        
        app_handle.emit_all("inject_status", "Successfully injected").unwrap();
        // Keep successful status visible for 7 seconds before returning to idle
        thread::sleep(Duration::from_secs(7));
        app_handle.emit_all("inject_status", "Idle").unwrap();
    } else {
        // Stop animation on injection error
        should_animate.store(false, Ordering::Relaxed);
        let _ = animation_thread.join();
    }
    result
}

fn monitor_process_after_injection(process_name: &str, app_handle: &AppHandle) -> bool {
    // Monitor for up to 25 seconds to catch delayed crashes on older PCs
    // Show animated "Finalizing" status during monitoring
    
    // Start animation thread for continuous finalizing animation
    let should_animate = Arc::new(AtomicBool::new(true));
    let should_animate_clone = should_animate.clone();
    let app_handle_clone = app_handle.clone();
    
    let animation_thread = thread::spawn(move || {
        let dots = ["", ".", "..", "..."];
        let mut dot_index = 0;
        
        while should_animate_clone.load(Ordering::Relaxed) {
            let status = format!("Finalizing{}", dots[dot_index]);
            app_handle_clone.emit_all("inject_status", &status).unwrap();
            dot_index = (dot_index + 1) % dots.len();
            
            thread::sleep(Duration::from_millis(300));
        }
    });
    
    // Monitor the process
    const MONITOR_DURATION_MS: u64 = 20000;
    const CHECK_INTERVAL_MS: u64 = 500;
    let mut elapsed = 0u64;
    
    while elapsed < MONITOR_DURATION_MS {
        thread::sleep(Duration::from_millis(CHECK_INTERVAL_MS));
        elapsed += CHECK_INTERVAL_MS;
        
        match injector::find_process_id(process_name) {
            Ok(Some(_)) => {
                // Process still alive, continue monitoring
                println!("Process alive at {}ms", elapsed);
            }
            Ok(None) => {
                // Process died - stop animation and return crashed
                println!("Process died after {}ms", elapsed);
                should_animate.store(false, Ordering::Relaxed);
                let _ = animation_thread.join();
                return true;
            }
            Err(e) => {
                // Error checking process, assume failure
                eprintln!("Error monitoring process: {}", e);
                should_animate.store(false, Ordering::Relaxed);
                let _ = animation_thread.join();
                return true;
            }
        }
    }
    
    // Process survived the monitoring period - stop animation and return success
    println!("Process survived {}ms monitoring period", MONITOR_DURATION_MS);
    should_animate.store(false, Ordering::Relaxed);
    let _ = animation_thread.join();
    false
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

fn wait_for_process_with_animation(process_name: &str, _app_handle: &AppHandle) -> Result<u32, String> {
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
