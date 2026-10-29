# 本居小铃 · Kosuzu
**拥有读懂任何文字的能力**

面向 Adobe After Effects 的本地词典汉化框架。用自己的词典翻译插件名称与参数文字，通过面板开关切换译文和原文。

**仅发布框架，默认不附带翻译词典。** 支持 Windows x64，当前以 After Effects 2026 为测试目标；Premiere Pro 暂未接入。
**Releases**提供了一键安装的版本，本项目不定期维护，本项目为GPT6sol+codex许愿式编程创建产物，目的其实仅本人日常使用。
## 构建

依赖：Rust 1.75+（`x86_64-pc-windows-msvc`，rustup 默认安装即可）、MSVC Build Tools。
不需要 Visual Studio IDE，也不需要 Adobe SDK。

```bat
Build.cmd      :: 编译，产物落到 .\backend
Verify.cmd     :: 单元测试 + 四项端到端自检
```

## 安装

```bat
:: 先关闭 After Effects（安装脚本检测到 AE 在运行会拒绝）
Install.cmd
```

安装器会申请管理员权限（只为写入 AE 的 `ScriptUI Panels`），自动检测 C 盘的 AE 2023–2026，
也可以手动指定目录。程序装到 `%LOCALAPPDATA%\Kosuzu\app`，面板写进所选 AE。

启动 AE，从「窗口」菜单打开 `Kosuzu.jsx`，勾选「参数」「名称」即可。后端由面板自动拉起，
不需要手动运行任何程序。

卸载：关闭 AE，运行 `Uninstall.ps1`。它只移除面板，应用目录、词典与备份都保留。

## 目录结构

```
Kosuzu-Source/
├── Kosuzu.jsx                   面板（ExtendScript，安装时拼装成单文件）
├── backend/
│   ├── json.jsxinc              面板用的严格 JSON 编解码
│   ├── dictionary-manager.jsxinc 词典窗口
│   └── *.dll / *.exe            构建产物（Build.cmd 生成，不入库）
├── dictionaries/                词典（本仓库只带 10 条示例）
├── crates/
│   ├── kosuzu-core/             零 Win32 依赖：JSON、词典合并、查表、协议
│   ├── kosuzu-winutil/          宽字符串、时间戳、原子写文件
│   ├── kosuzu-native/           KosuzuNative.dll
│   ├── kosuzu-controller/       KosuzuController.exe
│   ├── kosuzu-clipboard/        KosuzuClipboard.exe
│   ├── kosuzu-dictcheck/        KosuzuDictionaryCheck.exe
│   └── kosuzu-selfcheck/        KosuzuSelfCheck.exe（自检工具）
├── docs/DESIGN.md               设计取舍、与原 C++ 实现的实测对比
├── Install.cmd / Install-AE-Panel.ps1 / Uninstall.ps1
└── .cargo/config.toml           关键：静态链接 CRT
```

## 架构

一个 DLL 加三个可执行文件，全部是薄组件：

| 组件 | 职责 |
| --- | --- |
| `KosuzuNative.dll` | 被注入 AE 进程。改写共享 `IDWriteFactory` vtable 的槽位 18/19，命中词典时替换文字；同时跑一个工作线程，轮询请求目录、写心跳 |
| `KosuzuController.exe` | 找到唯一的 `AfterFX.exe`，把 DLL 注入进去，远程调用它的 `LB_Start` |
| `KosuzuClipboard.exe` | 面板没法可靠读写剪贴板，由它代劳（`copy`/`paste` + 回执文件） |
| `KosuzuDictionaryCheck.exe` | 安装脚本覆盖用户词典前做静态校验，退出码非 0 就中止 |

后端与面板之间**不用管道、不用 socket**，只交换文件。运行时目录：

```
%LOCALAPPDATA%\Kosuzu\app\runtime\
├── connection.json            控制器写入：注入是否成功
└── ae-<PID>\
    ├── status.json            引擎心跳（每秒一次）
    ├── requests\<id>.json     面板写入的命令
    └── responses\<id>.json    引擎写入的应答（面板读完自己删）
```

## 文件协议

面板每 2 秒读一次 `status.json`，心跳超过 5 秒视为断连。

`status.json`：

| 字段 | 含义 |
| --- | --- |
| `running` | 引擎是否存活 |
| `pid` | 宿主进程 PID，面板会与 `connection.json` 的 pid 比对 |
| `heartbeat` | 毫秒时间戳 |
| `engine` | 引擎标识，用于区分实现版本 |
| `state.names` / `state.parameters` | 两个开关的当前状态 |
| `rules` | 已装载词条数 |
| `generation` | 每次成功重载 +1 |
| `hits` | 实际替换过的文字布局次数 |
| `error` | 最近一次错误文本，空字符串表示正常 |

`connection.json`：`{created, pid, ok, error}`。

请求包 `{id, created, method, state}`：

* `method` 只接受 `set` / `reload` / `stop` / `status`；
* `id` 必须与文件名一致，只允许字母数字与连字符，最长 100；
* `created` 与当前时间相差超过 30 秒即拒绝；
* 单轮最多处理 8 个请求，响应先写临时文件再 `MoveFileEx` 替换，避免读到半截 JSON。

## 词典格式

`dictionaries/` 下四个文件，按「基础 → 自定义」顺序合并，后者覆盖前者：

```
Kosuzu.base.names.json        Kosuzu.names.json
Kosuzu.base.parameters.json   Kosuzu.parameters.json
```

* 必须是扁平 JSON 对象，键是原文，值是译文；
* 键不能为空，不能是 `__proto__` / `constructor` / `prototype`；
* 空译文不会覆盖已有译文；原文与译文相同、或译文全是空白的条目会被丢弃；
* 单文件上限 10 MiB，单条原文/译文上限 8192 个 UTF-16 单元，总条目上限 100,000；
* 词典只当数据读，不会被执行。

## 改代码前必读

1. **不能 panic**。整个工作区以 `panic = "abort"` 编译，`catch_unwind` 无效；
   一个 `unwrap` 会让宿主 After Effects 直接退出。导出函数与绘制回调里不要出现
   `unwrap` / `expect` / 切片索引 / `format!`。
2. **绘制回调不能分配、不能加锁**。`lookup::translate_raw` 是这条线的边界：
   表用 `ArcSwap` 无锁发布，翻译缓冲是栈上的 `MaybeUninit<[u16; 8193]>`。
3. **注入的 DLL 必须静态链接 CRT**。删掉 `.cargo/config.toml` 里的 `+crt-static`
   会让产物依赖 `VCRUNTIME140.dll`，在没装 VC++ 运行库的机器上加载失败。
4. **协议字段不能改名**。`status.json`、`connection.json`、请求响应包的字段名和错误文本
   都是面板在读，改了面板就断。

## 已知限制

* 只覆盖走共享 DirectWrite 工厂的文字绘制。使用其他绘制路径、独立工厂或自己缓存文本的插件可能不生效。
* 只支持同时运行一个 AE 进程。
* 目前只验证 After Effects，尚未接入 Premiere Pro。
* 词典需要用户自己积累；本仓库只带 10 条示例。

## 许可证

* 框架代码采用 [MIT License](LICENSE)。不随项目分发任何第三方插件翻译词库；外部词典和第三方插件的权利归各自权利人所有。
* 本软件的“本居小铃”名取自《东方 Project》， 系上海爱丽丝幻乐团的既有作品角色，仅作软件命名致敬，与原作无关联。
