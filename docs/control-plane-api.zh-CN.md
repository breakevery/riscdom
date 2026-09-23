[English](control-plane-api.md) | 中文

# 控制平面 HTTP API（设计）

> **适用 v0.9，v1.0 冻结。** 本文是设计文档：它定义集成者照着实现客户端的接口，不是今天就交付的实现。

**面向读者。** 发行版与集成的开发者：要嵌入 RiscDom 的人、要写控制平面客户端的人、要把 RiscDom 接进自己系统的人。本文不是用户手册——「如何安装与运行 RiscDom」写在别处。

**这是什么。** RiscDom v0.9 的主线是控制平面：人监督 AI 与 AI 监督 AI 走**同一套** HTTP 接口。在内核看来，来自监工 AI 的指令和来自人的指令都是控制平面授权的指令；审计链靠 `agent_id` 区分二者。本条设计存在的意义，就是避免去建两条会各自演化、最终冲突的控制通道。

**实现状态（v0.9）。** §5 已全部落地——§5.1 的 32 个查询端点、§5.2 的 35 个控制端点、§5.3 的宿主本地端点、§4 的错误模型、带 `Last-Event-ID` 补发与 `gap` 帧的事件 envelope、以及 §3 的 bearer token。仅两条路由为预留：`/v0/resources`（§6 G3）与 `POST /v0/vm/start`（§6 G1），二者都以 `501` 明示。权限**强制**（§3）已落地：每条被服务的路由恰好声明一个 capability，actor 不持有时服务端以 `403` 拒绝。**词汇表里每一个 capability 都在某处被强制**：第 29 个 `sandbox.read` 由下面的沙箱查询服务（并充当两条申请决策的门），第 30 个 `sandbox.switch` 由同一表面上的切换服务，第 31 个 `sandbox.assemble` 则**在申请决策的处理器内部**强制——一条决策需要它的请求 `action` 所隐含的能力，而处理器正是知道该请求的地方。`sandbox.assemble` 自己的路由随装配端点落地。

## 1. 定位与协议

- **Layer 3。** 控制平面是一个宿主（[architecture-evolution.md](architecture-evolution.md) §4 的定义）：它与 `ui/src-tauri` 并列，只依赖 Layer 2 的稳定 API（`host-core` 的公开面）。它不直接触碰 `agent` / `sandbox` / `audit`。
- **传输层。** 命令与查询走 HTTP，事件推送走 **SSE**（Server-Sent Events）。刻意不用 WebSocket；理由记在 [handoff.md](handoff.zh-CN.md) §1，线格式见 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)。
- **设备无关语义。** 同一套协议承载本地 IPC 与网络连接。本地套接字与远程主机对同一请求回同一份载荷，只有传输地址不同。这就是 architecture-evolution.md §7 留下的那道「缝」。
- **每个内核能力都要有端点。** architecture-evolution.md §6 与 §12 的约束：内核能力若没有控制平面端点，官方管理程序就用不上它，那就是装饰。下文 §5 是覆盖表，缺口一并列出。

## 2. 约定

- 基础路径：`/v0/`——「v0」意为不稳定。v0.x 内错误码、字段名、端点路径都可能变。这个前缀是「诚实地破坏」的承诺，不是「保持不变」的承诺。
- 内容类型：请求与响应为 `application/json; charset=utf-8`，SSE 流除外（`text/event-stream`）。
- 身份：`agent_id` 为 `<device>-<pid>-<seq>`，`task_id` 为 `task-<pid>-<seq>`（v0.8 批次 B）。客户端把二者都当作不透明字符串。
- 响应体永远不含 API key 或 token。这与宿主的既有不变式一致：前端永远看不到 key。

## 3. 认证与权限

- 客户端发送 `Authorization: Bearer <token>`。
- **开箱即用时 token 就是一个文件。** 除非以 `--no-auth` 启动，服务程序会装入 `TokenAuth`：首次启动生成 32 字节随机值写入 `<data-dir>/token`（仅属主可读），此后每个请求都必须出示该值。token 从不被打印或记入日志——请从文件里读。加 `--no-auth` 会取消该要求并打印警告，因为控制端点里包含破坏性操作。
- 控制平面只有一个钩子，做成 trait 形状：

