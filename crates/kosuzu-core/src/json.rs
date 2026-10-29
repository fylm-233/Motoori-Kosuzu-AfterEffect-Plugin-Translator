//! 严格 JSON 解析/序列化。
//!
//! 为什么不用 serde_json：
//! 1. 协议对“畸形输入”的拒绝行为是被测试覆盖的（`01`、`1.`、`1e`、`NaN`、尾逗号、孤立代理项），
//!    自己实现可以逐条对齐原 C++ `Json.h` 的语义，而不是去赌第三方库的宽松程度。
//! 2. 注入型 DLL 里少一个 proc-macro 依赖，构建链更短，二进制更小。
//!
//! 与原实现的唯一有意差异：本实现接受 JSON 数组（原 `Json.h` 不支持数组）。
//! 协议与词典格式都不含数组，因此属于安全的超集。

use std::collections::BTreeMap;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// 取字段；缺失时报 `Missing field <key>`（与原实现的 `at()` 一致）。
    pub fn get(&self, key: &str) -> Result<&Value> {
        match self {
            Value::Object(map) => map
                .get(key)
                .ok_or_else(|| Error::new(format!("Missing field {}", key))),
            _ => Err(Error::new(format!("Missing field {}", key))),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
}

const MAX_DEPTH: u32 = 32;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn fail<T>(&self) -> Result<T> {
        Err(Error::new(format!("Invalid JSON at {}", self.i)))
    }

    fn take(&mut self) -> Result<u8> {
        if self.i >= self.s.len() {
            return self.fail();
        }
        let byte = self.s[self.i];
        self.i += 1;
        Ok(byte)
    }

    fn skip_ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }

    fn hex4(&mut self) -> Result<u32> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let c = self.take()?;
            let digit = match c {
                b'0'..=b'9' => (c - b'0') as u32,
                b'a'..=b'f' => (c - b'a') as u32 + 10,
                b'A'..=b'F' => (c - b'A') as u32 + 10,
                _ => return self.fail(),
            };
            value = (value << 4) | digit;
        }
        Ok(value)
    }

    fn string(&mut self) -> Result<String> {
        if self.take()? != b'"' {
            return self.fail();
        }
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.take()?;
            match c {
                b'"' => {
                    // 与原实现一致：字符串必须是合法 UTF-8，否则报 Invalid UTF8。
                    return match std::str::from_utf8(&out) {
                        Ok(text) => Ok(text.to_owned()),
                        Err(_) => Err(Error::new("Invalid UTF8")),
                    };
                }
                0x00..=0x1f => return self.fail(),
                b'\\' => {
                    let escape = self.take()?;
                    match escape {
                        b'"' | b'\\' | b'/' => out.push(escape),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                if self.take()? != b'\\' || self.take()? != b'u' {
                                    return self.fail();
                                }
                                let second = self.hex4()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return self.fail();
                                }
                                0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00)
                            } else if (0xdc00..=0xdfff).contains(&first) {
                                // 孤立低位代理项：C++ 侧 WC_ERR_INVALID_CHARS 报 Invalid UTF16。
                                return Err(Error::new("Invalid UTF16"));
                            } else {
                                first
                            };
                            let ch = match char::from_u32(scalar) {
                                Some(ch) => ch,
                                None => return self.fail(),
                            };
                            let mut buffer = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                        }
                        _ => return self.fail(),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn number(&mut self) -> Result<f64> {
        let begin = self.i;
        if self.i < self.s.len() && self.s[self.i] == b'-' {
            self.i += 1;
        }
        if self.i >= self.s.len() {
            return self.fail();
        }
        if self.s[self.i] == b'0' {
            self.i += 1;
        } else if (b'1'..=b'9').contains(&self.s[self.i]) {
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
        } else {
            return self.fail();
        }
        if self.i < self.s.len() && self.s[self.i] == b'.' {
            self.i += 1;
            let digits = self.i;
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
            if self.i == digits {
                return self.fail();
            }
        }
        if self.i < self.s.len() && (self.s[self.i] == b'e' || self.s[self.i] == b'E') {
            self.i += 1;
            if self.i < self.s.len() && (self.s[self.i] == b'+' || self.s[self.i] == b'-') {
                self.i += 1;
            }
            let digits = self.i;
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
            if self.i == digits {
                return self.fail();
            }
        }
        let text = match std::str::from_utf8(&self.s[begin..self.i]) {
            Ok(text) => text,
            Err(_) => return self.fail(),
        };
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => Ok(value),
            _ => self.fail(),
        }
    }

    fn value(&mut self, depth: u32) -> Result<Value> {
        if depth > MAX_DEPTH {
            return self.fail();
        }
        self.skip_ws();
        let c = if self.i < self.s.len() {
            self.s[self.i]
        } else {
            0
        };
        match c {
            b'{' => {
                self.i += 1;
                let mut map = BTreeMap::new();
                self.skip_ws();
                if self.i < self.s.len() && self.s[self.i] == b'}' {
                    self.i += 1;
                    return Ok(Value::Object(map));
                }
                loop {
                    self.skip_ws();
                    let key = self.string()?;
                    self.skip_ws();
                    if self.take()? != b':' {
                        return self.fail();
                    }
                    let value = self.value(depth + 1)?;
                    map.insert(key, value);
                    self.skip_ws();
                    match self.take()? {
                        b'}' => return Ok(Value::Object(map)),
                        b',' => continue,
                        _ => return self.fail(),
                    }
                }
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.i < self.s.len() && self.s[self.i] == b']' {
                    self.i += 1;
                    return Ok(Value::Array(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    self.skip_ws();
                    match self.take()? {
                        b']' => return Ok(Value::Array(items)),
                        b',' => continue,
                        _ => return self.fail(),
                    }
                }
            }
            b'"' => return Ok(Value::String(self.string()?)),
            _ => {}
        }
        if self.s[self.i..].starts_with(b"true") {
            self.i += 4;
            return Ok(Value::Bool(true));
        }
        if self.s[self.i..].starts_with(b"false") {
            self.i += 5;
            return Ok(Value::Bool(false));
        }
        if self.s[self.i..].starts_with(b"null") {
            self.i += 4;
            return Ok(Value::Null);
        }
        Ok(Value::Number(self.number()?))
    }
}

/// 解析一段 JSON 字节。前导 UTF-8 BOM 会被跳过。
pub fn parse(bytes: &[u8]) -> Result<Value> {
    let body = if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        &bytes[3..]
    } else {
        bytes
    };
    let mut parser = Parser { s: body, i: 0 };
    let value = parser.value(0)?;
    parser.skip_ws();
    if parser.i != parser.s.len() {
        return parser.fail();
    }
    Ok(value)
}

pub fn parse_str(text: &str) -> Result<Value> {
    parse(text.as_bytes())
}

/// JSON 字符串字面量转义。只转义 `"`、`\` 与控制字符，非 ASCII 原样保留。
pub fn quote(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out: Vec<u8> = Vec::with_capacity(text.len() + 2);
    out.push(b'"');
    for byte in text.bytes() {
        match byte {
            b'"' | b'\\' => {
                out.push(b'\\');
                out.push(byte);
            }
            0x00..=0x1f => {
                out.extend_from_slice(b"\\u00");
                out.push(HEX[(byte >> 4) as usize]);
                out.push(HEX[(byte & 0x0f) as usize]);
            }
            other => out.push(other),
        }
    }
    out.push(b'"');
    // 输入是合法 &str，只插入了 ASCII 转义序列，因此这里必然合法；仍不 panic。
    String::from_utf8(out).unwrap_or_default()
}

/// 把布尔渲染成 JSON 字面量。
pub fn bool_literal(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}
