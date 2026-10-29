//! 自检共用的小工具：内存/CPU 采样、状态等待、文件式命令、文字宽度测量。

use std::path::{Path, PathBuf};

use kosuzu_core::json::{self, Value};
use kosuzu_winutil as win;
use windows::Win32::Graphics::DirectWrite::{
    IDWriteFactory, IDWriteTextFormat, DWRITE_TEXT_METRICS,
};
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

/// 本进程的私有内存占用（字节），用于对比词典装载的内存增量。
pub fn private_usage() -> usize {
    let mut counters = PROCESS_MEMORY_COUNTERS_EX::default();
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters as *mut PROCESS_MEMORY_COUNTERS_EX as *mut PROCESS_MEMORY_COUNTERS,
            counters.cb,
        )
    };
    if ok == 0 {
        0
    } else {
        counters.PrivateUsage
    }
}

/// 本进程累计 CPU 时间（毫秒）。
pub fn cpu_time_ms() -> u64 {
    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = creation;
    let mut kernel = creation;
    let mut user = creation;
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return 0;
    }
    let ticks = |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64;
    (ticks(kernel) + ticks(user)) / 10_000
}

pub fn runtime_dir(root: &Path) -> PathBuf {
    root.join("runtime").join(format!("ae-{}", std::process::id()))
}

/// 等 status.json 出现并能解析，最多 `timeout_ms`。
pub fn wait_status(runtime: &Path, timeout_ms: u64) -> Result<Value, String> {
    let path = runtime.join("status.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    while std::time::Instant::now() < deadline {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(value) = json::parse(&bytes) {
                return Ok(value);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Err(format!("status.json not ready under {}", runtime.display()))
}

/// 走真实文件协议发一条命令，返回响应 JSON。
pub fn command(
    runtime: &Path,
    sequence: u32,
    method: &str,
    state: Option<&str>,
) -> Result<Value, String> {
    let id = format!("test-{}", sequence);
    let body = format!(
        "{{\"id\":\"{}\",\"created\":{},\"method\":{},\"state\":{}}}",
        id,
        win::now_ms(),
        json::quote(method),
        state.unwrap_or("null")
    );
    let requests = runtime.join("requests");
    let target = requests.join(format!("{}.json", id));
    let temp = requests.join(format!("{}.json.tmp", id));
    std::fs::write(&temp, body.as_bytes()).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, &target).map_err(|error| error.to_string())?;

    let answer_path = runtime.join("responses").join(format!("{}.json", id));
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2000);
    while std::time::Instant::now() < deadline {
        if let Ok(bytes) = std::fs::read(&answer_path) {
            let value = json::parse(&bytes).map_err(|error| error.0)?;
            std::fs::remove_file(&answer_path).ok();
            return Ok(value);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err(format!("no response for {} within 2000ms", id))
}

pub fn response_ok(value: &Value) -> bool {
    value.get("ok").ok().and_then(Value::as_bool).unwrap_or(false)
}

/// 失败时把后端给的原因带进断言消息，方便定位。
pub fn describe(value: &Value) -> String {
    format!(
        "ok={} error={}",
        response_ok(value),
        value
            .get("error")
            .ok()
            .and_then(Value::as_str)
            .unwrap_or_default()
    )
}

pub fn status_number(value: &Value, field: &str) -> f64 {
    value
        .get(field)
        .ok()
        .and_then(Value::as_f64)
        .unwrap_or(-1.0)
}

/// 用真实 DirectWrite 排版一次并返回宽度。任何命中翻译都会让宽度变化。
pub fn measure(
    factory: &IDWriteFactory,
    format: &IDWriteTextFormat,
    text: &[u16],
) -> Result<f32, String> {
    let layout = unsafe { factory.CreateTextLayout(text, format, 1000.0, 100.0) }
        .map_err(|error| format!("CreateTextLayout failed: {}", error))?;
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut metrics) }
        .map_err(|error| format!("GetMetrics failed: {}", error))?;
    Ok(metrics.width)
}

pub fn write_dictionary(root: &Path, kind: &str, base: bool, body: &str) -> Result<(), String> {
    let directory = root.join("dictionaries");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let name = if base {
        format!("Kosuzu.base.{}.json", kind)
    } else {
        format!("Kosuzu.{}.json", kind)
    };
    std::fs::write(directory.join(name), body.as_bytes()).map_err(|error| error.to_string())
}