```rust
/// 把一个请求解析为发起它的 actor，或拒绝。
pub trait Authn: Send + Sync {
    fn authorise(&self, req: &ReqMeta) -> Result<Actor, AuthError>;
}

/// 一个请求被允许以什么身份行动。
pub struct Actor {
    /// 与审计链的 `agent_id` 一一对应。
    pub agent_id: String,
    /// `human` / `supervisor` / `executor` —— 仅用于审计叙述。
    pub kind: ActorKind,
    /// 该 actor 能做什么。处理器运行前，服务端拿路由的 capability 与这个集合比对。
    pub capabilities: BTreeSet<Capability>,
}
```

- **钩子负责认证，服务端负责授权。** `authorise` 回答的是「调用者是谁」；这个 actor 能不能做这件事是另一个决定，且由服务端作出：每条被服务的路由都恰好声明一个 capability，请求路径会问钩子返回的 actor 是否 `allows` 它，不持有即 `403 forbidden`，`cause` 为 `capability`（`server/src/http.rs`）。钩子也能看到这项要求（`ReqMeta.capability`）以便自行判断，但它**不能**凭空授予：只能返回持有更少的 actor。
- **capability 是路由表的类型化列**，不是处理器记得去查的字符串（`server/src/routes.rs`）。写不出一条不声明 capability 的路由，也就不存在悄悄跳过检查的路由。
- **默认拒绝。** 除非 actor 确实持有路由所要的权限，否则一律拒绝；空集合的 actor 什么也到不了。「没有 capability」不可表达。
- **词汇表就是 §5 表格里的 32 个名字**（`agent.run`、`audit.read`、`runs.control`、`settings.write`……）。v0.9 只有两种 actor 形状：token 持有者（`operator`、`human`）持有全部 32 项；`--no-auth` 的默认持有同一集合，因此两者过了钩子之后行为一致。故 `403` 只可能来自返回更窄 actor 的钩子。按能力细分的 token 属 v1.0；这个集合就是它们日后的填充位置。
- 钩子返回的 `Actor` 就是该请求写下的每一行审计所携带的身份。「人做的」与「监工 AI 做的」由 `agent_id` 区分，正是 architecture-evolution.md §6 的要求。
- **token 永不落日志。** 不进访问日志、不进错误、不进审计 detail。钩子返回 `Actor`，原始 token 随即丢弃；`ReqMeta` 的 `Debug` 亦对其打码，误写的 `{:?}` 也写不出去。
- **传输安全归调用方（开源版边界）。** 开源版只提供明文 HTTP 加认证钩子，仅此而已。TLS 终止、网络边界、或只绑本地，是部署决策；把控制平面暴露到回环之外的分发方，自行负责把它放在 TLS 之后。这条写在这里，以免有集成者以为开源版替他做了。
- 未装 `Authn` 时，控制平面对所有请求返回 `unauthorized`（失败关闭）。没有匿名模式：`--no-auth` 的钩子是**已安装**的钩子，把所有人授权为 owner，而不是没有认证。

## 4. 错误模型

每个非 2xx 响应携带一个错误对象：

```json
{
  "code": "not_found",
  "message": "no run with id run-12345-7",
  "retryable": false,
  "cause": "run_id"
}
```

| 字段 | 类型 | 含义 |
|---|---|---|
| `code` | string | 稳定、机器可读。下列清单在 v0.9 内封闭。 |
| `message` | string | 人类可读，永不含机密。 |
| `retryable` | bool | 原样重试是否有合理成功可能。 |
| `cause` | string \| null | 出错的输入字段或子系统，无则 null。 |

错误码与 HTTP 状态码映射：

| `code` | HTTP | 何时 |
|---|---|---|
| `bad_request` | 400 | JSON 畸形、缺必填字段、`path`/`run_id` 非法——包括 workspace 策略拒绝的路径（逃出 workspace，或带 `..` 穿越），其 `cause` 为 `"path"`。 |
| `unauthorized` | 401 | 无 token，或钩子拒绝了 token。 |
| `forbidden` | 403 | 已认证，但 actor 缺少该端点的 capability。**`403` 只用于认证与授权**；服务端用不了的参数是 `400`。 |
| `not_found` | 404 | 未知 `run_id`、`session_id`、快照名。 |
| `method_not_allowed` | 405 | 路径存在，但不接受该方法；`message` 指出该用哪个。 |
| `conflict` | 409 | 状态冲突：`resume` 时无 VM、下载已在跑。 |
| `not_implemented` | 501 | 已预留、暂无内核方法的端点（§6）。 |
| `unavailable` | 503 | 依赖未就绪（无 LLM、无 QEMU、无工具链）。 |
| `internal` | 500 | 其余。 |

