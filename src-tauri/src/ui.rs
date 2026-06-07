use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;

pub const DIALOG_EVENT: &str = "ui_dialog";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiMessage {
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

impl UiMessage {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            args: Vec::new(),
        }
    }

    pub fn with_arg(mut self, value: impl ToString) -> Self {
        self.args.push(value.to_string());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum UiDialogLevel {
    Info,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDialog {
    pub level: UiDialogLevel,
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
}

#[allow(dead_code)]
impl UiDialog {
    pub fn info(key: impl Into<String>) -> Self {
        Self {
            level: UiDialogLevel::Info,
            key: key.into(),
            args: Vec::new(),
        }
    }

    pub fn error(key: impl Into<String>) -> Self {
        Self {
            level: UiDialogLevel::Error,
            key: key.into(),
            args: Vec::new(),
        }
    }

    pub fn with_arg(mut self, value: impl ToString) -> Self {
        self.args.push(value.to_string());
        self
    }
}

pub fn emit_dialog(app_handle: &AppHandle, dialog: &UiDialog) {
    if !is_ui_ready(app_handle) {
        show_native_fallback(app_handle, dialog);
        return;
    }

    if let Err(error) = app_handle.emit_all(DIALOG_EVENT, dialog) {
        crate::log_error!(
            "Failed to emit launcher dialog '{}' at level {:?}: {error}",
            dialog.key,
            dialog.level
        );
        show_native_fallback(app_handle, dialog);
    }
}

fn is_ui_ready(app_handle: &AppHandle) -> bool {
    app_handle
        .try_state::<AppState>()
        .map(|state| state.is_ui_ready())
        .unwrap_or(false)
}

fn show_native_fallback(app_handle: &AppHandle, dialog: &UiDialog) {
    let message = translate_dialog_message(dialog);

    match dialog.level {
        UiDialogLevel::Info => crate::dialogs::show_info(&message),
        UiDialogLevel::Error => {
            if let Err(error) = crate::restore_main_window(app_handle) {
                crate::log_error!("Failed to focus launcher window for error dialog: {error}");
            }

            crate::dialogs::show_error(&message);
        }
    }
}

fn translate_dialog_message(dialog: &UiDialog) -> String {
    let template =
        crate::localization::get_translation(&dialog.key).unwrap_or_else(|| dialog.key.clone());

    interpolate_translation(&template, &dialog.args)
}

fn interpolate_translation(template: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut remaining = template;
    let mut arg_index = 0;

    while let Some(position) = remaining.find("{}") {
        result.push_str(&remaining[..position]);

        if let Some(arg) = args.get(arg_index) {
            result.push_str(arg);
            arg_index += 1;
        } else {
            result.push_str("{}");
        }

        remaining = &remaining[position + 2..];
    }

    result.push_str(remaining);
    result
}
