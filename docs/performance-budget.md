[中文](performance-budget.zh-CN.md) | English

# Performance budgets

**Status** v1.0 specification (M7h) ｜ **Date** 2026-09-29 ｜ **Audience** contributors, and whoever has to
decide whether a change is allowed to cost something.

**What this document is.** [decisions §18](decisions.md) settles four numbers and calls them **budgets**:

> **VM start ≤ 2 s; dispatch round trip ≤ 100 ms (same machine) / ≤ 500 ms (cross network); ten agents on one
> node ≤ 2 GB; log growth predictable.**

This document writes down what each number *means* — where it is measured **from** and **to**, in what
condition — so that "this change broke a budget" is a statement someone can check rather than a feeling. It
is a **specification**; the measuring harness is a later batch.

**A budget is not a measurement.** §18's own words: these are the order of magnitude at which the product
stops feeling like a local tool, and **a change that breaks one is a regression that has to be explained** —
not a test that fails. That asymmetry is deliberate and it shapes everything below: a budget is stated in
round numbers, measured with a named tool in a named condition, and **the number in this document is the
ceiling, not a promise of what any machine achieves**.

## 1. VM start (≤ 2 s)

**From:** the instant a node decides to start a guest — the call to the sandbox's `start`, after the port
leases are held and the QEMU command line is assembled (`sandbox/src/vm.rs::start`).
**To:** the moment the guest is usable — the start call returning `Ok`, which is after three handshakes have
completed in order:

1. **Serial connects.** QEMU blocks on the serial socket while `wait=on`, so the serial handshake completing
   is also the gate the guest boots behind.
2. **QMP connects and negotiates.** The QMP client consumes the greeting and negotiates capabilities.
3. **A restored guest is running.** *Only for a snapshot restore* (`-incoming`): the start waits until the
   restarted guest reports running (`wait_for_running`, bounded by the migrate timeout).

**What is measured is a cold start of a configured guest on a machine that has already run one** — QEMU on
the path, kernel and disk image warm in the page cache. A first-ever start on a cold cache, or a start that
is really restoring a large snapshot, is a different act and is **not** what the 2 s figure bounds (§6).

## 2. Dispatch round trip (≤ 100 ms same machine / ≤ 500 ms cross network)

**From:** the instant the control plane begins handling a request that carries a task
(`POST /v0/tasks`, `server/src/routes.rs` → `AppState::dispatch_task`).
**To:** the instant that request has an answer — the executor has produced a `TaskOutcome` and the control
plane has it in hand.

**The budget is on the control plane and the transport, not on the work.** A dispatch today is
**synchronous and in-process** (`LocalDispatcher` holds this node's own handle), and the executor runs the
whole agent turn — model call and all — inside that call. Model inference is unbounded and is not what any
of these numbers is about. The measured quantity is therefore **the dispatch hop**: the time to accept a
task, hand it to an executor and receive the executor's outcome envelope, **measured against an executor
whose work is trivially short** — a local executor answering without a model round trip. What is being
bounded is the overhead RiscDom adds, in the shape of an `.HTTP` request and an in-process call.

- **Same machine (≤ 100 ms).** Dispatcher and executor in one node: the control-plane hop alone, no network.
- **Cross network (≤ 500 ms).** Dispatcher and a **remote** executor on another node: the same hop, plus one
  network round trip. **The remote executor does not exist yet** — the dispatch interface reserves the place
  for it (`agent/src/dispatch.rs`: "the vector is where a second local agent — or a remote one — goes") — so
  this half is specified now and measured when that batch lands. The difference between the two numbers,
  **400 ms**, is the network allowance the design accepts between two nodes.

## 3. Memory (ten agents on one node ≤ 2 GB)

**What is counted:** **ten concurrently running agents on one node** — ten agent turns in flight, each with
its own guest. The measurement object is **the resident memory of the processes serving those agents**: the
node's own process plus the ten QEMU guest processes it started, summed as RSS. Guest RAM is the visible
part — an agent's guest is configured at the default `VM_MEMORY_MB = 128` MB (`agent/src/tools.rs`) →
**ten guests ≈ 1.25 GB** — and the rest of the ceiling is QEMU's per-process overhead, the page cache the
guests touch, and the node's own working set.

- **Not counted:** the node's baseline with zero agents idle is not the subject (it is small and belongs to
  no budget); a **single** agent's footprint is not the subject either — the budget is about the shape of the
  curve when ten run at once, because that is what "a node that scales by instances" has to answer for.