**与 `DispatchError` 的关系。** `DispatchError`（`agent/src/dispatch.rs`）是进程内的派发抽象：`NoSuchAgent(AgentId)` 与 `Failed(String)`。控制平面的映射为：`NoSuchAgent` → `not_found`（`cause: "agent_id"`），`Failed` → `internal` 且原因进 `message`。v0.9 只定义这个形状；跨设备传输会扩展它（超时、远端错误），该扩展明确不在本批范围内。

## 5. 端点表

查询类命令为 `GET`。控制类命令为 `POST`。「权限」列是服务端在处理器运行前检查的前置条件（§3；§6 缺口 G2）。最后一列是与端点对应的 Tauri 命令名，便于集成者把两个面对齐。

### 5.1 查询类（32）

| 端点 | 方法 | 权限 | 请求 | 响应 | 对应 Tauri 命令 |
|---|---|---|---|---|---|
| `/v0/audit/status` | GET | `audit.read` | — | `AuditStatusView` | `get_audit_status` |
| `/v0/audit/events` | GET | `audit.read` | query：`limit`、`actor`、`action_prefix` | `[StoredEventView]` | `list_audit_events` |
| `/v0/runs` | GET | `runs.read` | query：`limit`（默认 20） | `[RunView]` | `list_runs` |
| `/v0/runs/{run_id}` | GET | `runs.read` | path：`run_id` | `RunView` 或 `null` | `get_run` |
| `/v0/runs/diff` | GET | `runs.read` | query：`run_a`、`run_b` | `[FingerprintFieldDiff]` | `compare_run_fingerprints` |
| `/v0/llm/provider-presets` | GET | `llm.read` | — | `[ProviderPresetView]` | `get_provider_presets` |
| `/v0/llm/config` | GET | `llm.read` | — | `LlmConfigStatus` | `get_llm_config_status` |
| `/v0/llm/readiness` | GET | `llm.read` | — | `LlmReadiness` | `get_llm_readiness` |
| `/v0/llm/local-probe` | GET | `llm.read` | — | `LocalProbeResult` | `probe_local_llm` |
| `/v0/llm/stored-key` | GET | `llm.read` | query：`provider_id` | `{ "present": bool }` | `has_stored_key` |
| `/v0/sessions` | GET | `session.read` | query：`limit` | `[SessionMeta]` | `list_sessions` |
| `/v0/sessions/current` | GET | `session.read` | — | `{ "session_id": string \| null }` | `get_current_session_id` |
| `/v0/snapshots` | GET | `snapshot.read` | — | `[SnapshotMetaView]` | `list_snapshots` |
| `/v0/vm/running` | GET | `vm.read` | — | `{ "running": bool }` | `vm_is_running` |
| `/v0/vm/status` | GET | `vm.read` | — | `VmStatusView` | `vm_status` |
| `/v0/toolchain` | GET | `toolchain.read` | — | `ToolchainView` | `probe_toolchain` |
| `/v0/toolchain/download` | GET | `toolchain.read` | — | `ToolchainDownloadStatus` | `toolchain_download_status` |
| `/v0/qemu` | GET | `qemu.read` | — | `QemuView` | `probe_qemu` |
| `/v0/qemu/status` | GET | `qemu.read` | — | `QemuView` | `get_qemu_status` |
| `/v0/qemu/download` | GET | `qemu.read` | — | `QemuDownloadStatus` | `qemu_download_status` |
| `/v0/preflight` | GET | `preflight.read` | — | `PreflightView` | `preflight_status` |
| `/v0/settings/theme` | GET | `settings.read` | — | `{ "theme": string }` | `get_theme` |
| `/v0/settings/language` | GET | `settings.read` | — | `{ "language": string }` | `get_language` |
| `/v0/workspace/root` | GET | `workspace.read` | — | `{ "root": string }` | `workspace_root` |
| `/v0/workspace/files` | GET | `workspace.read` | — | `[string]` | `get_workspace_files` |
| `/v0/workspace/file` | GET | `workspace.read` | query：`path` | `{ "content": string }` | `read_workspace_file` |
| `/v0/serial` | GET | `serial.read` | — | `{ "buffer": string }` | `get_serial_buffer` |
| `/v0/sandboxes` | GET | `sandbox.read` | — | `{ "sandboxes": [SandboxView], "current": string \| null, "default": string }` | `list_sandboxes` |
| `/v0/sandboxes/current` | GET | `sandbox.read` | — | `{ "current": string \| null, "default": string }` | `current_sandbox` |
| `/v0/sandboxes/candidates` | GET | `sandbox.read` | — | `CandidatesView` | `sandbox_candidates` |
| `/v0/sandboxes/{name}` | GET | `sandbox.read` | path: `name` | `SandboxView`，或 `404` | `get_sandbox` |
| `/v0/sandboxes/requests` | GET | `sandbox.read` | query: `status`? | `{ "requests": [SandboxRequestView] }`，`status` 未知时 `400` | `list_sandbox_requests` |
| `/v0/executors` | GET | `agent.run` | 无 | `{ "executors": [{ "agent_id": string }] }` | `list_executors` |

