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
| `set_llm_config(api_key, base_url, model, provider_id?, remember?)` | `()`（`provider_id` 缺省 `deepseek`；预设且 base_url/model 为空时自动填充；`custom` 必填两者；`remember` 缺省 `true` → 写入系统钥匙串） |
| `clear_llm_config()` | `()` |
| `get_llm_config_status()` | `{ configured, provider_id, base_url, model, persisted }`（**不含 key**） |
| `get_provider_presets()` | `ProviderPresetView[]`（5 个内置预设，纯数据） |
| `has_stored_key(provider_id)` | `bool`（钥匙串是否有该服务商的条目，**不返回 key**） |
| `load_stored_key(provider_id)` | `()`（从钥匙串读 key 写入内存；无条目返回 `Err("no_stored_key")`） |
| `run_agent(user_input)` | `AgentOutcomeView` |
| `get_workspace_files()` | `string[]` |
| `read_workspace_file(path)` | `string`（经策略检查） |
| `get_serial_buffer()` | `string` |
| `stop_current_vm()` | `()`（停止并清空 host 持有的 VM；空槽为 no-op） |
| `vm_is_running()` | `bool`（host 是否持有常驻 VM） |
| `export_audit_jsonl(path)` | `usize`（写入工作区内） |

## 事件（host → 前端）

- `agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final`
- `agent:stream:delta` / `agent:stream:done`（LLM 流式增量，见下）
- `serial:chunk`
- `vm:state`

## 服务商预设

`host` 通过 `get_provider_presets` 把 `agent::presets::builtin_presets()`（纯数据）暴露给前端；
`set_llm_config` 接受 `provider_id`，当传入的是预设且 `base_url` / `model` 为空时用预设值填充；
`provider_id = "custom"` 时两者必填。5 个内置预设：`deepseek`（默认）/ `openai` /
`ollama`（本地，无需 key）/ `lmstudio`（本地，无需 key）/ `custom`。详见 `agent/README.md`。

## 系统钥匙串（keyring）

密钥持久化在 OS 凭据库（Windows Credential Manager / macOS Keychain /
Linux Secret Service），服务名 `com.breakevery.riscdom`，账号名 `llm-api-key:<provider_id>`。

- 仅 **host** 依赖 `keyring` crate；`agent` 不依赖。
- 写入失败 **静默降级**：记 `host.keyring.save_failed`，返回成功但 `persisted = false`，
  不 panic、不阻塞启动。
- 相关审计事件（detail 只记 `provider_id`，**绝不记 key**）：
  `host.keyring.save` / `host.keyring.save_failed` / `host.keyring.delete` / `host.keyring.load`。
- `clear_llm_config` 只删当前 provider 的条目，不会动其它 provider。
- `DEEPSEEK_API_KEY` 环境变量在启动时被采纳到**内存**（开发便利），**不会**写入钥匙串。

### 手工验证步骤（13c）

a. 首次启动、无配置 → 横幅显示 `no_config`。
b. 填 DeepSeek key + 勾选“保存到系统钥匙串” → 保存 → 状态显示
   “已配置 · DeepSeek · 已保存到系统钥匙串”。
c. 关闭应用重启 → 状态自动恢复为“已配置”（从钥匙串读取）。
d. 清除配置 → 重启 → 状态为未配置（钥匙串条目已删）。
e. 不勾选记住 → 保存后状态为“仅本次会话”；重启后需重填。

## 快照

- **真实快照（`.mig`）**：由 sandbox 通过 QMP `migrate` + 本地 TCP 中继落盘（见
  `sandbox/docs/snapshot-experiment.md`）；因 Windows 上 `migrate` → `file:` 不可用。
- **重启式降级（`.json`）**：旧方案，保留兼容。
- host 命令：`list_snapshots` / `delete_snapshot`（扫描 `<workspace>/.riscdom/snapshots`，
  审计事件 `host.snapshot.delete`）。
