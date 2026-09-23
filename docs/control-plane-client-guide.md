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

Every endpoint requires one capability, named in the API document's §5 tables — 32 names
such as `agent.run`, `audit.read`, `runs.control`, `settings.write` and `vm.control`. The
server checks it before the handler runs, so a client can plan around it instead of
discovering it:

```bash
# The token holder holds all 32, so this succeeds.
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

31 endpoints, all `GET`. The responses are the host's view types; their fields are the ones
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
read is checked against the workspace policy: a path that leaves the root is the caller's
parameter being unusable, so it is `400` with `cause: "path"`.

### Sandboxes

```bash
curl -sS 'http://127.0.0.1:7821/v0/sandboxes'
# {"sandboxes":[{"name":"blink","source":"manual","runnable":true,"shadowed":false,
#   "memory_mb":256,"toolchain_path":"...","qemu_exe":"...","kernel":null,
#   "display_name":"Blink","notes":null},
#  {"name":"default","source":"discovered","runnable":true,"shadowed":false,...}],
#  "current":"blink","default":"blink"}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/current'
# {"current":"blink","default":"blink"}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/candidates'
# {"toolchains":[{"kind":"toolchain","version":"15.2.0-1","origin":"installed",...}],
#  "qemus":[{"kind":"qemu","version":"11.1.0","origin":"installed",...}]}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/blink'   # one entry, the same shape as a list row
```

The list is the **merged registry**: the definitions written by hand in `settings.json`,
then what the scan found, then the built-in `default`. `source` is `manual` or
`discovered`; `runnable` is computed per read (a QEMU that exists and answers `--version`,
a toolchain that exists, and a kernel that exists or can be compiled), so a definition
whose resource was uninstalled stays listed and answers `runnable: false`. A hand-written
definition wins a name collision, and the scanned entry **stays in the list** marked
`shadowed: true`.

`/v0/sandboxes/candidates` is the raw scan instead — the two independent lists, nothing
merged and nothing written back — and `/v0/sandboxes/{name}` is a `404` naming the
parameter when no definition carries that name. The literal sub-paths (`current`,
`candidates`, and `requests` / `switch` / `assemble`) are never read as a name.

### The project: out, and back in
The workspace is the project, and a project travels as one archive. Export answers
**bytes** — the only non-JSON body here apart from the event stream — and import takes
bytes.

```bash
# Out. `curl -o` writes the archive; an empty workspace exports a valid empty one.
curl -sS -X POST http://127.0.0.1:7821/v0/workspace/export \
  -H "Authorization: Bearer ***" -o project.tar.gz

# In. `--data-binary` matters: a tool that strips newlines or re-encodes would
# corrupt the archive, and the Content-Type picks the reader (zip / gzip / tar).
curl -sS -X POST 'http://127.0.0.1:7821/v0/workspace/import' \
  -H "Authorization: Bearer ***" -H 'Content-Type: application/gzip' \
  --data-binary @project.tar.gz
# {"files":17,"bytes":48211}

# A file that is already there is a 409 unless you say otherwise.
curl -sS -X POST 'http://127.0.0.1:7821/v0/workspace/import?force=true' \
  -H "Authorization: Bearer ***" -H 'Content-Type: application/gzip' \
  --data-binary @project.tar.gz
```

What an archive may not do is checked as it is read, and each refusal names itself: an
entry that escapes the workspace, a symlink or hard link, something under `.riscdom/`
(the host's own state — the audit DB, snapshots, the preflight cache), or a body that is
not a readable archive are all `400` with `cause: "archive"`; a file already in the
workspace is `409` with `cause: "exists"`; and more than 64 MiB is `413`. Importing
needs `workspace.write`, exporting `workspace.read`.

The same two trips from the CLI:

```bash
riscdom workspace export --out project.tar.gz   # or `> project.tar.gz`: count on stderr
riscdom workspace import project.tar.gz         # refuses to replace what is there
riscdom workspace import project.tar.gz --force # replaces it, and says how much landed
```

### Sandbox requests: asking, and deciding

A **request** is how an actor that may not switch says what it wants. An agent's
`request_sandbox` tool lands one; `POST /v0/sandboxes/requests` does the same over HTTP
and needs `agent.run`.

```bash
# Leave an ask. 201 answers the id; the ask switches nothing.
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/requests \
  -d '{"action":"switch","sandbox":"big","reason":"the guest needs more memory"}'
# {"id":"req-4711-1"}

# Read the queue (newest first), or just what is waiting.
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/requests?status=pending'
# {"requests":[{"id":"req-4711-1","requester_agent_id":"local-4711-1","action":"switch",
#   "sandbox":"big","definition":null,"reason":"the guest needs more memory",
#   "requested_at_ms":1758533002110,"status":"pending","decided_by":null,"decided_at_ms":null}]}

