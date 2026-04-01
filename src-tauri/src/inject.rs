
use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::System::Threading::{
    CreateRemoteThread, OpenProcess, PROCESS_ALL_ACCESS,
};
use windows::Win32::System::Memory::{
    VirtualAllocEx, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE,
};


use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;

use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use std::ffi::CString;

use windows::core::s;


pub unsafe fn inject_dll(pid: u32, dll_path: &str) {
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

pub unsafe fn get_pid(process_name: &str) -> Option<u32> {
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