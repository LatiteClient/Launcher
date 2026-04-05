// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod dialogs;
mod inject;
mod launch_request;
mod launcher;
mod options;
mod paths;
mod release;

use app_state::AppState;
use launch_request::InjectRequest;
use tauri::State;
use tauri::Manager;

#[tauri::command]
async fn inject(
    request: InjectRequest,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let close_after_injected = match state.get_bool_option("misc_close_after_injected") {
        Ok(value) => value,
        Err(error) => {
            dialogs::show_error("Latite Client", &error);
            return Err(error);
        }
    };

    let result = launcher::inject(state.inner(), request, &app_handle).await;

    // After injection returns (guard is released), emit Idle status
    // This ensures status accurately reflects the injection state
    let _ = app_handle.emit_all("inject_status", "Idle");

    if let Err(error) = &result {
        let error_msg = error.message();
        
        // Only show dialog if it hasn't already been shown by report_failure
        if !error.dialog_already_shown() {
            // Map full error messages to short status display messages
            let status_msg = match error_msg {
                msg if msg.contains("Failed to download DLL") => "Failed to Download DLL",
                msg if msg.contains("Failed to open Minecraft") => "Cannot open Minecraft",
                msg if msg.contains("Minecraft not found") => "Minecraft not found",
                msg if msg.contains("Failed to create") => "Failed to create file",
                msg if msg.contains("Failed to write") => "Failed to write file",
                msg if msg.contains("Invalid URL") => "Invalid DLL URL",
                msg if msg.contains("does not exist") => "DLL does not exist",
                msg if msg.contains("is not a file") => "Invalid DLL path",
                msg if msg.contains("is not a DLL") => "Invalid DLL file",
                msg if msg.contains("Failed to inject") => "Failed to inject",
                _ => "Injection failed",
            };
            
            // Emit short status message (it will auto-clear to Idle after 6 seconds)
            let _ = app_handle.emit_all("inject_status", status_msg);
            
            // Show the full error message in dialog
            dialogs::show_error("Latite Client", error_msg);
        }
    } else if close_after_injected {
        app_handle.exit(0);
    }

    result.map_err(launcher::LaunchError::into_message)
}

#[tauri::command]
fn update_option(id: &str, value: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.update_bool_option(id, value)
}

#[tauri::command]
fn get_option(id: &str, state: State<'_, AppState>) -> Result<bool, String> {
    state.get_bool_option(id)
}

#[tauri::command]
fn open_folder() -> Result<(), String> {
    let folder_path = crate::paths::get_latite_path()?;
    
    // Use explorer to open the folder
    std::process::Command::new("explorer")
        .arg(folder_path.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| format!("Failed to open folder: {}", e))?;
    
    Ok(())
}

#[tauri::command]
fn minimize_window(app_handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_window("main") {
        window.minimize().map_err(|e| format!("Failed to minimize: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn get_string_option(id: &str, state: State<'_, AppState>) -> Result<String, String> {
    state.get_string_option(id)
}

#[tauri::command]
fn set_string_option(id: &str, value: String, state: State<'_, AppState>) -> Result<(), String> {
    state.set_string_option(id, value)
}

fn main() {
    let app_state = match AppState::new() {
        Ok(state) => state,
        Err(error) => {
            let message = format!("Failed to initialize launcher: {error}");
            dialogs::show_error("Latite Client", &message);
            eprintln!("{message}");
            return;
        }
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            inject,
            update_option,
            get_option,
            open_folder,
            minimize_window,
            get_string_option,
            set_string_option
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
