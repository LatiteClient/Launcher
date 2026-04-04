use std::ffi::CString;

use windows::core::PCSTR;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxA, MB_ICONERROR, MB_OK, MB_TOPMOST};

// TODO: implement actual tauri dialog instead of ghetto ass win32 messagebox
pub fn show_error(title: &str, message: &str) {
    let title = sanitize_text(title);
    let message = sanitize_text(message);

    unsafe {
        MessageBoxA(
            None,
            PCSTR(message.as_ptr().cast()),
            PCSTR(title.as_ptr().cast()),
            MB_ICONERROR | MB_OK | MB_TOPMOST,
        );
    }
}

fn sanitize_text(text: &str) -> CString {
    CString::new(text).unwrap_or_else(|_| {
        let sanitized = text.replace('\0', " ");
        CString::new(sanitized).expect("sanitized dialog text should not contain interior nulls")
    })
}
