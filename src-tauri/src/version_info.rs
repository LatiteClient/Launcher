use std::{
    ffi::{c_void, OsStr},
    fmt,
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use windows::{
    core::PCWSTR,
    Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    },
};

#[derive(Debug)]
pub enum FileVersionError {
    MissingVersionInfo { path: PathBuf },
    Other(String),
}

impl FileVersionError {
    pub fn is_missing_version_info(&self) -> bool {
        matches!(self, Self::MissingVersionInfo { .. })
    }
}

impl fmt::Display for FileVersionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVersionInfo { path } => write!(
                formatter,
                "No file version information is available for {}.",
                path.display()
            ),
            Self::Other(message) => formatter.write_str(message),
        }
    }
}

pub fn get_file_version(path: &Path) -> Result<String, FileVersionError> {
    unsafe { get_file_version_inner(path) }
}

pub fn match_supported_package_version<'a>(
    package_major: u16,
    package_minor: u16,
    package_build: u16,
    supported_versions: &'a [String],
) -> Option<&'a str> {
    let package_build = package_build.to_string();

    supported_versions
        .iter()
        .filter_map(|version| {
            let (major, minor, patch) = parse_three_part_version(version)?;

            if major != package_major
                || minor != package_minor
                || !version_pattern_matches_prefix(patch, &package_build)
            {
                return None;
            }

            let concrete_characters = patch
                .bytes()
                .filter(|character| !matches!(character, b'x' | b'X'))
                .count();
            Some(((patch.len(), concrete_characters), version.as_str()))
        })
        .max_by_key(|(specificity, _)| *specificity)
        .map(|(_, version)| version)
}

unsafe fn get_file_version_inner(path: &Path) -> Result<String, FileVersionError> {
    let path_wide = wide_null(path.as_os_str());
    let version_info_size = GetFileVersionInfoSizeW(PCWSTR(path_wide.as_ptr()), None);

    if version_info_size == 0 {
        return Err(FileVersionError::MissingVersionInfo {
            path: path.to_path_buf(),
        });
    }

    let mut data = vec![0u8; version_info_size as usize];
    GetFileVersionInfoW(
        PCWSTR(path_wide.as_ptr()),
        None,
        version_info_size,
        data.as_mut_ptr().cast(),
    )
    .map_err(|error| {
        FileVersionError::Other(format!(
            "Failed to read file version information for {}: {error}",
            path.display()
        ))
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
        return Err(FileVersionError::Other(format!(
            "Failed to query fixed file version information for {}.",
            path.display()
        )));
    }

    if file_info.is_null() || len < size_of::<VS_FIXEDFILEINFO>() as u32 {
        return Err(FileVersionError::Other(format!(
            "Fixed file version information for {} is invalid.",
            path.display()
        )));
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

#[cfg(test)]
mod tests {
    use super::match_supported_package_version;

    #[test]
    fn package_build_matches_supported_patch_prefix() {
        let supported_versions = vec![
            "1.21.12".to_string(),
            "1.26.21".to_string(),
            "1.26.20".to_string(),
        ];

        assert_eq!(
            match_supported_package_version(1, 21, 12004, &supported_versions),
            Some("1.21.12")
        );
        assert_eq!(
            match_supported_package_version(1, 26, 2101, &supported_versions),
            Some("1.26.21")
        );
    }

    #[test]
    fn package_build_prefers_the_most_specific_supported_patch() {
        let supported_versions = vec!["1.21.1".to_string(), "1.21.12".to_string()];

        assert_eq!(
            match_supported_package_version(1, 21, 12004, &supported_versions),
            Some("1.21.12")
        );
    }

    #[test]
    fn package_build_rejects_non_matching_major_minor_or_patch() {
        let supported_versions = vec!["1.21.12".to_string()];

        assert_eq!(
            match_supported_package_version(1, 20, 12004, &supported_versions),
            None
        );
        assert_eq!(
            match_supported_package_version(1, 21, 13004, &supported_versions),
            None
        );
    }

}

fn parse_three_part_version(version: &str) -> Option<(u16, u16, &str)> {
    let normalized = version.trim().trim_start_matches(['v', 'V']).trim();
    let mut parts = normalized.split('.');

    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?;

    if parts.next().is_some()
        || patch.is_empty()
        || !patch
            .bytes()
            .all(|character| character.is_ascii_digit() || matches!(character, b'x' | b'X'))
    {
        return None;
    }

    Some((major, minor, patch))
}

fn version_pattern_matches_prefix(pattern: &str, version: &str) -> bool {
    pattern.len() <= version.len()
        && pattern
            .bytes()
            .zip(version.bytes())
            .all(|(pattern_byte, version_byte)| {
                if matches!(pattern_byte, b'x' | b'X') {
                    version_byte.is_ascii_digit()
                } else {
                    pattern_byte == version_byte
                }
            })
}
