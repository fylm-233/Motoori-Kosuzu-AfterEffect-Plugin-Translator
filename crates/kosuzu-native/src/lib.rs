//! `KosuzuNative.dll` 的导出面。
//!
//! 导出名、参数顺序、返回码与原 C++ DLL 完全一致，因此
//! `KosuzuController.exe`（或任何按原 ABI 写好的控制器）都能直接驱动这个 DLL。
//!
//! 所有导出函数都写成不可 panic 的形式：本 crate 以 `panic = "abort"` 编译，
//! 一旦 panic 会直接终止宿主进程（AE）。宁可返回错误码，也不 unwrap。

#![allow(non_snake_case)]

mod hook;
mod worker;

use kosuzu_winutil as win;

use std::path::PathBuf;

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};
use kosuzu_core::dict::{self, Rule};
use kosuzu_core::lookup;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::CreateThread;

use hook::{DwriteMatrix, GdiLayoutFn, LayoutFn};

static STARTED: AtomicBool = AtomicBool::new(false);

/// 与 C++ 侧 `LBRule` 布局一致：指针、长度、指针、长度、类别。
#[repr(C)]
pub struct RawRule {
    pub source: *const u16,
    pub source_length: u32,
    pub target: *const u16,
    pub target_length: u32,
    pub category: u32,
}

/// 启动工作线程。`argument` 指向以 NUL 结尾的 UTF-16 包根目录。
#[no_mangle]
pub unsafe extern "system" fn LB_Start(argument: *mut c_void) -> u32 {
    if argument.is_null() {
        return 0;
    }
    if STARTED.swap(true, Ordering::AcqRel) {
        return 1;
    }

    let source = argument as *const u16;
    let mut length = 0usize;
    while length < 32768 && *source.add(length) != 0 {
        length += 1;
    }
    if length >= 32768 {
        STARTED.store(false, Ordering::Release);
        return 0;
    }

    let units = std::slice::from_raw_parts(source, length + 1);
    let root = win::path_from_units(units);
    let boxed = Box::into_raw(Box::new(root));
    let thread = CreateThread(
        std::ptr::null(),
        0,
        Some(thread_main),
        boxed as *const c_void,
        0,
        std::ptr::null_mut(),
    );
    if thread.is_null() {
        drop(Box::from_raw(boxed));
        STARTED.store(false, Ordering::Release);
        return 0;
    }
    CloseHandle(thread);
    1
}

unsafe extern "system" fn thread_main(parameter: *mut c_void) -> u32 {
    let root: PathBuf = *Box::from_raw(parameter as *mut PathBuf);
    worker::Engine::new(root).run();
    0
}

/// 装载规则表。0 成功，-1 失败（旧快照保持不变）。
#[no_mangle]
pub unsafe extern "system" fn LB_SetRules(rules: *const RawRule, count: u32) -> i32 {
    if (count != 0 && rules.is_null()) || count > dict::MAX_RULES {
        return -1;
    }
    let mut list: Vec<Rule> = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let raw = &*rules.add(index);
        if raw.source.is_null() || raw.target.is_null() {
            return -1;
        }
        if raw.source_length == 0
            || raw.target_length == 0
            || raw.source_length > dict::MAX_RULE_UNITS
            || raw.target_length > dict::MAX_RULE_UNITS
        {
            return -1;
        }
        let source =
            std::slice::from_raw_parts(raw.source, raw.source_length as usize).to_vec();
        let target =
            std::slice::from_raw_parts(raw.target, raw.target_length as usize).to_vec();
        list.push(Rule {
            source,
            target,
            category: raw.category,
        });
    }
    match lookup::set_rules_owned(list) {
        Ok(()) => 0,
        Err(()) => -1,
    }
}

#[no_mangle]
pub extern "system" fn LB_SetModes(value: u32) {
    lookup::set_modes(value);
}

#[no_mangle]
pub unsafe extern "system" fn LB_Translate(
    source: *const u16,
    length: u32,
    output: *mut u16,
    capacity: u32,
) -> i32 {
    lookup::translate_raw(source, length, output, capacity)
}

#[no_mangle]
pub extern "system" fn LB_SetOriginals(layout: Option<LayoutFn>, gdi: Option<GdiLayoutFn>) -> i32 {
    match hook::set_originals(layout, gdi) {
        Ok(()) => 0,
        Err(()) => -1,
    }
}

#[no_mangle]
pub extern "system" fn LB_SuccessfulLayouts() -> u64 {
    hook::successful_layouts()
}

#[no_mangle]
pub unsafe extern "system" fn LB_CreateTextLayout(
    factory: *mut c_void,
    text: *const u16,
    length: u32,
    format: *mut c_void,
    width: f32,
    height: f32,
    output: *mut *mut c_void,
) -> i32 {
    hook::create_text_layout(factory, text, length, format, width, height, output)
}

#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "system" fn LB_CreateGdiCompatibleTextLayout(
    factory: *mut c_void,
    text: *const u16,
    length: u32,
    format: *mut c_void,
    width: f32,
    height: f32,
    pixels_per_dip: f32,
    transform: *const DwriteMatrix,
    use_gdi_natural: i32,
    output: *mut *mut c_void,
) -> i32 {
    hook::create_gdi_compatible_text_layout(
        factory,
        text,
        length,
        format,
        width,
        height,
        pixels_per_dip,
        transform,
        use_gdi_natural,
        output,
    )
}
