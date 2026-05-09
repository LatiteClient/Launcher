use std::{
    ffi::{c_void, OsStr},
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::Path,
};

use windows::{
    core::PCWSTR,
    Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    },
};

pub fn get_file_version(path: &Path) -> Result<String, String> {
    unsafe { get_file_version_inner(path) }
}

unsafe fn get_file_version_inner(path: &Path) -> Result<String, String> {
    let path_wide = wide_null(path.as_os_str());
    let version_info_size = GetFileVersionInfoSizeW(PCWSTR(path_wide.as_ptr()), None);

    if version_info_size == 0 {
        return Err(format!(
            "No file version information is available for {}.",
            path.display()
        ));
    }

    let mut data = vec![0u8; version_info_size as usize];
    GetFileVersionInfoW(
        PCWSTR(path_wide.as_ptr()),
        None,
        version_info_size,
        data.as_mut_ptr().cast(),
    )
    .map_err(|error| {
        format!(
            "Failed to read file version information for {}: {error}",
            path.display()
        )
    })?;

    let root = wide_null(OsStr::new("\\"));
    let mut file_info: *mut c_void = std::ptr::null_mut();
    let mut len: u32 = 0;

    if !VerQueryValueW(
        data.as_ptr().cast(),
        PCWSTR(root.as_ptr()),
        &mut file_info,
        &mut len,
    )
    .as_bool()
    {
        return Err(format!(
            "Failed to query fixed file version information for {}.",
            path.display()
        ));
    }

    if file_info.is_null() || len < size_of::<VS_FIXEDFILEINFO>() as u32 {
        return Err(format!(
            "Fixed file version information for {} is invalid.",
            path.display()
        ));
    }

    let file_info = &*(file_info as *const VS_FIXEDFILEINFO);
    let major = hiword(file_info.dwFileVersionMS);
    let minor = loword(file_info.dwFileVersionMS);
    let build = hiword(file_info.dwFileVersionLS);

    Ok(format!("{major}.{minor}.{build}"))
}

fn wide_null(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn hiword(value: u32) -> u16 {
    (value >> 16) as u16
}

fn loword(value: u32) -> u16 {
    value as u16
}
