//! 动态加载 `KosuzuNative.dll` 并按原 ABI 取导出。
//!
//! 刻意不静态链接：自检程序要能指向任意一个已构建好的 DLL，
//! 这样同一份自检既能验 C++ 版也能验 Rust 版。

use std::ffi::c_void;
use std::path::Path;

use kosuzu_winutil as win;
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RawRule {
    pub source: *const u16,
    pub source_length: u32,
    pub target: *const u16,
    pub target_length: u32,
    pub category: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Matrix {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub dx: f32,
    pub dy: f32,
}

pub type LayoutFn = unsafe extern "system" fn(
    *mut c_void,
    *const u16,
    u32,
    *mut c_void,
    f32,
    f32,
    *mut *mut c_void,
) -> i32;

pub type GdiLayoutFn = unsafe extern "system" fn(
    *mut c_void,
    *const u16,
    u32,
    *mut c_void,
    f32,
    f32,
    f32,
    *const Matrix,
    i32,
    *mut *mut c_void,
) -> i32;

pub type SetRules = unsafe extern "system" fn(*const RawRule, u32) -> i32;
pub type SetModes = extern "system" fn(u32);
pub type TranslateFn = unsafe extern "system" fn(*const u16, u32, *mut u16, u32) -> i32;
pub type SetOriginals = extern "system" fn(Option<LayoutFn>, Option<GdiLayoutFn>) -> i32;
pub type SuccessfulLayouts = extern "system" fn() -> u64;
pub type StartFn = unsafe extern "system" fn(*mut c_void) -> u32;

pub struct Native {
    module: HMODULE,
    pub set_rules: SetRules,
    pub set_modes: SetModes,
    pub translate: TranslateFn,
    pub set_originals: SetOriginals,
    pub successful_layouts: SuccessfulLayouts,
    pub create_text_layout: LayoutFn,
    pub create_gdi_layout: GdiLayoutFn,
    pub start: StartFn,
}

impl Native {
    pub fn load(path: &Path) -> Result<Self, String> {
        let module = unsafe { LoadLibraryW(win::wide_path(path).as_ptr()) };
        if module.is_null() {
            return Err(format!("Cannot load {}", path.display()));
        }
        unsafe {
            Ok(Native {
                module,
                set_rules: symbol(module, c"LB_SetRules")?,
                set_modes: symbol(module, c"LB_SetModes")?,
                translate: symbol(module, c"LB_Translate")?,
                set_originals: symbol(module, c"LB_SetOriginals")?,
                successful_layouts: symbol(module, c"LB_SuccessfulLayouts")?,
                create_text_layout: symbol(module, c"LB_CreateTextLayout")?,
                create_gdi_layout: symbol(module, c"LB_CreateGdiCompatibleTextLayout")?,
                start: symbol(module, c"LB_Start")?,
            })
        }
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        unsafe { FreeLibrary(self.module) };
    }
}

unsafe fn symbol<T: Copy>(module: HMODULE, name: &core::ffi::CStr) -> Result<T, String> {
    match GetProcAddress(module, name.as_ptr() as *const u8) {
        Some(function) => {
            if std::mem::size_of::<T>() != std::mem::size_of_val(&function) {
                return Err(format!("Export {} has unexpected size", name.to_string_lossy()));
            }
            Ok(std::mem::transmute_copy(&function))
        }
        None => Err(format!("Missing export {}", name.to_string_lossy())),
    }
}
