[中文](control-plane-api.zh-CN.md) | English

# Control plane HTTP API (design)

> **Applies to v0.9. Frozen at v1.0.** This is a design document: it defines the
> interface an integrator codes against, not an implementation that ships today.

**Audience.** Developers of distributions and integrations: anyone embedding RiscDom,
writing a control-plane client, or wiring RiscDom into their own system. This document
is not the user manual — installing and running RiscDom is documented elsewhere.

**What this is.** RiscDom's main line for v0.9 is a control plane: a human supervising
AIs and an AI supervising AIs go through **the same** HTTP interface. To the kernel an
instruction from a supervisor AI and one from a human are both authorised instructions
from the control plane; the audit chain tells them apart by `agent_id`. Building two
control channels instead of one is the mistake this design exists to avoid.

**Implementation status (v0.9).** Everything in §5 is implemented — the 32 query
endpoints of §5.1, the 36 controls of §5.2, the host-local endpoints of §5.3, the error
model of §4, the event envelope with `Last-Event-ID` replay and `gap` frames, and the
bearer token of §3. Only two routes are reserved: `/v0/resources` (§6, G3) and
`POST /v0/vm/start` (§6, G1), and both say so with `501`. Capability enforcement (§3) is in: every served
route declares exactly one capability and the server refuses with `403` when the actor
does not hold it. **Every capability in the vocabulary is enforced somewhere**: the
29th, `sandbox.read`, is served by the sandbox queries below (and gates the two request
decisions), the 30th, `sandbox.switch`, by the switch on the same surface, and the
31st, `sandbox.assemble`, **inside** the request decisions below — a decision needs the
capability its request's `action` implies, and the handler is where the request is known.
`sandbox.assemble` gets a route of its own when the assemble endpoint lands.

## 1. Position and protocol

- **Layer 3.** The control plane is a host (as defined in
  [architecture-evolution.md](architecture-evolution.md) §4): it sits beside
  `ui/src-tauri` and depends only on Layer 2's stable API (the `host-core` public surface).
  It does not reach into `agent` / `sandbox` / `audit` directly.
- **Transport.** HTTP for commands and queries, **SSE** (Server-Sent Events) for the
  event push. WebSocket is deliberately not used; the reasoning is recorded in
  [handoff.md](handoff.md) §1 and the wire format is in
  [control-plane-events.md](control-plane-events.md).
- **Device-independent semantics.** The same protocol carries local IPC and a networked
  connection. A local socket and a remote host answer the same requests with the same
  payloads; only the transport address differs. This is the "seam" left open in
  architecture-evolution.md §7.
- **Every kernel capability has one endpoint.** The constraint from
  architecture-evolution.md §6 and §12: if a kernel capability has no control-plane
  endpoint, the official management program cannot use it, and it is decoration. §5
  below is the coverage table, gaps included.

## 2. Conventions

- Base path: `/v0/` — "v0" means unstable. Error codes, field names and endpoint paths
  may change within v0.x. The prefix is a promise to break things honestly, not to keep
  them.
- Content type: `application/json; charset=utf-8` for requests and responses, except the
  SSE stream (`text/event-stream`).
- Identities: `agent_id` is `<device>-<pid>-<seq>`, `task_id` is `task-<pid>-<seq>`
  (v0.8 batch B). A client treats both as opaque strings.
- Nothing in a response body ever contains an API key or a token. This mirrors the host
  invariant that the frontend never sees the key.

## 3. Authentication and capabilities

- Clients send `Authorization: Bearer <token>`.
- **Out of the box the token is a file.** The served program installs `TokenAuth` unless it
  is started with `--no-auth`: on first start it generates 32 random bytes into
  `<data-dir>/token`, owner-readable only, and every request must present that value. The
  token is never printed or logged — read it from the file. With `--no-auth` the
  requirement is dropped and the server prints a warning, because the control endpoints
  include destructive ones.
- The control plane has one hook, shaped as a trait:

