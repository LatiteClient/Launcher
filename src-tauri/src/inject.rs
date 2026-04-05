use std::ffi::{c_void, CStr};
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::Duration;

use windows::core::s;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32First, Process32Next, PROCESSENTRY32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeProcess, GetExitCodeThread, OpenProcess, WaitForSingleObject,
    INFINITE, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

type ThreadStartRoutine = unsafe extern "system" fn(*mut c_void) -> u32;

pub struct TargetProcess {
    pid: u32,
    handle: OwnedHandle,
}

impl TargetProcess {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    fn raw(&self) -> HANDLE {
        self.handle.raw()
    }
}

pub enum ProcessWaitOutcome {
    Running,
    Exited(u32),
}

pub fn open_target_process(pid: u32) -> Result<TargetProcess, String> {
    unsafe { open_target_process_inner(pid) }
}

pub fn inject_dll(target_process: &TargetProcess, dll_path: &Path) -> Result<(), String> {
    unsafe { inject_dll_inner(target_process, dll_path) }
}

pub fn wait_for_process_exit(
    target_process: &TargetProcess,
    timeout: Duration,
) -> Result<ProcessWaitOutcome, String> {
    unsafe { wait_for_process_exit_inner(target_process, timeout) }
}

pub fn find_process_id(process_name: &str) -> Result<Option<u32>, String> {
    unsafe { find_process_id_inner(process_name) }
}

unsafe fn open_target_process_inner(pid: u32) -> Result<TargetProcess, String> {
    let process_handle = OpenProcess(
        PROCESS_CREATE_THREAD
            | PROCESS_QUERY_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_WRITE
            | PROCESS_VM_READ
            | PROCESS_SYNCHRONIZE,
        false,
        pid,
    )
    .map_err(|error| format!("Failed to open target process {pid}: {error}"))?;

    Ok(TargetProcess {
        pid,
        handle: OwnedHandle::new(process_handle),
    })
}

unsafe fn inject_dll_inner(target_process: &TargetProcess, dll_path: &Path) -> Result<(), String> {
    let pid = target_process.pid();

    let full_dll_path = std::fs::canonicalize(dll_path)
        .map_err(|error| format!("Failed to resolve DLL path {}: {error}", dll_path.display()))?;
    let dll_path_wide: Vec<u16> = full_dll_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let dll_path_size = dll_path_wide.len() * size_of::<u16>();

    let allocated_memory = VirtualAllocEx(
        target_process.raw(),
        None,
        dll_path_size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );

    if allocated_memory.is_null() {
        return Err(win32_error("VirtualAllocEx failed"));
    }

    let allocated_memory = RemoteAllocation::new(target_process.raw(), allocated_memory);

    WriteProcessMemory(
        target_process.raw(),
        allocated_memory.raw(),
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
        target_process.raw(),
        None,
        0,
        Some(start_routine),
        Some(allocated_memory.raw()),
        0,
        None,
    )
    .map_err(|error| format!("CreateRemoteThread failed: {error}"))?;
    let remote_thread = OwnedHandle::new(remote_thread);

    let wait_result = WaitForSingleObject(remote_thread.raw(), INFINITE);

    if wait_result == WAIT_FAILED {
        return Err(win32_error("WaitForSingleObject failed"));
    }

    if wait_result != WAIT_OBJECT_0 {
        return Err(format!(
            "WaitForSingleObject returned an unexpected status: {wait_result:?}"
        ));
    }

    // Check if the LoadLibraryW call succeeded
    let mut thread_exit_code: u32 = 0;
    GetExitCodeThread(remote_thread.raw(), &mut thread_exit_code)
        .map_err(|error| format!("Failed to get thread exit code: {error}"))?;

    if thread_exit_code == 0 {
        return Err("LoadLibraryW failed: DLL could not be loaded (exit code was 0).".to_string());
    }

    println!(
        "Injected {} into process {pid}. LoadLibraryW returned: {thread_exit_code:#x}",
        full_dll_path.display()
    );
    Ok(())
}

unsafe fn wait_for_process_exit_inner(
    target_process: &TargetProcess,
    timeout: Duration,
) -> Result<ProcessWaitOutcome, String> {
    let wait_result = WaitForSingleObject(target_process.raw(), duration_to_wait_millis(timeout));

    if wait_result == WAIT_FAILED {
        return Err(win32_error(
            "WaitForSingleObject failed while monitoring process",
        ));
    }

    if wait_result == WAIT_TIMEOUT {
        return Ok(ProcessWaitOutcome::Running);
    }

    if wait_result != WAIT_OBJECT_0 {
        return Err(format!(
            "WaitForSingleObject returned an unexpected status while monitoring process: {wait_result:?}"
        ));
    }

    let mut process_exit_code: u32 = 0;
    GetExitCodeProcess(target_process.raw(), &mut process_exit_code)
        .map_err(|error| format!("Failed to get process exit code: {error}"))?;

    Ok(ProcessWaitOutcome::Exited(process_exit_code))
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

fn duration_to_wait_millis(duration: Duration) -> u32 {
    duration.as_millis().min(u32::MAX as u128) as u32
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

struct RemoteAllocation {
    process: HANDLE,
    address: *mut c_void,
}

impl RemoteAllocation {
    fn new(process: HANDLE, address: *mut c_void) -> Self {
        Self { process, address }
    }

    fn raw(&self) -> *mut c_void {
        self.address
    }
}

impl Drop for RemoteAllocation {
    fn drop(&mut self) {
        unsafe {
            if let Err(error) = VirtualFreeEx(self.process, self.address, 0, MEM_RELEASE) {
                eprintln!(
                    "Failed to free remote allocation at {:p}: {error}",
                    self.address
                );
            }
        }
    }
}