### 5.2 控制类（36）—— 已于 v0.9 批次 4 实装，沙箱 F1、F2b-2、F2c、项目进出与任务端点扩充

| 端点 | 方法 | 权限 | 请求 | 响应 | 对应 Tauri 命令 |
|---|---|---|---|---|---|
| `/v0/agent/run` | POST | `agent.run` | `{ "user_input": string, "sandbox"? }` | `AgentOutcomeView` | `run_agent` |
| `/v0/tasks` | POST | `agent.run` | `{ "target": string, "input": string, "sandbox"?, "id"? }` | `TaskOutcome`，`target` 未知时 `404` | `dispatch_task` |
| `/v0/runs/export` | POST | `audit.export` | `{ "run_id", "path" }` | `{ "events_exported": number }` | `export_run_audit` |
| `/v0/vm/stop` | POST | `vm.control` | — | `204 No Content` | `stop_current_vm` |
| `/v0/snapshots/save` | POST | `snapshot.write` | `{ "name": string }` | `{ "bytes_written": number }` | `save_snapshot_real` |
| `/v0/snapshots/resume` | POST | `snapshot.write` | `{ "name": string }` | `204 No Content` | `resume_from_snapshot_real` |
| `/v0/snapshots/delete` | POST | `snapshot.write` | `{ "name": string }` | `{ "deleted": bool }` | `delete_snapshot` |
| `/v0/sessions/create` | POST | `session.write` | `{ "title": string }` | `{ "session_id": string }` | `create_session` |
| `/v0/sessions/open` | POST | `session.write` | `{ "session_id": string }` | `SessionDetailView` | `open_session` |
| `/v0/sessions/rename` | POST | `session.write` | `{ "session_id", "title" }` | `204 No Content` | `rename_session` |
| `/v0/sessions/delete` | POST | `session.write` | `{ "session_id": string }` | `204 No Content` | `delete_session` |
| `/v0/sessions/clear` | POST | `session.write` | — | `204 No Content` | `clear_all_sessions` |
| `/v0/toolchain/download` | POST | `toolchain.install` | — | `202 { "state": "started" }` | `start_toolchain_download` |
| `/v0/toolchain/download/cancel` | POST | `toolchain.install` | — | `202 { "state": "cancelling" }` | `cancel_toolchain_download` |
| `/v0/toolchain/path` | POST | `toolchain.configure` | `{ "path": string }` | `204 No Content` | `set_toolchain_path` |
| `/v0/toolchain/path/clear` | POST | `toolchain.configure` | — | `204 No Content` | `clear_toolchain_path` |
| `/v0/qemu/path` | POST | `qemu.configure` | `{ "path": string }` | `204 No Content` | `set_qemu_path` |
| `/v0/qemu/path/clear` | POST | `qemu.configure` | — | `204 No Content` | `clear_qemu_path` |
| `/v0/qemu/download` | POST | `qemu.configure` | — | `202 { "state": "started" }`；今天是每个平台都 `503 unavailable`（见下方注记） | `start_qemu_download` |
| `/v0/qemu/download/cancel` | POST | `qemu.configure` | — | `202 { "state": "cancelling" }`；没在跑则 `409` | `cancel_qemu_download` |
| `/v0/preflight/run` | POST | `preflight.run` | — | `202 { "state": "running" }` | `run_preflight` |
| `/v0/preflight/ack` | POST | `preflight.run` | — | `PreflightView` | `acknowledge_preflight` |
| `/v0/audit/alert` | POST | `settings.write` | `{ "enabled": bool }` | `204 No Content` | `set_audit_alert` |
| `/v0/audit/export` | POST | `audit.export` | `{ "path": string }` | `{ "events_exported": number }` | `export_audit_jsonl` |
| `/v0/settings/theme` | POST | `settings.write` | `{ "theme": string }` | `204 No Content` | `set_theme` |
| `/v0/settings/language` | POST | `settings.write` | `{ "language": string }` | `204 No Content` | `set_language` |
| `/v0/llm/config` | POST | `llm.configure` | `{ "api_key", "base_url", "model", "provider_id"?, "remember"? }` | `204 No Content` | `set_llm_config` |
| `/v0/llm/stored-key/load` | POST | `llm.configure` | `{ "provider_id": string }` | `204 No Content` | `load_stored_key` |
| `/v0/llm/config/clear` | POST | `llm.configure` | — | `204 No Content` | `clear_llm_config` |
| `/v0/serial/export` | POST | `serial.export` | `{ "path": string }` | `{ "bytes_written": number }` | `export_serial_log` |
| `/v0/sandboxes/switch` | POST | `sandbox.switch` | `{ "name": string }` | `{ "from": string \| null, "to": string }`，或 `404` / `409` / `503` / `500`（见下方注） | `switch_sandbox` |
| `/v0/sandboxes/requests` | POST | `agent.run` | `{ "action": "switch"\|"define"\|"assemble", "sandbox"?, "reason"? }` | `201 { "id": string }` | `request_sandbox` |
| `/v0/sandboxes/requests/{id}/approve` | POST | `sandbox.read`，再按请求的 action（见下方注） | — | `SandboxRequestView`，或 `404` / `409` / `403` | `approve_sandbox_request` |
| `/v0/sandboxes/requests/{id}/reject` | POST | 同 `approve` | — | `SandboxRequestView`，或 `404` / `409` / `403` | `reject_sandbox_request` |
| `/v0/workspace/import` | POST | `workspace.write` | **归档本体**（zip / tar.gz / tar），`Content-Type: application/zip` \| `application/gzip` \| `application/x-tar`；query `force`? | `200 { "files": number, "bytes": number }`，或 `400` / `409` / `413`（见下方注） | `import_workspace` |
| `/v0/workspace/export` | POST | `workspace.read` | — | **归档本体**（`application/gzip`，`Content-Disposition: attachment`） | `export_workspace` |

