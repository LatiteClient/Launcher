use std::path::Path;

use reqwest::header::USER_AGENT;
use serde::Deserialize;

// TODO: Move to main Latite repo instead of using Latite-Releases
const RELEASE_API_URL: &str =
    "https://api.github.com/repos/Imrglop/Latite-Releases/releases/latest";
const DLL_DOWNLOAD_URL: &str =
    "https://github.com/Imrglop/Latite-Releases/releases/latest/download/Latite.dll";
const REQUEST_USER_AGENT: &str = "Latite Launcher/0.1";

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

pub async fn fetch_latest_release_name() -> Result<String, String> {
    let response = reqwest::Client::new()
        .get(RELEASE_API_URL)
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

pub async fn download_latest_dll(destination: &Path) -> Result<(), String> {
    let response = reqwest::get(DLL_DOWNLOAD_URL)
        .await
        .map_err(|error| format!("Failed to download DLL: {error}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Failed to download DLL: {status}"));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read downloaded DLL bytes: {error}"))?;

    std::fs::write(destination, &bytes)
        .map_err(|error| format!("Failed to write DLL to {}: {error}", destination.display()))?;

    println!("DLL downloaded successfully to {}.", destination.display());
    Ok(())
}
