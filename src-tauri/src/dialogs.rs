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
    let message = append_error_report_message(msg);

    MessageDialogBuilder::new(&title, &message)
        .kind(MessageDialogKind::Error)
        .show(|_| {});
}

fn append_error_report_message(message: &str) -> String {
    let prefix = crate::localization::get_translation("launcher.error.reportBug.name")
        .unwrap_or_else(|| {
            "If you think this is a bug, you should report it in the Latite Client Discord in #report-bugs ".to_string()
        });
    let note = crate::localization::get_translation("launcher.error.reportBugPinned.name")
        .unwrap_or_else(|| "(make sure to read the pinned post!)".to_string());

    format!("{message}\n\n{prefix}{note}")
}
