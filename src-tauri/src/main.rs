// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_state;
mod crash_dumps;
mod dialogs;
mod inject;
mod latite_dll;
mod launch_request;
mod launcher;
mod localization;
mod logging;
mod options;
mod paths;
mod release;
mod single_instance;
mod ui;
mod version_info;
mod minecraft;

use app_state::AppState;
use launch_request::{BuildKind, InjectRequest};
use tauri::{
    CustomMenuItem, Manager, State, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
};
use ui::UiDialog;

const MAIN_WINDOW_LABEL: &str = "main";
const TRAY_ID: &str = "launcher-tray";
const TRAY_SHOW_MENU_ITEM_ID: &str = "tray-show";
const TRAY_EXIT_MENU_ITEM_ID: &str = "tray-exit";
const PREVENT_MULTIPLE_INSTANCES_OPTION_ID: &str = "prevent_multiple_instances";
const DUPLICATE_INSTANCE_EVENT: &str = "duplicate_instance_attempted";

#[tauri::command]
async fn inject(
    request: InjectRequest,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let close_after_injected = match state.get_bool_option("misc_close_after_injected") {
        Ok(value) => value,
        Err(error) => {
            ui::emit_dialog(
                &app_handle,
                &UiDialog::error("launcher.error.generic.name").with_arg(&error),
            );
            return Err(error);
        }
    };

    let result = launcher::inject(state.inner(), request, &app_handle).await;

    if let Err(error) = &result {
        if !error.dialog_already_shown() {
            ui::emit_dialog(
                &app_handle,
                &UiDialog::error("launcher.error.generic.name").with_arg(error.message()),
            );
        }
    } else if close_after_injected {
        app_handle.exit(0);
    }

    result.map_err(launcher::LaunchError::into_message)
}

#[tauri::command]
fn update_option(
    id: &str,
    value: bool,
    state: State<'_, AppState>,
    instance_manager: State<'_, single_instance::InstanceManager>,
) -> Result<(), String> {
    let previous_instance_guard_enabled = if id == PREVENT_MULTIPLE_INSTANCES_OPTION_ID {
        Some(instance_manager.is_enabled()?)
    } else {
        None
    };

    if id == PREVENT_MULTIPLE_INSTANCES_OPTION_ID && value {
        instance_manager.set_enabled(true)?;
    }

    if let Err(error) = state.update_bool_option(id, value) {
        if let Some(previous_enabled) = previous_instance_guard_enabled {
            let _ = instance_manager.set_enabled(previous_enabled);
        }

        return Err(error);
    }

    if id == PREVENT_MULTIPLE_INSTANCES_OPTION_ID && !value {
        instance_manager.set_enabled(false)?;
    }

    Ok(())
}

#[tauri::command]
fn get_option(id: &str, state: State<'_, AppState>) -> Result<bool, String> {
    state.get_bool_option(id)
}

#[tauri::command]
fn set_ui_ready(state: State<'_, AppState>) {
    state.set_ui_ready();
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
fn focus_main_window(app_handle: tauri::AppHandle) -> Result<(), String> {
    restore_main_window(&app_handle)
}

#[tauri::command]
fn close_window(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    close_or_hide_main_window(&app_handle, state.inner())
}

#[tauri::command]
fn open_folder() -> Result<(), String> {
    let folder_path = paths::get_latite_path()?;

    // Use explorer to open the folder
    std::process::Command::new("explorer")
        .arg(folder_path.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| format!("Failed to open folder: {}", e))?;

    Ok(())
}

#[tauri::command]
fn get_launcher_version(app_handle: tauri::AppHandle) -> String {
    app_handle.package_info().version.to_string()
}

#[tauri::command]
fn log_updater_event(level: &str, message: &str) {
    match level {
        "error" => crate::log_error!("Updater: {message}"),
        _ => crate::log_info!("Updater: {message}"),
    }
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

pub(crate) fn restore_main_window(app_handle: &tauri::AppHandle) -> Result<(), String> {
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
            ui::emit_dialog(
                &app_handle,
                &UiDialog::error("launcher.error.generic.name").with_arg(&error),
            );
        }
    });
}

