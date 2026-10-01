[中文](control-plane-events.zh-CN.md) | English

# Control plane event stream (design)

> **Applies to v0.9. Frozen at v1.0.** This is a design document: it defines the wire
> format a client codes against, not an implementation that ships today.

**Audience.** Developers of distributions and integrations: anyone consuming the event
stream — a management client, the official management program, or an AI supervisor
watching executors.

**Scope.** Commands and queries are in [control-plane-api.md](control-plane-api.md).
This document covers the push side: SSE framing, one common envelope for all events, the
per-event payloads, and filtering.

**Where the events come from.** `host-core/src/events.rs` defines twenty event names and an
`EventSink` trait (`emit(&self, event: &str, payload: serde_json::Value)`). Three
implementations exist today: `TauriEventSink` (to the webview), `RecordingEventSink`
(tests), and `LineEventSink` (`worker`, JSON lines on stderr). **Every transport wraps what it
sends in the envelope below** — the SSE sink, the Tauri sink (whose webview unwraps at its
single boundary), and the worker's line protocol. The emit sites still pass a raw payload,
because the envelope is the transport's business: `EventSink::emit` keeps its
`(&str, Value)` signature and no emit site changed shape. **Implemented in v0.9 batch 3**,
together with the three payload shapes §3 marks as changed; the other eight travel exactly
as the host emits them.

## 1. SSE protocol

- **Content type.** `text/event-stream; charset=utf-8`, with `Cache-Control: no-cache`
  and `Connection: keep-alive`.
- **Endpoint.** `GET /v0/events`. The stream opens on `200`; the connection stays open
  until the client closes it or the server stops.
- **Frames use `id:` and `data:` only — no `event:` field.** *Decision and rationale:*
  setting SSE's `event` field would make the browser's `EventSource` dispatch to a named
  listener and stop firing `onmessage`, forcing a client to register twenty listeners.
  Leaving it unset delivers every frame to one `onmessage` handler, and the envelope's
  `event` field does the routing. One handler, one router.
- **Heartbeat.** A comment line (starting with `:`) every 15 seconds keeps intermediaries
  from closing an idle stream and lets a client detect a dead connection:

```text
: keep-alive

```

- **Authentication.** The `Authorization: Bearer <token>` header, as in the API
  document. A browser's `EventSource` cannot set headers, and this server implements
  **neither** of the two workarounds a browser would otherwise need: there is no
  cookie-session endpoint and no query-string token. The shape that works today is
  `fetch` with the header plus a `ReadableStream` reader — the frames are `id:` / `data:`
  lines, parsed as they arrive — which is exactly what the management UI served from
  `--web-root` does (v0.9 D2a), same origin as the API. A cookie session and a
  short-lived query token remain **client-integration options that are not implemented**;
  the query-string token is discouraged anyway, since it lands in access logs.
- **Reconnect.** A dropped stream is resumed by reconnecting with the same `(event
  filters)` and the last id the client saw, sent as the `Last-Event-ID` header (browsers
  do this automatically; other clients must do it themselves).
- **`id:` is the replay cursor.** The server emits `id: <ts>-<seq>`, where `ts` is the
  event timestamp in epoch milliseconds and `seq` is a **server-wide** monotonic frame
  counter — server-wide because that is what makes the id usable after a reconnect.
  It is opaque to the client: store it, send it back, do not parse it.
- **Replay is best-effort and bounded.** The server keeps a bounded ring buffer of recent
  frames. If a client's `Last-Event-ID` is newer than the buffer's oldest entry the
  server replays the gap; if it is older, the server cannot fill it and says so with a
  `gap` frame (§2) instead of pretending the history is complete.

### 1.1 Stream opening

Immediately after the connection opens, the server sends one `hello` frame describing
the stream, so a client can tell "no events yet" from "not connected":

```text
id: 0-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"server","task_id":null,"ts":1758533001207,"payload":{"buffer":{"from":0,"to":42},"filters":{"event":[],"agent_id":null,"task_id":null}}}

```

## 2. The unified envelope

Every frame carries the same object. This is the batch's core deliverable: the twenty
events keep their own payloads, but they all travel inside one envelope whose fields have
one meaning each.

