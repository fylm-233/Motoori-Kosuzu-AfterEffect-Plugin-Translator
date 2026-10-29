//! 对应原 `native/LookupCheck.cpp`：模式位、精确匹配、缓冲区、并发重载、查表开销。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::dll::{Native, RawRule};

pub struct Owned {
    pub source: Vec<u16>,
    pub target: Vec<u16>,
    pub category: u32,
}

impl Owned {
    pub fn new(source: &str, target: &str, category: u32) -> Self {
        Owned {
            source: source.encode_utf16().collect(),
            target: target.encode_utf16().collect(),
            category,
        }
    }

    pub fn raw(&self) -> RawRule {
        RawRule {
            source: self.source.as_ptr(),
            source_length: self.source.len() as u32,
            target: self.target.as_ptr(),
            target_length: self.target.len() as u32,
            category: self.category,
        }
    }
}

fn check(condition: bool, message: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned())
    }
}

fn translate(native: &Native, source: &str) -> (i32, String) {
    let units = Owned::new(source, "", 0).source;
    let mut out = vec![0u16; 64];
    let code = unsafe { (native.translate)(units.as_ptr(), units.len() as u32, out.as_mut_ptr(), 64) };
    let end = out.iter().position(|unit| *unit == 0).unwrap_or(out.len());
    (code, String::from_utf16_lossy(&out[..end]))
}

pub fn run(native: &Native) -> Result<(), String> {
    let rules = vec![
        Owned::new("Intensity", "Strength", 2),
        Owned::new("Effect", "Name", 1),
    ];
    let raws: Vec<RawRule> = rules.iter().map(Owned::raw).collect();
    check(
        unsafe { (native.set_rules)(raws.as_ptr(), raws.len() as u32) } == 0,
        "LB_SetRules rejected valid rules",
    )?;

    (native.set_modes)(0);
    check(translate(native, "Intensity").0 == 0, "mode 0 must not translate")?;

    (native.set_modes)(2);
    let (code, text) = translate(native, "Intensity");
    check(code == 1 && text == "Strength", "parameter translation failed")?;
    check(translate(native, "Effect").0 == 0, "name must stay when only parameters on")?;
    check(translate(native, "intensity").0 == 0, "matching must be case sensitive")?;

    // 缓冲区不足 → 3
    let source = &rules[0].source;
    let mut tiny = [0u16; 2];
    check(
        unsafe { (native.translate)(source.as_ptr(), source.len() as u32, tiny.as_mut_ptr(), 2) } == 3,
        "small buffer must report 3",
    )?;

    (native.set_modes)(1);
    check(translate(native, "Effect").0 == 1, "name translation failed")?;
    check(translate(native, "Intensity").0 == 0, "parameter must stay when only names on")?;

    // 1000 次重载期间并发查表
    (native.set_modes)(2);
    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(AtomicU64::new(0));
    let reader = {
        let stop = stop.clone();
        let errors = errors.clone();
        let source = rules[0].source.clone();
        // 只把函数指针搬进线程：Native 句柄本身不是 Send。
        let translate = native.translate;
        std::thread::spawn(move || {
            let mut out = [0u16; 64];
            while !stop.load(Ordering::Acquire) {
                let code =
                    unsafe { translate(source.as_ptr(), source.len() as u32, out.as_mut_ptr(), 64) };
                if code != 1 {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        })
    };
    for _ in 0..1000 {
        let raws: Vec<RawRule> = rules.iter().map(Owned::raw).collect();
        check(
            unsafe { (native.set_rules)(raws.as_ptr(), raws.len() as u32) } == 0,
            "reload failed",
        )?;
    }
    stop.store(true, Ordering::Release);
    reader.join().map_err(|_| "reader panicked".to_owned())?;
    check(errors.load(Ordering::Relaxed) == 0, "concurrent lookups lost the table")?;

    // 1,000,000 次查表开销
    const ITERATIONS: u32 = 1_000_000;
    let mut out = [0u16; 64];
    let begin = std::time::Instant::now();
    let mut total = 0i64;
    for _ in 0..ITERATIONS {
        total += unsafe {
            (native.translate)(source.as_ptr(), source.len() as u32, out.as_mut_ptr(), 64)
        } as i64;
    }
    let elapsed = begin.elapsed();
    check(total == ITERATIONS as i64, "some lookups missed")?;
    (native.set_modes)(0);

    println!(
        "PASS lookup: modes, exact match, buffers, 1000 concurrent reloads; {} lookups in {:.1} ms ({:.0} ns each)",
        ITERATIONS,
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1e9 / ITERATIONS as f64
    );
    Ok(())
}
