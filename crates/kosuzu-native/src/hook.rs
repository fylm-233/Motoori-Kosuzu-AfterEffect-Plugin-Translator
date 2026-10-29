//! DirectWrite 文本布局拦截。
//!
//! 做法与原 C++ 版本完全相同：取共享 `IDWriteFactory` 的 vtable，
//! 把槽位 18（`CreateTextLayout`）与 19（`CreateGdiCompatibleTextLayout`）换成自己的实现，
//! 翻译命中时改文字再调原函数；未命中时原样透传。
//!
//! 这里之所以不需要任何“Rust 专用技巧”：COM 接口就是「指向 vtable 的指针」，
//! 函数指针就是 `extern "system"` 的函数，二者在 Rust 里都是一等公民。

use core::ffi::c_void;
use core::mem::MaybeUninit;
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering};

use kosuzu_core::lookup;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_READWRITE};

use kosuzu_winutil as win;

/// 绘制回调里的栈缓冲区大小：8192 个 UTF-16 单元 + 结尾 NUL。
pub const BUFFER_UNITS: usize = 8193;

const VTABLE_LAYOUT: usize = 18;
const VTABLE_GDI_LAYOUT: usize = 19;

const E_UNEXPECTED: i32 = 0x8000_FFFFu32 as i32;

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
    *const DwriteMatrix,
    i32,
    *mut *mut c_void,
) -> i32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DwriteMatrix {
    pub m11: f32,
    pub m12: f32,
    pub m21: f32,
    pub m22: f32,
    pub dx: f32,
    pub dy: f32,
}

#[repr(C)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

const IID_IDWRITE_FACTORY: Guid = Guid {
    data1: 0xb859_ee5a,
    data2: 0xd838,
    data3: 0x4b5b,
    data4: [0xa2, 0xe8, 0x1a, 0xdc, 0x7d, 0x93, 0xdb, 0x48],
};

const DWRITE_FACTORY_TYPE_SHARED: u32 = 0;

static ORIGINAL_LAYOUT: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static ORIGINAL_GDI: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static SAVED_LAYOUT: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static SAVED_GDI: AtomicPtr<c_void> = AtomicPtr::new(ptr::null_mut());
static VTABLE: AtomicPtr<*mut c_void> = AtomicPtr::new(ptr::null_mut());
static HOOKED: AtomicBool = AtomicBool::new(false);
static HITS: AtomicU64 = AtomicU64::new(0);

/// 已成功替换的文字布局次数，写进 status.json 的 `hits`。
pub fn successful_layouts() -> u64 {
    HITS.load(Ordering::Relaxed)
}

/// 挂上钩子。可重复调用，第二次起直接返回成功。
pub unsafe fn install() -> Result<(), &'static str> {
    if HOOKED.load(Ordering::Acquire) {
        return Ok(());
    }

    let dwrite = LoadLibraryW(win::wide("dwrite.dll").as_ptr());
    if dwrite.is_null() {
        return Err("DirectWrite unavailable");
    }
    let proc = GetProcAddress(dwrite, c"DWriteCreateFactory".as_ptr() as *const u8);
    let create_factory: Option<unsafe extern "system" fn(u32, *const Guid, *mut *mut c_void) -> i32> =
        core::mem::transmute(proc);
    let create_factory = match create_factory {
        Some(function) => function,
        None => return Err("DirectWrite unavailable"),
    };

    let mut factory: *mut c_void = ptr::null_mut();
    if create_factory(DWRITE_FACTORY_TYPE_SHARED, &IID_IDWRITE_FACTORY, &mut factory) < 0
        || factory.is_null()
    {
        return Err("DirectWrite unavailable");
    }

    // COM 对象的前 8 字节就是 vtable 指针。
    let vtable = *(factory as *mut *mut *mut c_void);
    if vtable.is_null() {
        return Err("DirectWrite unavailable");
    }
    let slot_layout = vtable.add(VTABLE_LAYOUT);
    let slot_gdi = vtable.add(VTABLE_GDI_LAYOUT);
    let saved_layout = *slot_layout;
    let saved_gdi = *slot_gdi;
    if saved_layout.is_null() || saved_gdi.is_null() {
        return Err("Invalid original renderer");
    }

    ORIGINAL_LAYOUT.store(saved_layout, Ordering::Release);
    ORIGINAL_GDI.store(saved_gdi, Ordering::Release);
    SAVED_LAYOUT.store(saved_layout, Ordering::Release);
    SAVED_GDI.store(saved_gdi, Ordering::Release);
    VTABLE.store(vtable, Ordering::Release);

    let mut old_protect: u32 = 0;
    let size = 2 * core::mem::size_of::<*mut c_void>();
    if VirtualProtect(
        slot_layout as *const c_void,
        size,
        PAGE_READWRITE,
        &mut old_protect,
    ) == 0
    {
        return Err("Cannot enable renderer");
    }
    ptr::write_volatile(slot_layout, create_text_layout as *mut c_void);
    ptr::write_volatile(slot_gdi, create_gdi_compatible_text_layout as *mut c_void);
    let mut restored: u32 = 0;
    VirtualProtect(slot_layout as *const c_void, size, old_protect, &mut restored);

    HOOKED.store(true, Ordering::Release);
    Ok(())
}

