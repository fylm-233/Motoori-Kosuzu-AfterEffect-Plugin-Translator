//! `KosuzuClipboard.exe`
//!
//! ScriptUI 的 `system.callSystem` 无法可靠地读写剪贴板，所以词典窗口把「待译原文」先写成
//! UTF-8 文本文件，再由这个小程序搬运到剪贴板（或反向搬回），最后写一个 `OK`/`ERROR` 回执。
//!
//! 用法：`KosuzuClipboard.exe copy|paste <数据文件> <回执文件>`

#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::path::PathBuf;

use kosuzu_winutil as win;
use windows_sys::Win32::Foundation::{GlobalFree, LocalFree};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows_sys::Win32::System::Threading::Sleep;
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE};

/// 剪贴板里文本的长度上限，和文件上限保持一致。
const MAX_CLIPBOARD_BYTES: usize = 10 * 1024 * 1024;
/// 剪贴板被别的进程占用时的重试次数与间隔。
const OPEN_ATTEMPTS: u32 = 5;
const OPEN_RETRY_MS: u32 = 15;

struct Arguments {
    action: String,
    data: PathBuf,
    receipt: PathBuf,
}

fn parse_arguments() -> Option<Arguments> {
    let mut count = 0i32;
    let argv = unsafe { CommandLineToArgvW(windows_sys::Win32::System::Environment::GetCommandLineW(), &mut count) };
    if argv.is_null() || count != 4 {
        if !argv.is_null() {
            unsafe { LocalFree(argv as *mut c_void) };
        }
        return None;
    }
    let parts: Vec<String> = (0..count as usize)
        .map(|index| unsafe { win::wide_cstr(*argv.add(index)) })
        .collect();
    unsafe { LocalFree(argv as *mut c_void) };
    Some(Arguments {
        action: parts[1].clone(),
        data: PathBuf::from(&parts[2]),
        receipt: PathBuf::from(&parts[3]),
    })
}

fn write_receipt(path: &PathBuf, ok: bool) {
    std::fs::write(path, if ok { "OK" } else { "ERROR" }).ok();
}

fn copy_to_clipboard(data: &PathBuf) -> bool {
    let bytes = match win::read_limited(data, MAX_CLIPBOARD_BYTES) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return false,
    };
    let mut units: Vec<u16> = text.encode_utf16().collect();
    if units.first() == Some(&0xfeff) {
        units.remove(0);
    }

    let size = (units.len() + 1) * std::mem::size_of::<u16>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, size) };
    if memory.is_null() {
        return false;
    }
    let pointer = unsafe { GlobalLock(memory) };
    if pointer.is_null() {
        unsafe { GlobalFree(memory) };
        return false;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(units.as_ptr(), pointer as *mut u16, units.len());
        *(pointer as *mut u16).add(units.len()) = 0;
        GlobalUnlock(memory);
    }
    if unsafe { EmptyClipboard() } == 0
        || unsafe { SetClipboardData(CF_UNICODETEXT as u32, memory) }.is_null()
    {
        unsafe { GlobalFree(memory) };
        return false;
    }
    // 所有权已交给系统，不能再 GlobalFree。
    true
}

fn paste_from_clipboard(data: &PathBuf) -> bool {
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT as u32) };
    if handle.is_null() {
        return false;
    }
    let bytes = unsafe { GlobalSize(handle) };
    if bytes < 2 || bytes > MAX_CLIPBOARD_BYTES || bytes % std::mem::size_of::<u16>() != 0 {
        return false;
    }
    let units = bytes / std::mem::size_of::<u16>();
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        return false;
    }
    let buffer = unsafe { std::slice::from_raw_parts(pointer as *const u16, units) };
    // 等价于 wcsnlen_s：找不到 NUL 说明数据不完整。
    let length = match buffer.iter().position(|unit| *unit == 0) {
        Some(length) => length,
        None => {
            unsafe { GlobalUnlock(handle) };
            return false;
        }
    };
    let text = String::from_utf16_lossy(&buffer[..length]);
    unsafe { GlobalUnlock(handle) };
    std::fs::write(data, text.as_bytes()).is_ok()
}

fn run() -> bool {
    let arguments = match parse_arguments() {
        Some(arguments) => arguments,
        None => return false,
    };

    let owner = unsafe {
        CreateWindowExW(
            0,
            win::wide("STATIC").as_ptr(),
            win::wide("Kosuzu Clipboard").as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        )
    };

    let mut opened = false;
    for _ in 0..OPEN_ATTEMPTS {
        if unsafe { OpenClipboard(owner) } != 0 {
            opened = true;
            break;
        }
        unsafe { Sleep(OPEN_RETRY_MS) };
    }

    let mut ok = false;
    if opened {
        ok = match arguments.action.as_str() {
            "copy" => copy_to_clipboard(&arguments.data),
            "paste" => paste_from_clipboard(&arguments.data),
            _ => false,
        };
        unsafe { CloseClipboard() };
    }

    write_receipt(&arguments.receipt, ok);
    if !owner.is_null() {
        unsafe { DestroyWindow(owner) };
    }
    ok
}

fn main() {
    // 无论成功失败都要有回执，否则面板会一直等。
    std::process::exit(if run() { 0 } else { 1 });
}
