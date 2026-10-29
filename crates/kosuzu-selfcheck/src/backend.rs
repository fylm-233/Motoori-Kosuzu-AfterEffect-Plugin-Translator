//! 对应原 `native/BackendCheck.cpp`：在真实共享 DirectWrite 工厂上验证
//! 「装载 → 翻译 → 坏词典回滚 → 自定义覆盖 → 并发重载 → 关闭恢复 → 再开」的全链路。

use std::ffi::c_void;
use std::path::Path;

use windows::core::w;
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
};

use crate::dll::Native;
use crate::harness;

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

pub fn run(native: &Native, root: &Path) -> Result<(), String> {
    harness::write_dictionary(root, "names", true, "{}")?;
    harness::write_dictionary(root, "names", false, "{}")?;
    harness::write_dictionary(root, "parameters", true, "{\"Intensity\":\"强度\"}")?;
    harness::write_dictionary(root, "parameters", false, "{}")?;

    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
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

    let source: Vec<u16> = "Intensity".encode_utf16().collect();
    let original = harness::measure(&factory, &format, &source)?;

    let before_memory = harness::private_usage();
    let begin = std::time::Instant::now();
    let root_units = kosuzu_winutil::wide(&root.to_string_lossy());
    let started = unsafe { (native.start)(root_units.as_ptr() as *mut c_void) };
    check(started == 1, "LB_Start failed")?;

    let runtime = harness::runtime_dir(root);
    let status = harness::wait_status(&runtime, 3000)?;
    let startup_ms = begin.elapsed().as_secs_f64() * 1000.0;
    check(
        status.get("running").ok().and_then(|v| v.as_bool()) == Some(true),
        "engine reported not running",
    )?;
    check(
        harness::status_number(&status, "rules") == 1.0,
        "expected exactly 1 rule after initial load",
    )?;

    // 打开参数开关 → 共享工厂真的换了文字
    let answer = harness::command(&runtime, 1, "set", Some("{\"names\":false,\"parameters\":true}"))?;
    check(
        harness::response_ok(&answer),
        &format!("set command rejected: {}", harness::describe(&answer)),
    )?;
    let translated = harness::measure(&factory, &format, &source)?;
    check(
        (translated - original).abs() > 0.1,
        "shared factory did not translate",
    )?;

    // 坏词典重载必须失败，且旧快照继续生效
    harness::write_dictionary(root, "parameters", false, "{broken")?;
    let answer = harness::command(&runtime, 2, "reload", None)?;
    check(!harness::response_ok(&answer), "broken dictionary was accepted")?;
    check(
        (harness::measure(&factory, &format, &source)? - translated).abs() < 0.001,
        "invalid reload changed the active snapshot",
    )?;

    // 自定义词典覆盖基础词库
    harness::write_dictionary(root, "parameters", false, "{\"Intensity\":\"强度调整\"}")?;
    let answer = harness::command(&runtime, 3, "reload", None)?;
    check(harness::response_ok(&answer), "custom dictionary reload failed")?;
    let overridden = harness::measure(&factory, &format, &source)?;
    check(
        (overridden - translated).abs() > 0.1,
        "custom override did not take effect",
    )?;

    // 4 线程 × 500 次排版，期间做 10 次重载
    let failures = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let mut readers = Vec::new();
    for _ in 0..4 {
        let factory = factory.clone();
        let format = format.clone();
        let failures = failures.clone();
        let text = source.clone();
        readers.push(std::thread::spawn(move || {
            for _ in 0..500 {
                match harness::measure(&factory, &format, &text) {
                    Ok(width) if width > 0.0 => {}
                    _ => {
                        failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }));
    }
    for sequence in 10..20 {
        let answer = harness::command(&runtime, sequence, "reload", None)?;
        check(harness::response_ok(&answer), "reload during concurrency failed")?;
    }
    for reader in readers {
        reader.join().map_err(|_| "reader thread panicked".to_owned())?;
    }
    check(
        failures.load(std::sync::atomic::Ordering::Relaxed) == 0,
        "concurrent layouts failed during reload",
    )?;

    // 导出计数器应记录到这些真实命中（2000 次排版 + 前面的少量单次测量）。
    let counted = (native.successful_layouts)();
    check(counted >= 2000, "hit counter did not track translated layouts")?;

    // 关闭 → 恢复原文
    let answer = harness::command(&runtime, 30, "stop", None)?;
    check(harness::response_ok(&answer), "stop failed")?;
    check(
        (harness::measure(&factory, &format, &source)? - original).abs() < 0.001,
        "stop did not restore the original renderer",
    )?;

    // 停止后还能再开
    let answer = harness::command(&runtime, 31, "set", Some("{\"names\":false,\"parameters\":true}"))?;
    check(harness::response_ok(&answer), "restart after stop failed")?;
    check(
        (harness::measure(&factory, &format, &source)? - original).abs() > 0.1,
        "re-enable did not translate",
    )?;

    // 两个开关都关 → 恢复原文
    let answer = harness::command(&runtime, 32, "set", Some("{\"names\":false,\"parameters\":false}"))?;
    check(harness::response_ok(&answer), "disable failed")?;
    let final_width = harness::measure(&factory, &format, &source)?;
    check(
        (final_width - original).abs() < 0.001,
        "disabling both modes did not restore the original",
    )?;

    let after_memory = harness::private_usage();
    let hits = harness::status_number(&harness::wait_status(&runtime, 2000)?, "hits");

    println!(
        "PASS backend: shared DirectWrite hook, widths {:.2}/{:.2}/{:.2}, startup {:.0} ms, private delta {} bytes, invalid reload rollback, custom override, 2000 layouts during 10 reloads, stop/re-enable, hits {:.0}",
        original,
        translated,
        final_width,
        startup_ms,
        after_memory as i64 - before_memory as i64,
        hits
    );
    println!("      hit counter exported by the DLL: {}", counted);
    Ok(())
}
