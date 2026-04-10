// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod dialogs;
mod localization;
mod inject;
mod launch_request;
mod launcher;
mod logging;
mod options;
mod paths;
mod release;
mod ui;

use app_state::AppState;
use launch_request::{BuildKind, InjectRequest};
use tauri::{
    CustomMenuItem, Manager, State, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
};

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "launcher-tray";
const TRAY_SHOW_MENU_ITEM_ID: &str = "tray-show";
const TRAY_EXIT_MENU_ITEM_ID: &str = "tray-exit";

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
    let window = get_main_window(&app_handle)?;

    window
        .minimize()
        .map_err(|error| format!("Failed to minimize window: {error}"))
}

#[tauri::command]
fn close_window(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    close_or_hide_main_window(&app_handle, state.inner())
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

fn get_main_window(app_handle: &tauri::AppHandle) -> Result<tauri::Window, String> {
    app_handle
        .get_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "Main window is unavailable.".to_string())
}

fn close_or_hide_main_window(
    app_handle: &tauri::AppHandle,
    state: &AppState,
) -> Result<(), String> {
    if state.get_bool_option("misc_hide_on_close")? {
        hide_main_window_to_tray(app_handle, state)
    } else {
        app_handle.exit(0);
        Ok(())
    }
}

fn hide_main_window_to_tray(app_handle: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    ensure_tray(app_handle, state)?;
    let window = get_main_window(app_handle)?;
    window
        .hide()
        .map_err(|error| format!("Failed to hide window to tray: {error}"))
}

fn restore_main_window(app_handle: &tauri::AppHandle) -> Result<(), String> {
    let window = get_main_window(app_handle)?;

    window
        .show()
        .map_err(|error| format!("Failed to show launcher window: {error}"))?;
    window
        .unminimize()
        .map_err(|error| format!("Failed to restore launcher window: {error}"))?;
    window
        .set_focus()
        .map_err(|error| format!("Failed to focus launcher window: {error}"))?;

    let state = app_handle.state::<AppState>();

    if state.is_tray_icon_visible() {
        if let Some(tray_handle) = app_handle.tray_handle_by_id(TRAY_ID) {
            tray_handle
                .destroy()
                .map_err(|error| format!("Failed to remove system tray icon: {error}"))?;
        }

        state.set_tray_icon_visible(false);
    }

    Ok(())
}

fn schedule_restore_main_window(app_handle: &tauri::AppHandle) {
    let app_handle = app_handle.clone();

    tauri::async_runtime::spawn_blocking(move || {
        if let Err(error) = restore_main_window(&app_handle) {
            dialogs::show_error(&error);
        }
    });
}

fn ensure_tray(app_handle: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    if state.is_tray_icon_visible() {
        return Ok(());
    }
    let show_label = localization::get_translation("launcher.tray.show.name")
        .unwrap_or_else(|| "Show Launcher".to_string());
    let exit_label = localization::get_translation("launcher.tray.exit.name")
        .unwrap_or_else(|| "Exit".to_string());
    let tooltip = localization::get_translation("launcher.tray.tooltip.name")
        .unwrap_or_else(|| "Latite Client Launcher".to_string());

    let tray_menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new(TRAY_SHOW_MENU_ITEM_ID.to_string(), show_label))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new(TRAY_EXIT_MENU_ITEM_ID.to_string(), exit_label));

    let tray_app_handle = app_handle.clone();

    SystemTray::new()
        .with_id(TRAY_ID)
        .with_tooltip(&tooltip)
        .with_menu(tray_menu)
        .on_event(move |event| {
            handle_system_tray_event(&tray_app_handle, event);
        })
        .build(app_handle)
        .map(|_| {
            state.set_tray_icon_visible(true);
        })
        .map_err(|error| format!("Failed to create system tray icon: {error}"))
}

fn handle_system_tray_event(app_handle: &tauri::AppHandle, event: SystemTrayEvent) {
    match event {
        SystemTrayEvent::MenuItemClick { id, .. } if id == TRAY_SHOW_MENU_ITEM_ID => {
            schedule_restore_main_window(app_handle);
        }
        SystemTrayEvent::MenuItemClick { id, .. } if id == TRAY_EXIT_MENU_ITEM_ID => {
            app_handle.exit(0);
        }
        SystemTrayEvent::LeftClick { .. } | SystemTrayEvent::DoubleClick { .. } => {
            schedule_restore_main_window(app_handle);
        }
        _ => {}
    }
}

fn main() {
    if let Err(error) = logging::init() {
        logging::log_startup_error(&format!("Failed to initialize launcher logging: {error}"));
    }

    let app_state = match AppState::new() {
        Ok(state) => state,
        Err(error) => {
            let message = format!("Failed to initialize launcher: {error}");
            dialogs::show_error(&message);
            crate::log_error!("{message}");
            return;
        }
    };

    tauri::Builder::default()
        .manage(app_state)
        .on_window_event(|event| {
            if event.window().label() != MAIN_WINDOW_LABEL {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event.event() {
                let app_handle = event.window().app_handle();
                let state = app_handle.state::<AppState>();

                match state.get_bool_option("misc_hide_on_close") {
                    Ok(true) => {
                        api.prevent_close();

                        if let Err(error) = hide_main_window_to_tray(&app_handle, state.inner()) {
                            dialogs::show_error(&error);
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        dialogs::show_error(&error);
                    }
                }
            }
        })
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
            close_window,
            open_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
