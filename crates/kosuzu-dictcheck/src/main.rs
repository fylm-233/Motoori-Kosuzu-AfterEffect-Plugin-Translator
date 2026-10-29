//! `KosuzuDictionaryCheck.exe`
//!
//! 安装脚本在把用户词典迁进新版本之前先跑一遍它：退出码非 0 就停止覆盖，保留原文件。
//! 退出码与原 C++ 实现逐条对齐（安装脚本 `Install-AE-Panel.ps1` 只看 0/非 0，但排错时靠码值定位）。
//!
//! | 码 | 含义 |
//! |----|------|
//! | 0 | 通过 |
//! | 1 | JSON 解析失败 |
//! | 2 | 参数个数不对 |
//! | 3 | 文件超过 10 MiB |
//! | 4 | 文件不可读 |
//! | 5 | 顶层不是对象 |
//! | 6 | 键为空/保留字，或值不是字符串 |
//! | 7 | 原文或译文过长，或含 NUL |

use std::path::PathBuf;

use kosuzu_core::dict::MAX_RULE_UNITS;
use kosuzu_core::json;
use kosuzu_core::utf;

const RESERVED_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

fn check(path: &PathBuf) -> i32 {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return 4,
    };
    if metadata.len() > kosuzu_winutil::MAX_TEXT_BYTES as u64 {
        return 3;
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return 4,
    };
    let value = match json::parse(&bytes) {
        Ok(value) => value,
        Err(_) => return 1,
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => return 5,
    };
    for (key, value) in object {
        let text = match value.as_str() {
            Some(text) => text,
            None => return 6,
        };
        if key.is_empty() || RESERVED_KEYS.contains(&key.as_str()) {
            return 6;
        }
        let source = utf::to_utf16(key);
        let target = utf::to_utf16(text);
        if source.len() > MAX_RULE_UNITS as usize
            || target.len() > MAX_RULE_UNITS as usize
            || utf::has_nul(&source)
            || utf::has_nul(&target)
        {
            return 7;
        }
    }
    0
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 2 {
        std::process::exit(2);
    }
    std::process::exit(check(&PathBuf::from(&arguments[1])));
}
