//! UTF-16 与 UTF-8 之间的转换及词典判定用小工具。
//!
//! 之所以把词条一律保存成 UTF-16，是因为 DirectWrite 回调用的是 `WCHAR`；
//! 查表时若每次都要 UTF-8 ↔ UTF-16 转换，绘制路径上就会产生分配。

use crate::{Error, Result};

/// UTF-8 → UTF-16。
pub fn to_utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// UTF-16 → UTF-8。遇到孤立代理项时报错（对应 C++ 的 WC_ERR_INVALID_CHARS）。
pub fn to_utf8(text: &[u16]) -> Result<String> {
    String::from_utf16(text).map_err(|_| Error::new("Invalid UTF16"))
}

/// 是否含 NUL。含 NUL 的词条会让 `wcslen` 与真实长度不一致，必须拒绝。
pub fn has_nul(text: &[u16]) -> bool {
    text.contains(&0)
}

/// 是否只有空白（空格、CR、LF、TAB）。对应 C++ 的 `find_first_not_of(L" \r\n\t")`。
pub fn is_blank(text: &[u16]) -> bool {
    text.iter()
        .all(|c| matches!(c, 0x20 | 0x0d | 0x0a | 0x09))
}
