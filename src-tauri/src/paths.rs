use std::path::{Path, PathBuf};

pub fn get_launcher_path() -> Result<PathBuf, String> {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .map_err(|error| format!("LOCALAPPDATA is unavailable: {error}"))?;
    let launcher_path = PathBuf::from(local_app_data)
        .join("Latite")
        .join("Launcher");

    ensure_directory(&launcher_path)
}

pub fn get_dlls_path() -> Result<PathBuf, String> {
    let dlls_path = get_launcher_path()?.join("DLLs");
    ensure_directory(&dlls_path)
}

pub fn get_dll_path() -> Result<PathBuf, String> {
    Ok(get_dlls_path()?.join("Latite.dll"))
}

pub fn get_options_path() -> Result<PathBuf, String> {
    Ok(get_launcher_path()?.join("options.json"))
}

fn ensure_directory(path: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(path)
        .map_err(|error| format!("Failed to create {}: {error}", path.display()))?;
    std::fs::canonicalize(path)
        .map_err(|error| format!("Failed to canonicalize {}: {error}", path.display()))
}
