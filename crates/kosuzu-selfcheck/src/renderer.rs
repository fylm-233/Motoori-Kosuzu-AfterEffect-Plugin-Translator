//! 对应原 `native/RendererCheck.cpp`：验证拦截函数只改文字、不动其余参数；
//! 再用一个真实的 DirectWrite 工厂走一遍 vtable 调用，确认宽度确实随之变化。

use std::ffi::c_void;
use std::sync::Mutex;

use windows::core::w;
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_ISOLATED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_TEXT_METRICS,
};
use windows::core::Interface;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

use crate::dll::{GdiLayoutFn, LayoutFn, Matrix, Native, RawRule};

static OBSERVED: Mutex<Option<Vec<u16>>> = Mutex::new(None);
static OBSERVED_WIDTH: Mutex<f32> = Mutex::new(0.0);
static OBSERVED_PIXELS: Mutex<f32> = Mutex::new(0.0);
static OBSERVED_NATURAL: Mutex<i32> = Mutex::new(0);
static OBSERVED_MATRIX: Mutex<usize> = Mutex::new(0);

fn set_observed(text: &[u16], width: f32) {
    *OBSERVED.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.to_vec());
    *OBSERVED_WIDTH.lock().unwrap_or_else(|e| e.into_inner()) = width;
}

fn observed() -> Option<Vec<u16>> {
    OBSERVED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn observed_width() -> f32 {
    *OBSERVED_WIDTH.lock().unwrap_or_else(|e| e.into_inner())
}

unsafe extern "system" fn sample_layout(
    _factory: *mut c_void,
    text: *const u16,
    length: u32,
    _format: *mut c_void,
    width: f32,
    _height: f32,
    _output: *mut *mut c_void,
) -> i32 {
    set_observed(std::slice::from_raw_parts(text, length as usize), width);
    0
}

#[allow(clippy::too_many_arguments)]
unsafe extern "system" fn sample_gdi_layout(
    _factory: *mut c_void,
    text: *const u16,
    length: u32,
    _format: *mut c_void,
    width: f32,
    _height: f32,
    pixels_per_dip: f32,
    transform: *const Matrix,
    use_gdi_natural: i32,
    _output: *mut *mut c_void,
) -> i32 {
    set_observed(std::slice::from_raw_parts(text, length as usize), width);
    *OBSERVED_PIXELS.lock().unwrap_or_else(|e| e.into_inner()) = pixels_per_dip;
    *OBSERVED_NATURAL.lock().unwrap_or_else(|e| e.into_inner()) = use_gdi_natural;
    *OBSERVED_MATRIX.lock().unwrap_or_else(|e| e.into_inner()) = transform as usize;
    0
}

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

/// 把 vtable 的某个槽位换成自己的函数，返回原函数。
unsafe fn patch_slot(slot: *mut *mut c_void, replacement: *mut c_void) -> *mut c_void {
    let mut old_protect = 0u32;
    VirtualProtect(
        slot as *const c_void,
        std::mem::size_of::<*mut c_void>(),
        PAGE_READWRITE,
        &mut old_protect,
    );
    let original = *slot;
    *slot = replacement;
    let mut restored = 0u32;
    VirtualProtect(
        slot as *const c_void,
        std::mem::size_of::<*mut c_void>(),
        old_protect,
        &mut restored,
    );
    original
}

fn measure(factory: &IDWriteFactory, format: &IDWriteTextFormat, text: &[u16]) -> Result<f32, String> {
    let units: Vec<u16> = text.to_vec();
    let layout = unsafe { factory.CreateTextLayout(&units, format, 1000.0, 100.0) }
        .map_err(|error| format!("CreateTextLayout failed: {}", error))?;
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut metrics) }
        .map_err(|error| format!("GetMetrics failed: {}", error))?;
    Ok(metrics.width)
}