/// 恢复原函数。即使中途失败也不会留下半挂状态：只有确实仍是自己的函数才写回。
pub unsafe fn uninstall() {
    lookup::set_modes(0);
    if !HOOKED.swap(false, Ordering::AcqRel) {
        return;
    }
    let vtable = VTABLE.load(Ordering::Acquire);
    if vtable.is_null() {
        return;
    }
    let slot_layout = vtable.add(VTABLE_LAYOUT);
    let slot_gdi = vtable.add(VTABLE_GDI_LAYOUT);
    let mut old_protect: u32 = 0;
    let size = 2 * core::mem::size_of::<*mut c_void>();
    if VirtualProtect(
        slot_layout as *const c_void,
        size,
        PAGE_READWRITE,
        &mut old_protect,
    ) == 0
    {
        return;
    }
    if *slot_layout == create_text_layout as *mut c_void {
        ptr::write_volatile(slot_layout, SAVED_LAYOUT.load(Ordering::Acquire));
    }
    if *slot_gdi == create_gdi_compatible_text_layout as *mut c_void {
        ptr::write_volatile(slot_gdi, SAVED_GDI.load(Ordering::Acquire));
    }
    let mut restored: u32 = 0;
    VirtualProtect(slot_layout as *const c_void, size, old_protect, &mut restored);
}

/// 从缓冲区读回 NUL 结尾长度，上限 BUFFER_UNITS，避免越界扫描。
unsafe fn bounded_len(units: *const u16) -> u32 {
    let mut length = 0usize;
    while length < BUFFER_UNITS && *units.add(length) != 0 {
        length += 1;
    }
    length as u32
}

/// 拦截 `IDWriteFactory::CreateTextLayout`。
pub unsafe extern "system" fn create_text_layout(
    factory: *mut c_void,
    text: *const u16,
    length: u32,
    format: *mut c_void,
    width: f32,
    height: f32,
    output: *mut *mut c_void,
) -> i32 {
    let original = ORIGINAL_LAYOUT.load(Ordering::Acquire);
    if original.is_null() {
        return E_UNEXPECTED;
    }
    let original: LayoutFn = core::mem::transmute(original);

    let mut buffer = MaybeUninit::<[u16; BUFFER_UNITS]>::uninit();
    let destination = buffer.as_mut_ptr() as *mut u16;
    let code = lookup::translate_raw(text, length, destination, BUFFER_UNITS as u32);

    let (pointer, used) = if code == 1 {
        (destination as *const u16, bounded_len(destination))
    } else {
        (text, length)
    };

    let result = original(factory, pointer, used, format, width, height, output);
    if code == 1 && result >= 0 {
        HITS.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// 拦截 `IDWriteFactory::CreateGdiCompatibleTextLayout`。
pub unsafe extern "system" fn create_gdi_compatible_text_layout(
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
    let original = ORIGINAL_GDI.load(Ordering::Acquire);
    if original.is_null() {
        return E_UNEXPECTED;
    }
    let original: GdiLayoutFn = core::mem::transmute(original);

    let mut buffer = MaybeUninit::<[u16; BUFFER_UNITS]>::uninit();
    let destination = buffer.as_mut_ptr() as *mut u16;
    let code = lookup::translate_raw(text, length, destination, BUFFER_UNITS as u32);

    let (pointer, used) = if code == 1 {
        (destination as *const u16, bounded_len(destination))
    } else {
        (text, length)
    };

    let result = original(
        factory,
        pointer,
        used,
        format,
        width,
        height,
        pixels_per_dip,
        transform,
        use_gdi_natural,
        output,
    );
    if code == 1 && result >= 0 {
        HITS.fetch_add(1, Ordering::Relaxed);
    }
    result
}

/// 供 `LB_SetOriginals` 使用：登记“原函数”，同时拒绝登记自己，避免自递归。
pub fn set_originals(layout: Option<LayoutFn>, gdi: Option<GdiLayoutFn>) -> Result<(), ()> {
    let layout = layout.ok_or(())?;
    let gdi = gdi.ok_or(())?;
    if layout as usize == create_text_layout as *const () as usize
        || gdi as usize == create_gdi_compatible_text_layout as *const () as usize
    {
        return Err(());
    }
    ORIGINAL_LAYOUT.store(layout as *mut c_void, Ordering::Release);
    ORIGINAL_GDI.store(gdi as *mut c_void, Ordering::Release);
    Ok(())
}
