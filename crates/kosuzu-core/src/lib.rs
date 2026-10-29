//! Kosuzu 纯逻辑内核。
//!
//! 这一层不引用任何 Win32 API，因此可以在任意主机上 `cargo test`。
//! 与原 C++ 实现（native/Json.h、native/Lookup.cpp、native/Backend.cpp）逐条对齐语义，
//! 便于用同一份测试用例双向比对。

pub mod dict;
pub mod error;
pub mod json;
pub mod lookup;
pub mod protocol;
pub mod utf;

pub use error::{Error, Result};
