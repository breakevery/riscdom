[中文](cross-device-dispatch.zh-CN.md) | English

# Cross-device dispatch

> **Status** v1.0 specification (M6-1) ｜ **Date** 2026-09-30 ｜ **Audience** kernel developers, and whoever
> deploys more than one node.

[roadmap §5](roadmap-v1.0.md) lists **five** things the kernel provides for layer three, and this document is
the third of them: *"a dispatch interface: hand a task to a node by name"*. It says what "by name" means, who
may hand what to whom, and what comes back. The other four — the node registry, capability descriptions,
cross-node event correlation and the pending-approval slot — are separate batches' business.

## 1. One route, one parameter

`POST /v0/tasks` already took `target`, `input`, `sandbox`, `instance` and `id`, and routed to **this node's**
executor fleet ([control-plane-api.md](control-plane-api.md) §5.2). It gains one more optional member:

```json
{ "target": "helper", "input": "…", "node": "dev-b" }
```

- **`node` names the node the task should run on.** Absent, or this node's own name, means the local fleet —
  byte for byte what the endpoint did before.
- **`target` is relative to that node.** `node: "dev-b", target: "helper"` means *the executor called `helper`
  on `dev-b`* — the label is a name in **that** node's `settings.executors`, not a copy of this node's.
- **No new route, and no new capability.** §5 calls the interface "hand a task to a node by name", so the
  name travels as a parameter and the route table does not move. The caller's own authority is unchanged:
  `/v0/tasks` is still `agent.run`, exactly as before.

## 2. What travels, and how

An ordinary **§3 frame between peers** ([connection.md](connection.md) §3) — the same mechanism §6.7's probes,
§33's reports and a closed segment already use. §14.6's rule is why there is nothing new: *"M-to-M
communication: reuse the cross-device protocol — no protocol of M's own."*

```json
{ "task": 1, "task_id": "task-dev-a-1-7", "target": "helper", "input": "…", "sandbox": null, "instance": null }
{ "task_reply": 1, "task_id": "task-dev-a-1-7", "agent_id": "dev-b", "outcome": { … } | "error": "…" }
```

