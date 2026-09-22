[中文](README.zh-CN.md) | English

# riscdom-server — the control plane

> **Applies to v0.9. The surface is unstable until v1.0.** This is the integrator's
> document: how to build it, how to start it, and what answers today. The protocol it
> implements is settled in [docs/control-plane-api.md](../docs/control-plane-api.md) and
> [docs/control-plane-events.md](../docs/control-plane-events.md).

`riscdom-server` is the RiscDom control plane as a process: the same HTTP + SSE interface a
human supervisor and an AI supervisor both use. It is **Layer 3** over the kernel facade
(`host`, Layer 2) and names no Tauri type. `tauri` is still *linked* because `host`
depends on it unconditionally — a known cost, not a reference.

This crate is the **skeleton**: two smoke endpoints, the event stream, and the
authentication hook. The remaining endpoints of the API table are later batches.

## Build

```bash
cargo build -p server            # debug
cargo build --release -p server  # release
```

The binary is `target/<profile>/riscdom-server` (`riscdom-server.exe` on Windows). It is a
separate binary from the CLI `riscdom`, which is a later line of work.

## Run

```bash
riscdom-server --bind 127.0.0.1:7821 --workspace ./my-workspace
```

| Argument | Default | Meaning |
|---|---|---|
| `--bind <addr>` | `127.0.0.1:7821`, or `$RISCDOM_BIND` | Address to listen on. |
| `--workspace <dir>` | the current directory | The workspace this host owns. Its audit chain lives at `<workspace>/.riscdom/audit.db`. |
| `--data-dir <dir>` | this platform's host data dir | Where settings and sessions live (the v0.8 injected-data-dir path). |
| `--heartbeat-ms <n>` | `15000` | SSE heartbeat period; `0` disables it. |
| `--auth` | on | Require the bearer token in `<data-dir>/token` (the default). |
| `--no-auth` | | Drop the requirement and print a warning: for local debugging. |

`--help` prints the same table. Exit codes: `0` clean stop, `1` workspace or bind failure,
`2` usage error. Stop it with `Ctrl+C`.

The loopback default is deliberate: this build serves **plaintext HTTP**, so it does not
listen on a public interface unless you tell it to.

## Endpoints

| Endpoint | Method | Answers |
|---|---|---|
| `/v0/health` | GET | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | The SSE event stream (`text/event-stream`, replayable with `Last-Event-ID`). |
| `/v0/runs`, `/v0/sessions`, `/v0/snapshots`, `/v0/llm/…`, `/v0/preflight`, `/v0/serial` | GET | The rest of the query surface: see the API document's §5.1. |
| `/v0/sessions/create`, `/v0/settings/theme`, … | POST | The 27 controls of §5.2: sessions, snapshots, VM, toolchain, preflight, LLM config, exports. |

Everything else answers `404` with the error model of the API document:

```json
{ "code": "not_found", "message": "no endpoint GET /v0/nope", "retryable": false, "cause": null }
```

```bash
curl -sS http://127.0.0.1:7821/v0/health
curl -sS http://127.0.0.1:7821/v0/status
```

## The event stream

```bash
curl -sS -N http://127.0.0.1:7821/v0/events
```

The first frame is always `hello`; after it, every frame the host emits arrives wrapped in
the common envelope. A frame uses `id:` and `data:` only and ends with a blank line —
deliberately no `event:` field, which would break a browser's single `onmessage` handler
(rationale in [docs/control-plane-events.md](../docs/control-plane-events.md) §1).

```text
id: 1790074876659-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"local-17480-1","task_id":null,"ts":1790074876659,"payload":{"buffer":{"from":0,"to":0},"filters":{"agent_id":null,"event":[],"task_id":null}}}

id: 1790074877033-1
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"local-17480-1","task_id":null,"ts":1790074877033,"payload":{"name":"write_source","arguments":{}}}

```

The heartbeat is a comment line every 15 seconds (or the period you set):

```text
: keep-alive

```

## Authentication

**The token is on by default.** On first start the server generates 32 random bytes into
`<data-dir>/token` (owner-readable only: mode `600` on Unix, an owner-only ACL on Windows —
if the platform cannot restrict it, the server refuses to start) and requires every request
to present it:

```bash
curl -sS http://127.0.0.1:7821/v0/health \
  -H "Authorization: Bearer $(cat /path/to/data-dir/token)"
```

The token is **never printed or logged**; the start-up line names the file, not the value.
An operator may provision the file instead of accepting a generated one. `--no-auth`
removes the requirement and prints a warning — the control endpoints include destructive
ones (delete a session, stop the VM, change the LLM configuration).

The hook is the `Authn` trait, so a distribution can install its own. Refusals map into the
error model: `401 unauthorized`, `403 forbidden`.

A distribution that exposes the control plane beyond the loopback interface is responsible
for transport security: the open-source build ships plaintext HTTP and the hook, nothing
more.

## Not implemented yet

- **Capability enforcement.** Each route declares its capability and the server hands it to
  the auth hook, but deciding whether a caller *holds* it is the permission intermediary's
  job, a later batch. Today the token is the whole gate.
- **`POST /v0/vm/start`** and **`GET /v0/resources`** answer `501`.