```rust
/// Resolve a request to the actor that made it, or refuse.
pub trait Authn: Send + Sync {
    fn authorise(&self, req: &ReqMeta) -> Result<Actor, AuthError>;
}

/// What a request is allowed to act as.
pub struct Actor {
    /// Maps 1:1 onto the audit chain's `agent_id`.
    pub agent_id: String,
    /// `human` / `supervisor` / `executor` — for the audit narrative only.
    pub kind: ActorKind,
    /// What this actor may do. The server checks the route's capability against
    /// this set before the handler runs.
    pub capabilities: BTreeSet<Capability>,
}
```

- **The hook authenticates; the server authorises.** `authorise` answers *who* the caller
  is. Whether that actor may do the thing is a separate, later decision, and it is the
  server's: every served route declares exactly one capability, the request path asks the
  returned actor whether it `allows` that capability, and a request that does not is
  `403 forbidden` with `cause` set to `capability` (`server/src/http.rs`). The hook
  sees the requirement too (`ReqMeta.capability`) if it wants to reason about it, but it
  cannot grant one: it can only return an actor that holds less.
- **A capability is a typed column of the route table**, not a string a handler remembers
  to check (`server/src/routes.rs`). There is no way to write down a route without naming
  its capability, so there is no route that silently skips the check.
- **Default deny.** An actor is refused unless it positively holds what the route asks for;
  an actor with an empty set can reach nothing. "No capability" is not expressible.
- **The vocabulary is the 32 names in the §5 tables** (`agent.run`, `audit.read`,
  `runs.control`, `settings.write`, …). v0.9 ships two actor shapes: the token holder
  (`operator`, `human`) holds all 32, and the `--no-auth` default holds the same set, so
  both behave identically once past the hook. A `403` therefore only comes from a hook
  that returns a narrower actor. Per-capability tokens are v1.0 work; the set is the shape
  they will fill in.
- The `Actor` returned by the hook is what every audit row this request writes carries.
  "A human did it" and "a supervisor AI did it" are distinguished by `agent_id`, exactly
  as architecture-evolution.md §6 requires.
- **Tokens are never logged.** Not in the access log, not in an error, not in an audit
  detail. The hook returns an `Actor`; the raw token is dropped. `ReqMeta`'s `Debug`
  redacts it, so a stray `{:?}` cannot write it either.
- **Transport security is the caller's responsibility (open-source boundary).** The
  open-source build offers plaintext HTTP plus the auth hook, nothing more. TLS
  termination, a network boundary, or a local-only bind is a deployment decision, and a
  distribution that exposes the control plane beyond the loopback interface is
  responsible for putting it behind TLS. This is stated here so no integrator assumes
  the open-source build does it for them.
- If no `Authn` is installed, the control plane refuses every request with
  `unauthorized` (fail closed). There is no anonymous mode: the `--no-auth` hook is an
  *installed* hook that authorises everyone as the owner, not an absence of auth.

## 4. Error model

Every non-2xx response carries one error object:

```json
{
  "code": "not_found",
  "message": "no run with id run-12345-7",
  "retryable": false,
  "cause": "run_id"
}
```

| Field | Type | Meaning |
|---|---|---|
| `code` | string | Stable, machine-readable. The list below is closed for v0.9. |
| `message` | string | Human-readable, never contains a secret. |
| `retryable` | bool | Whether an identical retry can plausibly succeed. |
| `cause` | string \| null | The offending input field or subsystem, when there is one. |

Codes and HTTP status mapping:

| `code` | HTTP | When |
|---|---|---|
| `bad_request` | 400 | Malformed JSON, missing required field, bad `path`/`run_id` — including a path the workspace policy refuses (it escapes the workspace, or it traverses with `..`), which carries `cause: "path"`. |
| `unauthorized` | 401 | No token, or the hook refused the token. |
| `forbidden` | 403 | Authenticated, but the actor lacks the endpoint's capability. **`403` is only ever authentication or authorisation**; a parameter the server cannot use is a `400`. |
| `not_found` | 404 | Unknown `run_id`, `session_id`, snapshot name. |
| `method_not_allowed` | 405 | The path is served, but not under this method; `message` names the one to use. |
| `conflict` | 409 | A state clash: VM absent on `resume`, a download already running. |
| `not_implemented` | 501 | A reserved endpoint with no kernel method yet (§6). |
| `unavailable` | 503 | The dependency is not ready (no LLM, no QEMU, no toolchain). |
| `internal` | 500 | Anything else. |