```json
{
  "version": 1,
  "kind": "event",
  "event": "agent:tool_call",
  "agent_id": "dev-12345-1",
  "task_id": "task-12345-1",
  "ts": 1758533001207,
  "payload": { "name": "write_source", "arguments": { "path": "src/main.c" } }
}
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `version` | integer | yes | Envelope schema version. `1` in v0.9. |
| `kind` | string | yes | Frame kind: `event` / `hello` / `gap` (see below). |
| `event` | string \| null | yes | One of the twenty names, or `null` for `hello` / `gap`. |
| `agent_id` | string | yes | The agent that caused the event, `<device>-<pid>-<seq>`. |
| `task_id` | string \| null | yes | The task it belongs to: the id a run was given (`POST /v0/agent/run`'s `task_id`), the id a dispatch carries (`POST /v0/tasks`'s `id`), or `null` when the event is not tied to one (v1.0 M6-3a). |
| `ts` | integer | yes | Epoch milliseconds. |
| `payload` | object | yes | Event-specific body (§3). |

`kind` values in v0.9:

- `event` — one of the twenty events; `event` names it.
- `hello` — the stream opened; `payload.buffer` and `payload.filters` describe it.
- `gap` — the requested replay id was too old to replay; `payload.lost_after` is the
  oldest id the server still holds. A client that sees a `gap` must re-sync from a
  query (§ the API document) rather than assume it missed nothing.

**Implemented in v0.9 batch 4.** A reconnecting client sends `Last-Event-ID` and the server replays what follows it from a bounded in-memory buffer (the last 1024 frames). If the cursor has fallen out of that buffer, the client gets a `gap` frame first — naming the oldest id still held — and then the frames from there on.

### 2.1 How `version` evolves

- **Adding a payload field does not bump `version`.** Clients ignore unknown payload keys.
- **Adding a new event name or a new `kind` value does not bump `version`.** Clients
  ignore frames they do not recognise.
- **Changing a field's meaning, type, or removing it bumps `version`.** The bump is the
  only signal a client gets that it must change, so it is reserved for exactly that.
- The envelope's top-level fields are frozen for v0.9: a new one would be a `version`
  bump, because a client validating the envelope would have to change.

## 3. The twenty events, normalised

The payload keys are unified here so a client written against this document keeps working
after the emit sites are normalised (a later batch). "Changed" means the mapping from
today's payload is not the identity; the rest keep their keys and are merely wrapped.

| # | `event` | Today's payload (v0.8) | Envelope `payload` (v1) | Changed |
|---|---|---|---|---|
| 1 | `agent:iteration` | `{model, messages}` | `{model, messages}` | no |
| 2 | `agent:tool_call` | `{name, arguments}` | `{name, arguments}` | no |
| 3 | `agent:tool_result` | `{ok, result}` | `{ok, result}` | no |
| 4 | `agent:final` | `{kind, content, reason, iterations}` | `{kind, content, reason, iterations}` | no |
| 5 | `agent:stream:delta` | `{text}` | `{text}` | no |
| 6 | `agent:stream:done` | `{}` | `{}` | no |
| 7 | `serial:chunk` | `{chunk}` | `{chunk}` | no |
| 8 | `vm:state` | `{state, running, since_ms}` (+ `name` only on snapshot) | `{state, running, since_ms, name}` — `name` always present | **yes** |
| 9 | `preflight:progress` | `{step, state, detail}` | `{step, state, detail}` | no |
| 10 | `audit:failed` | `{error}` | `{message}` | **yes** |
| 11 | `toolchain:download` | internally tagged enum: `{"kind":"progress","downloaded":d,"total":n}`, … | `{state, ...}` — the same fields under the tag `state` | **yes** |
| 12 | `qemu:download` (v0.9 sandbox F1) | internally tagged enum: `{"kind":"progress",…}`, … | `{state, ...}` — the same fields under the tag `state`, the toolchain's shape | **yes** |
| 13 | `sandbox:switch` (v0.9 sandbox F2b-2) | — (new in v0.9) | `{from, to, ok, reason}` — `from` is the definition that was current (`null` when none was), `to` the one asked for, `reason` the code when `ok` is `false` | **new** |
| 14 | `sandbox:request` (v0.9 sandbox F2c) | — (new in v0.9) | `{id, status, requester, action}` — one frame per change: `pending` when an ask lands, then `approved` / `rejected` when somebody decides; `approving performs nothing` | **new** |
| 15 | `m:sandbox:spawn` (v1.0 M2a-1; emitted v1.0 gap 2/N) | — (new in v1.0) | `{instance_id, definition}` — the dispatcher derived one | **new** |
| 16 | `m:sandbox:reap` (v1.0 M2a-1; emitted v1.0 gap 2/N) | — (new in v1.0) | `{instance_id, definition}` — the dispatcher destroyed one | **new** |
| 17 | `m:request:ask` (v1.0 gap 2/N) | — (new in v1.0) | `{id, status, decided_by}` — `status` is `pending` and `decided_by` is `null` until somebody decides | **new** |
| 18 | `m:request:approve` (v1.0 M2a-1; emitted v1.0 gap 2/N) | — (new in v1.0) | `{id, status, decided_by}` | **new** |
| 19 | `m:request:reject` (v1.0 gap 2/N) | — (new in v1.0) | `{id, status, decided_by}` | **new** |
| 20 | `m:task:dispatch` (v1.0 gap 2/N) | — (new in v1.0) | `{task_id, target, outcome}` — `outcome` is the `TaskOutcome`'s kind (`Final` / `MaxIterations` / `Failed`) | **new** |

**Four** of the twenty change shape, **eight** are the identity mapping, and the last eight
— `sandbox:switch` (F2b-2), `sandbox:request` (F2c) and the dispatcher's six — are new: they
have no v0.8 payload to map from.

The dispatcher's six are the acts an **AI supervisor** takes (v1.0 M2a-1; emitted since
v1.0 gap 2/N). Their audit rows spell the same names with dots (`m.sandbox.spawn`), and the
row's `agent_id` names the caller when the request declared one — `X-RiscDom-Agent`
(`docs/control-plane-api.md` §3). Two acts carry **no name of their own here**: a switch
keeps its single `sandbox:switch` frame, whose contract is "one frame per attempt, either
way" (F2b-2) — a second frame for the same attempt would be a second thing to count — and a
decision on a request keeps `sandbox:request`, one frame per change. What the batch adds for
both is the **chain row** (`m.sandbox.switch`, `m.request.approve`, `m.request.reject`).
**All three landed in v0.9 batch 3** (the fourth in the F1 batch — see below), at the emit
sites: the envelope wraps them, and the keys below are what a client sees in `payload`
today.

### 3.1 Old → new migration

`vm:state`. Today `name` is added to the payload only for a snapshot event. In v1 `name`
is always present and `null` unless the event is a snapshot. A client that tested "does
`payload.name` exist" to detect a snapshot must switch to `payload.state === "snapshot"`.
`state` stays one of `running` / `stopped` / `snapshot`.

`audit:failed`. The key is renamed for consistency with the API error model (§4 there),
where the human-readable text is `message`:

| old | new |
|---|---|
| `payload.error` | `payload.message` |

`toolchain:download`. Today the payload is the internally tagged `DownloadEvent` enum, whose
tag is `kind`; v1 renames that tag to `state` and keeps the variant fields beside it:

| old | new |
|---|---|
| `{"kind":"started","total_bytes":n}` | `{"state":"started","total_bytes":n}` |
| `{"kind":"progress","downloaded":d,"total":n}` | `{"state":"progress","downloaded":d,"total":n}` |
| `{"kind":"verifying"}` | `{"state":"verifying"}` |
| `{"kind":"extracting"}` | `{"state":"extracting"}` |
| `{"kind":"done","install_path":p}` | `{"state":"done","install_path":p}` |
| `{"kind":"failed","reason":r}` | `{"state":"failed","reason":r}` |
| `{"kind":"cancelled"}` | `{"state":"cancelled"}` |

`qemu:download` (v0.9 sandbox F1) migrates the **same** way — it is the same enum under the
same tag — so one table describes both assemblies' downloads. What differs is only how
likely a frame is: the toolchain's family carries a real download, while the QEMU one
refuses today (no release is pinned, `docs/qemu-distribution.md` §5) and so carries the
refusal rather than progress.

## 4. Filtering

**One parameter is enforced today: `task_id` (v1.0 M6-3b).** The other two — `event` and
`agent_id` — are still the design shape they have been since v0.9: they are accepted, echoed back in
`hello`, and **ignored**, so a client that sends them receives the whole stream exactly as before.

- **Query parameters, repeatable:**
  `GET /v0/events?event=agent:tool_call&event=vm:state&agent_id=dev-12345-1&task_id=task-12345-1`
  - `event` — any of the twenty names; repeat to select several. Absent means all. **Not honoured yet.**
  - `agent_id` — select one agent's events. **Not honoured yet.**
  - `task_id` — **select one task's events. This one is enforced**: the server drops frames that
    belong to another task, in the live stream **and** in `Last-Event-ID` replay. An event tied to no
    task is not any task's, so it is dropped too. A blank `?task_id=` is treated as absent (the whole
    stream) rather than as a task named "".
- **Server-side filter.** The server drops non-matching frames before they are written,
  so a filtered stream costs a client no bandwidth for events it will ignore.
- **Client-side filter is still required.** A client must tolerate receiving an event it
  did not ask for (a future server, a bug, a changed default) and ignore it. Filtering is
  an optimisation, never a correctness guarantee.
- **`hello` and `gap` are never filtered.** They describe the stream itself, so they are
  always delivered — and so is a comment heartbeat, which is not an event at all.

## 5. Frame examples

One frame per event, each as it appears on the wire (a blank line ends every frame). The
`id` values are illustrative.

`agent:iteration` — one LLM iteration started:

```text
id: 1758533001207-1
data: {"version":1,"kind":"event","event":"agent:iteration","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001207,"payload":{"model":"local-model","messages":[{"role":"user","content":"blink"}]}}

```

`agent:tool_call` — the model asked for a tool:

```text
id: 1758533001880-2
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001880,"payload":{"name":"write_source","arguments":{"path":"src/main.c","content":"int main(void){return 0;}"}}}

