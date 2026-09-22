[English](README.md) | 中文

# host-tauri

RiscDom 宿主的 **Tauri 半边**：53 个 `#[tauri::command]` 与 `TauriEventSink` 传输，架在内核门面之上。前端只通过 Tauri command 与它通信，绝不直接接触 `agent` / `sandbox` / `audit`——**内核能力在 [`host-core`](../host-core/README.zh-CN.md)**，本 crate 再导出它（`pub use host_core::*`），因此外壳只依赖一个 crate。

依赖方向：`host-tauri → host-core → {agent, sandbox, audit}`；`ui/src-tauri → host-tauri`。下层不得依赖 `host-tauri`——v0.9 A1 拆分存在的理由，就是让 `worker` 与 `server` 不链接 Tauri。

## 模块

- `commands` —— 53 个 Tauri 命令，是 `host-core` 的 `AppState` 上的薄包装
- `events` —— `TauriEventSink` 传输，外加 `host-core` 事件面（envelope、事件名、`EventSink`）的再导出
- `lib.rs` —— 门面：`pub use host_core::*`

审计库、`AppState`、快照、会话、两条下载路径、预检、运行指纹对比、keyring、路径与错误类型都在 `host-core`，经门面在此可达（`host_tauri::AppState`、`host_tauri::state::…` 等）。

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
| `save_snapshot_real(name)` | `u64`（真实快照字节数；无 VM → `Err("no running vm")`） |
| `resume_from_snapshot_real(name)` | `()`（先停当前 VM，再以 `-incoming` 恢复） |
| `export_audit_jsonl(path)` | `usize`（写入工作区内） |
| `export_run_audit(run_id, path)` | `usize`（一次 run 的记录，自足：从链的第一条事件到该 run 结束；abandoned 的 run 以 `host.run.abandoned` 标记结尾，进行中的 run 会被拒） |
| `compare_run_fingerprints(run_a, run_b)` | `FingerprintFieldDiff[]`（v0.6 批次 1：两个 run 的配置指纹并排，每个**顶层**字段一行 —— `field` / `a` / `b` / `is_different` —— 顺序即 `AppState::run_fingerprint` 的声明顺序，无差异的字段也在列表里；文档取自链上的 `run.start` 事件，run id 不存在会报错） |
| `workspace_root()` | `string`（工作区根绝对路径） |

## 事件（host → 前端）

- `agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final`
- `agent:stream:delta` / `agent:stream:done`（LLM 流式增量，见下）
- `serial:chunk`
- `vm:state`

## 服务商预设

`host-tauri` 通过 `get_provider_presets` 把 `agent::presets::builtin_presets()`（纯数据）暴露给前端；
`set_llm_config` 接受 `provider_id`，当传入的是预设且 `base_url` / `model` 为空时用预设值填充；
`provider_id = "custom"` 时两者必填。5 个内置预设：`deepseek`（默认）/ `openai` /
`ollama`（本地，无需 key）/ `lmstudio`（本地，无需 key）/ `custom`。详见 `agent/README.md`。

## 工具链（阶段 24b/24c）

`probe_toolchain()` 返回 `{ found, path, source, diagnostics }`，`source` 为
`EnvVar` / `KnownPath` / `Path` / `Manual`。`set_toolchain_path(path)` 会先用 `--version` 校验该文件
再保存；`clear_toolchain_path()` 回到自动探测；无可用编译器时 `run_agent` 直接以
`toolchain_missing` 错误拒绝（随后附上探测详情）。审计事件：`host.toolchain.set` /
`host.toolchain.clear`。

### 一键下载（v0.3 第③步）

- `start_toolchain_download()` —— 下载本平台对应的官方 xPack RISC-V GCC（SHA-256 固定），
  校验后解压到 `<app data>/toolchain/<version>/`，并把它设为当前工具链（同时写入
  `settings.json`）。进行中再次调用会返回 `download already in progress`；
  版本已安装则为**幂等**（不重复下载）。