# Decide it. Approving changes the record and NOTHING else — the switch is its own call.
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/requests/req-4711-1/approve
# {"id":"req-4711-1",...,"status":"approved","decided_by":"operator","decided_at_ms":1758533009999}
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/switch -d '{"name":"big"}'   # now it moves
```

Both decisions need `sandbox.read` — a decider has to be able to see the queue — and then
the capability the request's own `action` implies: `sandbox.switch` for a `switch`, and
`sandbox.assemble` for `define` / `assemble`. An actor holding only `sandbox.read` can
read the queue and gets `403` with `cause: "capability"` (naming the one it wanted) on
the decision. An unknown id is `404` with `cause: "id"`; deciding an already-decided
request is `409` — a decision is not reversible. `?status=` takes `pending` / `approved` /
`rejected` (`expired` is reserved: v0.9 has no TTL, so a pending request waits until
somebody decides it).

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
| 400 | `bad_request` | Fix the request; `cause` names the parameter — including a workspace path the policy refuses (`cause: "path"`). |
| 401 | `unauthorized` | Send a valid credential. |
| 403 | `forbidden` | Not allowed: `cause: "capability"` means the actor lacks the endpoint's capability. This status is only ever authentication or authorisation. Do not retry. |
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

The API table's §5.2 controls, plus the sandbox switch and the request and project in/out
endpoints, are all `POST`s. All of them need the
token (that is the point of the batch that added them: they include destructive operations).

```bash
# Run one agent turn. Progress arrives as `agent:*` events on the stream. The
# optional `sandbox` is a declaration, not a switch: this run uses that definition
# (toolchain, QEMU, memory) and the node is left where it is.
curl -sS -X POST http://127.0.0.1:7821/v0/agent/run \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"user_input":"compile the blink example","sandbox":"blink"}'
# 404 cause "name"    — no definition is called that (a typo is not a fallback)
# 409 cause "sandbox" — a VM from another definition is already running: stop it, or
#   `POST /v0/sandboxes/switch` to blink. A run never switches the node itself.
```

```bash
# Dispatch a task to a *configured executor* — the sibling of the run above, and
# not a synonym for it. `run` works on this node; a task is routed by its target.
curl -sS http://127.0.0.1:7821/v0/executors -H "Authorization: Bearer $RISCDOM_TOKEN"
# {"executors":[{"agent_id":"executor-0"}]} — nothing configured is []

curl -sS -X POST http://127.0.0.1:7821/v0/tasks \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"target":"executor-0","input":"say hi"}'
# {"task_id":"task-4711-1","agent_id":"device-4711-9","outcome":{"Final":{...}}}
# 404 cause "target" — this node owns nobody by that name (including itself: a task
#   for this node is `POST /v0/agent/run`). Synchronous, like a run — the answer
#   *is* the outcome, and there is no task to poll. `id` is optional (the server
#   mints one), and a task whose run merely *failed* is still a `200`: its `outcome`
#   says `Failed`. The `agent_id` in the answer is the identity the executor
#   announced, not the label the task was addressed by.
```

```bash
# Switch this node to another sandbox definition. Validation happens before the
# running VM is touched, and the answer says where the node came from:
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/switch \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"blink"}'
# {"from":null,"to":"blink"}

# What it refuses with, and why: a name nobody has (`404`, `cause: "name"`), a
# switch while a run is in flight (`409`, `cause: "run"`), a second switch while
# one is in progress (`409`, `cause: "sandbox"`), a definition that cannot run
# (`503`, `cause` = the reason code), or a VM that would not start (`500`,
# `cause: "sandbox_start_failed"` — the node is then stopped, not half-switched).

