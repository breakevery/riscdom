[中文](control-plane-client-guide.zh-CN.md) | English

# Writing a control-plane client

> **Applies to v0.9 (unstable until v1.0).** This guide is for whoever writes the client:
> how to call the query endpoints, how to read their answers, how to handle the errors,
> and how to subscribe to the event stream. The normative tables live in
> [control-plane-api.md](control-plane-api.md) and [control-plane-events.md](control-plane-events.md);
> this is the working walkthrough.

Everything here is plain HTTP. There is no SDK and no generated client: the surface is the
HTTP interface, so `curl`, `fetch`, or any HTTP library is enough.

## 1. Before the first call

Start the server against the workspace you want to inspect (see
[../server/README.md](../server/README.md) for the arguments):

```bash
riscdom-server --bind 127.0.0.1:7821 --workspace ./my-workspace
```

Every request carries a credential:

```bash
curl -sS http://127.0.0.1:7821/v0/health \
  -H 'Authorization: Bearer <token>'
```

**The token is required by default.** On first start the server writes 32 random bytes to
`<data-dir>/token` (owner-readable only) and refuses every request without it, so read the
file and send its contents:

```bash
export RISCDOM_TOKEN="$(cat /path/to/data-dir/token)"
curl -sS http://127.0.0.1:7821/v0/health -H "Authorization: Bearer $RISCDOM_TOKEN"
```

The token is **never printed or logged** — the start-up line names the file, not the value
— so the file is the only place to get it. A deployment may provision the file itself.
`--no-auth` drops the requirement (a warning is printed): local debugging only, because the
control endpoints include destructive ones. `401` means the credential was missing or
wrong; `403` means it was accepted but its actor does not hold the endpoint's capability
(see below).

A liveness check and a summary are the two calls that need no parameters:

```bash
curl -sS http://127.0.0.1:7821/v0/health
# {"status":"ok","version":"0.8.0","uptime_ms":1971}

curl -sS http://127.0.0.1:7821/v0/status
# {"agents":1,"agent_id":"local-17480-1","connections":1,"sse_subscribers":0,
#  "status":"ok","uptime_ms":1997,"version":"0.8.0"}
```

`connections` counts open TCP connections; `sse_subscribers` counts live event streams;
`agents` is the number of agents this host knows about.

### Capabilities: what a credential may do

Authentication and permission are two decisions. `401` means the server did not accept the
credential; `403` means it did, and the actor it resolved is not allowed to do *this*.

Every endpoint requires one capability, named in the API document's §5 tables — 28 names
such as `agent.run`, `audit.read`, `runs.control`, `settings.write` and `vm.control`. The
server checks it before the handler runs, so a client can plan around it instead of
discovering it:

```bash
# The token holder holds all 28, so this succeeds.
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:7821/v0/status \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
# 200

# No credential at all: refused before any capability is considered.
curl -sS http://127.0.0.1:7821/v0/status
# {"code":"unauthorized","message":"missing or invalid bearer token","retryable":false,"cause":null}
# 401

# An actor that authenticated but does not hold the capability.
curl -sS -X POST http://127.0.0.1:7821/v0/sessions/clear \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
# {"code":"forbidden","message":"the actor may not session.write","retryable":false,"cause":"capability"}
# 403
```

The third case does not arise under the v0.9 default: the token holder holds everything and
`NoAuth` (`--no-auth`) hands out the same set. It is what a distribution's own `Authn` hook
produces when it returns a narrower actor — and why a client should read `cause` rather than
assume "authenticated" means "allowed". The rule is default deny: an endpoint is refused
unless the credential's actor holds exactly the capability its route declares.

### A secure deployment

The loopback default is not a formality: this build speaks plaintext HTTP. Three things
belong in a deployment that goes further.

1. **Keep the token file to its owner.** `<data-dir>/token` is written `600` on Unix and with
   an owner-only ACL on Windows, and the server refuses to start if it cannot restrict it.
   Back it up, copy it or mount it with those permissions intact, and keep the value out of
   shared shell history (`$(cat …)` keeps it out of `ps`, too).
2. **Bind narrowly, then put TLS in front.** Keep `--bind 127.0.0.1:7821` and let a reverse
   proxy own the outside. Terminating TLS is the proxy's job; the server has no TLS.
3. **Never combine `--no-auth` with a non-loopback bind.** That is "anyone who can reach the
   port can delete sessions and stop the VM".

