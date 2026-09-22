[English](control-plane-api.md) | 中文

# 控制平面 HTTP API（设计）

> **适用 v0.9，v1.0 冻结。** 本文是设计文档：它定义集成者照着实现客户端的接口，不是今天就交付的实现。

**面向读者。** 发行版与集成的开发者：要嵌入 RiscDom 的人、要写控制平面客户端的人、要把 RiscDom 接进自己系统的人。本文不是用户手册——「如何安装与运行 RiscDom」写在别处。

**这是什么。** RiscDom v0.9 的主线是控制平面：人监督 AI 与 AI 监督 AI 走**同一套** HTTP 接口。在内核看来，来自监工 AI 的指令和来自人的指令都是控制平面授权的指令；审计链靠 `agent_id` 区分二者。本条设计存在的意义，就是避免去建两条会各自演化、最终冲突的控制通道。

**实现状态（v0.9）。** §5.1 的 26 个查询端点、§5.3 的宿主本地端点、§4 的错误模型、以及事件 envelope 均已实现。§5.2（控制类）、`gap` 帧、以及权限强制尚未实现。

## 1. 定位与协议

- **Layer 3。** 控制平面是一个宿主（[architecture-evolution.md](architecture-evolution.md) §4 的定义）：它与 `ui/src-tauri` 并列，只依赖 Layer 2 的稳定 API（`host` 的公开面）。它不直接触碰 `agent` / `sandbox` / `audit`。
- **传输层。** 命令与查询走 HTTP，事件推送走 **SSE**（Server-Sent Events）。刻意不用 WebSocket；理由记在 [handoff.md](handoff.zh-CN.md) §1，线格式见 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)。
- **设备无关语义。** 同一套协议承载本地 IPC 与网络连接。本地套接字与远程主机对同一请求回同一份载荷，只有传输地址不同。这就是 architecture-evolution.md §7 留下的那道「缝」。
- **每个内核能力都要有端点。** architecture-evolution.md §6 与 §12 的约束：内核能力若没有控制平面端点，官方管理程序就用不上它，那就是装饰。下文 §5 是覆盖表，缺口一并列出。

## 2. 约定

- 基础路径：`/v0/`——「v0」意为不稳定。v0.x 内错误码、字段名、端点路径都可能变。这个前缀是「诚实地破坏」的承诺，不是「保持不变」的承诺。
- 内容类型：请求与响应为 `application/json; charset=utf-8`，SSE 流除外（`text/event-stream`）。
- 身份：`agent_id` 为 `<device>-<pid>-<seq>`，`task_id` 为 `task-<pid>-<seq>`（v0.8 批次 B）。客户端把二者都当作不透明字符串。
- 响应体永远不含 API key 或 token。这与宿主的既有不变式一致：前端永远看不到 key。

## 3. 认证