fn show_duplicate_instance_fallback_dialog() {
    let message = localization::get_translation("launcher.instanceAlreadyOpen.message.name")
        .unwrap_or_else(|| {
            "A second launcher window was prevented from opening because Latite Client Launcher is already running. Use this window to launch Latite Client.\n\nYou can disable this behavior in Settings.".to_string()
        });

    dialogs::show_info(&message);
}

fn ensure_tray(app_handle: &tauri::AppHandle, state: &AppState) -> Result<(), String> {
    if state.is_tray_icon_visible() {
        return Ok(());
    }

    let translate_tray_text = |key: &str, fallback: &str| {
        localization::get_translation(key).unwrap_or_else(|| fallback.to_string())
    };

    let show_label = translate_tray_text("launcher.tray.show.name", "Show Launcher");
    let exit_label = translate_tray_text("launcher.tray.exit.name", "Exit");
    let tooltip = translate_tray_text("launcher.tray.tooltip.name", "Latite Client Launcher");

    let tray_menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new(
            TRAY_SHOW_MENU_ITEM_ID.to_string(),
            show_label,
        ))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new(
            TRAY_EXIT_MENU_ITEM_ID.to_string(),
            exit_label,
        ));

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

    match crash_dumps::delete_old_crash_dumps() {
        Ok(deleted_count) if deleted_count > 0 => {
            crate::log_info!("Deleted {deleted_count} crash dump(s) older than seven days.");
        }
        Ok(_) => {}
        Err(error) => {
            crate::log_error!("Failed to clean old crash dumps: {error}");
        }
    }

    let options_store = match options::OptionsStore::load() {
        Ok(store) => store,
        Err(error) => {
            let message = format!("Failed to initialize launcher: {error}");
            dialogs::show_error(&message);
            crate::log_error!("{message}");
            return;
        }
    };

    let instance_guard = match options_store.get_bool(PREVENT_MULTIPLE_INSTANCES_OPTION_ID) {
        Ok(true) => match single_instance::try_acquire() {
            Ok(Some(guard)) => Some(guard),
            Ok(None) => {
                if let Err(error) = single_instance::signal_duplicate_instance_attempt() {
                    crate::log_error!("Failed to notify existing launcher instance: {error}");
                }

                return;
            }
            Err(error) => {
                let message = format!("Failed to initialize launcher instance guard: {error}");
                dialogs::show_error(&message);
                crate::log_error!("{message}");
                return;
            }
        },
        Ok(false) => None,
        Err(error) => {
            let message = format!("Failed to read launcher instance setting: {error}");
            dialogs::show_error(&message);
            crate::log_error!("{message}");
            return;
        }
    };

    let instance_manager = single_instance::InstanceManager::new(instance_guard);
    let app_state = AppState::new(options_store);

    tauri::Builder::default()
        .manage(app_state)
        .manage(instance_manager)
        .setup(|app| {
            let app_handle = app.handle();

            if let Err(error) = single_instance::start_duplicate_attempt_monitor(move || {
                schedule_restore_main_window(&app_handle);

                let state = app_handle.state::<AppState>();
                if !state.is_ui_ready() {
                    show_duplicate_instance_fallback_dialog();
                    return;
                }

                if let Err(error) = app_handle.emit_all(DUPLICATE_INSTANCE_EVENT, ()) {
                    crate::log_error!("Failed to emit duplicate instance event: {error}");
                    show_duplicate_instance_fallback_dialog();
                }
            }) {
                crate::log_error!("Failed to monitor duplicate launcher attempts: {error}");
            }

            Ok(())
        })
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
                            ui::emit_dialog(
                                &app_handle,
                                &UiDialog::error("launcher.error.generic.name").with_arg(&error),
                            );
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        ui::emit_dialog(
                            &app_handle,
                            &UiDialog::error("launcher.error.generic.name").with_arg(&error),
                        );
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            inject,
            update_option,
            get_option,
            set_ui_ready,
            get_string_option,
            update_string_option,
            get_latite_build,
            update_latite_build,
            minimize_window,
            focus_main_window,
            close_window,
            open_folder,
            get_launcher_version,
            log_updater_event
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
