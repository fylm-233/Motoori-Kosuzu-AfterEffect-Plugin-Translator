//! 对应原 `native/CapacityCheck.cpp`：用真实全量词典测启动耗时、内存增量、空闲 CPU。

use std::ffi::c_void;
use std::path::Path;

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
    // 畸形 JSON 必须被拒（与原 CapacityCheck 的样本一致）
    for text in ["01", "1.", "1e", "NaN", "{\"x\":true,}", "\"\\ud800\""] {
        check(
            kosuzu_core::json::parse_str(text).is_err(),
            "malformed JSON accepted",
        )?;
    }

    let before = harness::private_usage();
    let begin = std::time::Instant::now();
    let root_units = kosuzu_winutil::wide(&root.to_string_lossy());
    let started = unsafe { (native.start)(root_units.as_ptr() as *mut c_void) };
    check(started == 1, "LB_Start failed")?;

    let runtime = harness::runtime_dir(root);
    let status = harness::wait_status(&runtime, 5000)?;
    let startup_ms = begin.elapsed().as_secs_f64() * 1000.0;
    check(
        status.get("running").ok().and_then(|v| v.as_bool()) == Some(true),
        "engine reported not running",
    )?;
    let after = harness::private_usage();

    let cpu_before = harness::cpu_time_ms();
    std::thread::sleep(std::time::Duration::from_millis(3000));
    let idle = harness::cpu_time_ms().saturating_sub(cpu_before);

    let rules = harness::status_number(&status, "rules");
    check(rules > 0.0, "no rules loaded")?;

    println!(
        "PASS capacity: rules={:.0}, startup={:.0} ms, private_delta={} bytes, idle_cpu_3s={} ms (local fixture only)",
        rules,
        startup_ms,
        after as i64 - before as i64,
        idle
    );
    Ok(())
}