上表中的响应类型即 `host-core` 的视图类型（`host-core/src/state.rs`），客户端可直接从该文件读字段。`AgentOutcomeView` 为 `{ kind, content, reason, iterations }`，其中 `kind` 取 `final` / `max_iterations` / `failed`。

长时间运行的控制类命令立即应答，进度走 SSE（`/v0/agent/run` → `agent:*`；`/v0/preflight/run` → `preflight:progress`；`/v0/toolchain/download` → `toolchain:download`）。上表的 `202` 体是应答，不是结果。

### 5.3 宿主本地端点

有三个端点属于宿主进程而非内核，因此不在上面的表里。它们同样是本文接口面的一部分。

| 端点 | 方法 | 权限 | 应答 |
|---|---|---|---|
| `/v0/health` | GET | `health.read` | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `status.read` | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | `events.subscribe` | SSE 事件流（见 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)）。 |

### 5.4 表格附注

- **`/v0/tasks` 是路由，`/v0/agent/run` 是**在这里**跑。** 两者形似而不同的：`POST /v0/agent/run` 在**本节点**上跑一轮，`POST /v0/tasks` 把一条 `Task` 送到它的 `target` 指定的执行者，并回该执行者产出的 `TaskOutcome`。节点**故意不是**它自己的执行者之一——目标写它就是 `404`——所以两个端点从不重叠；`GET /v0/executors` 列出任务能抵达谁。执行者队伍来自 `settings.json` 里的 `executors`（label + program + args），**仅此一处**：v0.9 没有运行时注册端点。不带 `id` 的任务由服务端补一个；而一次**失败**的运行依旧是 `200`——它的 `outcome` 自会说明；`404` / `500` 分别留给无人拥有的目标与断掉的派发。