```

`agent:tool_result` — the tool returned:

```text
id: 1758533001902-3
data: {"version":1,"kind":"event","event":"agent:tool_result","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001902,"payload":{"ok":true,"result":"wrote 24 bytes"}}

```

`agent:stream:delta` — incremental assistant text:

```text
id: 1758533001950-4
data: {"version":1,"kind":"event","event":"agent:stream:delta","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001950,"payload":{"text":"Compiling"}}

```

`agent:stream:done` — the stream finished:

```text
id: 1758533001975-5
data: {"version":1,"kind":"event","event":"agent:stream:done","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001975,"payload":{}}

```

`agent:final` — the run finished:

```text
id: 1758533002010-6
data: {"version":1,"kind":"event","event":"agent:final","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533002010,"payload":{"kind":"final","content":"Blink compiled and ran; the banner is on the serial console.","reason":null,"iterations":1}}

```

`serial:chunk` — new serial output:

```text
id: 1758533002033-7
data: {"version":1,"kind":"event","event":"serial:chunk","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533002033,"payload":{"chunk":"hello from riscv\n"}}

```

`vm:state` — the VM started:

```text
id: 1758533002050-8
data: {"version":1,"kind":"event","event":"vm:state","agent_id":"dev-12345-1","task_id":null,"ts":1758533002050,"payload":{"state":"running","running":true,"since_ms":1758533002050,"name":null}}

```

`vm:state` — a snapshot was saved (note `name` present):

```text
id: 1758533002099-9
data: {"version":1,"kind":"event","event":"vm:state","agent_id":"dev-12345-1","task_id":null,"ts":1758533002099,"payload":{"state":"snapshot","running":true,"since_ms":1758533002050,"name":"after-blink"}}

```

`preflight:progress` — one environment check step:

```text
id: 1758533002110-10
data: {"version":1,"kind":"event","event":"preflight:progress","agent_id":"dev-12345-1","task_id":null,"ts":1758533002110,"payload":{"step":"gcc_runs","state":"ok","detail":"riscv64-unknown-elf-gcc (GCC) 13.2.0"}}

```

`audit:failed` — an audit write failed after its retries:

```text
id: 1758533002140-11
data: {"version":1,"kind":"event","event":"audit:failed","agent_id":"dev-12345-1","task_id":null,"ts":1758533002140,"payload":{"message":"database is locked"}}

```

`toolchain:download` — download progress (normalised form):

```text
id: 1758533002160-12
data: {"version":1,"kind":"event","event":"toolchain:download","agent_id":"dev-12345-1","task_id":null,"ts":1758533002160,"payload":{"state":"progress","downloaded":10485760,"total":209715200}}

```

`gap` — the requested replay was too old:

```text
id: 1758533002200-13
data: {"version":1,"kind":"gap","event":null,"agent_id":"server","task_id":null,"ts":1758533002200,"payload":{"lost_after":"1758533000100-88"}}

```

A client consuming the stream with a browser:

```js
// `EventSource` cannot send the `Authorization` header, so the stream is read with
// `fetch`. The frames are `id:` / `data:` lines, separated by a blank line.
const response = await fetch("/v0/events", {
  headers: { Authorization: `Bearer ${token}` },
});
const reader = response.body.getReader();
const decoder = new TextDecoder();
let pending = "";
for (;;) {
  const { value, done } = await reader.read();
  if (done) break;
  pending += decoder.decode(value, { stream: true });
  const frames = pending.split("\n\n");
  pending = frames.pop() ?? ""; // the last one may be half-arrived
  for (const frame of frames) {
    const data = frame
      .split("\n")
      .find((line) => line.startsWith("data: "))
      ?.slice(6);
    if (!data) continue; // a comment frame (the heartbeat)
    const env = JSON.parse(data);
    if (env.kind === "event" && env.event === "vm:state") {
      renderVmBadge(env.payload.running, env.payload.since_ms);
    }
  }
}
```
