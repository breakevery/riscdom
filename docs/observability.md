[中文](observability.zh-CN.md) | English

# Observability

**Status** v1.0 specification (M7g) ｜ **Date** 2026-09-29 ｜ **Audience** administrators and operators:
whoever runs a node and has to see what it is doing.

**What this document is.** [decisions §17](decisions.md) settles three things — **structured logs**,
**metrics** and a **tracing id** — and this document writes them down: what a node reports about itself,
in what shape, and where. It is the **contract** §17 asks for ("machine-readable by contract, not by
convention"); the code that produces it lands in later batches.

**Two sources, one identity.** Everything here describes what a node says about **its own work**. What a
*run* did is the audit chain's business ([control-plane-events.md](control-plane-events.md) is the
stream), and the difference is deliberate — see §5.

## 1. Structured logs

**One JSON object per line, on stderr.** The control plane already writes runtime lines to stderr through
one function (`server/src/log.rs`); under this contract that function emits an object instead of a
sentence.

| Field | Type | May be absent | Meaning |
|---|---|---|---|
| `ts` | number | no | Milliseconds since the Unix epoch — the unit every other timestamp in this project uses. |
| `level` | `"error"` \| `"info"` | no | The line's own level. `error` is a failure; `info` is the per-connection chatter. |
| `target` | string | no | Where the line came from — the module path, e.g. `riscdom_server::http`. |
| `message` | string | no | The line, for a human. |
| `agent_id` | string | yes | The agent this line is about, when it is about one (§4's identity). |
| `task_id` | string | yes | The dispatched task this line is about, when it is about one. |

- **The switch is unchanged.** `--log-level <off|error|info>`, defaulting to **`off`**, stays exactly
  what it is today, and for the same reason: the control plane also runs *inside* another program (the
  CLI's local mode), where stderr belongs to the caller — a line dropped into the middle of a caller's
  JSON error object is a line that breaks it.
- **The binary's banner is not a log line.** `riscdom-server`'s start-up banner, its usage text and its
  fatal errors are that program's console output — what the operator asked to see — and they stay human
  text. The embedded control plane never runs that code path at all (`server/src/main.rs`).
- **No secrets, ever.** The rule every JSON object in this project keeps: an API key, a token or a
  private key never appears in a line — not even truncated.

## 2. Metrics

**Prometheus text exposition, one endpoint.** A node that is asked for metrics answers with the
Prometheus text format (`text/plain; version=0.0.4`): `# HELP` and `# TYPE` for each family, one line per
sample.

**The route — specified here, implemented later.**

| Method | Path | Capability | Answer |
|---|---|---|---|
| `GET` | `/metrics` | **`status.read`** | Prometheus text exposition |

- **It reuses `status.read`; no new capability.** [decisions §83](decisions.md) is the rule: a route must
  declare a capability, and a capability no route requires is not a capability. Metrics *are* a node's own
  status — the same fact `/v0/status` answers for — so `status.read` is the honest declaration and the
  vocabulary does not move.
- **It is authenticated like everything else.** `/metrics` sits under the same `Authn` as `/v0/*`; the
  only unauthenticated surface is the Web UI's assets (`/`, `/assets/*`), which carry no secrets. An
  operator who wants to scrape it without a token can run a node with `--no-auth`, which is a deployment
  choice, not a second door.
- **Adding it is not free**, and that is why it is a later batch: `/metrics` is a new row in
  [control-plane-api.md](control-plane-api.md) §5's tables and a new row in the tool-schema tables, so it
  lands through the same gates every other route did (the counts and the marked tables are asserted by
  tests).

**The families, and where each number already lives.** The first batch is deliberately small — every
metric below is a fact the node can answer today from `/v0/status`, `/v0/audit/status` or its own store.
The prefix is `riscdom_`; a name says what it is, a label would only say it twice.

| Metric | Type | Source | Meaning |
|---|---|---|---|
| `riscdom_uptime_milliseconds` | gauge | `/v0/status` → `uptime_ms` | How long this node has been up. |
| `riscdom_connections` | gauge | `/v0/status` → `connections` | Open client connections right now. |
| `riscdom_sse_subscribers` | gauge | `/v0/status` → `sse_subscribers` | Clients following the event stream right now. |
| `riscdom_agents` | gauge | `/v0/status` → `agents` | The agents this host knows about. |
| `riscdom_audit_events` | gauge | `/v0/audit/status` → `count` | Rows in the audit chain. |
| `riscdom_audit_failures` | gauge | `/v0/audit/status` → `failures` | Audit writes still waiting to land. |
| `riscdom_info` | gauge = `1` | `/v0/status` → `version`, `agent_id` | Identity, as the conventional `*_info` metric: labels `version` and `agent_id`. |

- **No unbounded labels.** Nothing here is labelled by run, task or peer: a label whose value set grows
  with work done is how a metrics endpoint becomes a memory leak. If a per-task view is ever wanted, it
  is a query over the audit chain, not a metric.
- **A gauge, not a counter, where the source is a gauge.** These first families report *what is true now*;
  a counter is added when something is genuinely monotonic (the total number of audit rows written, say),
  and it is named `_total` when it is.

## 3. Tracing

**One identity, reused.** §17's why is the whole rule: the audit chain already carries an identity per
event, so **the tracing id is that pair** — `agent_id` and `task_id` — rather than a second namespace to
correlate.

- **`agent_id`** is `<device>-<pid>-<seq>` ([control-plane-api.md](control-plane-api.md) §2): the device
  name, the process, and a sequence. It says **which agent** did the work.
- **`task_id`** is `task-<pid>-<seq>`, or a caller's own name: it says **which unit of work** the agent was
  doing. The event envelope already carries both, and `null` is a legal `task_id` — work that belongs to no
  dispatched task.

**The gap, and how it is specified to close.** [roadmap §12](roadmap-v1.0.md) records it: **`POST
/v0/agent/run` takes no `task_id`**, so when several clients follow one node, an event frame cannot be
attributed to the task that produced it. The fix is small and additive, and it has a precedent already in
the API: `POST /v0/tasks` takes an optional **`id`** for exactly this purpose.

- **The specified change** (not implemented): `POST /v0/agent/run` gains an **optional `task_id`** in its
  body. When a caller names one, every frame and every log line that run produces carries it; when nobody
  does, `task_id` is `null`, which is today's behaviour — the field is additive, so nothing that exists
  today changes meaning.
- **Why not a new id.** A generated tracing id would be a second identifier for one unit of work, and the
  two would have to be correlated — which is the namespace §17 refuses. The caller already knows what it
  is asking for; letting it say so is the smallest change that closes the gap.

## 4. What is not covered

- **The audit chain is not observability.** A node's record of what happened is the chain
  ([audit/README.md](../audit/README.md)); observability is the *live* view — lines, numbers, an identity
  to follow. A metric is not evidence and a log line is not a record: neither is chained, neither is
  hashed, and neither can be handed to a verifier.
- **The event stream is the control plane, not a log.** `/v0/events` is a client-facing subscription with
  an envelope, a vocabulary and replay ([control-plane-events.md](control-plane-events.md)); the lines in
  §1 are the node's own stderr, for whoever is watching the process.
- **The start-up banner, the usage text and fatal errors** are the `riscdom-server` binary's console
  output, not structured logs (§1).
- **No tracing backend, no agent, no exporter.** This document freezes the *shape* a node reports in; where
  an operator sends it (a log shipper, a Prometheus scrape, a collector) is theirs. The node never dials
  out to a monitoring service — that would be the project operating a service, which [roadmap §1](roadmap-v1.0.md)'s
  red line forbids.