A minimal nginx front end (illustrative — the build ships no proxy):

```nginx
server {
    listen 443 ssl;
    server_name riscdom.example.internal;
    ssl_certificate     /etc/ssl/riscdom/fullchain.pem;
    ssl_certificate_key /etc/ssl/riscdom/privkey.pem;

    location /v0/ {
        proxy_pass http://127.0.0.1:7821;
        # The bearer token is the credential; keep it on this hop.
        proxy_set_header Authorization $http_authorization;
        proxy_set_header Host $host;
        # SSE: no buffering, no idle timeout, HTTP/1.1.
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

`proxy_buffering off` and the long `proxy_read_timeout` are what keep `/v0/events` a live
stream instead of a request that ends when the first heartbeat is late.

## 2. The query endpoints

26 endpoints, all `GET`. The responses are the host's view types; their fields are the ones
in `host-core/src/state.rs`. Everything is JSON.

### Audit and runs

```bash
# How many events, and is the hash chain intact?
curl -sS 'http://127.0.0.1:7821/v0/audit/status'
# {"alert_on_failure":true,"chain":{"status":"Intact","length":42},"count":42,"failures":[]}

# Recent events, newest first. `limit` is required here.
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10'
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10&actor=local-17480-1'
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10&action_prefix=agent.tool'

# Runs from the derived index (limit defaults to 20).
curl -sS 'http://127.0.0.1:7821/v0/runs?limit=20'

# One run, or null when this log has never seen it.
curl -sS 'http://127.0.0.1:7821/v0/runs/run-17480-3'

# Two runs' configuration fingerprints, field by field.
curl -sS 'http://127.0.0.1:7821/v0/runs/diff?run_a=run-17480-3&run_b=run-17480-4'
```

A `RunView` carries `run_id`, `status` (`open` / `ok` / `failed` / `interrupted` /
`abandoned`), `fingerprint` (64 hex characters) and `fingerprint_short` (the first 16),
`parent_run_id`, `session_id`, `resumed_from_snapshot`, `started_at_ms` and `ended_at_ms`.

> **`/v0/audit/status` does not consume the failure queue.** The desktop command takes the
> pending audit failures; this `GET` reports them without taking, so polling here never
> steals another client's alert.

> **An unknown run in `/v0/runs/diff` answers `500 internal`.** The host reports a missing
> run as a message, not as a typed not-found. Check `/v0/runs/{id}` when you need to tell
> "no such run" from "the diff failed".

### LLM configuration

Nothing here ever returns a key.

```bash
curl -sS 'http://127.0.0.1:7821/v0/llm/provider-presets'   # the dropdown's data
curl -sS 'http://127.0.0.1:7821/v0/llm/config'             # {"configured":false,...}
curl -sS 'http://127.0.0.1:7821/v0/llm/readiness'          # {"ready":false,"reason":"no_config",...}
curl -sS 'http://127.0.0.1:7821/v0/llm/local-probe'        # loopback OpenAI-compatible servers
curl -sS 'http://127.0.0.1:7821/v0/llm/stored-key?provider_id=deepseek'
# {"present":false}
```

### Sessions, snapshots, VM

```bash
curl -sS 'http://127.0.0.1:7821/v0/sessions?limit=20'   # limit required
curl -sS 'http://127.0.0.1:7821/v0/sessions/current'
# {"session_id":"s-17480-1"}   (or null)

curl -sS 'http://127.0.0.1:7821/v0/snapshots'
# [{"name":"after-blink","size_bytes":1048576,"created_at_ms":1758533002099,"mode":"tcp-relay"}]