- **`POST /v0/qemu/download` 按决定在每个平台上都拒绝。** RiscDom 引导用户自己安装 QEMU
  （`docs/qemu-distribution.md` §5）：没有 pin 任何发布版，因此该端点答 `503 unavailable`、
  `cause: "qemu"`，`message` 里是安装指引，且不会占下载槽。它背后的东西——槽、状态、取消、
  `qemu:download` 事件族与采用步骤——都已齐备，所以将来 pin 一个版本是**数据变更**：答案会变成 `202`。
- **两条审计导出给的是事件数，不是字节数。** 宿主的 `write_events_jsonl` 返回
  `events.len()`，因此 `/v0/audit/export` 与 `/v0/runs/export` 答
  `{ "events_exported": number }`——字段名说的就是数的是什么。`/v0/serial/export` 写的是捕获到的
  串口文本，确实按字节计数，因此保留 `bytes_written`。
- **逃出 workspace 的路径是 `400`，不是 `403`。** 导出与 `/v0/workspace/file` 的路径都过宿主的
  workspace 策略，策略拒绝的路径（逃出根目录，或带 `..` 穿越）属于调用方参数不可用：
  `400 bad_request`，`cause: "path"`。`403` 留给 §3 的 capability 检查。
- **查询类与控制类均已实现。** `POST /v0/vm/start`（§6 G1）与 `/v0/resources`（§6 G3）在各自的内核工作落地前回 `501`。
- **报成功的控制操作可能什么都没改。** 宿主的会话改名与删除是幂等的：未知 `session_id` 不算错误（端点回 `204`），而 `/v0/sessions/open` 回 `404`。端点是照搬宿主，而不是另造一套差异。
- **`POST /v0/toolchain/download` 会真的开始下载**固定的 RISC-V GCC 归档并回 `202`；进度以 `toolchain:download` 事件抵达。
- **沙箱查询读的是合并后的注册表，且从不写它**（v0.9 沙箱 F2a-2）。`/v0/sandboxes` 把三个来源摆进一个列表——手写定义、扫描所得、内置 `default`——每项携带 `source`（`manual` / `discovered`）、`runnable`（每次读取现算，从不存储）与 `shadowed`。同名时手写者胜，而被遮的那项**留在列表里并标出**。`/v0/sandboxes/candidates` 答的是原始扫描（两个互相独立的列表），其中没有任何一项是定义。`/v0/sandboxes/{name}` 在没有这个名字的定义时答 `404`，并在 `cause` 指出参数；字面子路径（`current`、`candidates`，以及 F2 线后面才落的两个：`requests`、`assemble`）永不被当作名字读。切换现在是它自己的路由（`POST`，见上表），故对它发 `GET` 是 `405`。
- **沙箱申请是一条请求，不是一条命令**（v0.9 沙箱 F2c）。`POST /v0/sandboxes/requests` 需要 `agent.run`——能跑 agent 的 actor 就是可以表达它所想的 actor——并答 `201` 带新 id。`GET /v0/sandboxes/requests?status=` 读队列（`sandbox.read`），新的在前；`status` 未知时 `400`。两条决策需要 `sandbox.read` 作为**路由的**门（决策者先要看得见队列），然后在**处理器内部**需要该请求自己的 `action` 所隐含的 capability：`switch` 申请需要 `sandbox.switch`，`define` / `assemble` 需要 `sandbox.assemble`。未持有即 `403 forbidden`，`cause: "capability"` 并指名是哪一个。这个区分就是重点：把节点搬走、和给它一个要跑的新定义，是两种不同的权力。**批准不执行任何事**——切换是另一次带授权的 `POST /v0/sandboxes/switch` 调用，所以一个指向不存在定义的申请照样可批。id 未知是 `404 not_found`、`cause: "id"`；对同一请求再次决策是 `409 conflict`——决策不可逆。**v0.9 没有 TTL**：`expired` 在状态词汇里存在，但没有任何路径产生它，pending 请求一直等到有人决它。
- **沙箱切换为每种失败各答一个状态**（v0.9 沙箱 F2b-2）。`POST /v0/sandboxes/switch` 是沙箱表面上唯一的一写，且是同步的：校验、停止、启动。它的应答都选成让客户端按名字分支、而不是按句子：新沙箱已在跑时 `200` 带 `{from, to}`；没有这个名字的定义时 `404 not_found`、`cause: "name"`（与名字路由同答）；运行中 `409 conflict`、`cause: "run"`（切换会拿走 loop 正在用的 VM），另一次切换进行中 `409 conflict`、`cause: "sandbox"`；定义不能跑时 `503 unavailable`，`cause` 就是原因码（`sandbox_qemu_missing`、`sandbox_toolchain_missing`、`sandbox_kernel_missing`）；每项校验都过而复 VM 仍起不来时 `500 internal`、`cause: "sandbox_start_failed"`——此时节点是**已停**，不是半切换。
- **项目以一个文件的形式离开、再以一个文件的形式回来**（v0.9 项目进出）。`POST /v0/workspace/export` 把 workspace 以一个 `tar.gz` 作答——**字节，不是 JSON**，是除事件流之外这个表面上的第一个此类 body——带 `Content-Disposition: attachment; filename="workspace.tar.gz"`。空 workspace 导出的是合法的空归档：「导出这个项目」不会因为项目是空的而失败。宿主自己的状态目录（`.riscdom/`：审计库、快照、预检缓存）**不打包**。`POST /v0/workspace/import` 把归档当**请求体**（不是 JSON）收，接受 `application/zip`、`application/gzip`、`application/x-tar`——`Content-Type` 说了它不认识的东西时，改看字节本身，所以一个 `application/octet-stream` 传上去的 `.tar.gz` 照样能用。它答 `{files, bytes}`。一条 entry 不允许做的事全由宿主检查，每件各有自己的应答：逃出 workspace、以符号链接/硬链接到访、命名 `.riscdom/`、或干脆不是可读归档 → `400`、`cause: "archive"`；workspace 里已有同名文件 → `409`、`cause: "exists"`，除非 `?force=true` 说了要替换；请求体超过 **64 MiB** → `413 payload_too_large`——这是 import 自己的上限，不是每个 JSON body 共用的那个 64 KiB。导入需要 `workspace.write`（第 32 个 capability），导出需要 `workspace.read`：读一个项目和替换它是两种权限。导入是**只做包含性检查**（不走扩展名白名单），理由与现有 exports 相同：项目不只有 `.c` / `.h` / `.S` / `.s`。
- **一次运行可以声明它要用哪个沙箱**（v0.9 沙箱 F2d）。`POST /v0/agent/run` 接受可选的 `sandbox` 名字，而它是**声明，不是切换**：这次运行用它启 VM（它的工具链、它的 QEMU、它的内存），而节点的 `current_sandbox` 原地不动——搬动节点是 `POST /v0/sandboxes/switch`，需要 `sandbox.switch`。三个应答，按此顺序，让调用方听到最具体的那个：没有这个名字的定义 → `404`、`cause: "name"`（拼写错误不能变成在另一个沙箱下的运行）；不是正在跑的 VM 所来自的定义 → `409`、`cause: "sandbox"`（VM 不能在运行时里被替换，报文指名两条出路——停掉它，或切换），而什么都没在跑时声明就被执行；**之后**才问环境，所以没有模型就是文档里的 `503`、`cause: "llm"`。不声明时解析照旧：节点在跑的、然后配置的默认、再是内置兕底——后者意味着「去发现宿主自己的 QEMU 与工具链」，与上一批之前完全一致。声明**不**到内核：启哪个 ELF 仍是模型的 `start_vm` 参数——因为那是运行时启的，而 `def.kernel` 是*切换*时启的。
- **权限既声明、也强制。** 每条路由在路由表里标注自己的 capability，处理器运行前服务端拿它与钩子返回的 actor 比对；不持有即 `403 forbidden`，`cause` 为 `"capability"`（§3）。v0.9 默认下每个 actor 都持有全部 32 项，故 `403` 只可能来自返回更窄 actor 的钩子——以及来自两条申请决策：它们在本路由自己的门通过之后，再检查该请求 `action` 所隐含的 capability。
- **参数。** 必填参数缺失或无法解析 → `400 bad_request`，`cause` 为该参数名。`limit` 在宿主命令要求处为必填、其余为可选：`/v0/runs` 默认 20，`/v0/audit/events` 与 `/v0/sessions` 必填。`/v0/workspace/file` 的 `?path=` 会做百分号解码。
- **`/v0/audit/status` 不消费失败队列。** Tauri 命令会**取走**待报的审计失败；`GET` 不能取，否则一个轮询客户端会吞掉另一个客户端的告警。该端点按现状报告队列。
- **`/v0/runs/diff` 遇到不存在的 run 回 `internal`。** 宿主把「找不到 run」报成不透明消息而非有类型的 not-found，控制平面若不臆造规则就无法映射成 `404`。一个宿主侧的类型化错误能闭合它；不在本批内。