pub fn run(native: &Native) -> Result<(), String> {
    let source: Vec<u16> = "Intensity".encode_utf16().collect();
    let target: Vec<u16> = "强度".encode_utf16().collect();
    let rule = RawRule {
        source: source.as_ptr(),
        source_length: source.len() as u32,
        target: target.as_ptr(),
        target_length: target.len() as u32,
        category: 2,
    };
    check(unsafe { (native.set_rules)(&rule, 1) } == 0, "set rules")?;
    check(
        (native.set_originals)(Some(sample_layout), Some(sample_gdi_layout)) == 0,
        "set originals",
    )?;

    // 1. 只改文字，不改宽度参数
    (native.set_modes)(2);
    let code = unsafe {
        (native.create_text_layout)(
            std::ptr::null_mut(),
            source.as_ptr(),
            source.len() as u32,
            std::ptr::null_mut(),
            500.0,
            100.0,
            std::ptr::null_mut(),
        )
    };
    check(code >= 0, "hook returned failure")?;
    check(observed().as_deref() == Some(target.as_slice()), "text was not translated")?;
    check(observed_width() == 500.0, "width argument was not preserved")?;
    check(
        String::from_utf16_lossy(&source) == "Intensity",
        "hook mutated the caller buffer",
    )?;

    // 2. 关闭后透传原文
    (native.set_modes)(0);
    unsafe {
        (native.create_text_layout)(
            std::ptr::null_mut(),
            source.as_ptr(),
            source.len() as u32,
            std::ptr::null_mut(),
            500.0,
            100.0,
            std::ptr::null_mut(),
        )
    };
    check(observed().as_deref() == Some(source.as_slice()), "mode off must pass original")?;

    // 3. GDI 兼容路径的附加参数必须原样透传
    let matrix = Matrix {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        dx: 10.0,
        dy: 20.0,
    };
    (native.set_modes)(2);
    unsafe {
        (native.create_gdi_layout)(
            std::ptr::null_mut(),
            source.as_ptr(),
            source.len() as u32,
            std::ptr::null_mut(),
            500.0,
            100.0,
            1.25,
            &matrix,
            1,
            std::ptr::null_mut(),
        )
    };
    check(observed().as_deref() == Some(target.as_slice()), "gdi path not translated")?;
    check(
        *OBSERVED_PIXELS.lock().unwrap_or_else(|e| e.into_inner()) == 1.25,
        "pixels per dip lost",
    )?;
    check(
        *OBSERVED_MATRIX.lock().unwrap_or_else(|e| e.into_inner()) == &matrix as *const Matrix as usize,
        "transform pointer lost",
    )?;
    check(
        *OBSERVED_NATURAL.lock().unwrap_or_else(|e| e.into_inner()) == 1,
        "gdi natural flag lost",
    )?;

    // 4. 真实 DirectWrite 工厂：把 vtable 槽位换成钩子，看宽度是否随开关变化
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED) }
        .map_err(|error| format!("DWriteCreateFactory failed: {}", error))?;
    let format = unsafe {
        factory.CreateTextFormat(
            w!("Microsoft YaHei"),
            None,
            DWRITE_FONT_WEIGHT_NORMAL,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            16.0,
            w!("zh-cn"),
        )
    }
    .map_err(|error| format!("CreateTextFormat failed: {}", error))?;

    let vtable = unsafe { *(factory.as_raw() as *mut *mut *mut c_void) };
    let original_layout: LayoutFn = unsafe { std::mem::transmute(*vtable.add(18)) };
    let original_gdi: GdiLayoutFn = unsafe { std::mem::transmute(*vtable.add(19)) };
    check(
        (native.set_originals)(Some(original_layout), Some(original_gdi)) == 0,
        "set real originals",
    )?;

    (native.set_modes)(0);
    let width_off = measure(&factory, &format, &source)?;

    let hook = native.create_text_layout as *const () as *mut c_void;
    let saved = unsafe { patch_slot(vtable.add(18), hook) };
    (native.set_modes)(2);
    let width_on = measure(&factory, &format, &source)?;
    (native.set_modes)(0);
    let width_off_again = measure(&factory, &format, &source)?;
    unsafe { patch_slot(vtable.add(18), saved) };

    check((width_off - width_off_again).abs() < 0.001, "off state changed width")?;
    check((width_off - width_on).abs() > 0.1, "translation did not change width")?;

    println!(
        "PASS renderer: callbacks preserve input/arguments, off-on-off widths {:.3} -> {:.3} -> {:.3}",
        width_off, width_on, width_off_again
    );
    Ok(())
}
