use std::collections::BTreeMap;

use serde::Serialize;
use tauri::{AppHandle, Manager};

pub const DIALOG_EVENT: &str = "ui_dialog";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiMessage {
    pub key: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
}

impl UiMessage {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            vars: BTreeMap::new(),
        }
    }

    pub fn with_var(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.vars.insert(key.into(), value.to_string());
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UiDialogLevel {
    Info,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDialog {
    pub level: UiDialogLevel,
    pub key: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
}

impl UiDialog {
    pub fn info(key: impl Into<String>) -> Self {
        Self {
            level: UiDialogLevel::Info,
            key: key.into(),
            vars: BTreeMap::new(),
        }
    }

    pub fn error(key: impl Into<String>) -> Self {
        Self {
            level: UiDialogLevel::Error,
            key: key.into(),
            vars: BTreeMap::new(),
        }
    }

    pub fn with_var(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.vars.insert(key.into(), value.to_string());
        self
    }
}

pub fn emit_dialog(app_handle: &AppHandle, dialog: &UiDialog) {
    if let Err(error) = app_handle.emit_all(DIALOG_EVENT, dialog) {
        crate::log_error!(
            "Failed to emit launcher dialog '{}' at level {:?}: {error}",
            dialog.key,
            dialog.level
        );
    }
}