# The request queue: leave an ask, read it back, decide it. `approve` performs
# nothing — the switch above is what moves the node (v0.9 sandbox F2c).
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/requests \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"action":"switch","sandbox":"blink","reason":"why not"}'
# {"id":"req-4711-1"}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/requests?status=pending' \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/requests/req-4711-1/reject \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'
# A decision needs the capability the ask implies (`sandbox.switch` / `sandbox.assemble`),
# so a read-only actor gets `403`, `cause: "capability"`; an unknown id is `404`,
# `cause: "id"`; a second decision is `409`.

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
# {"events_exported":42}
```

Notes a client should know:

- **A `204` answers most controls** (nothing to say), `200` answers the ones that return a
  value, and `202` answers the ones that start work in the background (`agent/run`,
  `preflight/run`, `toolchain/download`) — watch the stream for those.
- **The two audit exports answer a count of events** (`events_exported`), because that is
  what they write; `/v0/serial/export` answers `bytes_written`, because that is what it
  writes.
- **`POST /v0/qemu/download` refuses today, on every platform, by decision.** RiscDom guides
  the user to a QEMU they install themselves (`docs/qemu-distribution.md` §5) and pins no
  release, so the endpoint answers `503 unavailable` with `cause: "qemu"` and the guidance in
  `message` — and it claims no download slot. Its `GET` (status) and
  `/v0/qemu/download/cancel` are live, so a client can already code against the pair the way
  it codes against the toolchain's:

  ```bash
  curl -sS http://127.0.0.1:7821/v0/qemu/download -H "Authorization: Bearer ***"
  # {"in_progress":false,"last_event":null}

  curl -sS -X POST http://127.0.0.1:7821/v0/qemu/download -H "Authorization: Bearer ***" -d '{}'
  # {"code":"unavailable","message":"no QEMU download is pinned for windows-x86_64: …","cause":"qemu"}
  # 503
  ```

- **Parameters are validated**: a missing or unusable one is `400` with `cause` naming it.
- **State clashes are `409`** (`save` with no VM running, `resume` of an unknown snapshot is
  `404`, `toolchain/download/cancel` with nothing running).
- **`POST /v0/toolchain/download` really downloads** the pinned RISC-V GCC archive.
- **`POST /v0/vm/start` answers `501`** (reserved: today the VM starts inside a run), and so
  does `GET /v0/resources`.

## 7. Driving the control plane from the CLI

`riscdom` is the reference client, and the fastest way to check that a control plane answers
what this document says it answers. It is a client in the strict sense: every command is an HTTP
request, and the local mode simply starts the control plane inside its own process on a loopback
port the OS picks.

```bash
riscdom health --json                 # against a control plane it starts itself
riscdom --json --remote 127.0.0.1:7821 runs list --limit 5   # against one that is already up
```

| Command | Endpoint |
|---|---|
| `riscdom health` | `GET /v0/health` |
| `riscdom status` / `riscdom agents` | `GET /v0/status` |
| `riscdom runs list [--limit <n>]` | `GET /v0/runs` |
| `riscdom runs get <run_id>` | `GET /v0/runs/<run_id>` |
| `riscdom audit status` | `GET /v0/audit/status` |
| `riscdom audit events [--limit <n>]` | `GET /v0/audit/events` |
| `riscdom snapshots list` | `GET /v0/snapshots` |
| `riscdom sandboxes list` / `current` / `candidates` / `show <name>` | `GET /v0/sandboxes` / `/v0/sandboxes/current` / `/v0/sandboxes/candidates` / `/v0/sandboxes/<name>` |
| `riscdom sandboxes switch <name>` | `POST /v0/sandboxes/switch` |
| `riscdom sandboxes requests [--status <s>]` | `GET /v0/sandboxes/requests` |
| `riscdom sandboxes requests approve <id>` / `reject <id>` | `POST /v0/sandboxes/requests/<id>/approve` / `/reject` |
| `riscdom workspace export [--out <file>]` | `POST /v0/workspace/export` |
| `riscdom workspace import <archive> [--force]` | `POST /v0/workspace/import` |
| `riscdom run <task>` | `POST /v0/agent/run` |
| `riscdom run <task> --sandbox <name>` | `POST /v0/agent/run` (with `sandbox`) |
| `riscdom executors list` | `GET /v0/executors` |
| `riscdom tasks dispatch --target <agent_id> --input <text>` | `POST /v0/tasks` |
| `riscdom vm stop` / `vm start` | `POST /v0/vm/stop` / `/v0/vm/start` |
| `riscdom snapshots save` / `resume` / `delete <name>` | `POST /v0/snapshots/save` / `resume` / `delete` |
| `riscdom sessions create` / `open` / `rename` / `delete` / `clear-all` | `POST /v0/sessions/create` / `open` / `rename` / `delete` / `clear` |
| `riscdom runs abandon-stale` | `POST /v0/runs/abandon-stale` |
| `riscdom export audit-jsonl` / `run-audit <run_id>` / `serial-log` | `POST /v0/audit/export` / `/v0/runs/export` / `/v0/serial/export` |
| `riscdom llm set` / `clear` / `load-key <provider_id>` | `POST /v0/llm/config` / `/v0/llm/config/clear` / `/v0/llm/stored-key/load` |
| `riscdom qemu path <file>` / `clear` | `POST /v0/qemu/path` / `/v0/qemu/path/clear` |
| `riscdom toolchain download` / `cancel` / `path <file>` / `clear` | `POST /v0/toolchain/download` / `/v0/toolchain/download/cancel` / `/v0/toolchain/path` / `/v0/toolchain/path/clear` |
| `riscdom preflight run` / `ack` | `POST /v0/preflight/run` / `/v0/preflight/ack` |
| `riscdom audit alert set <on\|off>` / `theme set <theme>` / `language set <lang>` | `POST /v0/audit/alert` / `/v0/settings/theme` / `/v0/settings/language` |

```bash
# An export writes where the *server* says: `--out` is resolved against the
# workspace root, and the answer is a count, not the file.
riscdom export audit-jsonl --out audit.jsonl
# exported 1 event to audit.jsonl