- **One frame per task, one per reply.** There is nothing to batch: the task *is* the message, and the reply
  *is* the answer (v0.9 E0's "the answer is the outcome; there is no task table to poll").
- **`task_id` is `task-<device>-<pid>-<seq>`** ([agent/src/dispatch.rs](../agent/src/dispatch.rs)): it names the
  machine it was born on, so two nodes' ids cannot collide once they meet in one node's records. The id the
  caller sends is the id that comes back.
- **The reply is either an outcome or an error, never both.** `agent_id` names the executor that ran it —
  the **peer's** own identity, which the caller could not have known in advance.
- **The identity is the preamble's `from`.** The peer is authenticated by §3 like any other frame, so the
  sender is the node, not a person: a cross-device dispatch is a fact about two nodes.

## 3. Authorisation: default deny

The receiving node asks one question, of **its own** `peers.json`, about the sender's entry:

> does its `capabilities` list contain the word **`dispatch`**?

If not — including when the node has never heard of the sender — the task is **refused**. The word is an
ordinary claim string, the same shape as §6.7's `server` claim:

```json
{ "node_id": "dev-a", "address": "…", "public_key": { … }, "capabilities": ["dispatch"] }
```

- **It is not a control-plane capability name.** Declaring it grants nothing on that node's HTTP surface; the
  capability vocabulary (33 names) is unchanged, and the caller's `agent.run` is still what authorises asking.
- **Two axes, deliberately separate.** The **HTTP** axis says what a *credential* may ask a node to do. This
  axis says what a *peer node* may hand it. A task crosses the second only if it also passed the first.
- **The refusal is legible.** A peer that is not authorised is told so, and the caller sees
  `the peer did not authorise dispatch` rather than a timeout.

## 4. What the two sides do

**Sending.** The node sends one frame through the client it already has (the one the heartbeat and the digest
use) and waits for the reply: `REMOTE_DISPATCH_TIMEOUT` (30 s) of polling the shared reply slot the node's
reader fills, once per `REPLY_POLL` (100 ms). When the reply lands, the outcome is the task's outcome.

**Receiving.** The node's **probe thread** — the only reader a node has — recognises the frame, checks the
claim, runs the task **on its own fleet**, records `host.dispatch.received`, and answers. Failure to run, or a
`target` its fleet does not own, is answered rather than dropped: the sender always gets a reply or a reason.

**Four refusals, distinguishable by their message**, and none of them needs its own error variant:

| what happened | whose words | the message |
|---|---|---|
| the frame could not leave | the sender | `the peer is unreachable: …` |
| the peer did not authorise dispatch | the peer | `the peer did not authorise dispatch` |
| the peer's fleet failed | the peer | `the executor failed: …` |
| nothing came back in 30 s | the sender | `the peer did not answer in 30 s` |

## 5. The boundary: a node must be in a workgroup

**Cross-device dispatch presumes the node is in a workgroup.** Two things are needed and both come from one:

1. **A peer to reach.** There is nothing to dispatch *to* without one.
2. **A reader to hear the answer.** The reply arrives on the node's session, and the node's session is read by
   its probe thread. A node with no workgroup starts no prober, so a dispatch would be sent and never answered
   — the handle reports the timeout rather than pretending.

A caller that asks for a `node` it has no route to gets the unreachable refusal. Nothing else degrades.

## 6. What this is not

**[roadmap §5](roadmap-v1.0.md)'s red line: the kernel ships mechanism, and policy belongs to the caller.**
This document describes exactly that mechanism, and the absence of everything else is deliberate:

- **No scheduling.** No node *chooses* where a task goes. The caller names the node.
- **No decomposition, no dependency graph, no retry policy.** A frame goes out once; the answer, or the absence
  of one, is the caller's to judge.
- **No queue, and no task table.** A dispatch is synchronous, like the local one it stands beside.
- **No cross-node event correlation.** A task's events keeping one identity across devices is §5's *fourth*
  item and a later batch; today the id travels, and what a node does with it is its own business.
- **No project-side service.** Both nodes are the deployment's; the project ships software and a reference M,
  and runs neither ([decisions §33](decisions.md), §14.3).

**Frozen**: the `node` parameter; the two frame bodies; `task_id`'s shape; the `dispatch` claim and its default
deny; one frame each way and no batching; the answer's either/or; four legible refusals; the workgroup
boundary. **Not frozen**: whether a refusal earns its own HTTP status (today all four are `500` with a
distinguishable message), how a node learns *which* peers may dispatch without a hand-edited file, and
anything about M-to-M beyond "it is the cross-device protocol".

## 7. What the node a name points at can run

**[settled] A dispatch names a node; what that node can run is asked of the node itself (v1.0 M6-2a).**
§1 hands a task to a node by name, and the question that follows is "can that node run this?". On **this**
side the answer already exists, and one command now gathers it: `riscdom node capabilities` reads the five
things a person needs — the node's own `node_id`, the executors it can route to, the sandbox definitions it
knows, and whether QEMU and the RISC-V toolchain are ready here — and merges them into one answer.

**[settled] The merge is the CLI's, not a route.** There is no aggregate endpoint and this batch adds none:
`GET /v0/executors`, `/v0/sandboxes`, `/v0/qemu` and `/v0/toolchain` already answer, and the UI's node page
reads the same three of them. The CLI is a **pure HTTP client** — it never calls `AppState` — so it composes
them locally, and `node_id` is lifted out of `GET /v0/identity`. **No new route, no new capability, no SDK
change.** A section that fails is reported in place rather than taking the whole answer down, and a section
that answers `found: false` — no QEMU, no toolchain — is an **answer**, not a failure.

**[settled] It says what the node *is*, never where a task *should* go.** Which node a task should be sent to
stays the caller's (§6's first bullet, red line 1): the command hands the caller the facts, and the choice is
theirs. Nothing here schedules anything.

**For the other end, today, there is only a claim.** A peer's `peers.json` entry carries a `capabilities`
list, and the kernel reads exactly two words out of it (`server`, `dispatch`) — so it says what a node was
*configured to declare*, not what it can run now. As of **v1.0 M6-2b-1** that declaration is not only
readable but **shown**: `GET /v0/peers` answers with this node's own entries, `capabilities` and all, and
`riscdom node capabilities` prints them as a final **peer declarations** section — headed with what they are,
what each peer says about itself, a **claim rather than a fact** — so a caller can see who has declared what
without asking anyone. It is the local file's content (so it may include this node's own entry).

**And a node that *serves* its workgroup can see what its registrants said (v1.0 M6-2b-2).** Where the
paragraph above reads this node's own file, `GET /v0/online` answers with the **server role's runtime table**
— one row per node registered with it, `capabilities` and all. A node that runs no server role answers
`null`; a serving node nobody has registered with answers `[]`. Asking a peer what it can run, over the
wire, is still later; [connection.md §11](connection.md) records the two carried-but-unread channels that
touch the same question.

**Frozen**: the command's shape (`{node_id, executors, sandboxes, qemu, toolchain, peers}`), that it is the
CLI's own composition, and that it reports only. **Not frozen**: whether a peer's capability surface is ever
asked over the wire, and whether `/v0/peers` ever carries more than the declared claims.

## 8. The identity a task keeps

**[settled] A task has one identity, and it travels (v1.0 M6-1a).** The frame that crosses a device carries
the near node's `task_id`, and the peer's answer names it back (`TaskReply`), so the two records — the
sender's and the receiver's — name the same task. Nothing is re-minted on the far side: the id is the one
the near node gave it, which is what makes "this dispatching" and "that answer" one thing.

