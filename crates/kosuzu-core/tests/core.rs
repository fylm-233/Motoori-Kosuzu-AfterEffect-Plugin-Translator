//! 内核行为测试。用例刻意照抄 C++ 侧 LookupCheck / CapacityCheck / BackendCheck 覆盖的点，
//! 便于证明「换语言不换行为」。

use std::collections::BTreeMap;

use kosuzu_core::dict::{self, Rule};
use kosuzu_core::json::{self, Value};
use kosuzu_core::lookup;
use kosuzu_core::protocol::{self, Method, Modes};
use kosuzu_core::utf;

/// 查表引擎是进程级单例（DLL 只服务一个宿主进程），因此涉及它的用例必须串行。
fn lookup_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn rule(source: &str, target: &str, category: u32) -> Rule {
    Rule {
        source: utf::to_utf16(source),
        target: utf::to_utf16(target),
        category,
    }
}

fn translate(source: &str) -> (i32, String) {
    let src = utf::to_utf16(source);
    let mut out = vec![0u16; 64];
    let code = lookup::translate(&src, &mut out);
    let end = out.iter().position(|unit| *unit == 0).unwrap_or(out.len());
    (code, utf::to_utf8(&out[..end]).unwrap_or_default())
}

#[test]
fn json_rejects_the_same_malformed_inputs() {
    // 与 native/CapacityCheck.cpp 的拒绝清单逐条对齐。
    for text in ["01", "1.", "1e", "NaN", "{\"x\":true,}", "\"\\ud800\""] {
        assert!(
            json::parse_str(text).is_err(),
            "should reject: {}",
            text
        );
    }
    // 额外的畸形样本
    for text in ["", "{", "[1,]", "-", "1.2.3", "\"\\udc00\"", "{\"a\" 1}"] {
        assert!(json::parse_str(text).is_err(), "should reject: {}", text);
    }
}

#[test]
fn json_accepts_valid_documents() {
    let value = json::parse_str("\u{feff}{\"a\":\"强度\",\"b\":[1,true,null],\"c\":-1.5e3}").unwrap();
    let map = value.as_object().unwrap();
    assert_eq!(map.get("a").unwrap().as_str(), Some("强度"));
    assert_eq!(map.get("c").unwrap().as_f64(), Some(-1500.0));
    // 代理对
    let value = json::parse_str("\"\\ud83d\\ude00\"").unwrap();
    assert_eq!(value.as_str(), Some("😀"));
    // 重复键：后值覆盖前值（与原 std::map 行为一致）
    let value = json::parse_str("{\"k\":\"1\",\"k\":\"2\"}").unwrap();
    assert_eq!(value.as_object().unwrap().get("k").unwrap().as_str(), Some("2"));
}

#[test]
fn json_depth_is_bounded() {
    let deep = "[".repeat(40) + &"]".repeat(40);
    assert!(json::parse_str(&deep).is_err());
    let ok = "[".repeat(10) + &"]".repeat(10);
    assert!(json::parse_str(&ok).is_ok());
}

#[test]
fn json_quote_roundtrip() {
    for text in ["强度", "a\"b\\c", "line\nbreak", "\u{1}", "😀=中"] {
        let encoded = json::quote(text);
        let decoded = json::parse_str(&encoded).unwrap();
        assert_eq!(decoded.as_str(), Some(text), "roundtrip {}", text);
    }
}

