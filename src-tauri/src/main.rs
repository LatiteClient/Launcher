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

    let result = launcher::inject(state.inner(), request).await;

    if let Err(error) = &result {
        dialogs::show_error("Latite Client", error);
    } else if close_after_injected {
        app_handle.exit(0);
    }

    result
}

#[tauri::command]
fn update_option(id: &str, value: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.update_bool_option(id, value)
}

#[tauri::command]
fn get_option(id: &str, state: State<'_, AppState>) -> Result<bool, String> {
    state.get_bool_option(id)
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
        .invoke_handler(tauri::generate_handler![inject, update_option, get_option])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