**Relation to `DispatchError`.** `DispatchError` (`agent/src/dispatch.rs`) is the
in-process dispatch abstraction: `NoSuchAgent(AgentId)` and `Failed(String)`. The
control plane maps it as `NoSuchAgent` → `not_found` (`cause: "agent_id"`) and `Failed`
→ `internal` with the reason in `message`. v0.9 defines this shape only; cross-device
transport will extend it (timeouts, remote errors), and that extension is explicitly
out of scope for this batch.

## 5. Endpoint table

Query commands are `GET`. Control commands are `POST`. "Capability" is the precondition
the server checks before the handler runs (§3; §6 gap G2). The last column names the
Tauri command the endpoint wraps, so an integrator can line the two surfaces up.

### 5.1 Queries (32)

| Endpoint | Method | Capability | Request | Response | Tauri command |
|---|---|---|---|---|---|
| `/v0/audit/status` | GET | `audit.read` | — | `AuditStatusView` | `get_audit_status` |
| `/v0/audit/events` | GET | `audit.read` | query: `limit`, `actor`, `action_prefix` | `[StoredEventView]` | `list_audit_events` |
| `/v0/runs` | GET | `runs.read` | query: `limit` (default 20) | `[RunView]` | `list_runs` |
| `/v0/runs/{run_id}` | GET | `runs.read` | path: `run_id` | `RunView` or `null` | `get_run` |
| `/v0/runs/diff` | GET | `runs.read` | query: `run_a`, `run_b` | `[FingerprintFieldDiff]` | `compare_run_fingerprints` |
| `/v0/llm/provider-presets` | GET | `llm.read` | — | `[ProviderPresetView]` | `get_provider_presets` |
| `/v0/llm/config` | GET | `llm.read` | — | `LlmConfigStatus` | `get_llm_config_status` |
| `/v0/llm/readiness` | GET | `llm.read` | — | `LlmReadiness` | `get_llm_readiness` |
| `/v0/llm/local-probe` | GET | `llm.read` | — | `LocalProbeResult` | `probe_local_llm` |
| `/v0/llm/stored-key` | GET | `llm.read` | query: `provider_id` | `{ "present": bool }` | `has_stored_key` |
| `/v0/sessions` | GET | `session.read` | query: `limit` | `[SessionMeta]` | `list_sessions` |
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
| `/v0/workspace/file` | GET | `workspace.read` | query: `path` | `{ "content": string }` | `read_workspace_file` |
| `/v0/serial` | GET | `serial.read` | — | `{ "buffer": string }` | `get_serial_buffer` |
| `/v0/sandboxes` | GET | `sandbox.read` | — | `{ "sandboxes": [SandboxView], "current": string \| null, "default": string }` | `list_sandboxes` |
| `/v0/sandboxes/current` | GET | `sandbox.read` | — | `{ "current": string \| null, "default": string }` | `current_sandbox` |
| `/v0/sandboxes/candidates` | GET | `sandbox.read` | — | `CandidatesView` | `sandbox_candidates` |
| `/v0/sandboxes/{name}` | GET | `sandbox.read` | path: `name` | `SandboxView`, or `404` | `get_sandbox` |
| `/v0/sandboxes/requests` | GET | `sandbox.read` | query: `status`? | `{ "requests": [SandboxRequestView] }`, or `400` on an unknown `status` | `list_sandbox_requests` |
| `/v0/executors` | GET | `agent.run` | — | `{ "executors": [{ "agent_id": string }] }` | `list_executors` |

### 5.2 Controls (36) — implemented in v0.9 batch 4, extended by sandbox F1, F2b-2, F2c, project in/out and the task endpoint

