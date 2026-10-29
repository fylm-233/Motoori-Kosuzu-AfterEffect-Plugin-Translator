//! 词典读取与合并。
//!
//! 合并顺序与原实现一致：先基础词库 `Kosuzu.base.<kind>.json`，再自定义 `Kosuzu.<kind>.json`，
//! 后者覆盖前者；空的译文不会覆盖已有译文。合并后过滤掉「原文==译文」与「译文全是空白」的条目。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::bail;
use crate::json::{self, Value};
use crate::{utf, Error, Result};

/// 单个词典文件的大小上限（与 C++ 的 10 MiB 一致）。
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// 单条原文/译文的最大 UTF-16 长度。
pub const MAX_RULE_UNITS: u32 = 8192;
/// 单次装载的最大词条数。
pub const MAX_RULES: u32 = 100_000;

pub const CATEGORY_NAME: u32 = 1;
pub const CATEGORY_PARAMETER: u32 = 2;

const RESERVED_KEYS: [&str; 3] = ["__proto__", "constructor", "prototype"];

/// 一条已就绪的替换规则，文本已经是 UTF-16。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub source: Vec<u16>,
    pub target: Vec<u16>,
    /// 1 = 插件名称，2 = 参数。
    pub category: u32,
}

/// 两种词典种类，顺序即装载顺序。
pub const KINDS: [(&str, u32); 2] = [("names", CATEGORY_NAME), ("parameters", CATEGORY_PARAMETER)];

pub fn dictionary_path(dir: &Path, kind: &str, base: bool) -> PathBuf {
    let name = if base {
        format!("Kosuzu.base.{}.json", kind)
    } else {
        format!("Kosuzu.{}.json", kind)
    };
    dir.join(name)
}

pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|_| Error::new("Cannot read file"))?;
    if metadata.len() > MAX_FILE_BYTES {
        bail!("File too large");
    }
    std::fs::read(path).map_err(|_| Error::new("Cannot read file"))
}

/// 解析并校验一份词典：必须是对象，键非空、非保留字，值必须是字符串。
pub fn parse_dictionary(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let value = json::parse(bytes)?;
    let object = match &value {
        Value::Object(map) => map,
        _ => bail!("Dictionary must be an object"),
    };
    let mut out = BTreeMap::new();
    for (key, value) in object {
        if key.is_empty() || RESERVED_KEYS.contains(&key.as_str()) || value.as_str().is_none() {
            bail!("Invalid dictionary entry");
        }
        if let Some(text) = value.as_str() {
            out.insert(key.clone(), text.to_owned());
        }
    }
    Ok(out)
}

pub fn load_dictionary(path: &Path) -> Result<BTreeMap<String, String>> {
    parse_dictionary(&read_file(path)?)
}

/// 解析并校验，但把字符串从解析树里**移动**出来而不是复制。
/// 17k 条词条时这能省掉三万余次分配，是启动耗时的主要来源之一。
fn parse_dictionary_owned(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let object = match json::parse(bytes)? {
        Value::Object(object) => object,
        _ => bail!("Dictionary must be an object"),
    };
    let mut out = BTreeMap::new();
    for (key, value) in object {
        let text = match value {
            Value::String(text) => text,
            _ => bail!("Invalid dictionary entry"),
        };
        if key.is_empty() || RESERVED_KEYS.contains(&key.as_str()) {
            bail!("Invalid dictionary entry");
        }
        out.insert(key, text);
    }
    Ok(out)
}

/// 装载 `<dir>` 下四个词典文件，产出可直接灌入查表引擎的规则表。
pub fn load_rules(dir: &Path) -> Result<Vec<Rule>> {
    let mut rules: Vec<Rule> = Vec::new();
    for (kind, category) in KINDS {
        let mut merged: BTreeMap<String, String> = BTreeMap::new();
        for base in [true, false] {
            let map = parse_dictionary_owned(&read_file(&dictionary_path(dir, kind, base))?)?;
            for (key, value) in map {
                if !value.is_empty() {
                    merged.insert(key, value);
                }
            }
        }
        for (source, target) in merged {
            let source = utf::to_utf16(&source);
            let target = utf::to_utf16(&target);
            if source == target || utf::is_blank(&target) {
                continue;
            }
            rules.push(Rule {
                source,
                target,
                category,
            });
        }
    }
    Ok(rules)
}

/// 校验一条规则是否可被引擎接受（对应 `LB_SetRules` 的逐条检查）。
pub fn validate_rule(rule: &Rule) -> bool {
    !rule.source.is_empty()
        && !rule.target.is_empty()
        && rule.source.len() <= MAX_RULE_UNITS as usize
        && rule.target.len() <= MAX_RULE_UNITS as usize
        && (rule.category == CATEGORY_NAME || rule.category == CATEGORY_PARAMETER)
        && !utf::has_nul(&rule.source)
        && !utf::has_nul(&rule.target)
}