**[settled] And the events carry it (v1.0 M6-3a).** An event frame's `task_id` is no longer always `null`:
`POST /v0/agent/run` takes a `task_id` (a client following one node attributes what it sees to the task it
asked for), `POST /v0/tasks` binds the id it was given — or the one it minted — **before** it builds the
sink that carries the dispatch's events, and a task that runs under a node's own executor or in a `worker`
child publishes its events under the same id. So a task's story on **two** machines is readable as one:
the receipt on the receiver (`host.dispatch.received`), its own chain rows, and every stream frame in
between all name it.

**[settled] This is a mechanism, not a policy.** The id lets a reader *relate* events across devices; it
does not decide which node should run anything, and nothing here schedules ([§6](#6-what-this-is-not),
red line 1). An event that names no task still says `null`, which is what every frame did before this
batch.

**Frozen**: the envelope's `task_id` and its meaning; the frame fields that carry it; that a run's sink is
bound to the id at construction. **Not frozen**: whether the stream's documented `task_id` filter is ever
implemented (M6-3b — it is advertised in `hello` and not yet honoured), and whether a reply's own events
are correlated beyond their ids.

## 9. The pending-approval slot across devices (v1.0 M6-4a)

**[settled] An ask is a chain row, so it already travels.** §8 gives an event the task that caused it; the
pending-approval slot is the other half of a dispatch's story — a task that wants a sandbox switched
**asks**, and a person decides. An ask is an ordinary `m.request.ask` row on the asker's chain, so when a
stand-in's closed segment travels to the centre (M5-3c-2) and is merged, the ask is **transcribed with
everything else**. Nothing new crosses the wire for it: the transport half was already there.

**[settled] What was missing was the queue.** The live queue is seeded from the chain **once**, in the
constructor (`derive_requests_from` + `restore`, [decisions §84](decisions.md)), so a row a merge
transcribed stayed invisible until a restart. **M6-4a closes that**: a successful merge folds just the rows
it wrote into the queue through the same `restore`, so an ask made on another node appears at
`GET /v0/sandboxes/requests` on the centre — with its requester's identity and its sandbox name — without a
restart. The same one-row record the constructor writes says so (`host.sandbox_request.restore`).

**[settled] A collision is reported, never resolved.** Two nodes can mint the same `req-<pid>-<seq>`; the
row already queued is this node's own and the new one is **reported by id**, exactly as the constructor's
restore reports one. A **forked** merge transcribes nothing, so it folds in nothing: both sides stay where
they are and the conflict is recorded beside them.

**Not in this batch (M6-4b)**: the **decision** travelling back. An approval is an act on the decider's
chain (`m.request.approve`), and the asker's chain has no such row — so the asker's queue cannot see it.
Whether a decision reaches the asker as a row (a new reverse frame, or a segment in the other direction) is
M6-4b, and it is **not** a prerequisite for the propagation above.

**Frozen**: that an ask is a row and needs no new transport; that the refresh reuses `restore` and reports
collisions. **Not frozen**: how a decision travels back (M6-4b), and whether a remote decision ever clears
the asker's own queue.

## 10. A decision comes back (v1.0 M6-4b)

**[settled] The ask travels with the segment; the answer travels sideways.** §9 says a request made on a
stand-in arrives on the centre because it is an ordinary chain row. The **decision** goes the other way and
needs no segment: the centre answers **down the session the asker already holds** — the path §6.6's
registration acknowledgement and §6.2's address answer already use — so nothing is dialled and no route is
involved. One frame's body carries it:

    { "request_decision": 1, "request_id": "…", "decision": "approved" | "rejected",
      "decided_by": "<the node that decided>", "at_ms": … }

**[settled] The asker writes it as a local decision.** On the asker's chain the row is spelled
**`m.request.approve` / `m.request.reject`** — the same two actions a local decision uses, with the same
detail keys — and `decided_by` names the node that decided. That is what makes a remote decision **survive a
restart**: the queue is rebuilt from the chain (`derive_requests_from` folds exactly those actions), and a
second spelling would have needed a second reader.

**[settled] Nothing is deleted.** The decided row stays in the queue; the chain keeps the ask and the
decision. A node that is not dialled in hears nothing, like every other answer, and a decision about an ask
this node does not hold is **ignored** rather than fatal.

**Not here**: a workgroup-wide announcement of a decision — the centre answers the asker only.

**Frozen**: the body's shape and the `m.request.*` spelling of the row it becomes. **Not frozen**: whether a
decision is ever announced to the whole workgroup.

**Proved on the wire** (v1.0 batch DM): the asker's half is a real `RelayClient` dialled into a real in-network
server role — the centre decides an ask the asker left and the body is read off the asker's **own** session —
and a node deciding **its own** ask is checked against a spy dialled in under that node's own agent id, so
"nothing was sent" means the guard held rather than that there was nowhere to send.