## 6. 四项内核能力缺口

侦察发现四项内核能力没有一等命令。这里逐项明确处置，一项都不静默丢弃。

- **G1 — 启动 VM。** 决定：**独立端点 `POST /v0/vm/start`，v0.9 内预留并返回 `501 not_implemented`。** 内核把 VM 的起停当作能力（architecture-evolution.md §5），而当前 VM 在 `run_agent` 里隐式启动、随后由宿主持有。一个没有归属任务的显式启动尚无生命周期语义，所以现在只预留端点，由后续实现批次补上新的 `AppState` 方法。停止早已是实的（`POST /v0/vm/stop`）。
- **G2 — permission check。** 决定：**不做端点。** `check(capability)` 是**每个**端点执行前求值的前置条件，不是客户端调用的资源。形状：钩子先解析出 `Actor`（§3），随后请求路径按 §5 表格中的 capability 在处理器运行前检查。缺权限即 `403 forbidden`。该检查已实装：路由表的 capability 是类型化列（`server/src/routes.rs`），判定发生在请求路径中（`server/src/http.rs`）。内核级 `check()` 方法仍不需要，因为路由表就是每个端点的唯一事实来源。
- **G3 — resource accounting。** 决定：**预留聚合端点 `GET /v0/resources`，v0.9 内返回 `501`。** 已定的形状为 `{ "vm": {"running", "since_ms"}, "runs": {"active", "total"}, "sessions": {"count"}, "downloads": {"active"} }`，由既有的状态查询拼出。不为它新增内核方法；实现时聚合的就是 §5.1 的那些视图。
- **G4 — abandon stale runs。** 决定：**暴露，`POST /v0/runs/abandon-stale`。** 这是特殊的一个：内核方法已存在（`AppState::abandon_stale_runs`）且启动时被调用，但没有 Tauri 命令包它。响应：`{ "abandoned": [run_id, ...] }`。幂等，可反复调用。