curl -sS 'http://127.0.0.1:7821/v0/vm/running'   # {"running":true}
curl -sS 'http://127.0.0.1:7821/v0/vm/status'    # {"running":true,"since_ms":1758533002050}
```

A snapshot's `mode` is `tcp-relay` (a real migration stream) or `reboot-fallback` (a JSON
fallback).

### Toolchain, QEMU, preflight

```bash
curl -sS 'http://127.0.0.1:7821/v0/toolchain'
# {"found":true,"path":"...","source":"Path","diagnostics":"..."}
curl -sS 'http://127.0.0.1:7821/v0/toolchain/download'
# {"in_progress":false,"last_event":null}
curl -sS 'http://127.0.0.1:7821/v0/qemu'
curl -sS 'http://127.0.0.1:7821/v0/qemu/status'   # the same view
curl -sS 'http://127.0.0.1:7821/v0/preflight'
# {"ran":true,"checked":true,"ok":true,"rows":[...],"failed_step":null,...}
```

`source` is one of `EnvVar` / `KnownPath` / `Path` / `Manual`. The preflight answer is the
cached result for the current configuration: the endpoint reports, it does not run the
checks.

### Settings, workspace, serial

```bash
curl -sS 'http://127.0.0.1:7821/v0/settings/theme'      # {"theme":"system"}
curl -sS 'http://127.0.0.1:7821/v0/settings/language'   # {"language":"system"}
curl -sS 'http://127.0.0.1:7821/v0/workspace/root'      # {"root":"/abs/path/to/workspace"}
curl -sS 'http://127.0.0.1:7821/v0/workspace/files'     # ["src/main.c", ...]
curl -sS 'http://127.0.0.1:7821/v0/workspace/file?path=src%2Fmain.c'
# {"content":"int main(void) { return 0; }\n"}
curl -sS 'http://127.0.0.1:7821/v0/serial'              # {"buffer":"hello from riscv\n"}
```

`?path=` is percent-decoded, so `src%2Fmain.c` and `src/main.c` are the same request. A
read is checked against the workspace policy: outside the root, it is `403`.

### The reserved aggregate

```bash
curl -sS 'http://127.0.0.1:7821/v0/resources'
# {"code":"not_implemented","message":"resource accounting is reserved ...","retryable":false,"cause":"resources"}
```

## 3. Parameters

- A required parameter that is missing or unparsable is `400` with `cause` set to its name:

```json
{ "code": "bad_request", "message": "missing required parameter \"limit\"", "retryable": false, "cause": "limit" }
```

- `limit` is required where the host command requires it: `/v0/audit/events` and
  `/v0/sessions`. `/v0/runs` accepts it optionally and defaults to 20.
- There is no offset or cursor anywhere in v0.9: raise `limit` and filter client-side.

## 4. Handling errors

Every non-2xx answer is one object:

```json
{ "code": "not_found", "message": "no endpoint GET /v0/nope", "retryable": false, "cause": null }
```

Treat `code` as the contract and `message` as text for a human. A client should switch on
`code` and, at most, read `cause` to point at the offending field.

| Status | `code` | What to do |
|---|---|---|
| 400 | `bad_request` | Fix the request; `cause` names the parameter. |
| 401 | `unauthorized` | Send a valid credential. |
| 403 | `forbidden` | Not allowed: `cause: "capability"` means the actor lacks the endpoint's capability, otherwise the path is outside the workspace. Do not retry. |
| 404 | `not_found` | The endpoint or the resource is not there. |
| 405 | `method_not_allowed` | Use the method named in `message`. |
| 409 | `conflict` | A state clash; re-read the state and decide. |
| 500 | `internal` | A host failure; `message` is the host's own text. |
| 501 | `not_implemented` | Reserved; a later batch fills it in. |
| 503 | `unavailable` | A dependency is not ready (no LLM, no QEMU, no toolchain). |

Retry only when `retryable` is `true` — the controls (`202` + an event stream) are where
asynchronous work reports itself, and nothing in the query surface does.

```bash
# A missing required parameter.
curl -sS -o - -w '\n%{http_code}\n' 'http://127.0.0.1:7821/v0/audit/events'
# {"code":"bad_request","message":"missing required parameter \"limit\"","retryable":false,"cause":"limit"}
# 400

# The wrong method on a served path.
curl -sS -o - -w '\n%{http_code}\n' -X POST 'http://127.0.0.1:7821/v0/snapshots'
# {"code":"method_not_allowed","message":"POST is not allowed on /v0/snapshots; use GET","retryable":false,"cause":"method"}
# 405
```

## 5. Subscribing to the event stream

`GET /v0/events` is `text/event-stream`. It stays open, so use a client that streams:

```bash
curl -sS -N http://127.0.0.1:7821/v0/events \
  -H 'Authorization: Bearer <token>' \
  -H 'Accept: text/event-stream'
```

The first frame is always `hello`; after it come the host's events, one frame each:

```text
id: 1790074876659-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"local-17480-1","task_id":null,"ts":1790074876659,"payload":{"buffer":{"from":0,"to":0},"filters":{"event":[],"agent_id":null,"task_id":null}}}

id: 1790074877033-1
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"local-17480-1","task_id":null,"ts":1790074877033,"payload":{"name":"write_source","arguments":{"path":"src/main.c"}}}

