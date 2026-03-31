// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::Threading::{
    CreateRemoteThread, OpenProcess, PROCESS_ALL_ACCESS,
};
use windows::Win32::System::Memory::{
    VirtualAllocEx, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
};

use windows::Win32::UI::WindowsAndMessaging::{MessageBoxA, MB_OK};

use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;

use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use std::ffi::CString;

use windows::core::s;

// Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

const DLL_PATH: &str = "./Latite.dll";

unsafe fn inject_dll(pid: u32, dll_path: &str) {
    let process_handle = OpenProcess(PROCESS_ALL_ACCESS, false, pid).unwrap();
    let full_dll_path = std::fs::canonicalize(dll_path).unwrap();
    let dll_path_bytes = CString::new(full_dll_path.to_str().unwrap()).unwrap().into_bytes_with_nul();
    
    let allocated_memory = VirtualAllocEx(
        process_handle,
        None,
        dll_path_bytes.len(),
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );

    WriteProcessMemory(
        process_handle,
        allocated_memory,
        dll_path_bytes.as_ptr() as *const _,
        dll_path_bytes.len(),
        None,
    ).unwrap();

    let kernel32 = GetModuleHandleA(s!("kernel32.dll")).unwrap();

    let load_library = GetProcAddress(kernel32, s!("LoadLibraryA")).unwrap();
    
    let result = CreateRemoteThread(
        process_handle,
        None,
        0,
        Some(std::mem::transmute(load_library)),
        Some(allocated_memory),
        0,
        None,
    );

    println!("Create remote thread result: {}", result.is_ok());
}

unsafe fn get_pid(process_name: &str) -> Option<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::*;

    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).unwrap();
    let mut entry = PROCESSENTRY32::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32>() as u32;

    if Process32First(snapshot, &mut entry).is_ok() {
        loop {
            let mut exe_file = String::new();

            for i in 0..MAX_PATH as usize {
                if entry.szExeFile[i] == 0 {
                    break;
                }

                exe_file.push(entry.szExeFile[i] as u8 as char);
            }

            if exe_file == process_name {
                return Some(entry.th32ProcessID);
            }
            if !Process32Next(snapshot, &mut entry).is_ok() {
                break;
            }
        }
    }
    None
}

async fn download_file() {
    // TODO: Use github.com/latiteclient/Latite releases
    let response = reqwest::get("https://github.com/Imrglop/Latite-Releases/releases/latest/download/Latite.dll").await.unwrap();

    if response.status().is_success() {
        let bytes = response.bytes().await.unwrap();
        std::fs::write(DLL_PATH, &bytes).unwrap();
        println!("DLL downloaded successfully!");
    } else {
        println!("Failed to download DLL: {}", response.status());
    }
}

#[tauri::command]
async fn inject() {
    println!("Injecting...");

    if !std::fs::exists(DLL_PATH).unwrap() {
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


    unsafe { inject_dll(pid.unwrap(), DLL_PATH); }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet, inject])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
