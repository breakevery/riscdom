[English](README.md) | 中文

# host-core

RiscDom 宿主（host）的**可移植半边**：内核门面里所有不需要 webview 的部分。它的依赖树里没有 Tauri——这是这个 crate 存在的目的，不是当下 import 的巧合。

依赖方向：`host-core → {agent, sandbox, audit}`；`host → host-core`；`ui/src-tauri → host`。这里没有任何东西依赖 Tauri 半边，因此这部分代码可以由 CLI、由 `worker`、由控制平面驱动，而不必链接 GUI 工具链。

## 模块

- `state` —— `AppState`（审计库、sink、VM 槽、LLM 配置、workspace、编译器）
- `events` —— 那唯一的 event envelope、事件名、`EventSink` trait
- `dispatch` —— 宿主自己的执行器路由（接进 `agent::dispatch`）
- `executor` —— stdio 执行器句柄
- `preflight` —— 环境预检
- `qemu_download` / `toolchain_download` —— 两条下载路径（后者同时服务两种语言：`Toolchain::C` 与 `Toolchain::Zig`，v0.9 F3a-download-apply）
- `run_diff` —— 运行指纹对比
- `session` / `settings` —— 会话与本地设置
- `keyring` —— API key 存储
- `paths` —— workspace 与数据目录路径
- `error` —— `HostError`

## 与 `host-tauri` 的关系

`host-tauri` 是 Tauri 半边：53 个 `#[tauri::command]` 与 `TauriEventSink` 传输。它再导出本 crate 的公共面（`pub use host_core::*`），因此桌面壳只依赖一个 crate，而照着拆分前的 `host` 写的消费者无需改动即可编译。依赖只朝一个方向：`host-tauri → host-core`；`worker` 与 `server` 直接依赖本 crate，这正是它们都不链接 Tauri 的原因。

## 约束

- **无 Tauri。** 需要 webview 的改动属于 `host-tauri`，不属于这里。
- API key 只存在于内存：不落盘、不进日志、不进审计。
- 文件读写经 `agent::WorkspacePolicy` 检查。
- 本 crate 之外的一切都不直接触碰 `sandbox` / `agent`。
