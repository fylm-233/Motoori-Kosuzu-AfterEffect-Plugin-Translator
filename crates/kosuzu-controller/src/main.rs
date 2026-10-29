//! `KosuzuController.exe`
//!
//! 职责单一：找到唯一的 AfterFX.exe，把 `KosuzuNative.dll` 注入进去，再远程调用它的 `LB_Start`。
//! 面板（`Kosuzu.jsx`）只用 `File.execute()` 启动它，然后读 `runtime/connection.json`。
//!
//! 注入步骤与原 C++ 版本一一对应，没有换用任何“更聪明”的手段：
//! 1. `OpenProcess` + `VirtualAllocEx`/`WriteProcessMemory` 传路径；
//! 2. 在远端调用 `kernel32!LoadLibraryW`（地址 = 远端 kernel32 基址 + 本地偏移）；
//! 3. 用 `LOAD_LIBRARY_AS_IMAGE` 式的 `DONT_RESOLVE_DLL_REFERENCES` 只读导出表拿到 `LB_Start` 偏移；
//! 4. 远端线程执行 `LB_Start(包根目录)`，返回 1 表示工作线程已起。

#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use kosuzu_winutil as win;
use windows_sys::Win32::Foundation::{
    CloseHandle, FreeLibrary, FALSE, HANDLE, HMODULE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
    MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW, GetProcAddress, LoadLibraryExW,
    DONT_RESOLVE_DLL_REFERENCES, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
    GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
};
use windows_sys::Win32::System::Memory::{
    VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use windows_sys::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject, LPTHREAD_START_ROUTINE,
    PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION, PROCESS_VM_READ,
    PROCESS_VM_WRITE,
};

const WAIT_OBJECT_0: u32 = 0;
const STARTUP_TIMEOUT_MS: u32 = 15_000;

/// 保证 `CloseHandle` 一定被调用，即使中途 `?` 提前返回。
struct Owned(HANDLE);

impl Owned {
    fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_null() || handle as isize == INVALID_HANDLE_VALUE as isize {
            None
        } else {
            Some(Owned(handle))
        }
    }

    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// 远端进程里的参数块，离开作用域即释放。
struct Remote {
    process: HANDLE,
    pointer: *mut c_void,
}

impl Remote {
    fn new(process: HANDLE, text: &str) -> Result<Self, String> {
        let units = win::wide(text);
        let size = units.len() * std::mem::size_of::<u16>();
        let pointer = unsafe {
            VirtualAllocEx(
                process,
                std::ptr::null(),
                size,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if pointer.is_null() {
            return Err("Cannot pass native path to AE".to_owned());
        }
        let mut written = 0usize;
        let ok = unsafe {
            WriteProcessMemory(
                process,
                pointer,
                units.as_ptr() as *const c_void,
                size,
                &mut written,
            )
        };
        if ok == 0 || written != size {
            unsafe { VirtualFreeEx(process, pointer, 0, MEM_RELEASE) };
            return Err("Cannot pass native path to AE".to_owned());
        }
        Ok(Remote { process, pointer })
    }

    fn forget(&mut self) {
        self.pointer = std::ptr::null_mut();
    }
}

impl Drop for Remote {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { VirtualFreeEx(self.process, self.pointer, 0, MEM_RELEASE) };
        }
    }
}

fn find_after_fx() -> Vec<u32> {
    let mut found = Vec::new();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    let snapshot = match Owned::new(snapshot) {
        Some(guard) => guard,
        None => return found,
    };
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(snapshot.raw(), &mut entry) };
    while ok != 0 {
        if win::wide_to_string(&entry.szExeFile).eq_ignore_ascii_case("AfterFX.exe") {
            found.push(entry.th32ProcessID);
        }
        ok = unsafe { Process32NextW(snapshot.raw(), &mut entry) };
    }
    found
}

/// 取目标进程里某个模块的基址。`full` 为真时按完整路径比对，否则按文件名。
fn module_base(pid: u32, name: &str, full: bool) -> usize {
    let snapshot = unsafe {
        CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid)
    };
    let snapshot = match Owned::new(snapshot) {
        Some(guard) => guard,
        None => return 0,
    };
    let mut entry: MODULEENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
    let mut ok = unsafe { Module32FirstW(snapshot.raw(), &mut entry) };
    while ok != 0 {
        let candidate = if full {
            win::wide_to_string(&entry.szExePath)
        } else {
            win::wide_to_string(&entry.szModule)
        };
        if candidate.to_lowercase() == name.to_lowercase() {
            return entry.modBaseAddr as usize;
        }
        ok = unsafe { Module32NextW(snapshot.raw(), &mut entry) };
    }
    0
}

