use tauri::api::dialog::{MessageDialogBuilder, MessageDialogKind};

#[allow(dead_code)]
pub fn show_info(msg: &str) {
    let title = crate::localization::get_translation("launcher.dialog.title.name")
        .unwrap_or_else(|| "Latite Client".to_string());

    MessageDialogBuilder::new(&title, msg)
        .kind(MessageDialogKind::Info)
        .show(|_| {});
}

#[allow(dead_code)]
pub fn show_error(msg: &str) {
    let title = crate::localization::get_translation("launcher.dialog.title.name")
        .unwrap_or_else(|| "Latite Client".to_string());

    MessageDialogBuilder::new(&title, msg)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}
