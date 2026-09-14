# host

智芯城（RiscDom）的 **Tauri 宿主后端**。前端只通过 Tauri command 与它通信，
绝不直接接触 `agent` / `sandbox` / `audit`。

依赖方向：`host → {agent, sandbox, audit}`；`ui/src-tauri → host`。

## 模块

- `state` — `AppState`（审计库、sink、VM 槽、LLM 配置、工作区、编译器）
- `commands` — Tauri commands
- `events` — 事件名常量 + `EventSink`（Tauri / 测试用录制实现）
- `error` — `HostError`

## Commands

| command | 返回 |
| --- | --- |
| `get_audit_status()` | `{ count, chain }` |
| `list_audit_events(limit, actor?, action_prefix?)` | `StoredEventView[]`（倒序） |
| `set_llm_config(api_key, base_url, model)` | `()` |
| `clear_llm_config()` | `()` |
| `get_llm_config_status()` | `{ configured, base_url, model }`（**不含 key**） |
| `run_agent(user_input)` | `AgentOutcomeView` |
| `get_workspace_files()` | `string[]` |
| `read_workspace_file(path)` | `string`（经策略检查） |
| `get_serial_buffer()` | `string` |
| `export_audit_jsonl(path)` | `usize`（写入工作区内） |

## 事件（host → 前端）

- `agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final`
- `serial:chunk`
- `vm:state`

## 安全

- **API key 只在内存**（`AppState::llm_config`）。不落盘、不进审计、不进日志、
  不进 `get_llm_config_status` 的返回；其 `Debug` 只显示前 4 后 4。
- 文件读写经 `agent::WorkspacePolicy` 检查（默认拒绝 + 防穿越）。
- 前端无法绕过 host 直接调用 sandbox / agent。

## ⚠️ MVP 说明：审计/串口桥接（重要）

`run_agent` 期间，host 起一个 **200ms 轮询** 的后台线程（`AuditBridge`），
把审计日志中的新事件转成前端事件：

- `agent.llm.request` → `agent:iteration`
- `agent.tool.call` → `agent:tool_call`
- `agent.tool.result` → `agent:tool_result`
- `vm.start` / `vm.stop` / `vm.snapshot.save` → `vm:state`

**串口来源（MVP 降级，已报告）**：`agent` crate 的 `AgentLoop` 内部私有持有 VM，
host 无法访问其 `serial_output()`，而按红线本轮不允许扩展 agent。
因此 `serial:chunk` 目前**从审计里 `read_serial` 工具的结果派生**
（`serial_full_text`），用 `SerialDiff` 做增量去重。数据是真实的串口内容，
但更新时机是"模型调用 `read_serial` 时"，而非连续流。

v0.2：给 agent 增加最小增量 API（让 host 持有 VM 槽），改为直接轮询
`sandbox::RiscVVirtualMachine::serial_output()`；sandbox 侧则改为主动回调。
届时 `AppState.vm` 字段会真正启用（当前保留未用）。

## 测试

```text
cargo test -p host
```

- `tests/commands_smoke.rs`：状态/配置/文件（含 key 不泄漏断言）
- `tests/serial_polling.rs`：增量 emit 不重复不丢失；串口来源过滤