## 7. 版本与兼容

- 整个 v0.x 线的路径前缀都是 `/v0/`。v0.x 内破坏性改动不升前缀；客户端必须容忍。
- **v1.0 冻结 API。** 自 v1.0 起路径变为 `/v1/`，此后加字段不升版本，改语义才升版本。
- v0.x 期间客户端固定的是 RiscDom 版本区间，不是 API 版本。这是对不稳定的诚实，而非假装稳定。

## 8. 示例

查询——最近十条审计事件：

```bash
curl -sS http://127.0.0.1:7788/v0/audit/events?limit=10 \
  -H 'Authorization: Bearer <token>'
```

带过滤的查询——只看某个 actor 的工具调用：

```bash
curl -sS 'http://127.0.0.1:7788/v0/audit/events?actor=dev-12345-1&action_prefix=agent.tool' \
  -H 'Authorization: Bearer <token>'
```

控制——跑一轮 agent（进度走 SSE 流）：

```bash
curl -sS -X POST http://127.0.0.1:7788/v0/agent/run \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"user_input":"compile the blink example and show the serial output"}'
```

控制——停止 VM：

```bash
curl -sS -X POST http://127.0.0.1:7788/v0/vm/stop \
  -H 'Authorization: Bearer <token>'
```

错误——不存在的 run id（`HTTP/1.1 404 Not Found`）：

```json
{
  "code": "not_found",
  "message": "no run with id run-12345-7",
  "retryable": false,
  "cause": "run_id"
}
```

错误——无 token（`HTTP/1.1 401 Unauthorized`）：

```json
{
  "code": "unauthorized",
  "message": "missing or invalid bearer token",
  "retryable": false,
  "cause": "authorization"
}
```

事件推送——订阅（帧格式见 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)）：

```bash
curl -sS -N http://127.0.0.1:7788/v0/events \
  -H 'Authorization: Bearer <token>' \
  -H 'Accept: text/event-stream'
```

`127.0.0.1:7788` 是示例绑定；地址属部署设置。
