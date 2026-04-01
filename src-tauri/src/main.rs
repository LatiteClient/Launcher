// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use windows::Win32::UI::WindowsAndMessaging::{MessageBoxA, MB_OK};
use windows::core::s;

mod inject;
mod options;
mod paths;
use crate::inject::{inject_dll, get_pid};
use crate::options::{load_options, save_options};

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

fn get_dll_path() -> std::path::PathBuf {
    paths::get_dlls_path().join("Latite.dll")
}

async fn download_file() {
    // TODO: Use github.com/latiteclient/Latite releases
    let response = reqwest::get("https://github.com/Imrglop/Latite-Releases/releases/latest/download/Latite.dll").await.unwrap();

    if response.status().is_success() {
        let bytes = response.bytes().await.unwrap();
        std::fs::write(get_dll_path(), &bytes).unwrap();
        println!("DLL downloaded successfully!");
    } else {
        println!("Failed to download DLL: {}", response.status());
    }
}

#[tauri::command]
async fn inject() {
    println!("Injecting...");

    let dll_path = get_dll_path();
    
    if !std::fs::exists(dll_path.clone()).unwrap() {
        download_file().await;
    }

    let res = std::process::Command::new("minecraft:")
        .spawn();

    if !res.is_ok() {
        unsafe { MessageBoxA(None, s!("Minecraft does not seem to be installed!"), s!("Latite Client"), MB_OK) };
        return;
    }

    res.unwrap().wait().unwrap();


    let pid = unsafe { get_pid("Minecraft.Windows.exe") };

    if pid.is_none() {
        println!("Minecraft process not found!");
        return;
    } else {
        println!("Minecraft process found with PID: {}", pid.unwrap());
    }


    unsafe { inject_dll(pid.unwrap(), dll_path.to_str().unwrap()); }
}

fn main() {
    load_options();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet, inject])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    println!("Saving options");
    save_options();
}
    