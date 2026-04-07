use std::path::{Path, PathBuf};

use crate::launch_request::BuildKind;

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

pub fn get_logs_path() -> Result<PathBuf, String> {
    let logs_path = get_launcher_path()?.join("Logs");
    ensure_directory(&logs_path)
}

pub fn get_latite_build_path(build: BuildKind) -> Result<PathBuf, String> {
    let build_path = get_dlls_path()?.join(match build {
        BuildKind::Release => "release",
        BuildKind::Nightly => "nightly",
        BuildKind::Debug => "debug",
    });

    ensure_directory(&build_path)
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