- 客户端发送 `Authorization: Bearer <token>`。
- 控制平面只预留一个钩子，做成 trait 形状，机制日后再定：

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
}
```

- 钩子返回的 `Actor` 就是该请求写下的每一行审计所携带的身份。「人做的」与「监工 AI 做的」由 `agent_id` 区分，正是 architecture-evolution.md §6 的要求。
- **token 永不落日志。** 不进访问日志、不进错误、不进审计 detail。钩子返回 `Actor`，原始 token 随即丢弃。
- **传输安全归调用方（开源版边界）。** 开源版只提供明文 HTTP 加认证钩子，仅此而已。TLS 终止、网络边界、或只绑本地，是部署决策；把控制平面暴露到回环之外的分发方，自行负责把它放在 TLS 之后。这条写在这里，以免有集成者以为开源版替他做了。
- 未装 `Authn` 时，控制平面对所有请求返回 `unauthorized`（失败关闭）。没有匿名模式。

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
| `bad_request` | 400 | JSON 畸形、缺必填字段、`path`/`run_id` 非法。 |
| `unauthorized` | 401 | 无 token，或钩子拒绝了 token。 |
| `forbidden` | 403 | 已认证，但 actor 缺少该端点的 capability。 |
| `not_found` | 404 | 未知 `run_id`、`session_id`、快照名。 |
| `method_not_allowed` | 405 | 路径存在，但不接受该方法；`message` 指出该用哪个。 |
| `conflict` | 409 | 状态冲突：`resume` 时无 VM、下载已在跑。 |
| `not_implemented` | 501 | 已预留、暂无内核方法的端点（§6）。 |
| `unavailable` | 503 | 依赖未就绪（无 LLM、无 QEMU、无工具链）。 |
| `internal` | 500 | 其余。 |

**与 `DispatchError` 的关系。** `DispatchError`（`agent/src/dispatch.rs`）是进程内的派发抽象：`NoSuchAgent(AgentId)` 与 `Failed(String)`。控制平面的映射为：`NoSuchAgent` → `not_found`（`cause: "agent_id"`），`Failed` → `internal` 且原因进 `message`。v0.9 只定义这个形状；跨设备传输会扩展它（超时、远端错误），该扩展明确不在本批范围内。

## 5. 端点表

查询类命令为 `GET`。控制类命令为 `POST`。「权限」列是认证层检查的前置条件（如何强制见 §6 缺口 G2）。最后一列是与端点对应的 Tauri 命令名，便于集成者把两个面对齐。

### 5.1 查询类（26）

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
| `/v0/preflight` | GET | `preflight.read` | — | `PreflightView` | `preflight_status` |
| `/v0/settings/theme` | GET | `settings.read` | — | `{ "theme": string }` | `get_theme` |
| `/v0/settings/language` | GET | `settings.read` | — | `{ "language": string }` | `get_language` |
| `/v0/workspace/root` | GET | `workspace.read` | — | `{ "root": string }` | `workspace_root` |
| `/v0/workspace/files` | GET | `workspace.read` | — | `[string]` | `get_workspace_files` |
| `/v0/workspace/file` | GET | `workspace.read` | query：`path` | `{ "content": string }` | `read_workspace_file` |
| `/v0/serial` | GET | `serial.read` | — | `{ "buffer": string }` | `get_serial_buffer` |

### 5.2 控制类（27）

| 端点 | 方法 | 权限 | 请求 | 响应 | 对应 Tauri 命令 |
|---|---|---|---|---|---|
| `/v0/agent/run` | POST | `agent.run` | `{ "user_input": string }` | `AgentOutcomeView` | `run_agent` |
| `/v0/runs/export` | POST | `audit.export` | `{ "run_id", "path" }` | `{ "bytes_written": number }` | `export_run_audit` |
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
| `/v0/preflight/run` | POST | `preflight.run` | — | `202 { "state": "running" }` | `run_preflight` |
| `/v0/preflight/ack` | POST | `preflight.run` | — | `PreflightView` | `acknowledge_preflight` |
| `/v0/audit/alert` | POST | `settings.write` | `{ "enabled": bool }` | `204 No Content` | `set_audit_alert` |
| `/v0/audit/export` | POST | `audit.export` | `{ "path": string }` | `{ "bytes_written": number }` | `export_audit_jsonl` |
| `/v0/settings/theme` | POST | `settings.write` | `{ "theme": string }` | `204 No Content` | `set_theme` |
| `/v0/settings/language` | POST | `settings.write` | `{ "language": string }` | `204 No Content` | `set_language` |
| `/v0/llm/config` | POST | `llm.configure` | `{ "api_key", "base_url", "model", "provider_id"?, "remember"? }` | `204 No Content` | `set_llm_config` |
| `/v0/llm/stored-key/load` | POST | `llm.configure` | `{ "provider_id": string }` | `204 No Content` | `load_stored_key` |
| `/v0/llm/config/clear` | POST | `llm.configure` | — | `204 No Content` | `clear_llm_config` |
| `/v0/serial/export` | POST | `serial.export` | `{ "path": string }` | `{ "bytes_written": number }` | `export_serial_log` |

上表中的响应类型即 `host` 的视图类型（`host/src/state.rs`），客户端可直接从该文件读字段。`AgentOutcomeView` 为 `{ kind, content, reason, iterations }`，其中 `kind` 取 `final` / `max_iterations` / `failed`。

长时间运行的控制类命令立即应答，进度走 SSE（`/v0/agent/run` → `agent:*`；`/v0/preflight/run` → `preflight:progress`；`/v0/toolchain/download` → `toolchain:download`）。上表的 `202` 体是应答，不是结果。

### 5.3 宿主本地端点

有三个端点属于宿主进程而非内核，因此不在上面的表里。它们同样是本文接口面的一部分。

| 端点 | 方法 | 权限 | 应答 |
|---|---|---|---|
| `/v0/health` | GET | `health.read` | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `status.read` | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | `events.subscribe` | SSE 事件流（见 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)）。 |

### 5.4 表格附注

- **查询类已实现；§5.2 尚未。** §5.1 的每个端点今天都有应答，`/v0/resources` 在聚合落地前回 `501`（§6，G3）。
- **权限是声明、不是强制。** 服务端为每个端点标注权限，并经 `ReqMeta.capability` 交给 `Authn` 钩子；判断 actor **是否持有**该权限是权限中介的事，属后续批次。v0.9 默认（`NoAuth`）下所有查询均放行，`403` 只可能来自自行拒绝的钩子。
- **参数。** 必填参数缺失或无法解析 → `400 bad_request`，`cause` 为该参数名。`limit` 在宿主命令要求处为必填、其余为可选：`/v0/runs` 默认 20，`/v0/audit/events` 与 `/v0/sessions` 必填。`/v0/workspace/file` 的 `?path=` 会做百分号解码。
- **`/v0/audit/status` 不消费失败队列。** Tauri 命令会**取走**待报的审计失败；`GET` 不能取，否则一个轮询客户端会吞掉另一个客户端的告警。该端点按现状报告队列。
- **`/v0/runs/diff` 遇到不存在的 run 回 `internal`。** 宿主把「找不到 run」报成不透明消息而非有类型的 not-found，控制平面若不臆造规则就无法映射成 `404`。一个宿主侧的类型化错误能闭合它；不在本批内。

## 6. 四项内核能力缺口

侦察发现四项内核能力没有一等命令。这里逐项明确处置，一项都不静默丢弃。

- **G1 — 启动 VM。** 决定：**独立端点 `POST /v0/vm/start`，v0.9 内预留并返回 `501 not_implemented`。** 内核把 VM 的起停当作能力（architecture-evolution.md §5），而当前 VM 在 `run_agent` 里隐式启动、随后由宿主持有。一个没有归属任务的显式启动尚无生命周期语义，所以现在只预留端点，由后续实现批次补上新的 `AppState` 方法。停止早已是实的（`POST /v0/vm/stop`）。
- **G2 — permission check。** 决定：**不做端点。** `check(capability)` 是**每个**端点执行前求值的前置条件，不是客户端调用的资源。形状：认证层先解析出 `Actor`（§3），再按 §5 表格中的 capability 在处理器运行前检查。缺权限即 `403 forbidden`。该内核方法本身尚不存在；本批只固定接口。
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
