use std::{
    ffi::{c_char, CStr},
    os::windows::ffi::OsStrExt,
    path::Path,
};

use windows::{
    core::{s, PCSTR, PCWSTR},
    Win32::{
        Foundation::{FreeLibrary, HMODULE},
        System::LibraryLoader::{GetProcAddress, LoadLibraryExW, DONT_RESOLVE_DLL_REFERENCES},
    },
};

const MAX_SUPPORTED_MINECRAFT_VERSIONS: u32 = 512;

type LatiteGetDllVersion = unsafe extern "C" fn() -> *const c_char;
type LatiteGetSupportedMinecraftVersionCount = unsafe extern "C" fn() -> u32;
type LatiteGetSupportedMinecraftVersion = unsafe extern "C" fn(u32) -> *const c_char;
type ProcAddress = unsafe extern "system" fn() -> isize;

#[derive(Debug, Clone)]
pub struct LatiteDllMetadata {
    version: String,
    supported_minecraft_versions: Vec<String>,
}

impl LatiteDllMetadata {
    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn supported_minecraft_versions(&self) -> &[String] {
        &self.supported_minecraft_versions
    }

    pub fn supports_minecraft_version(&self, minecraft_version: &str) -> bool {
        self.supported_minecraft_versions
            .iter()
            .any(|version| supported_version_matches(version, minecraft_version))
    }
}

pub fn read_metadata(dll_path: &Path) -> Result<LatiteDllMetadata, String> {
    unsafe { read_metadata_inner(dll_path) }
}

pub fn versions_equivalent(left: &str, right: &str) -> bool {
    normalize_version(left).eq_ignore_ascii_case(&normalize_version(right))
}

pub fn supported_version_matches(pattern: &str, minecraft_version: &str) -> bool {
    let pattern = normalize_version(pattern);
    let minecraft_version = normalize_version(minecraft_version);

    pattern.len() == minecraft_version.len()
        && pattern
            .bytes()
            .zip(minecraft_version.bytes())
            .all(|(pattern_byte, version_byte)| {
                if matches!(pattern_byte, b'x' | b'X') {
                    version_byte.is_ascii_digit()
                } else {
                    pattern_byte.eq_ignore_ascii_case(&version_byte)
                }
            })
}

unsafe fn read_metadata_inner(dll_path: &Path) -> Result<LatiteDllMetadata, String> {
    let full_dll_path = std::fs::canonicalize(dll_path)
        .map_err(|error| format!("Failed to resolve DLL path {}: {error}", dll_path.display()))?;
    let dll_path_wide: Vec<u16> = full_dll_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let module = LoadLibraryExW(
        PCWSTR(dll_path_wide.as_ptr()),
        None,
        DONT_RESOLVE_DLL_REFERENCES,
    )
    .map_err(|error| {
        format!(
            "Failed to load {} for metadata inspection: {error}",
            full_dll_path.display()
        )
    })?;
    
    let module = LoadedModule::new(module);

    let get_dll_version: LatiteGetDllVersion = std::mem::transmute(required_export(
        module.raw(),
        "LatiteGetDllVersion",
        s!("LatiteGetDllVersion"),
    )?);
    let get_supported_count: LatiteGetSupportedMinecraftVersionCount =
        std::mem::transmute(required_export(
            module.raw(),
            "LatiteGetSupportedMinecraftVersionCount",
            s!("LatiteGetSupportedMinecraftVersionCount"),
        )?);
    let get_supported_version: LatiteGetSupportedMinecraftVersion =
        std::mem::transmute(required_export(
            module.raw(),
            "LatiteGetSupportedMinecraftVersion",
            s!("LatiteGetSupportedMinecraftVersion"),
        )?);

    let version = read_exported_string(get_dll_version(), "LatiteGetDllVersion")?;
    let supported_count = get_supported_count();

    if supported_count > MAX_SUPPORTED_MINECRAFT_VERSIONS {
        return Err(format!(
            "LatiteGetSupportedMinecraftVersionCount returned {supported_count}, which is larger than the supported limit of {MAX_SUPPORTED_MINECRAFT_VERSIONS}."
        ));
    }

    let mut supported_minecraft_versions = Vec::with_capacity(supported_count as usize);

    for index in 0..supported_count {
        let supported_version = read_exported_string(
            get_supported_version(index),
            "LatiteGetSupportedMinecraftVersion",
        )
        .map_err(|error| format!("{error} at index {index}"))?;

        supported_minecraft_versions.push(supported_version);
    }

    Ok(LatiteDllMetadata {
        version,
        supported_minecraft_versions,
    })
}

unsafe fn required_export(
    module: HMODULE,
    export_name: &str,
    proc_name: PCSTR,
) -> Result<ProcAddress, String> {
    GetProcAddress(module, proc_name)
        .ok_or_else(|| format!("DLL does not export required function {export_name}."))
}

unsafe fn read_exported_string(
    pointer: *const c_char,
    export_name: &str,
) -> Result<String, String> {
    if pointer.is_null() {
        return Err(format!("{export_name} returned a null string pointer."));
    }

    let value = CStr::from_ptr(pointer).to_string_lossy().into_owned();

    if value.trim().is_empty() {
        return Err(format!("{export_name} returned an empty string."));
    }

    Ok(value)
}

fn normalize_version(version: &str) -> String {
    version
        .trim()
        .trim_start_matches(['v', 'V'])
        .trim()
        .to_string()
}

struct LoadedModule(HMODULE);

impl LoadedModule {
    fn new(module: HMODULE) -> Self {
        Self(module)
    }

    fn raw(&self) -> HMODULE {
        self.0
    }
}

impl Drop for LoadedModule {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = FreeLibrary(self.0) {
                crate::log_error!("Failed to unload Latite DLL metadata module: {error}");
            }
        }
    }
}
