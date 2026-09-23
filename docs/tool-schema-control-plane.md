[中文](tool-schema-control-plane.zh-CN.md) | English

# Tool schema: the control plane, for an AI supervisor

> **Applies to v0.9 (unstable until v1.0).** Every endpoint below is written as an
> OpenAI/DeepSeek tool definition. Paste the array you need into a chat request's
> `tools[]` and the model can drive this node itself. The route tables are checked against
> the server's own route table by a test; see [How this file is kept true](#how-this-file-is-kept-true).

## 1. What this is, and what it is not

RiscDom's v0.9 promise is that the work can be divided: one AI drives the node, others run
inside it. The **supervisor** is the one that drives, and it is an external process — it
talks to the control plane over HTTP (see [control-plane-client-guide.md](control-plane-client-guide.md)),
not to anything inside the kernel.

So a supervisor's tool set is **the control plane**, expressed as tools. This document is
that set: one tool per endpoint, with the endpoint's capability named so a supervisor author
knows which credential a tool needs.

**It is not** the tool set of the model *inside* an executor. That model calls eight
different tools (write a source file, compile, start the VM …) — those are in
[tool-schema-executor.md](tool-schema-executor.md). The distinction is the whole point:

| | Executor's model | Supervisor |
|---|---|---|
| Runs | inside the kernel, per turn | outside, as a process |
| Interface | the eight `tool_specs()` tools | the control plane over HTTP |
| Schema | [tool-schema-executor.md](tool-schema-executor.md) | this document |
| Answers with | an `AgentOutcome` | HTTP status + JSON |

## 2. Names, and how they are derived

`name` is derived from the path, mechanically, so that a tool and a route can never drift
apart silently:

1. drop `/v0/`;
2. replace every `/`, `-`, `{`, `}` and `.` with `_`;
3. collapse repeated `_` and trim leading/trailing `_`.

`POST /v0/runs/abandon-stale` → `runs_abandon_stale`, `GET /v0/llm/provider-presets` →
`llm_provider_presets`.

Two documented exceptions:

- **A path served by both `GET` and `POST`** (six of them: `toolchain/download`,
  `qemu/download`, `settings/theme`, `settings/language`, `llm/config`,
  `sandboxes/requests`) gives the `POST` the suffix **`_post`**, so
  `POST /v0/settings/theme` → `settings_theme_post`.
- **The four path-parameter routes** get a verb instead of a joined path, because
  `runs_run_id` helps nobody: `GET /v0/runs/{run_id}` → `run_get`,
  `GET /v0/sandboxes/{name}` → `sandbox_get`, and the two request decisions →
  `sandbox_request_approve` / `sandbox_request_reject`.

Tool names are unique across the whole set (checked).

## 3. Every endpoint as a tool

`arguments` is the request: a query string for a `GET`, a JSON body for a `POST`.

### 3.1 Queries (32)

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

### 3.2 Controls (36)

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
| `toolchain_download_post` | POST | `/v0/toolchain/download` | `toolchain.install` | — |
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

### 3.3 Host-local, and the path-parameter routes (3 + 4)

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

## 4. The definitions

Each line is one entry for `tools[]`. `GET` arguments travel as the query string, `POST`
arguments as the JSON body — a supervisor's HTTP call is its to make; these objects only
say what the model may ask for.

### 4.1 Queries

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

### 4.2 Controls

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
{"type":"function","function":{"name":"toolchain_download_post","description":"Download the pinned RISC-V toolchain.","parameters":{"type":"object","properties":{},"required":[]}}}
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

### 4.3 Host-local and path-parameter routes

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

## 5. Using them

The tools describe *what* may be asked; the supervisor still makes the HTTP call. The shape
is one function call in, one call out:

```text
the model asks for   tasks({"target":"executor-0","input":"build the blink example"})
the supervisor does  POST http://127.0.0.1:7821/v0/tasks
                     Authorization: Bearer <token>
                     Content-Type: application/json
                     {"target":"executor-0","input":"build the blink example"}
                     -> 200 {"task_id":…,"agent_id":…,"outcome":{…}}
the model is told    that JSON, verbatim
```

Two practical notes:

- **The capability column is a credential decision, not a model decision.** A tool the
  caller's token cannot use will answer `403`; a supervisor that wants to hand a model only
  read access should leave those tools out of `tools[]` *and* hand over a token that cannot
  do more (v0.9 has one token that holds everything — per-capability tokens are v1.0 work).
- **`events` is not a tool call.** The event stream stays open; subscribe from the
  supervisor process and feed what arrives to the model as context, rather than offering it
  a tool that never returns.

## How this file is kept true

Three checks, one owner each:

- **The route tables against the code**: `server/src/routes.rs`'s own tests read the three
  marked tables above and compare them with the server's route table (`ROUTES` +
  `LOCAL_ROUTES` + the four path-parameter routes, which must also resolve). A route added
  to the server without a row here fails the build.
- **The definitions against the tables**: `scripts/check-tool-schema.mjs` fails when a
  table row has no `"name"` in the definitions, when two tools share a name, or when a name
  is not the derivation of §2.
- **This file against its translation**: the same script requires the marked blocks to be
  identical in `tool-schema-control-plane.zh-CN.md`.

What is *not* machine-checked: the `description` and `arguments` columns, and the
`parameters` objects. They are documentation — the wire shape is the API document's
(§5 of [control-plane-api.md](control-plane-api.md)), which is the normative table.
