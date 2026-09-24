[English](tool-schema-control-plane.md) | 中文

# 工具 schema：控制平面，给 AI 监工用

> **适用于 v0.9（在 v1.0 之前不稳定）。** 下面每个端点都写成了一条 OpenAI/DeepSeek 工具定义。把你需要的那一组贴进聊天请求的 `tools[]`，模型就能自己驱动这个节点。路由表由测试与服务端自己的路由表校对；见[本文件如何保持为真](#本文件如何保持为真)。

## 1. 这是什么，不是什么

RiscDom v0.9 的承诺是工作可以分工：一个 AI 驱动节点，其它的在节点里跑。**监工**就是驱动的那一个，而它是外部进程——它通过 HTTP 与控制平面说话（见 [control-plane-client-guide.zh-CN.md](control-plane-client-guide.zh-CN.md)），不碰内核里的任何东西。

所以监工的工具集**就是控制平面**，写成工具的样子。本文件就是那个集合：一个端点一条工具，并标出该端点需要的 capability，让监工作者知道一条工具要用哪种凭据。

**它不**是执行者**内部**那个模型的工具集。那个模型调用的是另外八个工具（写源文件、编译、启 VM……）——它们在 [tool-schema-executor.zh-CN.md](tool-schema-executor.zh-CN.md)。这个区分正是要点：

| | 执行者的模型 | 监工 |
|---|---|---|
| 在哪跑 | 内核里面，每轮一次 | 外面，作为一个进程 |
| 接口 | `tool_specs()` 的八个工具 | HTTP 上的控制平面 |
| schema | [tool-schema-executor.zh-CN.md](tool-schema-executor.zh-CN.md) | 本文件 |
| 以什么作答 | `AgentOutcome` | HTTP 状态码 + JSON |

## 2. 名字，以及它们怎么来的

`name` 从路径机械地推导出来，好让一条工具与一条路由不可能悄悄漂开：

1. 去掉 `/v0/`；
2. 把每个 `/`、`-`、`{`、`}` 和 `.` 换成 `_`；
3. 合并连续的 `_`，去掉首尾的 `_`。

`POST /v0/runs/abandon-stale` → `runs_abandon_stale`，`GET /v0/llm/provider-presets` → `llm_provider_presets`。

两条写明的例外：

- **同时服务 `GET` 与 `POST` 的路径**（共六条：`toolchain/download`、`qemu/download`、`settings/theme`、`settings/language`、`llm/config`、`sandboxes/requests`）给 `POST` 加后缀 **`_post`**，于是 `POST /v0/settings/theme` → `settings_theme_post`。
- **四条带路径参数的路由**用一个动词，而不是把路径拼起来，因为 `runs_run_id` 对谁都没帮助：`GET /v0/runs/{run_id}` → `run_get`，`GET /v0/sandboxes/{name}` → `sandbox_get`，两条申请裁决 → `sandbox_request_approve` / `sandbox_request_reject`。

工具名在整个集合里唯一（有检查）。

## 3. 每个端点作为一条工具

`arguments` 就是请求：`GET` 是查询串，`POST` 是 JSON body。

### 3.1 查询类（32）

<!-- tool-routes:queries:begin -->
| Tool | Method | Path | Capability | Arguments |
|---|---|---|---|---|
| `audit_status` | GET | `/v0/audit/status` | `audit.read` | — |
| `audit_events` | GET | `/v0/audit/events` | `audit.read` | `limit` (int, required), `actor` (str), `action_prefix` (str) |
| `runs` | GET | `/v0/runs` | `runs.read` | `limit` (int, default 20) |
| `runs_diff` | GET | `/v0/runs/diff` | `runs.read` | `run_a` (str), `run_b` (str) |
| `llm_provider_presets` | GET | `/v0/llm/provider-presets` | `llm.read` | — |
| `llm_config` | GET | `/v0/llm/config` | `llm.read` | — |
| `llm_readiness` | GET | `/v0/llm/readiness` | `llm.read` | — |
| `llm_local_probe` | GET | `/v0/llm/local-probe` | `llm.read` | — |
| `llm_stored_key` | GET | `/v0/llm/stored-key` | `llm.read` | `provider_id` (str) |
| `sessions` | GET | `/v0/sessions` | `session.read` | `limit` (int) |
| `sessions_current` | GET | `/v0/sessions/current` | `session.read` | — |
| `snapshots` | GET | `/v0/snapshots` | `snapshot.read` | — |
| `vm_running` | GET | `/v0/vm/running` | `vm.read` | — |
| `vm_status` | GET | `/v0/vm/status` | `vm.read` | — |
| `toolchain` | GET | `/v0/toolchain` | `toolchain.read` | — |
| `toolchain_download` | GET | `/v0/toolchain/download` | `toolchain.read` | — |
| `qemu` | GET | `/v0/qemu` | `qemu.read` | — |
| `qemu_status` | GET | `/v0/qemu/status` | `qemu.read` | — |
| `qemu_download` | GET | `/v0/qemu/download` | `qemu.read` | — |
| `preflight` | GET | `/v0/preflight` | `preflight.read` | — |
| `settings_theme` | GET | `/v0/settings/theme` | `settings.read` | — |
| `settings_language` | GET | `/v0/settings/language` | `settings.read` | — |
| `workspace_root` | GET | `/v0/workspace/root` | `workspace.read` | — |
| `workspace_files` | GET | `/v0/workspace/files` | `workspace.read` | — |
| `workspace_file` | GET | `/v0/workspace/file` | `workspace.read` | `path` (str, required) |
| `serial` | GET | `/v0/serial` | `serial.read` | — |
| `sandboxes` | GET | `/v0/sandboxes` | `sandbox.read` | — |
| `sandboxes_current` | GET | `/v0/sandboxes/current` | `sandbox.read` | — |
| `sandboxes_candidates` | GET | `/v0/sandboxes/candidates` | `sandbox.read` | — |
| `sandboxes_requests` | GET | `/v0/sandboxes/requests` | `sandbox.read` | `status` (str) |
| `resources` | GET | `/v0/resources` | `vm.read` | — (reserved: answers `501`) |
| `executors` | GET | `/v0/executors` | `agent.run` | — |
<!-- tool-routes:queries:end -->

### 3.2 控制类（36）

<!-- tool-routes:controls:begin -->
| Tool | Method | Path | Capability | Arguments |
|---|---|---|---|---|
| `agent_run` | POST | `/v0/agent/run` | `agent.run` | `user_input` (str), `sandbox` (str) |
| `tasks` | POST | `/v0/tasks` | `agent.run` | `target` (str), `input` (str), `sandbox` (str), `id` (str) |
| `runs_export` | POST | `/v0/runs/export` | `audit.export` | `run_id` (str), `path` (str, workspace-relative truth) |
| `runs_abandon_stale` | POST | `/v0/runs/abandon-stale` | `runs.control` | — |
| `vm_start` | POST | `/v0/vm/start` | `vm.control` | — (reserved: answers `501`) |
| `vm_stop` | POST | `/v0/vm/stop` | `vm.control` | — |
| `snapshots_save` | POST | `/v0/snapshots/save` | `snapshot.write` | `name` (str) |
| `snapshots_resume` | POST | `/v0/snapshots/resume` | `snapshot.write` | `name` (str) |
| `snapshots_delete` | POST | `/v0/snapshots/delete` | `snapshot.write` | `name` (str) |
| `sessions_create` | POST | `/v0/sessions/create` | `session.write` | `title` (str) |
| `sessions_open` | POST | `/v0/sessions/open` | `session.write` | `session_id` (str) |
| `sessions_rename` | POST | `/v0/sessions/rename` | `session.write` | `session_id` (str), `title` (str) |
| `sessions_delete` | POST | `/v0/sessions/delete` | `session.write` | `session_id` (str) |
| `sessions_clear` | POST | `/v0/sessions/clear` | `session.write` | — |
| `toolchain_download_post` | POST | `/v0/toolchain/download` | `toolchain.install` | — (body: `toolchain` = `c` or `zig`; default `c`) |
| `toolchain_download_cancel` | POST | `/v0/toolchain/download/cancel` | `toolchain.install` | — |
| `qemu_download_post` | POST | `/v0/qemu/download` | `qemu.configure` | — (refuses with install guidance today) |
| `qemu_download_cancel` | POST | `/v0/qemu/download/cancel` | `qemu.configure` | — |
| `toolchain_path` | POST | `/v0/toolchain/path` | `toolchain.configure` | `path` (str) |
| `toolchain_path_clear` | POST | `/v0/toolchain/path/clear` | `toolchain.configure` | — |
| `qemu_path` | POST | `/v0/qemu/path` | `qemu.configure` | `path` (str) |
| `qemu_path_clear` | POST | `/v0/qemu/path/clear` | `qemu.configure` | — |
| `preflight_run` | POST | `/v0/preflight/run` | `preflight.run` | — |
| `preflight_ack` | POST | `/v0/preflight/ack` | `preflight.run` | — |
| `audit_alert` | POST | `/v0/audit/alert` | `settings.write` | `enabled` (bool) |
| `audit_export` | POST | `/v0/audit/export` | `audit.export` | `path` (str) |
| `settings_theme_post` | POST | `/v0/settings/theme` | `settings.write` | `theme` (str) |
| `settings_language_post` | POST | `/v0/settings/language` | `settings.write` | `language` (str) |
| `llm_config_post` | POST | `/v0/llm/config` | `llm.configure` | `api_key` (str), `base_url` (str), `model` (str), `provider_id` (str), `remember` (bool) |
| `llm_stored_key_load` | POST | `/v0/llm/stored-key/load` | `llm.configure` | `provider_id` (str) |
| `llm_config_clear` | POST | `/v0/llm/config/clear` | `llm.configure` | — |
| `serial_export` | POST | `/v0/serial/export` | `serial.export` | `path` (str) |
| `sandboxes_switch` | POST | `/v0/sandboxes/switch` | `sandbox.switch` | `name` (str) |
| `sandboxes_requests_post` | POST | `/v0/sandboxes/requests` | `agent.run` | `action` (str), `sandbox` (str), `reason` (str) |
| `workspace_import` | POST | `/v0/workspace/import` | `workspace.write` | the archive itself as the body (`?force=true` to replace) |
| `workspace_export` | POST | `/v0/workspace/export` | `workspace.read` | — (bytes out, not JSON) |
<!-- tool-routes:controls:end -->

### 3.3 本机端点，与带路径参数的路由（3 + 4）

<!-- tool-routes:locals:begin -->
| Tool | Method | Path | Capability | Arguments |
|---|---|---|---|---|
| `health` | GET | `/v0/health` | `health.read` | — |
| `status` | GET | `/v0/status` | `status.read` | — |
| `events` | GET | `/v0/events` | `events.subscribe` | — |
<!-- tool-routes:locals:end -->

<!-- tool-routes:patterns:begin -->
| Tool | Method | Path | Capability | Arguments |
|---|---|---|---|---|
| `run_get` | GET | `/v0/runs/{run_id}` | `runs.read` | `run_id` (str) |
| `sandbox_get` | GET | `/v0/sandboxes/{name}` | `sandbox.read` | `name` (str) |
| `sandbox_request_approve` | POST | `/v0/sandboxes/requests/{id}/approve` | `sandbox.read`, then the request's action | `id` (str) |
| `sandbox_request_reject` | POST | `/v0/sandboxes/requests/{id}/reject` | as `approve` | `id` (str) |
<!-- tool-routes:patterns:end -->

## 4. 定义

每行是一条可放进 `tools[]` 的条目。`GET` 的参数走查询串，`POST` 的参数走 JSON body——那次 HTTP 调用是监工自己的事；这些对象只说明模型可以要什么。

### 4.1 查询类

<!-- tool-defs:queries:begin -->
```json
[
{"type":"function","function":{"name":"audit_status","description":"The audit chain's verdict: how many events it holds and whether it verifies.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"audit_events","description":"Recent audit events, newest first.","parameters":{"type":"object","properties":{"limit":{"type":"integer"},"actor":{"type":"string"},"action_prefix":{"type":"string"}},"required":["limit"]}}}
{"type":"function","function":{"name":"runs","description":"The run index, newest first.","parameters":{"type":"object","properties":{"limit":{"type":"integer"}},"required":[]}}}
{"type":"function","function":{"name":"runs_diff","description":"Two runs' configuration fingerprints, field by field.","parameters":{"type":"object","properties":{"run_a":{"type":"string"},"run_b":{"type":"string"}},"required":["run_a","run_b"]}}}
{"type":"function","function":{"name":"llm_provider_presets","description":"The built-in model provider presets.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"llm_config","description":"The configured model: provider, base URL and model name, and whether a key is set (never the key).","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"llm_readiness","description":"Whether a model is usable right now.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"llm_local_probe","description":"Whether a local (loopback) model server answers.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"llm_stored_key","description":"Whether the OS credential store holds a key for this provider.","parameters":{"type":"object","properties":{"provider_id":{"type":"string"}},"required":["provider_id"]}}}
{"type":"function","function":{"name":"sessions","description":"The stored sessions, newest first.","parameters":{"type":"object","properties":{"limit":{"type":"integer"}},"required":[]}}}
{"type":"function","function":{"name":"sessions_current","description":"The session a run would land in.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"snapshots","description":"The stored snapshots.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"vm_running","description":"Whether a VM is running.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"vm_status","description":"The running VM: its state, its ports and when it started.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"toolchain","description":"The RISC-V compiler this node would use.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"toolchain_download","description":"Whether a toolchain download is running.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"qemu","description":"The QEMU binary this node would use.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"qemu_status","description":"The same QEMU view as its own endpoint.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"qemu_download","description":"Whether a QEMU download is running.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"preflight","description":"The last environment preflight and its verdict.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"settings_theme","description":"The interface theme.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"settings_language","description":"The interface language.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"workspace_root","description":"The workspace root this node serves.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"workspace_files","description":"The workspace's files, as relative paths.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"workspace_file","description":"One file's contents, read-only.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
{"type":"function","function":{"name":"serial","description":"Everything the guest has written to the UART so far.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"sandboxes","description":"The merged sandbox registry: hand-written, scanned, and the built-in default.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"sandboxes_current","description":"The definition a run would use, and the fallback's name.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"sandboxes_candidates","description":"What is installed on this machine (the raw scan).","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"sandboxes_requests","description":"The sandbox requests waiting for a decision, newest first.","parameters":{"type":"object","properties":{"status":{"type":"string"}},"required":[]}}}
{"type":"function","function":{"name":"resources","description":"Reserved: resource accounting. Answers 501 today.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"executors","description":"The executors a task can be routed to.","parameters":{"type":"object","properties":{},"required":[]}}}
]
```
<!-- tool-defs:queries:end -->

### 4.2 控制类

<!-- tool-defs:controls:begin -->
```json
[
{"type":"function","function":{"name":"agent_run","description":"Run one agent turn on this node and answer its outcome.","parameters":{"type":"object","properties":{"user_input":{"type":"string"},"sandbox":{"type":"string"}},"required":["user_input"]}}}
{"type":"function","function":{"name":"tasks","description":"Dispatch one task to the executor its target names, and answer the TaskOutcome.","parameters":{"type":"object","properties":{"target":{"type":"string"},"input":{"type":"string"},"sandbox":{"type":"string"},"id":{"type":"string"}},"required":["target","input"]}}}
{"type":"function","function":{"name":"runs_export","description":"Write one run's audit interval as JSONL into the workspace.","parameters":{"type":"object","properties":{"run_id":{"type":"string"},"path":{"type":"string"}},"required":["run_id","path"]}}}
{"type":"function","function":{"name":"runs_abandon_stale","description":"Mark the runs a previous process left open as abandoned. Idempotent.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"vm_start","description":"Reserved: a standalone VM start. Answers 501 today.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"vm_stop","description":"Stop this node's VM.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"snapshots_save","description":"Save a snapshot of the running VM.","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}
{"type":"function","function":{"name":"snapshots_resume","description":"Restore a snapshot. It stops the VM first.","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}
{"type":"function","function":{"name":"snapshots_delete","description":"Delete a snapshot.","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}
{"type":"function","function":{"name":"sessions_create","description":"Start a session.","parameters":{"type":"object","properties":{"title":{"type":"string"}},"required":["title"]}}}
{"type":"function","function":{"name":"sessions_open","description":"One session and its messages.","parameters":{"type":"object","properties":{"session_id":{"type":"string"}},"required":["session_id"]}}}
{"type":"function","function":{"name":"sessions_rename","description":"Retitle a session.","parameters":{"type":"object","properties":{"session_id":{"type":"string"},"title":{"type":"string"}},"required":["session_id","title"]}}}
{"type":"function","function":{"name":"sessions_delete","description":"Delete a session.","parameters":{"type":"object","properties":{"session_id":{"type":"string"}},"required":["session_id"]}}}
{"type":"function","function":{"name":"sessions_clear","description":"Delete every session.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"toolchain_download_post","description":"Download a pinned toolchain: C (the RISC-V GCC) or Zig.","parameters":{"type":"object","properties":{"toolchain":{"type":"string","description":"c or zig; defaults to c"}},"required":[]}}}
{"type":"function","function":{"name":"toolchain_download_cancel","description":"Cancel a running toolchain download.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"qemu_download_post","description":"Start a QEMU download. Today it refuses with install guidance on every platform.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"qemu_download_cancel","description":"Cancel a running QEMU download.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"toolchain_path","description":"Use this compiler.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
{"type":"function","function":{"name":"toolchain_path_clear","description":"Forget the manual compiler path and auto-discover again.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"qemu_path","description":"Use this QEMU binary.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
{"type":"function","function":{"name":"qemu_path_clear","description":"Forget the manual QEMU path and auto-discover again.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"preflight_run","description":"Check the environment. Progress arrives as preflight:progress events.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"preflight_ack","description":"Accept the current configuration as it is.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"audit_alert","description":"Turn the audit-failure alert on or off.","parameters":{"type":"object","properties":{"enabled":{"type":"boolean"}},"required":["enabled"]}}}
{"type":"function","function":{"name":"audit_export","description":"Write the whole audit chain as JSONL into the workspace.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
{"type":"function","function":{"name":"settings_theme_post","description":"Set the interface theme.","parameters":{"type":"object","properties":{"theme":{"type":"string"}},"required":["theme"]}}}
{"type":"function","function":{"name":"settings_language_post","description":"Set the interface language.","parameters":{"type":"object","properties":{"language":{"type":"string"}},"required":["language"]}}}
{"type":"function","function":{"name":"llm_config_post","description":"Configure the model. With remember, the key goes to the OS credential store instead of this session only.","parameters":{"type":"object","properties":{"api_key":{"type":"string"},"base_url":{"type":"string"},"model":{"type":"string"},"provider_id":{"type":"string"},"remember":{"type":"boolean"}},"required":["api_key","base_url","model"]}}}
{"type":"function","function":{"name":"llm_stored_key_load","description":"Load a stored key from the OS credential store into this session.","parameters":{"type":"object","properties":{"provider_id":{"type":"string"}},"required":["provider_id"]}}}
{"type":"function","function":{"name":"llm_config_clear","description":"Forget the model configuration.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"serial_export","description":"Write the captured serial output into the workspace.","parameters":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}}}
{"type":"function","function":{"name":"sandboxes_switch","description":"Switch this node to another sandbox definition. The running VM is stopped and started again.","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}
{"type":"function","function":{"name":"sandboxes_requests_post","description":"Leave a sandbox request for someone who may switch. It switches nothing by itself.","parameters":{"type":"object","properties":{"action":{"type":"string"},"sandbox":{"type":"string"},"reason":{"type":"string"}},"required":["action"]}}}
{"type":"function","function":{"name":"workspace_import","description":"Unpack a project archive into the workspace. The body is the archive itself, not JSON.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"workspace_export","description":"The workspace as a tar.gz. Bytes out, not JSON.","parameters":{"type":"object","properties":{},"required":[]}}}
]
```
<!-- tool-defs:controls:end -->

### 4.3 本机端点与带路径参数的路由

<!-- tool-defs:locals:begin -->
```json
[
{"type":"function","function":{"name":"health","description":"The control plane's liveness.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"status","description":"Connections, subscribers, agents, and this node's identity.","parameters":{"type":"object","properties":{},"required":[]}}}
{"type":"function","function":{"name":"events","description":"Subscribe to the SSE event stream. Not a request/response tool: the connection stays open.","parameters":{"type":"object","properties":{},"required":[]}}}
]
```
<!-- tool-defs:locals:end -->

<!-- tool-defs:patterns:begin -->
```json
[
{"type":"function","function":{"name":"run_get","description":"One run, by id.","parameters":{"type":"object","properties":{"run_id":{"type":"string"}},"required":["run_id"]}}}
{"type":"function","function":{"name":"sandbox_get","description":"One sandbox definition, by name.","parameters":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}}}
{"type":"function","function":{"name":"sandbox_request_approve","description":"Approve a pending sandbox request. Changes the record and nothing else.","parameters":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}}
{"type":"function","function":{"name":"sandbox_request_reject","description":"Reject a pending sandbox request.","parameters":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}}
]
```
<!-- tool-defs:patterns:end -->

## 5. 怎么用

工具描述的是**可以要什么**；HTTP 调用仍旧由监工发出。形状是进一次函数调用、出一次调用：

```text
模型要        tasks({"target":"executor-0","input":"build the blink example"})
监工就发      POST http://127.0.0.1:7821/v0/tasks
             Authorization: Bearer <token>
             Content-Type: application/json
             {"target":"executor-0","input":"build the blink example"}
             -> 200 {"task_id":…,"agent_id":…,"outcome":{…}}
模型被告知    那段 JSON，原样
```

两点实务：

- **capability 那一列是凭据决定，不是模型决定。** 调用方 token 不能用的工具会答 `403`；想让模型只有读权限的监工，应当既把它排除在 `tools[]` 之外，**也**交出一个做不了更多的 token（v0.9 只有一个持有全部的 token——按 capability 分令牌是 v1.0 的工作）。
- **`events` 不是一次工具调用。** 事件流会一直开着；由监工进程订阅，把到达的东西当上下文喂给模型，而不是交给它一个永不返回的工具。

## 本文件如何保持为真

三项检查，各有各的归属：

- **路由表对代码**：`server/src/routes.rs` 自己的测试读上面三张带标记的表，与服务端的路由表（`ROUTES` + `LOCAL_ROUTES` + 四条带路径参数的路由，后者还必须能 `resolve`）比对。给服务端加了一条路由而这里没加行，构建就会失败。
- **定义对路由表**：`scripts/check-tool-schema.mjs` 在以下情况失败：某张表的一行在定义里没有对应的 `"name"`、两条工具重名、或某个名字不是 §2 的推导结果。
- **本文件对它的译文**：同一个脚本要求带标记的块在 `tool-schema-control-plane.md` 里逐字相同。

**没有**机器核对的部分：`description` 与 `arguments` 两列，以及 `parameters` 对象。它们是文档——线上形状以 API 文档为准（[control-plane-api.zh-CN.md](control-plane-api.zh-CN.md) 的 §5 才是规范表）。