| Endpoint | Method | Capability | Request | Response | Tauri command |
|---|---|---|---|---|---|
| `/v0/agent/run` | POST | `agent.run` | `{ "user_input": string, "sandbox"? }` | `AgentOutcomeView` | `run_agent` |
| `/v0/tasks` | POST | `agent.run` | `{ "target": string, "input": string, "sandbox"?, "id"? }` | `TaskOutcome`, `404` on an unknown `target` | `dispatch_task` |
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
| `/v0/qemu/download` | POST | `qemu.configure` | — | `202 { "state": "started" }`, or `503 unavailable` on every platform today (see the note below) | `start_qemu_download` |
| `/v0/qemu/download/cancel` | POST | `qemu.configure` | — | `202 { "state": "cancelling" }`, or `409` when nothing is running | `cancel_qemu_download` |
| `/v0/qemu/path/clear` | POST | `qemu.configure` | — | `204 No Content` | `clear_qemu_path` |
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
| `/v0/sandboxes/switch` | POST | `sandbox.switch` | `{ "name": string }` | `{ "from": string \| null, "to": string }`, or `404` / `409` / `503` / `500` (see the note below) | `switch_sandbox` |
| `/v0/sandboxes/requests` | POST | `agent.run` | `{ "action": "switch"\|"define"\|"assemble", "sandbox"?, "reason"? }` | `201 { "id": string }` | `request_sandbox` |
| `/v0/sandboxes/requests/{id}/approve` | POST | `sandbox.read`, then the request's action (see the note below) | — | `SandboxRequestView`, or `404` / `409` / `403` | `approve_sandbox_request` |
| `/v0/sandboxes/requests/{id}/reject` | POST | as `approve` | — | `SandboxRequestView`, or `404` / `409` / `403` | `reject_sandbox_request` |
| `/v0/workspace/import` | POST | `workspace.write` | **the archive itself** (zip / tar.gz / tar), with `Content-Type: application/zip` \| `application/gzip` \| `application/x-tar`; query `force`? | `200 { "files": number, "bytes": number }`, or `400` / `409` / `413` (see the note below) | `import_workspace` |
| `/v0/workspace/export` | POST | `workspace.read` | — | **the archive itself** (`application/gzip`, `Content-Disposition: attachment`) | `export_workspace` |

Response shapes named above are the `host-core` view types (`host-core/src/state.rs`); a client
may read their fields directly from that file. `AgentOutcomeView` is
`{ kind, content, reason, iterations }`, with `kind` one of `final` / `max_iterations` /
`failed`.

Long-running controls answer immediately and stream progress over SSE
(`/v0/agent/run` → `agent:*`; `/v0/preflight/run` → `preflight:progress`;
`/v0/toolchain/download` → `toolchain:download`). The `202` bodies above are the
acknowledgement, not the result.

### 5.3 Host-local endpoints

Three endpoints belong to the host process rather than to the kernel, so they are not in
the tables above. They are part of this document's surface all the same.

| Endpoint | Method | Capability | Answers |
|---|---|---|---|
| `/v0/health` | GET | `health.read` | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `status.read` | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | `events.subscribe` | The SSE event stream (see [control-plane-events.md](control-plane-events.md)). |

### 5.4 Notes on the tables

- **`/v0/tasks` routes; `/v0/agent/run` runs *here*.** The two look alike and are
  not: `POST /v0/agent/run` runs one turn on **this node**, while `POST /v0/tasks`
  sends a `Task` to the executor its `target` names and answers with the
  `TaskOutcome` the executor produced. The node is deliberately **not** one of its
  own executors — a target naming it is a `404` — so the two endpoints never
  overlap; `GET /v0/executors` lists who a task can reach. The fleet comes from
  `executors` in `settings.json` (label + program + args) and from nothing else:
  there is no runtime registration endpoint in v0.9. A task with no `id` gets one
  from the server, and a run that *failed* is still a `200` — its `outcome` says so;
  the `404`/`500` are for a target nobody owns and a dispatch that broke.
- **`POST /v0/qemu/download` refuses, by decision, on every platform.** RiscDom guides
  the user to a QEMU they install themselves (`docs/qemu-distribution.md` §5): no release is
  pinned, so the endpoint answers `503 unavailable` with `cause: "qemu"` and the install
  guidance in `message`, and it claims no download slot. Everything behind it — the slot,
  the status, the cancel, the `qemu:download` event family and the adoption step — is
  complete, so pinning a release later is a **data change**: the answer becomes `202`.
