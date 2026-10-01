[中文](capabilities.zh-CN.md) | English

# RiscDom — what actually runs today

> **The rule for this page.** It lists only functions that are **on disk and exercised** at
> `v1.0.0` — anything the gate or a real run touches. A design that is merely written down,
> or a roadmap item marked `[default]` / `[open]`, is **not** here. When in doubt the item
> was left out. Every entry names **how to invoke it**, so a reviewer can check it rather
> than trust it.

Commands assume a control plane is running and that `riscdom` is pointed at it, e.g.
`riscdom --remote 127.0.0.1:7821 <command>`. The HTTP column is the endpoint the command
uses; the full table is [control-plane-api.md](../control-plane-api.md) §5.

## M2 — one machine, several sandboxes

| Function | How to invoke | HTTP |
|---|---|---|
| **Run an agent task** — model writes code, compiles it, boots it, reads serial back | `riscdom run "<task>"` | `POST /v0/agent/run` |
| **Sandbox registry** — hand-written, scanned, and built-in definitions | `riscdom sandboxes list` / `current` / `candidates` / `show <name>` | `GET /v0/sandboxes…` |
| **Switch the active definition** (stops and starts the VM) | `riscdom sandboxes switch <name>` | `POST /v0/sandboxes/switch` |
| **Executor routing** — which executors a task can go to | `riscdom executors list` | `GET /v0/executors` |
| **VM lifecycle** — start / stop the guest by hand | `riscdom vm start` / `vm stop` | `POST /v0/vm/{start,stop}` |
| **Snapshots** — save / resume / delete a VM snapshot | `riscdom snapshots list` / `save <name>` / `resume <name>` / `delete <name>` | `GET /v0/snapshots`, `POST /v0/snapshots/{save,resume,delete}` |
| **Sessions** — persisted conversations, list / open / rename / delete | `riscdom sessions create <title>` / `open` / `rename` / `delete` / `clear` | `POST /v0/sessions/*` |
| **Run index** — what ran, newest first | `riscdom runs list [--limit n]` / `runs get <id>` | `GET /v0/runs…` |
| **Model configuration** (BYOK) | `riscdom llm set …` / `llm clear` / `llm load-key` | `POST /v0/llm/config…` |
| **Toolchain / QEMU** — discover, point at, download | `riscdom qemu path/clear/download/status`, `riscdom toolchain download/path/clear` | `POST /v0/qemu/*`, `POST /v0/toolchain/*` |
| **Environment preflight** — run and acknowledge the step list | `riscdom preflight run` / `preflight ack` | `POST /v0/preflight/*` |
| **Workspace import / export** — move a project in and out | `riscdom workspace import` / `workspace export` | `POST /v0/workspace/{import,export}` |
| **Status** — connections, subscribers, agents, `agent_id` | `riscdom health` / `status` / `agents` | `GET /v0/health`, `GET /v0/status` |

## M4 — connection (workgroup and relay)

| Function | How to invoke | HTTP |
|---|---|---|
| **Node identity** — public Ed25519 identity, when configured | `riscdom identity` | `GET /v0/identity` |
| **Peers** — who this node knows | `riscdom peers` | `GET /v0/peers` |
| **Rooms** — the rooms `rooms.json` defines | `riscdom rooms` | `GET /v0/rooms` |
| **Connection state** — configured / connected / problem | `riscdom connection` | `GET /v0/connection` |
| **Node capabilities** — executors + sandboxes + QEMU/toolchain readiness + peers' claims, in one answer | `riscdom node capabilities` | merge of the queries above |
| **Signing & transport** — one signed JSON line over TCP, direct then relay | *(library, `net`; self-tested by the gate)* | — |
| **Relay server** — route signed frames between nodes | `riscdom-relay` binary, or a node with `settings.network.server_role` | — |
| **In-network server** — a node serving its workgroup, with registration + 15 s heartbeats and an online table | `settings.network.server_role` | — |
| **Liveness** — probes workgroup peers; the server judges by unanimity | *(runs on the node's own thread)* | — |

Rooms, membership and joining are **configuration**; there is no join protocol in v1.0.

## M5 — audit v2

| Function | How to invoke | HTTP |
|---|---|---|
| **Chain status** — event count and the chain's verdict | `riscdom audit status` | `GET /v0/audit/status` |
| **Event list** — newest first, filterable by action prefix | `riscdom audit events [--limit n] [--action-prefix <p>]` | `GET /v0/audit/events` |
| **Export** — the whole chain, or one run's slice, as JSONL | `riscdom export audit-jsonl --out <f>` / `export run-audit <run_id> --out <f>` | `POST /v0/audit/export`, `POST /v0/runs/export` |
| **Independent verification** — check the chain outside the running app | `audit-verify <db> --runs` → `Intact { … }` / `Broken { … }` | *(local binary)* |
| **Alert threshold** — set the audit alert | `riscdom audit alert set …` | `POST /v0/audit/alert` |
| **Conflict record** — record that a fork was looked at (names no side) | `riscdom audit resolve <segment_id> [--note <t>]` | `POST /v0/audit/conflicts/{id}/resolve` |

The event rows are append-only: the triggers refuse `UPDATE`/`DELETE`. This is the property
[demo.md](demo.md) step 5 lets a reviewer break **on a copy**, and watch the checker call it
`Broken`.

## M6 — dispatch across devices

| Function | How to invoke | HTTP |
|---|---|---|
| **Dispatch a task to another node** | `riscdom tasks dispatch --target <node_id> --input "<task>"` | `POST /v0/tasks` |
| **Task identity continuity** — the `task_id` rides with the task across the node boundary | *(part of the dispatch envelope)* | — |
| **Cross-chain verification** — a delivered segment is checked against its own end frame and the anchor link | *(part of the audit flow)* | — |

The **cross-region level of M** (a list of LAN Ms, a `--config` file) is **not** in v1.0 —
see [known-issues.md](known-issues.md).

## M7 — the ecosystem

| Function | How to invoke |
|---|---|
| **Rust SDK** — a typed client of the control plane | `sdk/rust` (`riscdom-sdk`) |
| **TypeScript SDK** — endpoint tables, client and stream | `sdk/typescript` |
| **Reference supervisor and dispatcher** (Python) | `examples/python/supervisor.py`, `examples/python/dispatch.py` |
| **Backup tool** — export a node's state as one encrypted package | `riscdom-backup` |
| **Budgets** — the documented performance ceilings | [performance-budget.md](../performance-budget.md) |

## M8 — the freeze and the release

| Function | Where |
|---|---|
| **API freeze declared** — the three frozen surfaces | [api-compatibility.md](../api-compatibility.md) §1 |
| **`/v0/` is the v1.0 path prefix** — it does not move at v1.0 | [api-compatibility.md](../api-compatibility.md) §5 |
| **v1.0.0 released** — version, tag, release notes | tag `891c237`; [RELEASE_NOTES.md](../../RELEASE_NOTES.md) |
| **The control plane is its own repository** | `riscdom-server` (M8-4a) |

## Not listed here, on purpose

- Anything marked `[default]` or `[open]` in [roadmap-v1.0.md](../roadmap-v1.0.md) — e.g.
  Python as a guest language, the session DB's WAL mode, the GUI switch's click target.
- The **M8-4b / M8-4c / M8-4d** split work (the desktop repo, this repository's close-out,
  reconciliation) — designed, not done.
- Observability items that are still specification rather than code — see
  [known-issues.md](known-issues.md).
