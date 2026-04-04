use tauri::api::dialog::{MessageDialogBuilder, MessageDialogKind};

pub fn show_error(msg: &str) {
    MessageDialogBuilder::new("Latite Client", msg)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

pub fn show_info(msg: &str) {
    MessageDialogBuilder::new("Latite Client", msg)
        .kind(MessageDialogKind::Info)
        .show(|_| {});
}