- **The two audit exports answer a count of events, not bytes.** The host's
  `write_events_jsonl` returns `events.len()`, so `/v0/audit/export` and
  `/v0/runs/export` answer `{ "events_exported": number }` — the field name says what
  the number is. `/v0/serial/export` writes the captured serial text and really does
  answer a byte count, so it keeps `bytes_written`.
- **A path outside the workspace is a `400`, not a `403`.** The export and
  `/v0/workspace/file` paths go through the host's workspace policy, and a path it
  refuses (one that escapes the root, or one that traverses with `..`) is the
  caller's parameter being unusable: `400 bad_request` with `cause: "path"`. `403`
  is reserved for the capability check of §3.
- **Queries and controls are implemented.** `POST /v0/vm/start` (§6, G1) and
  `/v0/resources` (§6, G3) answer `501` until their kernel work lands.
- **A control that reports success may have been a no-op.** The host's session rename and
  delete are idempotent: an unknown `session_id` is not an error (the endpoint answers
  `204`), where `/v0/sessions/open` answers `404`. The endpoint mirrors the host rather
  than inventing a difference.
- **`POST /v0/toolchain/download` starts a real download** of the pinned RISC-V GCC archive
  and answers `202`; progress arrives as `toolchain:download` events.
- **The sandbox queries read a merged registry and never write it** (v0.9 sandbox
  F2a-2). `/v0/sandboxes` answers the three sources in one list — the hand-written
  definitions, what the scan found, and the built-in `default` — and each entry carries
  `source` (`manual` / `discovered`), `runnable` (computed per read, never stored) and
  `shadowed`. A hand-written definition wins a name collision and the shadowed entry
  **stays in the list, marked**. `/v0/sandboxes/candidates` answers the raw scan instead
  (the two independent lists), and nothing there is a definition. `/v0/sandboxes/{name}`
  is a `404` naming the parameter when no definition has that name; the literal sub-paths
  (`current`, `candidates`, and the two the rest of the F2 line reserves — `requests`,
  `assemble`) are never read as a name. The switch is a route of its own now (`POST`,
  declared above), so a `GET` on it is a `405`.
- **A sandbox request is an ask, not a command** (v0.9 sandbox F2c). `POST
  /v0/sandboxes/requests` needs `agent.run` — the actor that may run an agent is the actor
  that may say what it wants — and answers `201` with the new id. `GET
  /v0/sandboxes/requests?status=` reads the queue (`sandbox.read`), newest first; an
  unknown `status` is a `400`. The two decisions need `sandbox.read` as their **route's**
  gate (a decider has to be able to see the queue) and then, **inside the handler**, the
  capability the request's own `action` implies: `sandbox.switch` for a `switch` request,
  `sandbox.assemble` for `define` / `assemble`. A refusal is `403 forbidden` with `cause:
  "capability"` naming it. The split is the point: moving a node and handing it a new
  definition to run are different powers. **Approving performs nothing** — the switch is a
  second, authorised call to `POST /v0/sandboxes/switch`, so a request for a definition
  that does not exist is still approvable. An unknown id is `404 not_found` with `cause:
  "id"`; a second decision on the same request is `409 conflict` — a decision is not
  reversible. **There is no TTL in v0.9**: `expired` exists in the status vocabulary but no
  path produces it, and a pending request waits until somebody decides it.
- **The sandbox switch answers a status per reason** (v0.9 sandbox F2b-2). `POST
  /v0/sandboxes/switch` is the one write on the sandbox surface, and it is synchronous:
  validation, a stop, a start. Its answers are chosen so a client branches on a name, not on
  a sentence: `200` with `{from, to}` when a new sandbox is running; `404 not_found` with
  `cause: "name"` when no definition has that name (the same answer the name route gives);
  `409 conflict` with `cause: "run"` while a run is in flight (the switch would take the VM
  the loop is using) and with `cause: "sandbox"` while another switch is in progress;
  `503 unavailable` with `cause` set to the reason code (`sandbox_qemu_missing`,
  `sandbox_toolchain_missing`, `sandbox_kernel_missing`) when the definition cannot run; and
  `500 internal` with `cause: "sandbox_start_failed"` when every check passed and the VM
  still would not start — in which case the node is **stopped**, not half-switched.
