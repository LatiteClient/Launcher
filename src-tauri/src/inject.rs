use std::ffi::{c_void, CStr, OsString};
use std::mem::size_of;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use windows::core::{s, Error as WindowsError, HRESULT, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_BAD_LENGTH, ERROR_NO_MORE_FILES, HANDLE, WAIT_FAILED, WAIT_OBJECT_0,
    WAIT_TIMEOUT, WIN32_ERROR,
};
use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32First, Module32Next, Process32First, Process32Next,
    MODULEENTRY32, PROCESSENTRY32, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeProcess, GetExitCodeThread, OpenProcess,
    QueryFullProcessImageNameW, WaitForSingleObject, INFINITE, PROCESS_CREATE_THREAD,
    PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

type ThreadStartRoutine = unsafe extern "system" fn(*mut c_void) -> u32;
const MODULE_SNAPSHOT_RETRY_DELAY: Duration = Duration::from_millis(10);

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

pub enum ModuleLoadWaitOutcome {
    Loaded(usize),
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

pub fn wait_for_process_module_count_above(
    target_process: &TargetProcess,
    module_count: usize,
    poll_interval: Duration,
) -> Result<ModuleLoadWaitOutcome, String> {
    unsafe {
        wait_for_process_module_count_above_inner(target_process, module_count, poll_interval)
    }
}

pub fn find_process_id(process_name: &str) -> Result<Option<u32>, String> {
    unsafe { find_process_id_inner(process_name) }
}

pub fn get_process_image_path(pid: u32) -> Result<PathBuf, String> {
    unsafe { get_process_image_path_inner(pid) }
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

unsafe fn get_process_image_path_inner(pid: u32) -> Result<PathBuf, String> {
    let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
        .map_err(|error| format!("Failed to open process {pid} for image path query: {error}"))?;
    let process_handle = OwnedHandle::new(process_handle);

    let mut path_buffer = vec![0u16; 32_768];
    let mut path_length = u32::try_from(path_buffer.len())
        .expect("process image path buffer length should fit in u32");

    QueryFullProcessImageNameW(
        process_handle.raw(),
        PROCESS_NAME_WIN32,
        PWSTR(path_buffer.as_mut_ptr()),
        &raw mut path_length,
    )
    .map_err(|error| format!("Failed to query process {pid} image path: {error}"))?;

    path_buffer.truncate(path_length as usize);

    if path_buffer.is_empty() {
        return Err(format!("Process {pid} returned an empty image path."));
    }

    Ok(PathBuf::from(OsString::from_wide(&path_buffer)))
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

    crate::log_info!(
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

unsafe fn wait_for_process_module_count_above_inner(
    target_process: &TargetProcess,
    module_count: usize,
    poll_interval: Duration,
) -> Result<ModuleLoadWaitOutcome, String> {
    loop {
        match wait_for_process_exit_inner(target_process, Duration::ZERO)? {
            ProcessWaitOutcome::Running => {}
            ProcessWaitOutcome::Exited(exit_code) => {
                return Ok(ModuleLoadWaitOutcome::Exited(exit_code));
            }
        }

        match count_process_modules_inner(target_process.pid()) {
            Ok(loaded_modules) if loaded_modules > module_count => {
                return Ok(ModuleLoadWaitOutcome::Loaded(loaded_modules));
            }
            Ok(_) => {}
            Err(error) => match wait_for_process_exit_inner(target_process, Duration::ZERO)? {
                ProcessWaitOutcome::Running => return Err(error),
                ProcessWaitOutcome::Exited(exit_code) => {
                    return Ok(ModuleLoadWaitOutcome::Exited(exit_code));
                }
            },
        }

        match wait_for_process_exit_inner(target_process, poll_interval)? {
            ProcessWaitOutcome::Running => {}
            ProcessWaitOutcome::Exited(exit_code) => {
                return Ok(ModuleLoadWaitOutcome::Exited(exit_code));
            }
        }
    }
}

unsafe fn count_process_modules_inner(pid: u32) -> Result<usize, String> {
    let snapshot = create_module_snapshot(pid)?;
    let mut entry = MODULEENTRY32 {
        dwSize: u32::try_from(size_of::<MODULEENTRY32>())
            .expect("MODULEENTRY32 size should fit in u32"),
        ..Default::default()
    };

    if let Err(error) = Module32First(snapshot.raw(), &raw mut entry) {
        if is_win32_error(&error, ERROR_NO_MORE_FILES) {
            return Ok(0);
        }

        return Err(format!(
            "Failed to enumerate modules for process {pid}: {error}"
        ));
    }

    let mut count = 1;

    loop {
        if let Err(error) = Module32Next(snapshot.raw(), &raw mut entry) {
            if is_win32_error(&error, ERROR_NO_MORE_FILES) {
                return Ok(count);
            }

            return Err(format!(
                "Failed to enumerate modules for process {pid}: {error}"
            ));
        }

        count += 1;
    }
}

unsafe fn create_module_snapshot(pid: u32) -> Result<OwnedHandle, String> {
    let flags = TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32;

    loop {
        match CreateToolhelp32Snapshot(flags, pid) {
            Ok(snapshot) => return Ok(OwnedHandle::new(snapshot)),
            Err(error) if is_win32_error(&error, ERROR_BAD_LENGTH) => {
                thread::sleep(MODULE_SNAPSHOT_RETRY_DELAY);
                continue;
            }
            Err(error) => {
                return Err(format!(
                    "Failed to snapshot modules for process {pid}: {error}"
                ));
            }
        }
    }
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

fn is_win32_error(error: &WindowsError, code: WIN32_ERROR) -> bool {
    error.code() == HRESULT::from_win32(code.0)
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
                crate::log_error!(
                    "Failed to free remote allocation at {:p}: {error}",
                    self.address
                );
            }
        }
    }
}
