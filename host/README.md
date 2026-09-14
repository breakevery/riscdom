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
| `set_llm_config(api_key, base_url, model, provider_id?)` | `()`（`provider_id` 缺省为 `deepseek`；预设且 base_url/model 为空时自动填充；`custom` 必填两者） |
| `clear_llm_config()` | `()` |
| `get_llm_config_status()` | `{ configured, provider_id, base_url, model }`（**不含 key**） |
| `get_provider_presets()` | `ProviderPresetView[]`（5 个内置预设，纯数据） |
| `run_agent(user_input)` | `AgentOutcomeView` |
| `get_workspace_files()` | `string[]` |
| `read_workspace_file(path)` | `string`（经策略检查） |
| `get_serial_buffer()` | `string` |
| `export_audit_jsonl(path)` | `usize`（写入工作区内） |

## 事件（host → 前端）

- `agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final`
- `serial:chunk`
- `vm:state`

## 服务商预设

`host` 通过 `get_provider_presets` 把 `agent::presets::builtin_presets()`（纯数据）暴露给前端；
`set_llm_config` 接受 `provider_id`，当传入的是预设且 `base_url` / `model` 为空时用预设值填充；
`provider_id = "custom"` 时两者必填。5 个内置预设：`deepseek`（默认）/ `openai` /
`ollama`（本地，无需 key）/ `lmstudio`（本地，无需 key）/ `custom`。详见 `agent/README.md`。

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

## 手工验证

真实 API 全链路（需 key）在 v0.1.0 上**已执行通过**（2026-09-14）。

### 真实 DeepSeek 端到端结果（2026-09-14）

```text
cargo test -p agent -- --ignored --nocapture
real_deepseek_writes_and_runs_hello_world ... ok
AgentOutcome: Final，iterations = 6
串口（read_serial 工具结果）：
  wrote 634 bytes to hello.c
  compiled hello.c -> hello.elf (ok)
  VM started (qmp=*****, serial=*****)
  HELLO RISCV
测试耗时：约 7.6s（含编译 + QEMU 启动）；总耗时约 12.8s
```

- 串口含 `HELLO RISCV` ✅
- 模型只使用了白名单工具（write_source → compile → start_vm → read_serial → stop_vm）✅
- 注意：该测试本身只断言“串口含 HELLO RISCV + 非 Failed”，**未包含** `verify_chain` 断言（属测试覆盖缺口，未在本轮修改代码）。链完整性证据见 mock e2e（`cargo test -p host -- --ignored`，Intact length 29）。

### UI 全链路人工步骤

1. `cd ui && npm run tauri dev` 启动桌面窗口。
2. 设置栏填入 Key / Base URL / Model → “保存到本次会话”（状态点变绿，**不回显 key**）。
3. 对话框输入“写一个 RISC-V 裸机 Hello World”并发送。
4. 预期：左侧按 Agent 过程逐条追加（工具调用为可折叠块）→ 最终回答；
   右侧画布出现 `HELLO RISCV`；中栏审计事件数增加。
5. 关闭窗口后重启 → 设置栏 `configured == false`（确认 key 未持久化）。

### 无 key / 本地模型降级（12b）

1. 清空配置（设置栏“清除”）→ 设置面板顶部出现黄色横幅，`reason = no_config`。
2. 选 DeepSeek 但不填 Key 保存 → API Key 字段下方提示 `missing_api_key`（保存被拒，不写入）。
3. 选 Ollama（本地）保存 → `ready = true`，横幅消失（本地模型无需 key）。
4. 若本机有 Ollama / LM Studio 在跑，点横幅右侧“检测本地模型”应识别出服务商与模型数；
   点“使用”自动切换预设并保存。
5. 未就绪时在对话框发送消息 → 消息流插入一条系统提示（不调用 run_agent）。

前置接口：`get_llm_readiness()`、`probe_local_llm()`。探测超时每项 1.5s，失败静默。

自动化的等价验证（mock LLM，无需 key）：

```text
cargo test -p host -- --ignored --nocapture
```

断言：`agent:final` 到达、`serial:chunk` 含 `HELLO RISCV`、`verify_chain` 为 Intact。

## 测试

```text
cargo test -p host
```

- `tests/commands_smoke.rs`：状态/配置/文件（含 key 不泄漏断言）
- `tests/serial_polling.rs`：增量 emit 不重复不丢失；串口来源过滤
