use std::fmt;

/// 统一错误类型。错误文本沿用原 C++ 版本的字面量，
/// 因为这些字符串会经 status.json 显示到面板提示上，替换实现时不应让用户看到不同的措辞。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Error(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error(format!("IO error: {}", value))
    }
}

impl From<std::string::FromUtf16Error> for Error {
    fn from(_: std::string::FromUtf16Error) -> Self {
        Error::new("Invalid UTF16")
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// 构造错误的简写。
macro_rules! bail {
    ($($arg:tt)*) => { return Err($crate::error::Error::new(format!($($arg)*))) };
}
pub(crate) use bail;