- **A project leaves and enters as one file** (v0.9 project in/out). `POST
  /v0/workspace/export` answers the workspace as a `tar.gz` — **bytes, not JSON**, the
  first such body on this surface apart from the event stream — with
  `Content-Disposition: attachment; filename="workspace.tar.gz"`. An empty workspace
  exports a valid empty archive: "export this project" is never an error because the
  project is empty. The host's own state directory (`.riscdom/`: the audit DB,
  snapshots, the preflight cache) is **not** packed. `POST /v0/workspace/import` takes
  the archive as its **body** (not JSON), accepting `application/zip`,
  `application/gzip` and `application/x-tar` — and falling back to the bytes themselves
  when the `Content-Type` says nothing it knows, so a `.tar.gz` sent as
  `application/octet-stream` still works. It answers `{files, bytes}`. Everything an
  entry may not do is checked by the host, and each has its own answer: an entry that
  escapes the workspace, arrives as a symlink or hard link, names `.riscdom/`, or is
  simply not a readable archive is `400` with `cause: "archive"`; a file already in the
  workspace is `409` with `cause: "exists"` unless `?force=true` says to replace it; and
  a body above **64 MiB** is `413 payload_too_large` — the import's own ceiling, not the
  shared 64 KiB one every JSON body uses. Importing needs `workspace.write` (the 32nd
  capability) where exporting needs `workspace.read`: reading a project and replacing it
  are not the same permission. Import is **containment-only** (no extension allow-list),
  for the same reason the exports are: a project is not only `.c` / `.h` / `.S` / `.s`.
- **A run may declare the sandbox it wants** (v0.9 sandbox F2d). `POST /v0/agent/run`
  takes an optional `sandbox` name, and it is a **declaration, not a switch**: the run
  uses that definition for the VM it starts (its toolchain, its QEMU, its memory) and
  the node's `current_sandbox` is left alone — moving the node is
  `POST /v0/sandboxes/switch`, which needs `sandbox.switch`. Three answers, in this
  order, so the caller hears the most specific one: a name nobody has is `404` with
  `cause: "name"` (a typo must not become a run under some other sandbox); a name that
  is not what the running VM came from is `409` with `cause: "sandbox"` (a VM cannot be
  replaced from inside a run, and the message names both ways out — stop it, or
  switch), and with nothing running the declaration is honoured; and only then the
  environment is asked, so no model is the documented `503` with `cause: "llm"`. A run
  that declares none resolves as before: what the node is running, then its configured
  default, then the built-in fallback — which means "discover this host's own QEMU and
  toolchain", exactly the behaviour before this batch. The declaration does **not**
  reach the kernel: which ELF to boot is still the model's `start_vm` argument, because
  that is what a run boots, where `def.kernel` is what a *switch* boots.
- **Capabilities are declared and enforced.** Every route names its capability in the
  route table and the server checks it against the actor the hook returned before the
  handler runs; a missing capability is `403 forbidden` with `cause: "capability"` (§3).
  Under the v0.9 default every actor holds all 32, so a `403` can only come from a hook
  that returns a narrower actor — or from the two request decisions, which check the
  capability the request's `action` implies after the route's own gate has passed.
- **Parameters.** A required parameter that is missing or unparsable is `400 bad_request`
  with `cause` set to the parameter's name. `limit` is required where the host command
  requires it, and optional elsewhere: `/v0/runs` defaults to 20, while
  `/v0/audit/events` and `/v0/sessions` require it. `?path=` on
  `/v0/workspace/file` is percent-decoded.
- **`/v0/audit/status` does not consume the failure queue.** The Tauri command *takes* the
  pending audit failures; a `GET` must not, or one polling client would swallow another's
  alerts. The endpoint reports the queue as it stands.
- **An unknown run in `/v0/runs/diff` answers `internal`.** The host reports a missing run
  as an opaque message rather than a typed not-found, so the control plane cannot turn it
  into `404` without inventing a rule. A typed host error would close this; it is not in
  this batch.

## 6. The four kernel-capability gaps