- `cancel_toolchain_download()` —— 请求中止正在进行的下载；临时归档会被删除，状态回到 idle。
- `toolchain_download_status()` —— `{ in_progress, last_event }`。
- 进度以 `toolchain:download` 事件推给前端，载荷为 `DownloadEvent`
  （`started` / `progress` / `verifying` / `extracting` / `done` / `failed` / `cancelled`）。
- 审计：`host.toolchain.download.start` / `.done` / `.failed` / `.cancelled`
  （detail 记 version，成功时另记最终 path）。
- **校验和是强制的**——没有跳过校验的开关；会逃出安装目录的归档条目会被拒绝（Zip Slip 防护）。

手工步骤（Windows）：

**QEMU**：`probe_qemu()` / `get_qemu_status()` / `set_qemu_path()` / `clear_qemu_path()`
与工具链三件套对称，而手动路径现已**完整接通**：`run_agent` 会把它注入 agent 循环
（`AgentLoop::set_qemu_path`），沙箱因此启动的正是该可执行文件。设了手动路径就不再回落
到自动探测；路径不可用时以 `qemu_missing` 提前失败，而不是静默回退。

1. 打开应用 → **设置 → 工具链**。出现红色横幅说明未找到。
2. 展开 **探测详情**，查看尝试过的每个路径。
3. 点 **手动指定**，粘贴 `riscv64-unknown-elf-gcc.exe` 的完整路径；该行应变为绿点 +
   `Manual` 徽标。
4. 在对话框发一条消息：应能编译并启动，而不再报 `toolchain_missing`。
5. 点 **清除手动路径** → source 徽标回到 `KnownPath` / `Path`。

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
- host 命令：`list_snapshots` / `delete_snapshot` / `save_snapshot_real` /
  `resume_from_snapshot_real`（扫描 `<workspace>/.riscdom/snapshots`；审计事件
  `host.snapshot.delete` / `host.snapshot.save` / `host.snapshot.resume`）。
- 保存：从 `AppState::vm_slot` 取 host 持有的 VM，调 sandbox `save_snapshot_real`；
  快照名只允许 `[A-Za-z0-9_-]{1,64}`（防目录穿越），同名快照**拒绝覆盖**。
- 恢复：先 `stop_current_vm()`，再用 `-incoming tcp:` + 本地中继喂流；恢复出的 VM
  留在槽里，供后续 run 继续使用。`-kernel` 取工作区内**最新的 `*.elf`**（迁移流会覆盖
  内存，内核仅用于让 QEMU 正常起机）；工作区没有 ELF 时明确报错。

### 在 Windows 上验证（凭据管理器）

自 v0.2.2 起，Windows 构建启用 keyring 的 `windows-native` 后端，key 会真正写入凭据管理器。
keyring 的 OS 条目名是 `<user>.<service>`，即 `llm-api-key:deepseek.com.breakevery.riscdom`；
而 `cmdkey /list:<filter>` **只从名字开头匹配**，所以要么用完整名字，要么列出全部再过滤：

```text
cmdkey /list:llm-api-key:deepseek.com.breakevery.riscdom
cmdkey /list | findstr breakevery
```

`cargo test -p host-core --test keyring_os -- --ignored --nocapture` 会用一次性的
`com.breakevery.riscdom.test` 条目真实演练一遍，并自行清理。

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
- 注意：该测试本身只断言“串口含 HELLO RISCV + 非 Failed”，**未包含** `verify_chain` 断言（属测试覆盖缺口，未在本轮修改代码）。链完整性证据见 mock e2e（`cargo test -p host-core -- --ignored`，Intact length 29）。

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
cargo test -p host-core -- --ignored --nocapture
```

断言：`agent:final` 到达、`serial:chunk` 含 `HELLO RISCV`、`verify_chain` 为 Intact。

## 测试

测试随代码搬迁：它们住在 [`host-core/tests`](../host-core/tests)，因为它们所验证的行为在那里。

```text
cargo test -p host-core
```

- `tests/commands_smoke.rs`：状态/配置/文件（含 key 不泄漏断言）
- `tests/serial_polling.rs`：增量 emit 不重复不丢失；串口来源过滤
