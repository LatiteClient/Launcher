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
mod ui;

use app_state::AppState;
use launch_request::{BuildKind, InjectRequest};
use tauri::{Manager, State};

#[tauri::command]
async fn inject(
    request: InjectRequest,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let close_after_injected = match state.get_bool_option("misc_close_after_injected") {
        Ok(value) => value,
        Err(error) => {
            dialogs::show_error(&error);
            return Err(error);
        }
    };

    let result = launcher::inject(state.inner(), request, &app_handle).await;

    if let Err(error) = &result {
        if !error.dialog_already_shown() {
            dialogs::show_error(error.message());
        }
    } else if close_after_injected {
        app_handle.exit(0);
    }

    result.map_err(launcher::LaunchError::into_message)
}

#[tauri::command]
async fn check_for_updates(app_handle: tauri::AppHandle) -> Result<(), String> {
    let current_version = app_handle.package_info().version.to_string();
    launcher::check_for_updates(&current_version, &app_handle).await
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
fn get_string_option(id: &str, state: State<'_, AppState>) -> Result<String, String> {
    state.get_string_option(id)
}

#[tauri::command]
fn update_string_option(id: &str, value: String, state: State<'_, AppState>) -> Result<(), String> {
    state.update_string_option(id, value)
}

#[tauri::command]
fn get_latite_build(state: State<'_, AppState>) -> Result<BuildKind, String> {
    state.get_latite_build()
}

#[tauri::command]
fn update_latite_build(build: BuildKind, state: State<'_, AppState>) -> Result<(), String> {
    state.update_latite_build(build)
}

#[tauri::command]
fn minimize_window(app_handle: tauri::AppHandle) -> Result<(), String> {
    let window = app_handle
        .get_window("main")
        .ok_or_else(|| "Main window is unavailable.".to_string())?;

    window
        .minimize()
        .map_err(|error| format!("Failed to minimize window: {error}"))
}

#[tauri::command]
fn open_folder() -> Result<(), String> {
    let local_appdata =
        std::env::var("LOCALAPPDATA").map_err(|e| format!("Failed to get LOCALAPPDATA: {}", e))?;

    let folder_path = std::path::Path::new(&local_appdata).join("Latite");

    // Create the folder if it doesn't exist
    let _ = std::fs::create_dir_all(&folder_path);

    // Use explorer to open the folder
    std::process::Command::new("explorer")
        .arg(folder_path.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| format!("Failed to open folder: {}", e))?;

    Ok(())
}

fn main() {
    let app_state = match AppState::new() {
        Ok(state) => state,
        Err(error) => {
            let message = format!("Failed to initialize launcher: {error}");
            dialogs::show_error(&message);
            eprintln!("{message}");
            return;
        }
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            inject,
            check_for_updates,
            update_option,
            get_option,
            get_string_option,
            update_string_option,
            get_latite_build,
            update_latite_build,
            minimize_window,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