- **限制**：host 目前不持有常驻 VM（VM 只在单次 run 内由 `AgentLoop` 持有），因此
  UI 只能列出/删除快照；“保存当前状态 / 恢复”留待启用 `AppState.vm` 槽后的后续阶段。

## 会话持久化

对话保存在**应用数据目录**的 `sessions.db`（SQLite，复用 `rusqlite`，不新增依赖）：

- 位置：`app_data_dir/sessions.db`（由 `ui/src-tauri` 在 setup 里注册）；可用
  `RISCDOM_SESSION_DB_PATH` 覆盖；非 Tauri 上下文回退到 `<temp>/riscdom/sessions.db`。
- 命令：`list_sessions` / `create_session` / `open_session` / `rename_session` /
  `delete_session` / `clear_all_sessions` / `get_current_session_id`。
- 审计事件：`host.session.create` / `.open` / `.rename` / `.delete`（**不记**消息内容）。
- **绝不持久化**：API Key、system prompt 原文、流式中间状态、审计事件；删除会话时
  消息级联删除。
- 恢复：`open_session` 返回历史消息；`run_agent` 在启动新 `AgentLoop` 前用
  `push_history` 注入它们（不重放工具调用）。

## 安全

- **API key 只在内存**（`AppState::llm_config`）。不落盘、不进审计、不进日志、
  不进 `get_llm_config_status` 的返回；其 `Debug` 只显示前 4 后 4。
- 文件读写经 `agent::WorkspacePolicy` 检查（默认拒绝 + 防穿越）。
- 前端无法绕过 host 直接调用 sandbox / agent。

## 事件桥接与串口（当前实现）

每次 `run_agent` 起两个后台线程（串口转发器改为长驻，见第 2 项）：

1. **AuditBridge**（200ms 轮询审计）→ 只负责 `agent:iteration` / `agent:tool_call` /
   `agent:tool_result`，以及 `vm.start` / `vm.stop` / `vm.snapshot.save` → `vm:state`。
2. **串口转发器（长驻，20b）**——应用启动（`setup`）时创建，**不随 run 结束**：
   拥有广播通道的接收端（`Receiver<Vec<u8>>`，`recv_timeout(200ms)` 轮询），把每个
   分帧用 lossy UTF-8 转成字符串后 emit `serial:chunk`，并累加到
   `get_serial_buffer()` 的返回值中。发送端保存在 `AppState::serial_senders`，每次 run
   通过 `AgentLoop::attach_serial()` 注入同一份列表——因此**跨 run 连续**。
3. **流式转发器**（LLM 流式）→ 读取 `AgentLoop::subscribe_stream()` 的
   `Receiver<StreamEvent>`：`Delta` → `agent:stream:delta { text }`，`Done` →
   `agent:stream:done`；`ToolCallDelta` **不转发**（工具调用仍由 `agent:tool_call` 处理）。
   UI 用 `agent:final` 的最终 content **覆盖**流式内容，避免字节差异导致不一致。

串口**不再**从审计里 `read_serial` 的工具结果派生（旧的 `serial_full_text` /
`SerialDiff` 已删除）。数据由 sandbox 的串口读取线程经 `VMConfig.serial_observer`
实时扇出给所有订阅者；`read_serial` 工具语义不变（仍返回 VM 缓冲全量）。sandbox 侧
对 observer panic 做了 `catch_unwind`，并写审计事件 `sandbox.serial.observer_panic`。

订阅**只收到订阅之后**的数据（无历史回放）；需要全量请调用 `read_serial` 工具。

VM 生命周期（20b）：VM 归 `AppState::vm_slot` 所有。`run_agent` 用
`AgentLoop::with_vm(...)` 注入该槽，run 结束后 VM 仍留在槽内（不再随 run 销毁），
下一次 run 复用同一台 guest（再调 `start_vm` 会得到 “already running”）。`stop_current_vm()`
停止并清空槽，`vm_is_running()` 供 UI 判断按钮可用性。

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
