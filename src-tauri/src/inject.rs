use std::ffi::{c_void, CStr};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows::core::s;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Memory::{VirtualAllocEx, MEM_COMMIT, MEM_RESERVE, PAGE_READWRITE};
use windows::Win32::System::Threading::{
    CreateRemoteThread, OpenProcess, WaitForSingleObject, INFINITE, PROCESS_CREATE_THREAD,
    PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

type ThreadStartRoutine = unsafe extern "system" fn(*mut c_void) -> u32;

pub fn inject_dll(pid: u32, dll_path: &Path) -> Result<(), String> {
    unsafe { inject_dll_inner(pid, dll_path) }
}

pub fn find_process_id(process_name: &str) -> Result<Option<u32>, String> {
    unsafe { find_process_id_inner(process_name) }
}

unsafe fn inject_dll_inner(pid: u32, dll_path: &Path) -> Result<(), String> {
    let process_handle = OpenProcess(
        PROCESS_CREATE_THREAD
            | PROCESS_QUERY_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_WRITE
            | PROCESS_VM_READ,
        false,
        pid,
    )
    .map_err(|error| format!("Failed to open target process {pid}: {error}"))?;
    let process_handle = OwnedHandle::new(process_handle);

    let full_dll_path = std::fs::canonicalize(dll_path)
        .map_err(|error| format!("Failed to resolve DLL path {}: {error}", dll_path.display()))?;
    let dll_path_wide: Vec<u16> = full_dll_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dll_path_size = dll_path_wide.len() * size_of::<u16>();

    let allocated_memory = VirtualAllocEx(
        process_handle.raw(),
        None,
        dll_path_size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );

    if allocated_memory.is_null() {
        return Err(win32_error("VirtualAllocEx failed"));
    }

    WriteProcessMemory(
        process_handle.raw(),
        allocated_memory,
        dll_path_wide.as_ptr().cast(),
        dll_path_size,
        None,
    )
    .map_err(|error| format!("WriteProcessMemory failed: {error}"))?;

    let kernel32 = GetModuleHandleA(s!("kernel32.dll"))
        .map_err(|error| format!("Failed to get kernel32.dll handle: {error}"))?;
    let load_library = GetProcAddress(kernel32, s!("LoadLibraryW"))
        .ok_or_else(|| "Failed to resolve LoadLibraryW from kernel32.dll.".to_string())?;
    let start_routine: ThreadStartRoutine = std::mem::transmute(load_library);

    let remote_thread = CreateRemoteThread(
        process_handle.raw(),
        None,
        0,
        Some(start_routine),
        Some(allocated_memory),
        0,
        None,
    )
    .map_err(|error| format!("CreateRemoteThread failed: {error}"))?;
    let remote_thread = OwnedHandle::new(remote_thread);

    WaitForSingleObject(remote_thread.raw(), INFINITE);

    println!("Injected {} into process {pid}.", full_dll_path.display());
    Ok(())
}

unsafe fn find_process_id_inner(process_name: &str) -> Result<Option<u32>, String> {
    let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)
        .map_err(|error| format!("Failed to snapshot running processes: {error}"))?;
    let snapshot = OwnedHandle::new(snapshot);

    let mut entry = PROCESSENTRY32 {
        dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32>())
            .expect("PROCESSENTRY32 size should fit in u32"),
        ..Default::default()
    };

    if Process32First(snapshot.raw(), &raw mut entry).is_err() {
        return Ok(None);
    }

    loop {
        let exe_name = process_name_from_entry(&entry);

        // Ignore single-threaded launcher stubs and wait for the actual game process.
        if exe_name.eq_ignore_ascii_case(process_name) && entry.cntThreads > 1 {
            return Ok(Some(entry.th32ProcessID));
        }

        if Process32Next(snapshot.raw(), &raw mut entry).is_err() {
            break;
        }
    }

    Ok(None)
}

fn process_name_from_entry(entry: &PROCESSENTRY32) -> String {
    unsafe { CStr::from_ptr(entry.szExeFile.as_ptr().cast()) }
        .to_string_lossy()
        .into_owned()
}

fn win32_error(context: &str) -> String {
    format!("{context}: {}", windows::core::Error::from_win32())
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
