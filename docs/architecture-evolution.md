[中文](architecture-evolution.zh-CN.md) | English

> **This document is the v0.7.0 snapshot.** Decisions taken since then are recorded in
> [decisions.md](decisions.md).

# RiscDom Architecture Evolution: From a Single-Machine Sandbox to an AI Collaborative Runtime

Version: 1.0 ｜ Date: 2026-09-21 ｜ Nature: an architecture-evolution note, based on the v0.7.0 fact-finding

## 1. Background and goals

v0.7.0 has shipped. RiscDom today is "a trustworthy AI code-execution sandbox" — one AI working safely.

The target shape is "an AI collaborative-work runtime" — a group of AIs dividing the labour, with humans stepping in only at the key points.

This document answers: from the current architecture to the target shape, what layering is needed, which designs support the future, which ones have closed the road off, and how to walk it in stages.

## 2. Current architecture facts

Dependency graph (acyclic DAG):

```text
ui/src-tauri → host → { agent, sandbox, audit }
                agent → { sandbox, audit }
                sandbox → audit
                audit → (leaf)
```

Three key facts:

- Tauri coupling is minimal: AppState (state.rs:366) does not reference Tauri in its code. The coupling is concentrated in two files: commands.rs (52 commands, 66 references) and events.rs (8 references). The other 10 modules never touch Tauri.
- AppState is already per-instance: new() / in_memory() can be called many times, every field is per-instance, and the tests already use multiple instances.
- Two globals and two singletons stand in the way of multiple agents:
  - paths.rs:9 APP_DATA_DIR: OnceLock<PathBuf> — takes effect only once
  - state.rs:373 vm_slot: Arc<Mutex<Option<RiscVVirtualMachine>>> — one VM per AppState
  - audit: a single chain, a single Arc<Mutex<AuditStore>>, a single writer (sink.rs:59)
  - sandbox/relay.rs:34 HELD_PORTS process-level port registry

## 3. Target shape

From "being called" to "running continuously".

| Dimension | Current | Target |
|---|---|---|
| AI role | a single executor, passive | a supervisor + multiple executors, active |
| Working mode | one task at a time | continuous operation |
| Human role | operator | supervisor |
| Sandbox count | one | many |
| Device scope | a single machine | multiple devices (later) |

In stages: single-machine multi-agent → networked → remote management. The first stage introduces no distributed complexity.

## 4. Layered design

```text
Layer 3  Host            ui/src-tauri / a future server / CLI / management client
   ↓ depends only on Layer 2's stable API
Layer 2  Kernel facade   the host public API (the Tauri dependency isolated into Layer 3)
   ↓
Layer 1  Kernel capabilities  agent (the LLM loop), sandbox (the VM)
   ↓
Layer 0  Kernel foundation    audit (leaf)
```

This layering already exists in the dependency graph — it is simply unnamed. The work is to squeeze Layer 2's Tauri dependency into Layer 3.

## 5. The syscall layer: what the kernel provides

Principle: the kernel provides mechanisms, the distribution provides policies.

| Kernel provides (mechanism) | Distribution decides (policy) |
|---|---|
| start/stop a VM, run code, fetch results | when to start, what to run |
| appending to and verifying the audit chain | audit policy (who authorises, how long to keep) |
| creating/rolling back/listing snapshots | snapshot scheduling |
| producing and comparing fingerprints | what fingerprints are used for |
| the permission check check(capability) | permission policy |
| resource accounting | resource allocation |
| event emission (EventSink) | consuming the events |

EventSink (events.rs:29) is the part of the existing design closest to the right shape — the kernel emits, the host consumes.

## 6. The management API: a unified control plane

Core insight: the management side (a human managing AIs) and collaboration (AIs managing AIs) are one and the same set of interfaces.

```text
        Control plane (HTTP + WebSocket)
         /          |          \
  Human (phone/Web)  Supervisor AI  Executor AI
  supervise          assign tasks   take tasks
```

To an executor, "the command came from a supervisor AI" and "the command came from a human" are no different — both are authorised instructions from the control plane. The audit chain tells them apart by agent_id.

This is the architecture's core simplification. Otherwise two control channels get built, each evolving on its own, and they end up in conflict.

Protocol: HTTP (queries) + WebSocket (push)

Exposed capabilities: status queries, control (pause/resume/terminate), approvals, audit queries, resource views

Auth: for now only a hook is reserved; the mechanism is settled along with the v0.8 permission intermediary

