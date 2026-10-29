//! `KosuzuSelfCheck.exe` —— Rust 版后端的验收工具。
//!
//! 四个子命令对应原 C++ 的四个自检程序：
//!
//! ```text
//! KosuzuSelfCheck lookup   <dll>            # 原 LookupCheck
//! KosuzuSelfCheck renderer <dll>            # 原 RendererCheck
//! KosuzuSelfCheck backend  <dll> <root>     # 原 BackendCheck（真实共享 DirectWrite 工厂）
//! KosuzuSelfCheck capacity <dll> <root>     # 原 CapacityCheck（真实全量词典）
//! KosuzuSelfCheck all      <dll> <root>     # 前两项 + backend
//! ```
//!
//! 退出码 0 = 全部通过，1 = 有断言失败，2 = 用法错误。

mod backend;
mod capacity;
mod dll;
mod harness;
mod lookup;
mod renderer;

use std::path::{Path, PathBuf};

use dll::Native;

fn usage() -> ! {
    eprintln!(
        "usage: KosuzuSelfCheck <lookup|renderer|backend|capacity|all> <dll> [root]"
    );
    std::process::exit(2);
}

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() < 3 {
        usage();
    }
    let action = arguments[1].as_str();
    let dll: PathBuf = PathBuf::from(&arguments[2]);
    let root: Option<PathBuf> = arguments.get(3).map(PathBuf::from);

    let result = run_action(action, &dll, root.as_deref());
    match result {
        Ok(()) => std::process::exit(0),
        Err(message) => {
            eprintln!("FAIL {}", message);
            std::process::exit(1);
        }
    }
}

fn require_root(root: Option<&Path>) -> Result<PathBuf, String> {
    match root {
        Some(path) => {
            std::fs::create_dir_all(path).map_err(|error| error.to_string())?;
            Ok(path.to_path_buf())
        }
        None => Err("this action needs a test root directory".to_owned()),
    }
}

fn run_action(action: &str, dll: &Path, root: Option<&Path>) -> Result<(), String> {
    let native = Native::load(dll)?;
    match action {
        "lookup" => lookup::run(&native),
        "renderer" => renderer::run(&native),
        "backend" => backend::run(&native, &require_root(root)?),
        "capacity" => capacity::run(&native, &require_root(root)?),
        "all" => {
            lookup::run(&native)?;
            renderer::run(&native)?;
            backend::run(&native, &require_root(root)?)
        }
        _ => usage(),
    }
}
