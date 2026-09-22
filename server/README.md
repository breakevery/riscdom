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

`--help` prints the same table. Exit codes: `0` clean stop, `1` workspace or bind failure,
`2` usage error. Stop it with `Ctrl+C`.

The loopback default is deliberate: this build serves **plaintext HTTP**, so it does not
listen on a public interface unless you tell it to.

## Endpoints

| Endpoint | Method | Answers |
|---|---|---|
| `/v0/health` | GET | `{"status":"ok","version":"0.8.0","uptime_ms":N}` |
| `/v0/status` | GET | `{"status","version","uptime_ms","connections","sse_subscribers","agents","agent_id"}` |
| `/v0/events` | GET | The SSE event stream (`text/event-stream`). |

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

Requests carry `Authorization: Bearer <token>`. The hook is the `Authn` trait; the default
in v0.9 is `NoAuth`, which authorises every request as an anonymous actor and ignores the
token. The token is read into the request metadata and **never logged** (its `Debug`
redacts it). Refusals map into the error model: `401 unauthorized`, `403 forbidden`.

A distribution that exposes the control plane beyond the loopback interface is responsible
for transport security: the open-source build ships plaintext HTTP and the hook, nothing
more.

## Not implemented yet

- **`gap` frames and `Last-Event-ID` replay.** A subscriber that falls behind loses the
  frames it missed; the stream does not say so. The design reserves the `gap` kind and the
  `id` cursor for it — see [docs/control-plane-events.md](../docs/control-plane-events.md) §2.
- **The rest of the API table.** The 53 commands become 53 endpoints in later batches.
- **Payload normalisation.** Events are wrapped exactly as the host emits them; the
  unified v1 payload shapes are a later change.
