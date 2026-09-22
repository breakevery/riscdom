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

**Where the events come from.** `host/src/events.rs` defines eleven event names and an
`EventSink` trait (`emit(&self, event: &str, payload: serde_json::Value)`). Three
implementations exist today: `TauriEventSink` (to the webview), `RecordingEventSink`
(tests), and `LineEventSink` (`worker`, JSON lines on stderr). The control plane adds a
fourth sink that writes SSE frames; **it does not change the emit sites.** Normalising
the payloads at the source is a later batch's work; the envelope below is what the SSE
sink wraps around the payloads exactly as they are emitted.

## 1. SSE protocol

- **Content type.** `text/event-stream; charset=utf-8`, with `Cache-Control: no-cache`
  and `Connection: keep-alive`.
- **Endpoint.** `GET /v0/events`. The stream opens on `200`; the connection stays open
  until the client closes it or the server stops.
- **Frames use `id:` and `data:` only — no `event:` field.** *Decision and rationale:*
  setting SSE's `event` field would make the browser's `EventSource` dispatch to a named
  listener and stop firing `onmessage`, forcing a client to register eleven listeners.
  Leaving it unset delivers every frame to one `onmessage` handler, and the envelope's
  `event` field does the routing. One handler, one router.
- **Heartbeat.** A comment line (starting with `:`) every 15 seconds keeps intermediaries
  from closing an idle stream and lets a client detect a dead connection:

```text
: keep-alive

```

- **Authentication.** The `Authorization: Bearer <token>` header, as in the API
  document. A browser's `EventSource` cannot set headers, so a browser client either
  opens a session first (a `POST` that sets a cookie, then `new EventSource(...)` with
  `withCredentials`) or passes a short-lived token in the query string. Both are
  documented as client-integration options; the query-string token is discouraged (it
  lands in access logs) and is the client's explicit choice, never the default.
- **Reconnect.** A dropped stream is resumed by reconnecting with the same `(event
  filters)` and the last id the client saw, sent as the `Last-Event-ID` header (browsers
  do this automatically; other clients must do it themselves).
- **`id:` is the replay cursor.** The server emits `id: <ts>-<seq>`, where `ts` is the
  event timestamp in epoch milliseconds and `seq` is a per-connection monotonic counter.
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

Every frame carries the same object. This is the batch's core deliverable: the eleven
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
| `event` | string \| null | yes | One of the eleven names, or `null` for `hello` / `gap`. |
| `agent_id` | string | yes | The agent that caused the event, `<device>-<pid>-<seq>`. |
| `task_id` | string \| null | yes | The dispatched task it belongs to; `null` when not tied to one. |
| `ts` | integer | yes | Epoch milliseconds. |
| `payload` | object | yes | Event-specific body (§3). |

`kind` values in v0.9:

- `event` — one of the eleven events; `event` names it.
- `hello` — the stream opened; `payload.buffer` and `payload.filters` describe it.
- `gap` — the requested replay id was too old to replay; `payload.lost_after` is the
  oldest id the server still holds. A client that sees a `gap` must re-sync from a
  query (§ the API document) rather than assume it missed nothing.

### 2.1 How `version` evolves

- **Adding a payload field does not bump `version`.** Clients ignore unknown payload keys.
- **Adding a new event name or a new `kind` value does not bump `version`.** Clients
  ignore frames they do not recognise.
- **Changing a field's meaning, type, or removing it bumps `version`.** The bump is the
  only signal a client gets that it must change, so it is reserved for exactly that.
- The envelope's top-level fields are frozen for v0.9: a new one would be a `version`
  bump, because a client validating the envelope would have to change.

## 3. The eleven events, normalised

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
| 11 | `toolchain:download` | externally-tagged enum (`{"Started":{...}}`, …) | `{state, ...}` — lowercased `state` plus flat fields | **yes** |

**Three** of the eleven change shape; **eight** are the identity mapping.

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

`toolchain:download`. Today the payload is the externally-tagged `DownloadEvent` enum.
v1 flattens it and lowercases the tag into `state`:

| old | new |
|---|---|
| `{"Started":{"total_bytes":n}}` | `{"state":"started","total_bytes":n}` |
| `{"Progress":{"downloaded":d,"total":n}}` | `{"state":"progress","downloaded":d,"total":n}` |
| `{"Verifying"}` | `{"state":"verifying"}` |
| `{"Extracting"}` | `{"state":"extracting"}` |
| `{"Done":{"install_path":p}}` | `{"state":"done","install_path":p}` |
| `{"Failed":{"reason":r}}` | `{"state":"failed","reason":r}` |
| `{"Cancelled"}` | `{"state":"cancelled"}` |

## 4. Filtering

A client may subscribe to a subset. This is a design shape for v0.9; the mechanism is
built in the implementation batch.

- **Query parameters, repeatable:**
  `GET /v0/events?event=agent:tool_call&event=vm:state&agent_id=dev-12345-1&task_id=task-12345-1`
  - `event` — any of the eleven names; repeat to select several. Absent means all.
  - `agent_id` — select one agent's events.
  - `task_id` — select one task's events.
- **Server-side filter.** The server drops non-matching frames before they are written,
  so a filtered stream costs a client no bandwidth for events it will ignore.
- **Client-side filter is still required.** A client must tolerate receiving an event it
  did not ask for (a future server, a bug, a changed default) and ignore it. Filtering is
  an optimisation, never a correctness guarantee.
- **`hello` and `gap` are never filtered.** They describe the stream itself, so they are
  always delivered.

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
const src = new EventSource("/v0/events"); // with an established session cookie
src.onmessage = (e) => {
  const env = JSON.parse(e.data);
  if (env.kind === "event" && env.event === "vm:state") {
    renderVmBadge(env.payload.running, env.payload.since_ms);
  }
};
```