Constraint: every kernel capability must have a corresponding management API. If the official management program does not use it, it is decoration.

## 7. Cross-sandbox and cross-device collaboration

Collaboration model:

| Role | Responsibility |
|---|---|
| Supervisor | decompose the goal, assign tasks, collect results, handle failures |
| Executor | do the work inside its own sandbox |
| Control plane | task routing, state sync, instruction delivery |
| Audit chain | record "who made whom do what" |

Two levels, not done at the same time:

- Cross-sandbox (same machine, v0.9): multiple processes (the B2 model), a shared workspace; the control plane runs over local IPC; the audit chain stays a single chain + agent_id
- Cross-device (after v1.0): multiple machines, network connections; the control plane runs over the network; the audit chain must evolve → audit v2

Four "seams to leave now":

1. A globally unique agent identity (agent_id carries a device id + a process id + a sequence number)
2. A task-dispatch abstraction (dispatch to an "executor handle"; local vs remote is transparent to the supervisor)
3. An audit chain that leaves room for multiple devices (today a single chain + agent_id; cross-device becomes one chain per device + an aggregate chain)
4. A device-independent control-plane protocol (local IPC and network HTTP carry the same semantics)

## 8. Settled decisions

**Decision 1: Tauri decoupling — start at A3, end at A1**

- Now (A3): host stays a single crate, the Tauri dependency stays in commands.rs / events.rs and does not grow
- End state (A1): split into host-core + host-tauri, done when the server host is built

**Decision 2: the multi-agent process model — B2 (multiple processes on one machine)**

- One process per agent, each with its own AppState; a shared workspace, each process with its own data dir

**Decision 3: the audit chain — a single chain + agent_id**

- The chain structure and the hash formula do not change; every event gains an agent_id; multiple processes write the same SQLite, relying on WAL + retry

**Decision 4: the three dead-ends are filed as technical debt due before v0.8**

1. APP_DATA_DIR: OnceLock → injectable
2. the single vm_slot → one VM per AppState
3. no agent_id in the audit chain → add the field

## 9. Schema evolution: making room for multiple programming languages

Today: toolchain is a single object. Target: toolchain becomes a map {c: {...}, rust: {...}}.

Path (designed now, implemented later):

- fingerprint_schema v1 → v2
- the v1 fingerprints of old runs stay as they are; new runs are always v2
- diff_fingerprints supports v1↔v2: compare by shared fields, mark added fields as null
- the FINGERPRINT_FIELDS ordering constant does not change

Invariants held: the chain structure, the hash formula and the historical rows all stay untouched.

When it is implemented: v1.1 (Rust → Zig → Python, added one at a time).

## 10. Reference-implementation completeness

A decision changes: spreading i18n out goes from "deliberately not doing" to "doing".

Reason: the reference implementation is the third party's template. If it is itself incomplete, a third party will use that as an excuse not to do it. The kernel may be rough; the reference implementation may not.

Scope: the 190 hard-coded UI strings are spread out into full bilingual coverage. A separate batch, not mixed with the architecture re-assessment.

## 11. Evolution path and milestones

| Stage | Deliverable | Demonstrable |
|---|---|---|
| v0.7.0 (done) | a trustworthy cross-platform sandbox | packages for three platforms, the golden path, the audit chain |
| v0.8 | technical-debt cleanup + a single-machine multi-agent prototype | one supervisor + several executor processes |
| v0.9 | multi-agent collaboration + the management API + the official management-program repository | AIs divide a composite task and finish it; visible on a phone |
| v1.0 | kernel API freeze | a stable syscall layer |
| after | networked / remote | multiple devices |

Each stage is independently deliverable and independently demonstrable.

## 12. The official management-program repository

- Position: an official kernel-level tool, advanced in step with kernel features
- Nature: a separate repository, maintained from the same source (like Linux's coreutils / iproute2)
- Constraint: every kernel capability must have a corresponding management API + management UI
- Timing: designed first, the repository created at v0.9

## 13. Risks and uncertainties

1. No reference point — "AIs working autonomously and continuously" is something nobody in the industry has truly solved
2. Distributed complexity is high — crossing machines is a jump of an order of magnitude, which is why it is left to the later stages
3. The timescale is long — infrastructure is measured in years
4. Resource needs — the target shape needs a team-scale effort

## 14. Explicitly not doing

- No over-design for an imagined future: leave seams, do not pre-implement
- Do not change the audit invariants
- Do not introduce distributed complexity
- Do not let the kernel bloat