/// 在远端执行一个函数并等它返回。超时后放弃参数块（线程可能还在跑，不能提前释放）。
fn call_remote(
    process: HANDLE,
    start: usize,
    remote: &mut Remote,
) -> Result<u32, String> {
    let routine: LPTHREAD_START_ROUTINE = unsafe { std::mem::transmute::<usize, _>(start) };
    let thread = unsafe {
        CreateRemoteThread(
            process,
            std::ptr::null(),
            0,
            routine,
            remote.pointer,
            0,
            std::ptr::null_mut(),
        )
    };
    let thread = match Owned::new(thread) {
        Some(guard) => guard,
        None => return Err("AE blocked native startup".to_owned()),
    };
    if unsafe { WaitForSingleObject(thread.raw(), STARTUP_TIMEOUT_MS) } != WAIT_OBJECT_0 {
        remote.forget();
        return Err("Native startup timed out; restart AE before retry".to_owned());
    }
    let mut code: u32 = 0;
    if unsafe { GetExitCodeThread(thread.raw(), &mut code) } == 0 {
        return Err("Cannot read startup result".to_owned());
    }
    Ok(code)
}

fn attach(root: &Path, pid: &mut u32) -> Result<(), String> {
    let found = find_after_fx();
    if found.is_empty() {
        return Err("Open After Effects first".to_owned());
    }
    if found.len() != 1 {
        return Err("Multiple AE processes found; open one AE process for this test build".to_owned());
    }
    *pid = found[0];

    let process = Owned::new(unsafe {
        OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            FALSE,
            *pid,
        )
    })
    .ok_or_else(|| {
        "Cannot access AE; run AE with the same Windows account and privilege level".to_owned()
    })?;

    let dll = root.join("backend").join("KosuzuNative.dll");
    if !dll.is_file() {
        return Err("Native engine missing; reinstall full package".to_owned());
    }
    let dll_text = dll.to_string_lossy().to_string();

    let mut remote_base = module_base(*pid, &dll_text, true);
    // 绝不把第二个版本装到已经运行中的引擎上。
    if remote_base == 0 && module_base(*pid, "KosuzuNative.dll", false) != 0 {
        return Err("Another Kosuzu version is loaded; restart AE after updating".to_owned());
    }

    if remote_base == 0 {
        let kernel = unsafe { GetModuleHandleW(win::wide("kernel32.dll").as_ptr()) };
        let loader = match unsafe { GetProcAddress(kernel, c"LoadLibraryW".as_ptr() as *const u8) } {
            Some(function) => function as usize,
            None => return Err("Cannot resolve Windows loader".to_owned()),
        };
        let mut owner: HMODULE = std::ptr::null_mut();
        if unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                loader as *const u16,
                &mut owner,
            )
        } == 0
        {
            return Err("Cannot resolve Windows loader".to_owned());
        }
        let mut owner_path = vec![0u16; 32768];
        let length = unsafe { GetModuleFileNameW(owner, owner_path.as_mut_ptr(), 32768) };
        let owner_file = win::path_from_units(&owner_path[..length as usize]);
        let owner_name = owner_file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let remote_owner = module_base(*pid, &owner_name, false);
        if remote_owner == 0 {
            return Err("Windows loader module unavailable".to_owned());
        }

        let mut argument = Remote::new(process.raw(), &dll_text)?;
        call_remote(
            process.raw(),
            remote_owner + (loader - owner as usize),
            &mut argument,
        )?;

        remote_base = module_base(*pid, &dll_text, true);
        if remote_base == 0 {
            return Err("AE could not load native engine".to_owned());
        }
    }

    // 只映射不初始化，避免在控制器进程里跑一遍 DLL 的初始化。
    let local = unsafe {
        LoadLibraryExW(
            win::wide_path(&dll).as_ptr(),
            std::ptr::null_mut(),
            DONT_RESOLVE_DLL_REFERENCES,
        )
    };
    if local.is_null() {
        return Err("Cannot read native exports".to_owned());
    }
    let start = unsafe { GetProcAddress(local, c"LB_Start".as_ptr() as *const u8) };
    let offset = match start {
        Some(function) => function as usize - local as usize,
        None => {
            unsafe { FreeLibrary(local) };
            return Err("Native entry point missing".to_owned());
        }
    };
    unsafe { FreeLibrary(local) };

    let mut argument = Remote::new(process.raw(), &root.to_string_lossy())?;
    let code = call_remote(process.raw(), remote_base + offset, &mut argument)?;
    if code == 0 {
        return Err("Native worker failed to start".to_owned());
    }
    Ok(())
}

fn write_connection(runtime: &Path, pid: u32, error: &str) {
    let body = format!(
        "{{\"created\":{},\"pid\":{},\"ok\":{},\"error\":{}}}",
        win::now_ms(),
        pid,
        if error.is_empty() { "true" } else { "false" },
        kosuzu_core::json::quote(error)
    );
    win::write_atomic(&runtime.join("connection.json"), body.as_bytes()).ok();
}

fn main() {
    let root: PathBuf = win::package_root();
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).ok();

    let mut pid: u32 = 0;
    let error = match attach(&root, &mut pid) {
        Ok(()) => String::new(),
        Err(message) => message,
    };
    write_connection(&runtime, pid, &error);
    std::process::exit(if error.is_empty() { 0 } else { 1 });
}