# The sandbox registry, and which definition a run would use.
riscdom sandboxes list
# current    blink
# default    blink
#
# NAME                         SOURCE      RUNNABLE  SHADOWED  MEMORY_MB
# blink                        manual      true      false     256
# default                      discovered  true      false     -

riscdom sandboxes current          # just the two names
riscdom sandboxes show blink       # one definition, one line per field
riscdom sandboxes candidates       # what is installed here (the raw scan)

# Switching asks first (it stops the running VM and refuses while a run is in
# flight); `--yes` answers up front, and a non-terminal stdin needs it.
riscdom sandboxes switch blink --yes
# switched to blink

# The request queue: what is waiting for a decision, and deciding it. Both
# decisions ask first, for the same reason a switch does.
riscdom sandboxes requests                    # newest first
# no sandbox requests
riscdom sandboxes requests --status pending
riscdom sandboxes requests approve req-4711-1 --yes
# req-4711-1 is now approved

# The project, out and back. `--out` writes the archive and the count goes to
# stderr, so `riscdom workspace export > project.tar.gz` works the same way.
riscdom workspace export --out project.tar.gz
# exported 48211 bytes to project.tar.gz
riscdom workspace import project.tar.gz --force
# imported 17 file(s), 48211 bytes

# Configure the model; the key comes from a file, so it stays out of `ps`.
riscdom llm set --api-key-file ~/.riscdom/api-key \
  --base-url https://api.deepseek.com --model deepseek-chat
```

**`--follow` is the CLI's version of §5.** `riscdom run <task> --follow` subscribes to
`/v0/events` first, then starts the run, so every event the run produces is printed as it
arrives — the same frames a client of §5 would read — and the run's outcome comes last:

```bash
riscdom run "compile the blink example" --follow
```

Or under a named sandbox — a declaration, so the node is not switched:

```bash
riscdom run "compile the blink example" --sandbox blink --follow
```

```text
agent:llm.stream.start {"iteration":1}
agent:tool_call {"name":"write_source","arguments":{"path":"src/main.c"}}
serial:chunk {"chunk":"hello from riscv\n"}
agent:final {"kind":"final"}
kind       final
iterations 3
```

**`--wait` is the same trick for the two asynchronous controls.** `toolchain download`
and `preflight run` answer `202` and do the work on a thread; `--wait` subscribes before
it posts, prints the frames of that work's event family (`toolchain:download`,
`preflight:progress`) and stops at the one that says it is over — the download's `done`
/ `failed`, or the preflight's last step / first `failed`, which is where fail-fast ends
it. The exit code is the work's verdict (`3` when it failed), not the `202`'s.

```bash
riscdom toolchain download --wait
# toolchain:download {"install_path":"…","state":"done"}
# download ok
```

- **`--json`** prints exactly what the control plane sent — the same fields §2 and §5 document —
  so a client built against this document can be debugged with it. Failures print the error
  object of §4 on **stderr**. With `--follow` and `--wait`, each frame is the envelope verbatim.
- **`--api-key` warns** the way `--token` does (it lands in the shell history and in `ps`);
  `--api-key-file` is the shape to prefer, and `--remember` is what makes the key survive a
  restart (the host stores it in the OS credential store).
- **The destructive commands ask first** (`vm stop`, `snapshots resume`, `snapshots delete`,
  `sandboxes switch`, `sessions delete`, `sessions clear-all`, `llm clear`, `qemu clear`,
  `toolchain clear`): a
  prompt on a terminal, `--yes` to answer up front, and a refusal (exit `2`) when stdin is
  not a terminal — a script has to say `--yes`.
- **Exit codes** turn the status codes of §4 into something a script can branch on: `0` success,
  `1` a local failure (no connection, no token), `2` usage or `400`, `3` refused or `5xx`,
  `4` `401`/`403` (authentication and authorisation; a workspace path the policy refuses is
  the `400` above, so it exits `2`).
- **The token** comes from `<data-dir>/token` in local mode and from `--token-file`,
  `RISCDOM_TOKEN` or `--token` (in that order) in remote mode. It is never printed.

The full table, including the human-mode shapes, is in
[../cli/README.md](../cli/README.md).

## 8. What is not there yet

- **Fine-grained credentials.** Every route's capability is enforced (see §1); what v0.9 has
  only one of is credentials. The single token holds everything, so a client cannot be given
  read-only access to the audit chain alone — per-capability tokens are v1.0 work.
- **`POST /v0/vm/start`** and **`GET /v0/resources`** answer `501`.