- **RSS, not virtual size.** Virtual size counts mappings the process never faults in; an RSS number is the
  memory a machine actually has to have. The budget is an RSS budget.

## 4. Log growth (predictable)

**Predictable means: growth is a stated function of what happens, with nothing that grows by time alone.**
The audit store is an **append-only SQLite database** (`audit/src/store.rs`): one row per event, and the
`BEFORE UPDATE` / `BEFORE DELETE` triggers are a hard guarantee that the log **cannot be rewritten, pruned
or shrunk through SQL — there is no rotation, no retention window and no `DELETE`**. The store grows
monotonically and never gives memory or disk back.

So the budget is a shape, and it is checkable:

- **One event, one row.** Rows are proportional to events, never to uptime or to a client count. A node that
  is up but idle grows by zero.
- **Bounded bytes per row.** A row's size is the event's canonical JSON plus its hash pair; it does not
  depend on how long the process has run. Bytes ≈ events × average row size.
- **No hidden amplification.** The structured log lines of [observability §1](observability.md) are the same
  story on stderr: one line per event, nothing buffered into unbounded memory. A "predictable" writer also
  means **no unbounded label or key** (observability §2's rule) — a table keyed by something that grows with
  work done is how both a log and a metrics endpoint stop being predictable.

**What is not promised:** the *rate*. Predictable is about the shape of the curve, not a ceiling on events
per second — the operator controls the rate by controlling what the node is asked to do.

## 5. How a budget is checked

Each budget names its tool and its condition, so a check is reproducible:

| Budget | Measured with | Condition |
|---|---|---|
| VM start | a timestamp around `start` in the sandbox, or the `vm.started` moment on the event stream | a configured guest, warm cache (§1) |
| Dispatch round trip | a timestamp around the control-plane hop (§2) | a local executor with trivially short work; the remote case after the remote executor lands |
| Memory | the node's own process + its QEMU children, summed RSS | ten agents running at once |
| Log growth | the audit store's row count and file size ([observability §2](observability.md)) | before and after a known number of events |

- **What observability already provides.** [observability.md](observability.md) §2 specifies `/metrics`
  precisely so that a budget check has a source: `riscdom_audit_events` and the node's identity for the log
  side, the status gauges for connections and agents. **The metrics are not the whole harness** — there is no
  memory metric and no timing metric in §17's first family, so memory and the round trip are timed directly
  for now. Budgets and observability meet at the audit count and the status gauges, not everywhere.
- **The harness is a later batch.** This document fixes the definitions; the tool that measures them,
  records them and compares a run against the budget lands on its own, after the specs, like every other M7
  item.

## 6. What is not covered

- **No tail, no peak.** Every number here bounds a typical, reproducible act (§5's conditions), not a
  worst case. A p99, a peak under load, a cold-cache first start — none of them is what §18 budgets, and
  none of them is checked here. The budgets are the order of magnitude a user feels, not a service-level
  objective.
- **No absolute machine claim.** "≤ 2 s" is a ceiling for the class of machine the product targets, met on a
  warm host under §5's conditions — not a statement about every host that can run the node.
- **Nothing is enforced in code.** No test fails because a budget was missed; the point of §18 is that the
  *reviewer* asks, and the change explains itself. Turning a budget into a gate would need its own decision.
- **The measuring harness, the remote executor, and any memory metric** are later batches; this document is
  the contract they will be written against.
