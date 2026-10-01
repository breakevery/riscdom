[中文](architecture.zh-CN.md) | English

# RiscDom — the architecture in thirty minutes

> A short read for a reviewer. The long version is
> [architecture-evolution.md](../architecture-evolution.md); this page does not rewrite it,
> it points at it. Everything below is about the `v1.0.0` tag.

## 1. What it is

RiscDom is a **local, single-operator runtime for an AI agent that works in a sandbox**. A
human asks for something; a language model writes code; that code is compiled for **RISC-V
bare-metal** and booted under **QEMU**; the serial output comes back; and every meaningful
act is written to a **local, append-only, hash-chained audit log**. The same node can be
driven from a desktop app, from a command line, or from a phone browser on the LAN — and,
new in v1.0, one node can **dispatch a task to another node** across a network.

Two invariants run through everything:

- **The kernel provides mechanisms, never policy.** It records what happened and owns no
  judgement about what *should* have happened.
- **The audit log is append-only and local.** It is never uploaded; its chain is verifiable
  on the machine that holds it.

## 2. The pieces

Ten crates in one Cargo workspace, plus the front end:

| Crate / dir | What it is | Trusted for |
|---|---|---|
| `host-core` | The **portable host**: application state, settings, sessions, dispatch, the task/sandbox registry, workspace import/export, toolchain and QEMU download. No Tauri in it. | — |
| `host-tauri` | The **desktop shell**: Tauri commands and events, a thin facade over `host-core`. | — |
| `agent` | The **agent loop**: the LLM client, the tool set, the capability policy, the compiler wrapper. | decision-making |
| `sandbox` | The **QEMU RISC-V sandbox**: process start/stop, QMP control, serial capture, snapshots. | the guest boundary |
| `audit` | The **append-only store**: SQLite plus a hash chain, with the `audit-verify` and `audit-rebuild` checkers. | the evidence |
| `net` | The **connection layer**: node identity (Ed25519), signing, transport, discovery, rooms, the relay server and client. | network identity |
| `worker` | The **executor process** and its supervisor half (the two-process prototype). | — |
| `cli` | `riscdom`, the **command-line client** of the control plane. Since v1.0 it is a pure client: `--remote` is required and it starts nothing. | — |
| `server` | The **control plane**: HTTP + SSE over `host-core`. *(extracted to the `riscdom-server` repository in v1.0 M8-4a; still a workspace member here until M8-4c)* | the API surface |
| `backup` (`riscdom-backup`) | **Portability**: export a node's state as one encrypted package (M7e). | — |
| `sdk/rust` (`riscdom-sdk`) | A **typed Rust client** of the control plane (M7c); the TypeScript client lives at `sdk/typescript`. | — |
| `ui/` | The **React front end** and the `ui/src-tauri` Tauri shell. It never touches a Rust crate directly; it speaks HTTP. | — |

Dependency direction is one-way and acyclic:
`ui/src-tauri → host-tauri → host-core → {agent, sandbox, audit, net}` and
`agent → sandbox → audit`, `net → audit`.

## 3. One task's path

This is the single flow a reviewer needs. It is what the demo in
[demo.md](demo.md) exercises end to end.

```
human
  │  riscdom --remote 127.0.0.1:7821 run "write a RISC-V hello, compile, run, read serial"
  ▼
cli  ──HTTP POST /v0/agent/run──▶  server (control plane)
                                      │  in-process call
                                      ▼
                                  host-core  (AppState::run_agent)
                                      │
                                      ▼
                                  agent loop ──HTTPS──▶ LLM provider (BYOK)
                                      │  tool calls
                    ┌─────────────────┼──────────────────┐
                    ▼                 ▼                  ▼
              compile (RISC-V    sandbox: QEMU       read serial
              bare-metal GCC)    -machine virt        (TCP/file)
                                 -bios none
                                 -kernel <elf>
                    │                 │                  │
                    └───────────────►─┴──────────────────┘
                                      │  every act
                                      ▼
                                  audit: append + hash chain (SQLite, local)
                                      │  events
                                      ▼
                                  server ──SSE /v0/events──▶ cli / desktop / phone
```

In words:

1. **Ask.** The CLI (or desktop, or browser) sends the request to the control plane over
   HTTP. The control plane is a plain HTTP/1.1 server over the host:
   [control-plane-api.md](../control-plane-api.md).
2. **Decide.** `host-core` runs the agent loop in `agent`. The loop talks to a
   **bring-your-own-key** model provider over HTTPS. The API key never touches this
   project's servers — there are none; it stays in memory or the OS keyring.
3. **Build.** When the model asks to compile, the compiler wrapper invokes a RISC-V
   bare-metal GCC and produces an ELF. The agent only ever writes `int main(void)`; `crt0`
   is injected.
4. **Run.** `sandbox` starts `qemu-system-riscv64` (`-machine virt -cpu rv64 -bios none
   -kernel <elf>`), controls it over QMP (TCP on Windows), and captures the serial output.
5. **Record.** Every meaningful act becomes an **audit event**: appended to a SQLite store
   and linked into a **hash chain**. The chain is append-only — database triggers refuse
   `UPDATE` and `DELETE` on the event rows.
6. **Observe.** The control plane streams events to clients over **SSE**; the desktop app
   and the read-only LAN board both consume that stream.

## 4. Crossing a device

v1.0 adds a third layer: a task can run on **another node**. The pieces are in `net`:

- **Identity.** Each node has an Ed25519 key pair (`node.key`, or the OS keyring), exported
  as one JWK. A node's `node_id` is derived from its public key.
- **Signing.** A message is signed over the canonical JSON of `{v, from, to, ts, body}` and
  verified in a fixed six-step order. A signature authenticates *where* the capability
  model authorises; it does not grant power by itself.
- **Transport.** One signed JSON line per message over TCP — **direct first, through the
  relay otherwise**, byte-identical on both paths. Replay is bounded by a per-peer
  high-water mark.
- **Room and discovery.** `peers.json` and `rooms.json` describe who a node knows and which
  rooms it is in; discovery is a default-deny filter over membership.
- **The server.** `riscdom-relay` (and an in-network server a node can host) routes on the
  signed `to` alone and never dials a node; the server also takes registrations, heartbeats
  and liveness reports, and judges by unanimity among the witnesses that remain.

When a task is dispatched, `POST /v0/tasks` names a **target node**; the task keeps the
`task_id` the near node gave it as it crosses, so the two nodes' audit rows can be lined up
(see [cross-device-dispatch.md](../cross-device-dispatch.md) and
[cross-chain-verification.md](../cross-chain-verification.md)).

## 5. Where the truth is

- The long architecture record: [architecture-evolution.md](../architecture-evolution.md).
- The control plane's protocol: [control-plane-api.md](../control-plane-api.md).
- The connection layer's specification: [connection.md](../connection.md).
- The audit chain's design: [audit-v2.md](../audit-v2.md) and
  [cross-chain-verification.md](../cross-chain-verification.md).
- The security model: [security-model.md](../security-model.md).

*This page is a summary written for the audit package. The documents above are the
authority; where this page and one of them disagree, they are right.*
