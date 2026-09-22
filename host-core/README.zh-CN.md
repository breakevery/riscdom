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
- `qemu_download` / `toolchain_download` —— 两条下载路径
- `run_diff` —— 运行指纹对比
- `session` / `settings` —— 会话与本地设置
- `keyring` —— API key 存储
- `paths` —— workspace 与数据目录路径
- `error` —— `HostError`

## 与 `host` 的关系

`host` 是 Tauri 半边：53 个 `#[tauri::command]` 与 `TauriEventSink` 传输。A1 拆分分波推进期间，`host` 同时再导出本 crate 的公共面（`pub use host_core::*`），因此照着拆分前的 `host` 写的消费者无需改动即可编译。后续各波把消费者搬离 `host`——`worker` 与 `server` 搬去 `host-core`，桌面壳搬去更名后的 Tauri crate。

## 约束

- **无 Tauri。** 需要 webview 的改动属于 `host`，不属于这里。
- API key 只存在于内存：不落盘、不进日志、不进审计。
- 文件读写经 `agent::WorkspacePolicy` 检查。
- 本 crate 之外的一切都不直接触碰 `sandbox` / `agent`。
