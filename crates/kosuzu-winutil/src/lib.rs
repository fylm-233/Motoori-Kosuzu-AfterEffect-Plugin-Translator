//! 各原生组件共用的小工具：宽字符串、时间戳、原子写文件、模块路径。
//!
//! 只放“和业务无关的 Win32 胶水”，业务逻辑一律在 `kosuzu-core` 里，便于跨平台测试。

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;

/// 1601-01-01 到 1970-01-01 之间的毫秒数。
const EPOCH_DIFFERENCE_MS: u64 = 11_644_473_600_000;

/// 单个文本类文件的大小上限，与 C++ 侧一致（10 MiB）。
pub const MAX_TEXT_BYTES: usize = 10 * 1024 * 1024;

/// UTF-8 → 以 NUL 结尾的 UTF-16。
pub fn wide(text: &str) -> Vec<u16> {
    let mut out: Vec<u16> = text.encode_utf16().collect();
    out.push(0);
    out
}

/// 路径 → 以 NUL 结尾的 UTF-16。非 Unicode 路径用 lossy 处理，宁可读不到也不要 panic。
pub fn wide_path(path: &Path) -> Vec<u16> {
    wide(&path.to_string_lossy())
}

/// 只取到第一个 NUL 的宽字符串。
pub fn wide_to_string(units: &[u16]) -> String {
    let end = units.iter().position(|c| *c == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// 从宽字符串指针读到第一个 NUL。用于 `CommandLineToArgvW` 这类返回指针数组的 API。
///
/// # Safety
/// `pointer` 必须指向以 NUL 结尾的合法 UTF-16 缓冲区。
pub unsafe fn wide_cstr(pointer: *const u16) -> String {
    let mut length = 0usize;
    while length < 32768 && *pointer.add(length) != 0 {
        length += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length))
}

pub fn path_from_units(units: &[u16]) -> PathBuf {
    let end = units.iter().position(|c| *c == 0).unwrap_or(units.len());
    PathBuf::from(OsString::from_wide(&units[..end]))
}

/// 毫秒级 Unix 时间（与原实现的 FILETIME 换算一致）。
pub fn now_ms() -> u64 {
    let mut time = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    unsafe { GetSystemTimeAsFileTime(&mut time) };
    let ticks = ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
    ticks / 10_000 - EPOCH_DIFFERENCE_MS
}

/// 先写临时文件再 `MoveFileExW` 替换，避免面板读到半截 JSON。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut temp = path.as_os_str().to_owned();
    temp.push(".native.tmp");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, bytes)?;
    let temp_wide = wide_path(&temp);
    let target_wide = wide_path(path);
    let ok = unsafe {
        MoveFileExW(
            temp_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        std::fs::remove_file(&temp).ok();
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// 限长读取，防止异常大文件拖垮工作线程。
pub fn read_limited(path: &Path, max: usize) -> std::io::Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() as usize > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "File too large",
        ));
    }
    std::fs::read(path)
}

/// 当前模块（DLL 或 EXE）的完整路径。
pub fn module_path() -> PathBuf {
    let mut buffer = vec![0u16; 32768];
    let length = unsafe { GetModuleFileNameW(std::ptr::null_mut(), buffer.as_mut_ptr(), 32768) };
    path_from_units(&buffer[..length as usize])
}

/// 包根目录：`<root>/backend/KosuzuController.exe` 的上一级。
pub fn package_root() -> PathBuf {
    let exe = module_path();
    exe.parent()
        .and_then(|dir| dir.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}
