use tauri::api::dialog::{MessageDialogBuilder, MessageDialogKind};

#[allow(dead_code)]
pub fn show_info(msg: &str) {
    MessageDialogBuilder::new("Latite Client", msg)
        .kind(MessageDialogKind::Info)
        .show(|_| {});
}

#[allow(dead_code)]
pub fn show_error(msg: &str) {
    MessageDialogBuilder::new("Latite Client", msg)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}
