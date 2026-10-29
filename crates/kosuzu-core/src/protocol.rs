//! 文件式 IPC 协议：`runtime/ae-<pid>/requests/*.json` → `responses/*.json`，外加 `status.json` 心跳。
//!
//! 协议字段、错误文本、返回码全部沿用 0.8.x 的 C++ 实现，
//! 因此 `Kosuzu.jsx` 与 `dictionary-manager.jsxinc` 一行都不用改。

use crate::json::{self, bool_literal, Value};
use crate::{Error, Result};

/// 引擎标识。写进 status.json 的 `engine` 字段，便于在故障时区分是哪套实现。
pub const ENGINE_ID: &str = "native-dwrite-rust-0.1";

/// 请求的时间窗，超过即视为过期（毫秒）。
pub const REQUEST_TTL_MS: u64 = 30_000;
/// 单个请求文件的大小上限。
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modes {
    pub names: bool,
    pub parameters: bool,
}

impl Modes {
    pub fn bits(self) -> u32 {
        (self.names as u32) | ((self.parameters as u32) << 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Reload,
    Set(Modes),
    Stop,
    Status,
}

impl Method {
    /// `set` 请求的模式位；其他方法返回 `None`。
    pub fn bits(&self) -> Option<u32> {
        match self {
            Method::Set(modes) => Some(modes.bits()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: String,
    pub method: Method,
}

/// 引擎状态快照，直接渲染成 status.json 的内容。
#[derive(Debug, Clone)]
pub struct Status {
    pub running: bool,
    pub pid: u32,
    pub heartbeat: u64,
    pub modes: u32,
    pub rules: usize,
    pub generation: u64,
    pub hits: u64,
    pub error: String,
}

impl Status {
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(240);
        out.push_str("{\"running\":");
        out.push_str(bool_literal(self.running));
        out.push_str(",\"pid\":");
        out.push_str(&self.pid.to_string());
        out.push_str(",\"heartbeat\":");
        out.push_str(&self.heartbeat.to_string());
        out.push_str(",\"engine\":");
        out.push_str(&json::quote(ENGINE_ID));
        out.push_str(",\"state\":{\"names\":");
        out.push_str(bool_literal(self.modes & 1 != 0));
        out.push_str(",\"parameters\":");
        out.push_str(bool_literal(self.modes & 2 != 0));
        out.push_str("},\"rules\":");
        out.push_str(&self.rules.to_string());
        out.push_str(",\"generation\":");
        out.push_str(&self.generation.to_string());
        out.push_str(",\"hits\":");
        out.push_str(&self.hits.to_string());
        out.push_str(",\"error\":");
        out.push_str(&json::quote(&self.error));
        out.push('}');
        out
    }
}

/// 响应包：`{"id":..,"ok":..,"error":..,"result":{...}}`。
pub fn response_json(id: &str, ok: bool, error: &str, status: &Status) -> String {
    let mut out = String::with_capacity(300);
    out.push_str("{\"id\":");
    out.push_str(&json::quote(id));
    out.push_str(",\"ok\":");
    out.push_str(bool_literal(ok));
    out.push_str(",\"error\":");
    out.push_str(&json::quote(error));
    out.push_str(",\"result\":");
    out.push_str(&status.to_json());
    out.push('}');
    out
}

/// 请求文件名即请求 id：只允许 ASCII 字母数字与连字符，最长 100。
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

/// 解析请求体。`id` 来自文件名，必须与包内 `id` 字段一致，防止错配。
pub fn parse_request(id: &str, bytes: &[u8], now_ms: u64) -> Result<Request> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(Error::new("Request too large"));
    }
    let value = json::parse(bytes)?;

    let body_id = match value.get("id")?.as_str() {
        Some(text) => text,
        None => return Err(Error::new("Request id mismatch")),
    };
    if body_id != id {
        return Err(Error::new("Request id mismatch"));
    }

    let created = match value.get("created")? {
        Value::Number(number) => *number,
        _ => return Err(Error::new("Expired request")),
    };
    if (now_ms as f64 - created).abs() > REQUEST_TTL_MS as f64 {
        return Err(Error::new("Expired request"));
    }

    let method = match value.get("method")?.as_str() {
        Some(text) => text,
        None => return Err(Error::new("Invalid method")),
    };

    let method = match method {
        "reload" => Method::Reload,
        "stop" => Method::Stop,
        "status" => Method::Status,
        "set" => {
            let state = value.get("state")?;
            let names = state
                .get("names")?
                .as_bool()
                .ok_or_else(|| Error::new("Modes must be booleans"))?;
            let parameters = state
                .get("parameters")?
                .as_bool()
                .ok_or_else(|| Error::new("Modes must be booleans"))?;
            Method::Set(Modes { names, parameters })
        }
        _ => return Err(Error::new("Unknown method")),
    };

    Ok(Request {
        id: id.to_owned(),
        method,
    })
}
