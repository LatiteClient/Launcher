use std::path::{Path, PathBuf};

use reqwest::{header::USER_AGENT, Client};
use serde::Deserialize;

use crate::launch_request::BuildKind;

// TODO: Move to main Latite repo instead of using Latite-Releases
pub const RELEASE_REPO: &str = "Imrglop/Latite-Releases";
pub const LAUNCHER_REPO: &str = "LatiteClient/Launcher";

const RELEASE_DLL_DOWNLOAD_URL: &str =
    "https://github.com/Imrglop/Latite-Releases/releases/latest/download/Latite.dll";
const NIGHTLY_DLL_DOWNLOAD_URL: &str =
    "https://github.com/LatiteClient/Latite/releases/download/nightly/LatiteNightly.dll";
const DEBUG_DLL_DOWNLOAD_URL: &str =
    "https://github.com/LatiteClient/Latite/releases/download/debug/LatiteDebug.dll";
const DEBUG_PDB_DOWNLOAD_URL: &str =
    "https://github.com/LatiteClient/Latite/releases/download/debug/LatiteDebug.pdb";
const REQUEST_USER_AGENT: &str = "Latite Launcher/0.1";

#[derive(Clone, Copy)]
struct AssetDownload {
    file_name: &'static str,
    url: &'static str,
}

const RELEASE_ASSETS: [AssetDownload; 1] = [AssetDownload {
    file_name: "Latite.dll",
    url: RELEASE_DLL_DOWNLOAD_URL,
}];
const NIGHTLY_ASSETS: [AssetDownload; 1] = [AssetDownload {
    file_name: "LatiteNightly.dll",
    url: NIGHTLY_DLL_DOWNLOAD_URL,
}];
const DEBUG_ASSETS: [AssetDownload; 2] = [
    AssetDownload {
        file_name: "LatiteDebug.dll",
        url: DEBUG_DLL_DOWNLOAD_URL,
    },
    AssetDownload {
        file_name: "LatiteDebug.pdb",
        url: DEBUG_PDB_DOWNLOAD_URL,
    },
];

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

pub async fn fetch_latest_release_name(repo: &str) -> Result<String, String> {
    let response = reqwest::Client::new()
        .get(&format!(
            "https://api.github.com/repos/{}/releases/latest",
            repo
        ))
        .header(USER_AGENT, REQUEST_USER_AGENT)
        .send()
        .await
        .map_err(|error| format!("Failed to fetch latest release information: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Failed to fetch latest release information: {status}"
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read latest release response: {error}"))?;
    let release: GitHubRelease = serde_json::from_str(&body)
        .map_err(|error| format!("Failed to parse latest release response: {error}"))?;

    Ok(release.tag_name)
}

pub fn build_display_name(build: BuildKind) -> &'static str {
    match build {
        BuildKind::Release => "Latite Release",
        BuildKind::Nightly => "Latite Nightly",
        BuildKind::Debug => "Latite Debug",
    }
}

pub fn latite_dll_file_name(build: BuildKind) -> &'static str {
    match build {
        BuildKind::Release => "Latite.dll",
        BuildKind::Nightly => "LatiteNightly.dll",
        BuildKind::Debug => "LatiteDebug.dll",
    }
}

pub fn has_required_assets(build: BuildKind, directory: &Path) -> bool {
    build_assets(build)
        .iter()
        .all(|asset| directory.join(asset.file_name).is_file())
}

pub async fn download_build(build: BuildKind, directory: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("Failed to create {}: {error}", directory.display()))?;

    let client = Client::new();
    let mut downloads = Vec::with_capacity(build_assets(build).len());

    for asset in build_assets(build) {
        let destination = directory.join(asset.file_name);
        let temporary_destination = temporary_download_path(&destination);

        if temporary_destination.exists() {
            let _ = std::fs::remove_file(&temporary_destination);
        }

        if let Err(error) = download_asset(&client, asset.url, &temporary_destination).await {
            let _ = std::fs::remove_file(&temporary_destination);
            cleanup_temporary_files(&downloads);
            return Err(error);
        }

        downloads.push((temporary_destination, destination));
    }

    for (temporary_destination, destination) in &downloads {
        if let Err(error) = replace_downloaded_file(temporary_destination, destination) {
            cleanup_temporary_files(&downloads);
            return Err(error);
        }
    }

    if !has_required_assets(build, directory) {
        return Err(format!(
            "{} files are missing after download.",
            build_display_name(build)
        ));
    }

    crate::log_info!(
        "{} files downloaded successfully to {}.",
        build_display_name(build),
        directory.display()
    );
    Ok(directory.join(latite_dll_file_name(build)))
}

fn build_assets(build: BuildKind) -> &'static [AssetDownload] {
    match build {
        BuildKind::Release => &RELEASE_ASSETS,
        BuildKind::Nightly => &NIGHTLY_ASSETS,
        BuildKind::Debug => &DEBUG_ASSETS,
    }
}

fn temporary_download_path(destination: &Path) -> PathBuf {
    let mut file_name = destination
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "download".into());
    file_name.push(".download");
    destination.with_file_name(file_name)
}

fn cleanup_temporary_files(downloads: &[(PathBuf, PathBuf)]) {
    for (temporary_destination, _) in downloads {
        let _ = std::fs::remove_file(temporary_destination);
    }
}

fn replace_downloaded_file(temporary_destination: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        std::fs::remove_file(destination).map_err(|error| {
            format!(
                "Failed to replace {} with the downloaded file: {error}",
                destination.display()
            )
        })?;
    }

    std::fs::rename(temporary_destination, destination).map_err(|error| {
        format!(
            "Failed to move downloaded file into {}: {error}",
            destination.display()
        )
    })
}

async fn download_asset(client: &Client, url: &str, destination: &Path) -> Result<(), String> {
    let response = client
        .get(url)
        .header(USER_AGENT, REQUEST_USER_AGENT)
        .send()
        .await
        .map_err(|error| format!("Failed to download {}: {error}", destination.display()))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Failed to download {}: {status}",
            destination.display()
        ));
    }

    let bytes = response.bytes().await.map_err(|error| {
        format!(
            "Failed to read downloaded bytes for {}: {error}",
            destination.display()
        )
    })?;

    std::fs::write(destination, &bytes)
        .map_err(|error| format!("Failed to write {}: {error}", destination.display()))?;

    Ok(())
}