The reconnaissance found four kernel capabilities without a first-class command. Each is
resolved explicitly here; none is silently dropped.

- **G1 — starting a VM.** Decision: **a dedicated endpoint, `POST /v0/vm/start`,
  reserved and returning `501 not_implemented` in v0.9.** The kernel treats
  start/stop of a VM as a capability (architecture-evolution.md §5), and today the VM
  starts implicitly inside `run_agent` and is held by the host afterwards. An explicit
  start with no owning task has no lifetime semantics yet, so the endpoint is reserved
  now and backed by a new `AppState` method in an implementation batch. Stopping is
  already real (`POST /v0/vm/stop`).
- **G2 — permission check.** Decision: **not an endpoint.** `check(capability)` is the
  precondition evaluated for *every* endpoint, not a resource a client calls. Its shape:
  the hook resolves the `Actor` (§3), then the request path checks the endpoint's
  capability from the table in §5 before the handler runs. A missing capability is
  `403 forbidden`. The check is implemented: the route table's capability is a typed
  column (`server/src/routes.rs`) and the decision is taken in the request path
  (`server/src/http.rs`). A kernel-level `check()` method is still not needed, because
  the route table is the source of truth for every endpoint.
- **G3 — resource accounting.** Decision: **a reserved aggregate endpoint,
  `GET /v0/resources`, returning `501` in v0.9.** Its settled shape is
  `{ "vm": {"running", "since_ms"}, "runs": {"active", "total"}, "sessions": {"count"},
  "downloads": {"active"} }`, assembled from the status queries that already exist. No
  kernel method is added for it; when it is implemented it aggregates the same views as
  §5.1.
- **G4 — abandoning stale runs.** Decision: **exposed, `POST /v0/runs/abandon-stale`.**
  This is the odd one: the kernel method exists (`AppState::abandon_stale_runs`) and is
  called on startup, but no Tauri command wraps it. Response:
  `{ "abandoned": [run_id, ...] }`. It is idempotent and safe to call repeatedly.

## 7. Versioning and compatibility

- The path prefix is `/v0/` for the whole v0.x line. Within v0.x, breaking changes ship
  without a prefix bump; clients must tolerate them.
- **v1.0 freezes the API.** From v1.0 the path becomes `/v1/`, and from then on additive
  fields do not bump the version while a semantic change does.
- A client pins a RiscDom version range, not the API version, for v0.x. This is honest
  about the instability instead of pretending otherwise.

## 8. Examples

Query — the newest ten audit events:

```bash
curl -sS http://127.0.0.1:7788/v0/audit/events?limit=10 \
  -H 'Authorization: Bearer <token>'
```

Query with a filter — only tool calls by one actor:

```bash
curl -sS 'http://127.0.0.1:7788/v0/audit/events?actor=dev-12345-1&action_prefix=agent.tool' \
  -H 'Authorization: Bearer <token>'
```

Control — run one agent turn (progress arrives on the SSE stream):

```bash
curl -sS -X POST http://127.0.0.1:7788/v0/agent/run \
  -H 'Authorization: Bearer <token>' \
  -H 'Content-Type: application/json' \
  -d '{"user_input":"compile the blink example and show the serial output"}'
```

Control — stop the VM:

```bash
curl -sS -X POST http://127.0.0.1:7788/v0/vm/stop \
  -H 'Authorization: Bearer <token>'
```

Error — a run id that does not exist (`HTTP/1.1 404 Not Found`):

```json
{
  "code": "not_found",
  "message": "no run with id run-12345-7",
  "retryable": false,
  "cause": "run_id"
}
```

Error — no token (`HTTP/1.1 401 Unauthorized`):

```json
{
  "code": "unauthorized",
  "message": "missing or invalid bearer token",
  "retryable": false,
  "cause": "authorization"
}
```

Event push — subscribe (see [control-plane-events.md](control-plane-events.md) for the
frame format):

```bash
curl -sS -N http://127.0.0.1:7788/v0/events \
  -H 'Authorization: Bearer <token>' \
  -H 'Accept: text/event-stream'
```

`127.0.0.1:7788` is the example bind; the address is a deployment setting.