fn write_dict(dir: &std::path::Path, kind: &str, base: bool, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let path = dict::dictionary_path(dir, kind, base);
    std::fs::write(path, body).unwrap();
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kosuzu-rust-test-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn dictionary_merge_prefers_custom_and_drops_noise() {
    let dir = temp_dir("dict");
    write_dict(&dir, "names", true, "{\"Effect\":\"效果\"}");
    write_dict(&dir, "names", false, "{\"Effect\":\"效果改名\",\"Same\":\"Same\"}");
    write_dict(&dir, "parameters", true, "{\"Intensity\":\"强度\",\"Blank\":\"   \"}");
    write_dict(&dir, "parameters", false, "{}");

    let rules = dict::load_rules(&dir).unwrap();
    let names: Vec<&Rule> = rules.iter().filter(|r| r.category == 1).collect();
    let params: Vec<&Rule> = rules.iter().filter(|r| r.category == 2).collect();

    assert_eq!(names.len(), 1, "Same 应为原文==译文被过滤");
    assert_eq!(utf::to_utf8(&names[0].target).unwrap(), "效果改名");
    assert_eq!(params.len(), 1, "全空白译文应被过滤");
    assert_eq!(utf::to_utf8(&params[0].target).unwrap(), "强度");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dictionary_rejects_reserved_keys_and_broken_json() {
    let dir = temp_dir("dict-bad");
    write_dict(&dir, "names", true, "{\"__proto__\":\"x\"}");
    write_dict(&dir, "names", false, "{}");
    write_dict(&dir, "parameters", true, "{}");
    write_dict(&dir, "parameters", false, "{}");
    assert!(dict::load_rules(&dir).is_err());

    write_dict(&dir, "names", true, "{\"a\":1}");
    assert!(dict::load_rules(&dir).is_err());

    write_dict(&dir, "names", true, "{broken");
    assert!(dict::load_rules(&dir).is_err());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn lookup_modes_and_codes_match_cpp() {
    let _serial = lookup_lock();
    let rules = vec![
        rule("Intensity", "Strength", dict::CATEGORY_PARAMETER),
        rule("Effect", "Name", dict::CATEGORY_NAME),
    ];
    lookup::set_rules(&rules).unwrap();

    lookup::set_modes(0);
    assert_eq!(translate("Intensity").0, 0);

    lookup::set_modes(2);
    assert_eq!(translate("Intensity"), (1, "Strength".to_owned()));
    assert_eq!(translate("Effect").0, 0, "名称开关关闭时不替换名称");
    assert_eq!(translate("intensity").0, 0, "大小写敏感");

    // 缓冲区不足 → 3
    let src = utf::to_utf16("Intensity");
    let mut small = [0u16; 2];
    assert_eq!(lookup::translate(&src, &mut small), 3);

    lookup::set_modes(1);
    assert_eq!(translate("Effect"), (1, "Name".to_owned()));
    assert_eq!(translate("Intensity").0, 0);

    // 同一原文两种译文 → 2（拒绝替换，不猜）
    let rules = vec![
        rule("Dup", "Name-A", dict::CATEGORY_NAME),
        rule("Dup", "Param-B", dict::CATEGORY_PARAMETER),
    ];
    lookup::set_rules(&rules).unwrap();
    lookup::set_modes(3);
    assert_eq!(translate("Dup").0, 2);

    lookup::set_modes(0);
    lookup::clear_rules();
}

#[test]
fn lookup_rejects_invalid_rules_atomically() {
    let _serial = lookup_lock();
    let good = vec![rule("A", "甲", dict::CATEGORY_NAME)];
    lookup::set_rules(&good).unwrap();
    assert_eq!(lookup::rule_count(), 1);

    let mut bad = vec![rule("B", "乙", 9)];
    assert!(lookup::set_rules(&bad).is_err());
    assert_eq!(lookup::rule_count(), 1, "失败时旧快照必须保留");

    bad[0].category = dict::CATEGORY_NAME;
    bad[0].target = vec![0u16];
    assert!(lookup::set_rules(&bad).is_err());
    assert_eq!(lookup::rule_count(), 1);

    lookup::clear_rules();
}

#[test]
fn lookup_survives_concurrent_reloads() {
    let _serial = lookup_lock();
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    let rules = vec![rule("Intensity", "强度", dict::CATEGORY_PARAMETER)];
    lookup::set_rules(&rules).unwrap();
    lookup::set_modes(2);

    let stop = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(AtomicUsize::new(0));
    let mut readers = Vec::new();
    for _ in 0..4 {
        let stop = stop.clone();
        let errors = errors.clone();
        let rules = rules.clone();
        readers.push(std::thread::spawn(move || {
            let src = utf::to_utf16("Intensity");
            let mut out = vec![0u16; 64];
            while !stop.load(Ordering::Acquire) {
                let _ = &rules;
                if lookup::translate(&src, &mut out) != 1 {
                    errors.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for _ in 0..200 {
        lookup::set_rules(&rules).unwrap();
    }
    stop.store(true, Ordering::Release);
    for reader in readers {
        reader.join().unwrap();
    }
    assert_eq!(errors.load(Ordering::Relaxed), 0);
    lookup::set_modes(0);
    lookup::clear_rules();
}

#[test]
fn protocol_request_parsing() {
    assert!(protocol::valid_id("m9k-abc123"));
    assert!(!protocol::valid_id(""));
    assert!(!protocol::valid_id("a/b"));
    assert!(!protocol::valid_id(&"x".repeat(101)));

    let now = 1_700_000_000_000u64;
    let body = format!(
        "{{\"id\":\"r1\",\"created\":{},\"method\":\"set\",\"state\":{{\"names\":true,\"parameters\":false}}}}",
        now - 10
    );
    let request = protocol::parse_request("r1", body.as_bytes(), now).unwrap();
    assert_eq!(request.method, Method::Set(Modes { names: true, parameters: false }));
    assert_eq!(request.method.bits(), Some(1));

    // id 不一致
    assert!(protocol::parse_request("r2", body.as_bytes(), now).is_err());
    // 过期
    assert!(protocol::parse_request("r1", body.as_bytes(), now + 31_000).is_err());
    // 模式不是布尔
    let body = format!("{{\"id\":\"r1\",\"created\":{},\"method\":\"set\",\"state\":{{\"names\":1,\"parameters\":false}}}}", now);
    assert!(protocol::parse_request("r1", body.as_bytes(), now).is_err());
    // 未知方法
    let body = format!("{{\"id\":\"r1\",\"created\":{},\"method\":\"explode\"}}", now);
    assert!(protocol::parse_request("r1", body.as_bytes(), now).is_err());
}

#[test]
fn protocol_status_shape_matches_panel() {
    let status = protocol::Status {
        running: true,
        pid: 4321,
        heartbeat: 99,
        modes: 2,
        rules: 17_512,
        generation: 3,
        hits: 264,
        error: String::new(),
    };
    let text = status.to_json();
    let value = json::parse_str(&text).unwrap();
    // 面板实际读取的字段
    assert_eq!(value.get("running").unwrap().as_bool(), Some(true));
    assert_eq!(value.get("pid").unwrap().as_f64(), Some(4321.0));
    assert_eq!(value.get("rules").unwrap().as_f64(), Some(17512.0));
    let state = value.get("state").unwrap();
    assert_eq!(state.get("names").unwrap().as_bool(), Some(false));
    assert_eq!(state.get("parameters").unwrap().as_bool(), Some(true));
    assert!(matches!(value.get("engine").unwrap(), Value::String(_)));
}

#[test]
fn response_json_is_parseable_by_strict_codec() {
    let status = protocol::Status {
        running: false,
        pid: 1,
        heartbeat: 2,
        modes: 0,
        rules: 0,
        generation: 0,
        hits: 0,
        error: "AE blocked \"native\" startup\n".to_owned(),
    };
    let text = protocol::response_json("abc", false, &status.error, &status);
    let value = json::parse_str(&text).unwrap();
    assert_eq!(value.get("id").unwrap().as_str(), Some("abc"));
    assert_eq!(value.get("ok").unwrap().as_bool(), Some(false));
    assert_eq!(value.get("error").unwrap().as_str(), Some(status.error.as_str()));
}

#[test]
fn dictionary_maps_keep_insertion_contract() {
    // 确认 BTreeMap 输出顺序稳定，导出 JSON 时可复现。
    let mut map = BTreeMap::new();
    map.insert("b".to_owned(), "乙".to_owned());
    map.insert("a".to_owned(), "甲".to_owned());
    let keys: Vec<&String> = map.keys().collect();
    assert_eq!(keys, vec!["a", "b"]);
}