id: 1790074877100-2
data: {"version":1,"kind":"event","event":"serial:chunk","agent_id":"local-17480-1","task_id":null,"ts":1790074877100,"payload":{"chunk":"hello from riscv\n"}}

```

Rules a client can rely on:

- **Frames use `id:` and `data:` only.** There is no SSE `event:` field, so every frame
  arrives at one `onmessage` handler; route on the envelope's `event`.
- **Every frame ends with a blank line.**
- **The heartbeat is a comment line** every 15 seconds: `: keep-alive`. Ignore lines
  starting with `:`.
- **`id:` is opaque.** Store it if you plan to reconnect; do not parse it.
- **The envelope's keys are stable for v0.9**: `version`, `kind`, `event`, `agent_id`,
  `task_id`, `ts`, `payload`. Ignore unknown payload keys.

```js
const source = new EventSource("/v0/events"); // with an established session
source.onmessage = (e) => {
  const frame = JSON.parse(e.data);
  if (frame.kind !== "event") return;          // hello (and, later, gap)
  switch (frame.event) {
    case "vm:state":
      // `name` is always present now: null unless it is a snapshot.
      badge(frame.payload.running, frame.payload.since_ms, frame.payload.name);
      break;
    case "serial:chunk":
      terminal.write(frame.payload.chunk);
      break;
    case "audit:failed":
      warn(frame.payload.message);              // was `error` before v0.9
      break;
  }
};
```

**Reconnecting.** Every frame carries `id: <ts>-<seq>`. Remember the last one you saw and
send it back as `Last-Event-ID`; the server replays what follows it from its in-memory
buffer (the last 1024 frames):

```bash
curl -sS -N http://127.0.0.1:7821/v0/events \
  -H "Authorization: Bearer $RISCDOM_TOKEN" \
  -H 'Last-Event-ID: 1790074877033-7'
```

If your cursor has fallen out of that buffer the stream starts with a `gap` frame —
`payload.lost_after` names the oldest id still held — and the frames from there on follow.
Either way, re-read the state with the queries above rather than assuming the stream was
complete.

## 6. The control endpoints

27 `POST` endpoints, the API table's §5.2. All of them need the token (that is the point
of the batch that added them: they include destructive operations).

```bash
# Run one agent turn. Progress arrives as `agent:*` events on the stream.
curl -sS -X POST http://127.0.0.1:7821/v0/agent/run \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"user_input":"compile the blink example"}'

# Sessions.
curl -sS -X POST http://127.0.0.1:7821/v0/sessions/create \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"title":"blink"}'
# {"session_id":"sess-1a2b3c4d-0"}

curl -sS -X POST http://127.0.0.1:7821/v0/sessions/clear \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'

# Snapshots.
curl -sS -X POST http://127.0.0.1:7821/v0/snapshots/save \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"after-blink"}'
# {"bytes_written":1048576}

curl -sS -X POST http://127.0.0.1:7821/v0/snapshots/resume \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"after-blink"}'

# The VM, the settings, the exports.
curl -sS -X POST http://127.0.0.1:7821/v0/vm/stop \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'

curl -sS -X POST http://127.0.0.1:7821/v0/settings/theme \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"theme":"dark"}'

curl -sS -X POST http://127.0.0.1:7821/v0/audit/export \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"path":"/abs/path/inside/the/workspace/audit.jsonl"}'
```

Notes a client should know:

- **A `204` answers most controls** (nothing to say), `200` answers the ones that return a
  value, and `202` answers the ones that start work in the background (`agent/run`,
  `preflight/run`, `toolchain/download`) — watch the stream for those.
- **Parameters are validated**: a missing or unusable one is `400` with `cause` naming it.
- **State clashes are `409`** (`save` with no VM running, `resume` of an unknown snapshot is
  `404`, `toolchain/download/cancel` with nothing running).
- **`POST /v0/toolchain/download` really downloads** the pinned RISC-V GCC archive.
- **`POST /v0/vm/start` answers `501`** (reserved: today the VM starts inside a run), and so
  does `GET /v0/resources`.

## 7. What is not there yet

- **Fine-grained credentials.** Every route's capability is enforced (see §1); what v0.9 has
  only one of is credentials. The single token holds everything, so a client cannot be given
  read-only access to the audit chain alone — per-capability tokens are v1.0 work.
- **`POST /v0/vm/start`** and **`GET /v0/resources`** answer `501`.
