//! 工作线程：装载词典、轮询请求目录、写心跳。
//!
//! 整个循环里没有任何 `unwrap`：注入进程里的 panic 等于让 AE 直接退出，
//! 因此所有失败都降级成 status.json 里的 `error` 文本。

use std::path::{Path, PathBuf};

use kosuzu_core::dict;
use kosuzu_core::lookup;
use kosuzu_core::protocol::{self, Method, Status};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
use windows_sys::Win32::Graphics::Gdi::{InvalidateRect, RedrawWindow, RDW_ALLCHILDREN, RDW_INVALIDATE};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, Sleep};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::hook;
use kosuzu_winutil as win;

/// 每轮最多处理的请求数，避免一次目录扫描占满 CPU。
const REQUESTS_PER_PASS: usize = 8;
/// 心跳间隔（毫秒）。面板以 5000ms 为过期阈值。
const HEARTBEAT_MS: u64 = 1000;
/// 轮询间隔（毫秒）。
const POLL_MS: u32 = 100;

pub struct Engine {
    root: PathBuf,
    runtime: PathBuf,
    running: bool,
    modes: u32,
    rules: usize,
    generation: u64,
    error: String,
}

impl Engine {
    pub fn new(root: PathBuf) -> Self {
        let runtime = root
            .join("runtime")
            .join(format!("ae-{}", unsafe { GetCurrentProcessId() }));
        Engine {
            root,
            runtime,
            running: true,
            modes: 0,
            rules: 0,
            generation: 0,
            error: String::new(),
        }
    }

    fn status(&self) -> Status {
        Status {
            running: self.running,
            pid: unsafe { GetCurrentProcessId() },
            heartbeat: win::now_ms(),
            modes: self.modes,
            rules: self.rules,
            generation: self.generation,
            hits: hook::successful_layouts(),
            error: self.error.clone(),
        }
    }

    fn write_status(&self) -> std::io::Result<()> {
        win::write_atomic(
            &self.runtime.join("status.json"),
            self.status().to_json().as_bytes(),
        )
    }

    /// 装载词典。失败时保留上一份快照，并把错误写进状态。
    fn reload(&mut self) -> Result<(), String> {
        let rules = dict::load_rules(&self.root.join("dictionaries")).map_err(|e| e.0)?;
        let count = rules.len();
        lookup::set_rules_owned(rules)
            .map_err(|_| "Invalid dictionary text or length".to_owned())?;
        self.rules = count;
        self.generation = self.generation.wrapping_add(1);
        self.error.clear();
        Ok(())
    }

    fn initialize(&mut self) -> Result<(), String> {
        std::fs::create_dir_all(self.runtime.join("requests"))
            .map_err(|e| format!("Cannot create request directory: {}", e))?;
        std::fs::create_dir_all(self.runtime.join("responses"))
            .map_err(|e| format!("Cannot create response directory: {}", e))?;
        self.reload()
    }

    fn dispatch(&mut self, id: &str, path: &Path) -> (bool, String) {
        let bytes = match win::read_limited(path, protocol::MAX_REQUEST_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => return (false, error.to_string()),
        };
        let request = match protocol::parse_request(id, &bytes, win::now_ms()) {
            Ok(request) => request,
            Err(error) => return (false, error.0),
        };

        match request.method {
            Method::Status => (true, String::new()),
            Method::Reload => match self.reload() {
                Ok(()) => (true, String::new()),
                Err(error) => {
                    self.error = error.clone();
                    (false, error)
                }
            },
            Method::Set(modes) => {
                let bits = modes.bits();
                if bits != 0 {
                    if let Err(message) = unsafe { hook::install() } {
                        return (false, message.to_owned());
                    }
                }
                self.modes = bits;
                lookup::set_modes(bits);
                repaint();
                (true, String::new())
            }
            Method::Stop => {
                unsafe { hook::uninstall() };
                self.modes = 0;
                repaint();
                (true, String::new())
            }
        }
    }

    fn handle_requests(&mut self) {
        let directory = self.runtime.join("requests");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        let mut handled = 0usize;
        for entry in entries.flatten() {
            if handled >= REQUESTS_PER_PASS {
                break;
            }
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let id = match path.file_stem().and_then(|stem| stem.to_str()) {
                Some(id) => id,
                None => continue,
            };
            if !protocol::valid_id(id) {
                continue;
            }
            handled += 1;

            let (ok, error) = self.dispatch(id, &path);
            let body = protocol::response_json(id, ok, &error, &self.status());
            let target = self.runtime.join("responses").join(format!("{}.json", id));
            // 响应写不进去就没必要删请求，下一轮会重试。
            if win::write_atomic(&target, body.as_bytes()).is_ok() {
                std::fs::remove_file(&path).ok();
            }
        }
    }

    /// 进入轮询。首次装载失败时按原实现的做法写一份 `running:false` 的状态后退出线程。
    pub fn run(mut self) {
        if let Err(error) = self.initialize() {
            self.error = error;
            self.running = false;
            self.write_status().ok();
            return;
        }
        self.write_status().ok();

        let mut next_heartbeat = win::now_ms() + HEARTBEAT_MS;
        loop {
            self.handle_requests();
            let now = win::now_ms();
            if now >= next_heartbeat {
                self.write_status().ok();
                next_heartbeat = now + HEARTBEAT_MS;
            }
            unsafe { Sleep(POLL_MS) };
        }
    }
}

/// 让本进程的可见窗口重画，使新的替换结果立刻生效。
pub fn repaint() {
    unsafe { EnumWindows(Some(repaint_window), 0) };
}

unsafe extern "system" fn repaint_window(window: HWND, _param: LPARAM) -> BOOL {
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(window, &mut pid);
    if pid == GetCurrentProcessId() && IsWindowVisible(window) != 0 {
        InvalidateRect(window, std::ptr::null(), 0);
        RedrawWindow(
            window,
            std::ptr::null(),
            std::ptr::null_mut(),
            RDW_INVALIDATE | RDW_ALLCHILDREN,
        );
    }
    TRUE
}
