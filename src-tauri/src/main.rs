// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows::core::s;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxA, MB_ICONERROR, MB_OK};

mod inject;
mod options;
mod paths;
use crate::inject::{get_pid, inject_dll};
use crate::options::{
    get_bool_option, get_last_used_version, load_options, save_options, update_bool_option,
    update_last_used_version,
};

const MC_PROCESS_NAME: &str = "Minecraft.Windows.exe";
static mut IS_INJECTING: AtomicBool = AtomicBool::new(false);

fn get_dll_path() -> std::path::PathBuf {
    paths::get_dlls_path().join("Latite.dll")
}

async fn download_file() {
    // TODO: Use github.com/latiteclient/Latite releases
    let response = reqwest::get(
        "https://github.com/Imrglop/Latite-Releases/releases/latest/download/Latite.dll",
    )
    .await
    .unwrap();

    if response.status().is_success() {
        let bytes = response.bytes().await.unwrap();
        std::fs::write(get_dll_path(), &bytes).unwrap();
        println!("DLL downloaded successfully!");
    } else {
        println!("Failed to download DLL: {}", response.status());
    }
}

async fn fetch_latest_github_release_name() -> Option<String> {
    let url = "https://api.github.com/repos/Imrglop/Latite-Releases/releases/latest";
    let client = reqwest::Client::new();
    let response = client.get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36")
    .send().await.ok()?;

    if response.status().is_success() {
        let text = response.text().await.ok()?;
        println!("Response text: {}", text);

        let json: serde_json::Value = text.parse().ok()?;
        return json
            .get("tag_name")
            .and_then(|n| n.as_str())
            .map(|s| s.to_string());
    } else {
        println!("Failed to fetch latest release: {}", response.status());
    }

    None
}

#[tauri::command]
async fn inject() {
    unsafe {
        if (IS_INJECTING.load(Ordering::SeqCst)) {
            return;
        }

        IS_INJECTING.store(true, Ordering::SeqCst)
    };

    println!("Injecting...");

    let dll_path = get_dll_path();

    let old_last_used = get_last_used_version();
    let mut new_last_used = String::new();

    fetch_latest_github_release_name().await.map(|version| {
        println!("Latest release version: {}", version);
        update_last_used_version(version.as_str());
        new_last_used = version;
        save_options();
    });

    println!("new_last_used = {}", new_last_used);

    if !new_last_used.is_empty() && (!std::fs::exists(dll_path.clone()).unwrap() || old_last_used.is_none() || old_last_used != Some(new_last_used.clone()))  {
        download_file().await;
    }

    
    if !std::fs::exists(dll_path.clone()).unwrap() {
        unsafe {
            MessageBoxA(
                None,
                s!("Failed to download DLL!"),
                s!("Latite Client"),
                MB_ICONERROR | MB_OK,
            )
        };
        unsafe { IS_INJECTING.store(false, Ordering::SeqCst) };
        return;
    }

    let res = std::process::Command::new("explorer")
        .arg("minecraft:")
        .spawn();

    if !res.is_ok() {
        unsafe {
            MessageBoxA(
                None,
                s!("Minecraft does not seem to be installed!"),
                s!("Latite Client"),
                MB_ICONERROR | MB_OK,
            )
        };
        unsafe { IS_INJECTING.store(false, Ordering::SeqCst) };
        return;
    }

    res.unwrap().wait().unwrap();

    let mut pid = unsafe { get_pid(MC_PROCESS_NAME) };

    if pid.is_none() {
        for i in 0..100 {
            println!("Minecraft process not found, retrying... ({}/100)", i + 1);
            std::thread::sleep(std::time::Duration::from_millis(50));
            pid = unsafe { get_pid(MC_PROCESS_NAME) };
            if pid.is_some() {
                break;
            }
        }

        if pid.is_none() {
            unsafe {
                MessageBoxA(
                    None,
                    s!("Minecraft process not found, please try again"),
                    s!("Latite Client"),
                    MB_ICONERROR | MB_OK,
                )
            };
            unsafe { IS_INJECTING.store(false, Ordering::SeqCst) };
            return;
        }
    } else {
        println!("Minecraft process found with PID: {}", pid.unwrap());
    }

    unsafe {
        inject_dll(pid.unwrap(), dll_path.to_str().unwrap());
    }
    unsafe { IS_INJECTING.store(false, Ordering::SeqCst) };
}

#[tauri::command]
fn update_option(id: &str, value: bool) {
    update_bool_option(id, value);
}

#[tauri::command]
fn get_option(id: &str) -> bool {
    get_bool_option(id)
}

fn main() {
    load_options();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![inject, update_option, get_option])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    println!("Saving options");
    save_options();
}
