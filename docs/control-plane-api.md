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

**Implementation status (v0.9).** Everything in §5 is implemented — the 26 query
endpoints of §5.1, the 27 controls of §5.2, the host-local endpoints of §5.3, the error
model of §4, the event envelope with `Last-Event-ID` replay and `gap` frames, and the
bearer token of §3. Only two routes are reserved: `/v0/resources` (§6, G3) and
`POST /v0/vm/start` (§6, G1), and both say so with `501`. Capability enforcement (§3) is in: every served
route declares exactly one capability and the server refuses with `403` when the actor
does not hold it.

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
- **The vocabulary is the 28 names in the §5 tables** (`agent.run`, `audit.read`,
  `runs.control`, `settings.write`, …). v0.9 ships two actor shapes: the token holder
  (`operator`, `human`) holds all 28, and the `--no-auth` default holds the same set, so
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
| `bad_request` | 400 | Malformed JSON, missing required field, bad `path`/`run_id`. |
| `unauthorized` | 401 | No token, or the hook refused the token. |
| `forbidden` | 403 | Authenticated, but the actor lacks the endpoint's capability. |
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

### 5.1 Queries (26)

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
| `/v0/preflight` | GET | `preflight.read` | — | `PreflightView` | `preflight_status` |
| `/v0/settings/theme` | GET | `settings.read` | — | `{ "theme": string }` | `get_theme` |
| `/v0/settings/language` | GET | `settings.read` | — | `{ "language": string }` | `get_language` |
| `/v0/workspace/root` | GET | `workspace.read` | — | `{ "root": string }` | `workspace_root` |
| `/v0/workspace/files` | GET | `workspace.read` | — | `[string]` | `get_workspace_files` |
| `/v0/workspace/file` | GET | `workspace.read` | query: `path` | `{ "content": string }` | `read_workspace_file` |
| `/v0/serial` | GET | `serial.read` | — | `{ "buffer": string }` | `get_serial_buffer` |

### 5.2 Controls (27) — implemented in v0.9 batch 4

| Endpoint | Method | Capability | Request | Response | Tauri command |
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

- **Queries and controls are implemented.** `POST /v0/vm/start` (§6, G1) and
  `/v0/resources` (§6, G3) answer `501` until their kernel work lands.
- **A control that reports success may have been a no-op.** The host's session rename and
  delete are idempotent: an unknown `session_id` is not an error (the endpoint answers
  `204`), where `/v0/sessions/open` answers `404`. The endpoint mirrors the host rather
  than inventing a difference.
- **`POST /v0/toolchain/download` starts a real download** of the pinned RISC-V GCC archive
  and answers `202`; progress arrives as `toolchain:download` events.
- **Capabilities are declared and enforced.** Every route names its capability in the
  route table and the server checks it against the actor the hook returned before the
  handler runs; a missing capability is `403 forbidden` with `cause: "capability"` (§3).
  Under the v0.9 default every actor holds all 28, so a `403` can only come from a hook
  that returns a narrower actor.
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
