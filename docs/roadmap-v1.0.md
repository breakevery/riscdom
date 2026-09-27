[中文](roadmap-v1.0.zh-CN.md) | English

# RiscDom v1.0 Roadmap

**Version** 1.0 draft ｜ **Date** 2026-09-27 ｜ **Baseline** v0.9.9 (`3365970`) ｜ **Nature** the kernel
API freezes, and the three layers are delivered.

**Audience**: kernel developers — with §6 (the freeze level) and §8 (the sandbox plugin interface)
written to be normative for **distribution integrators** as well.

**How to read a decision.** Every decision below carries one of three tags. **[settled]** — the
discussion concluded and the shape will not move. **[default]** — settled for now and deliberately
changeable; all fourteen are collected in §14. **[open]** — deliberately left alone until the milestone
that needs it. A decision already recorded in [decisions.md](decisions.md) is **cited by number and not
re-decided here**: this document adds the shape the discussion reached on top of it.

**What this document is not.** It is a plan — not a date, not a specification, and not a promise that
every line of it will be built. Where a line says a mechanism exists, it is in the kernel or in a
decision already written. It is written down now because a discussion that took several conversations to
converge should not have to be held again: a plan that lives only in a conversation is a plan that is
lost.

## 1. Positioning and boundaries

**In one sentence.** v1.0 is where the kernel graduates: the **three layers work**, and the **kernel API
stops moving**.

**The analogy that keeps the shape straight.** The pieces of v1.0 are the pieces of an operating system,
and naming them that way is what makes "what belongs in the kernel" answerable:

| An operating system | RiscDom |
|---|---|
| a process | a sandbox **instance** — `sandbox` starts, stops and snapshots it |
| the syscall log | the **audit chain** — append-only, one event per act |
| the scheduler | **M**, the AI dispatcher §9 describes |
| distributed IPC | **cross-device** — `@`, rooms, the relay (§4) |

**Relationship to multi-agent: a capability line, not a release.** **[settled]** v0.8 and v0.9 delivered
the multi-agent **interface** — a roster, a dispatch endpoint, a remote handle, a sandbox request queue —
and the v0.9.9 release text says so in as many words ("the interface, not a strategy"). v1.0 delivers the
**capability**: several AIs, on several devices, actually dividing work. Read that way, "multi-agent" is
not the thing v1.0 adds; it is the line several releases advance, and v1.0 is the point at which it
becomes usable.

**The four red lines.** These are constraints, not goals; each has to survive every milestone below.

- **[settled]** **No built-in supervisor.** The kernel provides mechanism, never policy (decisions §1). A
  kernel that shipped an AI supervisor would have fixed a choice that belongs to the caller — and M (§9)
  is the proof it does not need to: M is a *caller* of the control plane.
- **[settled]** **No officially operated service.** Nothing in this repository runs a service on
  somebody's behalf; a deployer runs a node, an in-network server or a cross-region server (decisions
  §1).
- **[settled]** **Not a general-purpose sandbox.** RiscDom is "a pluggable secure-sandbox runtime with
  RISC-V at its core": RISC-V is the primary tone, the substrate and the default implementation, not the
  only one (decisions §2).
- **[settled]** **The audit invariants do not move.** The chain's guarantees are the product; audit v2
  (§7) extends the *semantics* to "main chain + temporary segments" and adds metadata at a segment's head
  **without changing the hash formula** (decisions §33). Anything that would change the formula is not a
  v1.0 item; it is a different project.

## 2. The three layers

| Layer | What it is | What it needs |
|---|---|---|
| **L1 — one device** | several sandboxes, several API keys, several AIs on one machine | the instance API (§3), M (§9) |
| **L2 — connection** | a workgroup on one LAN, and a cross-region server for the public internet | node identity, signing and rooms (§4) |
| **L3 — cross-device dispatch** | several AIs on several devices dispatching to one another | L1 **and** L2, plus audit v2 (§5) |

- **[settled]** L1 and L2 touch different parts of the kernel and may proceed **in parallel**.
- **[settled]** L3 depends on both, and its audit half depends on audit v2: a dispatch that crosses
  devices has to be verifiable in a chain that spans them.

## 3. Layer one: several sandboxes on one device

- **[settled]** **Definition versus instance.** A sandbox *definition* (what to run: image, resources,
  language) and a sandbox *instance* (one running thing) are two names with two lifetimes. A-2 and C-2
  are the **same definition** instantiated twice — which is exactly what the pair of names exists to
  express. decisions §31 already separates `default_sandbox` (configuration) from `current_sandbox`
  (runtime state); v1.0 adds the many-instance half.
- **[settled]** **Bare metal is not reused.** An instance gets its own VM: one serial port, one VM, one
  instance. The "one VM at a time per agent" the release notes record is what this replaces, and it is
  replaced by an explicit instance table, not by sharing.
- **[settled]** **M is an AI dispatcher, and it stays outside the kernel.** M has three levels — node,
  LAN, cross-region (§9) — and it drives the kernel through the **control plane**, the same interface a
  person uses (decisions §6). M never enters the kernel: no compiled-in supervision, no privileged back
  door.
- **[settled]** **An executor asks; M decides.** An executor that wants a sandbox change goes through
  the **pending-approval slot** — the request surface decisions §36 already defines (`POST
  /v0/sandboxes/requests` needs only `agent.run`; deciding needs `sandbox.read` plus the capability the
  request's action implies).
- **[settled]** **The five APIs the kernel gives M**: create an instance (`POST /v0/sandboxes/instances`),
  delete one (`DELETE /v0/sandboxes/instances/{id}`), list them (`GET /v0/sandboxes/instances`), ask what
  the node can do (`GET /v0/capabilities`), and the `Task.instance` field, so a dispatched task names the
  instance it runs on.
- **[settled]** **Several LLM configurations.** Today one provider configuration is one value; several
  AIs on one node need several. The setting becomes a **map** (by role or by name), and that is a
  `settings.json` schema change — **v1 → v2** — so it travels with the migration mechanism of §6.
- **[settled]** **New capabilities.** `sandbox.instantiate` (create an instance), `sandbox.mux` (act on
  more than one) and `executor.register` (join a node's roster as an executor) join the vocabulary, which
  already numbers `sandbox.read` 29th and `sandbox.assemble` 31st (decisions §31, §36).
- **[settled]** **M's audit events**: `m.sandbox.spawn`, `m.sandbox.reap` and `m.request.approve`. What M
  did is in the chain, attributed to M, next to what the executor asked for — the after-the-fact audit
  decisions §33 already requires of an AI-initiated authority.
- **[settled]** **A broken API key is a configuration change, not a new entity.** When M's key stops
  working, the fix is to change the key, the base URL or the model; M's loop restarts (§9) and the *same*
  M continues. Identity is not the credential.

## 4. Layer two: connection

**The model: a workgroup on a LAN, and a cross-region server for the public internet.** **[settled]**
Two shapes, not three. Earlier discussion carried three topologies; the convergence is what this section
records, and it converges *towards* decisions §7, not away from it.

**Inside one network — the workgroup.**

- **[settled]** A workgroup is the nodes on one LAN **plus an in-network server** (agreement **B**: the
  in-network server exists; this is not a flat peer-only network).
- **[default]** **Discovery.** The in-network server hands down a **static node table**; UDP broadcast is
  the supplement, for nodes that appear between hand-downs (decisions §7 already names UDP broadcast with
  room isolation as the discovery mechanism). See §14.14.
- **[settled]** Node identity is **Ed25519** — a public and a private key, kept as `<data-dir>/node.key`
  or in the keyring (decisions §13: a standard format, mode 600 by default, the keyring optional).
- **[settled]** **Signing.** `@` means address *and* signature (decisions §7): an `@`-addressed message is
  signed by the key it names.
- **[default]** **Rooms.** A room is a membership list plus its rules — rate, who may `@` whom, whether a
  signature is required — and its landing place is `rooms.json`. See §14.5.
- **[settled]** **Bulk credential import** is `peers.json`: `{node_id, addresses[], public_key,
  capabilities, rooms[]}` per entry, the shape decisions §13 already fixed.

**Across networks — the cross-region server.**

- **[settled]** It is a **dedicated** server: a new server, not an in-network server moonlighting as a
  bridge.
- **[settled]** Its four roles: **signalling** (who is where), **relay** (carry the messages two nodes
  cannot carry themselves), **management** (the registry and the room definitions) and **audit
  aggregation** (collect the chain's digests — see the bullet below).
- **[settled]** **No self-built hole punching.** RiscDom does not implement its own NAT traversal. Two
  nodes try a direct connection first; if it fails, the message goes through the relay — and **the relay
  is the main path**, not the exception. This is what decisions §7 means by "the public internet is
  treated as one large LAN": a stateless bridge both sides dial out to, which removes NAT and dynamic
  addressing without giving the bridge authority over anybody's data.
- **[settled]** **A direct connection leaves the data path.** Once a pair is talking directly, the relay
  carries no data for them; what stays is the management plane — the registry and the room definitions.
- **[settled]** **Audit travels on a schedule.** Digests are batched on a **30-second** timer; a **key
  event** (an ejection, a fork, a temporary centre's takeover) is pushed the moment it happens.
- **[settled]** **Deployment**: one machine to start with, several in v1.x. Multi-server redundancy is a
  commercialisation-layer item and is not part of this model (decisions §33).

**What the layers must not contradict** — all four already written down:

- **[settled]** **Logs are fully replicated.** Every node holds the whole history and cross-validates it —
  the group-chat model (decisions §7). The aggregation role above collects *digests*; it does not become
  the only place the history lives.
- **[settled]** **The centre is a special node**: the same kernel, differentiated by deployment — the
  cell-differentiation model (decisions §33).
- **[settled]** **Three suppression layers**: a waiting period (30 s – 2 min of silent retries), then
  global confirmation (most nodes must report the centre unreachable before anything fires), then backoff
  plus precedence (decisions §33).
- **[settled]** **The temporary centre.** A bridge machine can stand in; the events it writes carry
  `provisional: true`; when the centre returns, a conflict-free run folds in, a conflicting one keeps
  both sides marked `fork` — never a silent merge (decisions §33).

**The convergence, in one line.** **[settled]** "Centralised or decentralised" is not a choice between two
networks: it is **one model in two states** — a direct connection succeeded, or traffic goes through the
centre — and which state is in force is invisible to the user.

## 5. Layer three: dispatch across devices

**Three levels of M cooperate.** **[settled]** A cross-region M at the top, a LAN M under it, a node M
under that, and the executors at the leaves. Each level sees the level below it through the same interface
it sees a person through (decisions §6).

**What the kernel provides** — and this list is the whole of it:

- **[settled]** the **node registry**: which nodes exist, and how to reach them;
- **[settled]** **capability descriptions**: what a node can run (decisions §2: content is not constrained
  by architecture, so this is a question about the node, not about the guest);
- **[settled]** a **dispatch interface**: hand a task to a node by name;
- **[settled]** **cross-node event correlation**: a task's events keep one identity when the task crosses a
  device;
- **[settled]** the **pending-approval slot**: the request surface of decisions §36, which is what makes
  "ask before acting" possible across devices.

**What the kernel does not provide** — deliberately, and this is red line 1:

- **[settled]** no workflow engine, no dependency graph, no retry policy, no scheduling algorithm. Who
  divides a composite task and how the parts are shared out is **policy**, and policy belongs to the caller
  (§9). A kernel that shipped a scheduler would have made every deployment obey one.

- **[default]** **M talks to M over the cross-device protocol.** Two Ms are two control-plane clients; they
  need no protocol of their own. See §14.6.
- **[settled]** **Cross-chain audit verification depends on audit v2** (§7): without a verifiable way to
  relate two devices' chains, "this task happened" is a claim, not a fact.

## 6. The freeze level: six things that must be on disk

The API may not be declared frozen before these six are written down. Each is a document rather than a
promise, and each has a decision behind it already.

| # | What | What has to exist | Decision |
|---|---|---|---|
| 1 | **[settled]** API stability policy | what may change in a minor release, what needs a major one, and how a deprecation is announced | — |
| 2 | **[settled]** Data migration and schema evolution | the migration mechanism, and the rule that `SETTINGS_VERSION` (and the audit and session schemas) only move with one | §11 |
| 3 | **[settled]** Error model | the categories, the `Retryable` flag and the `Cause` chain that serialise across processes and devices | §12 |
| 4 | **[settled]** Credentials and key management | Ed25519, rotation in parallel, revocation lists, bulk import | §13 |
| 5 | **[settled]** Upgrade path | in-place, stepwise across a major version, no version skippable, one migration tool per step | §14 |
| 6 | **[settled]** Security disclosure policy | `SECURITY.md` plus private reporting | §15 |

- **[settled]** §1's red lines are the test the policy in row 1 has to pass: a policy that would let the
  kernel grow a supervisor, an operated service or a general-purpose sandbox is not a policy this project
  can adopt.

## 7. audit v2

- **[open]** **This section touches red line 5** — *when in doubt, ask first*
  (`PROJECT_CONSTITUTION.md` §8) — and it is **not authorised by this document**. decisions §33 says so in
  as many words: "the cross-device design of v1.0 must be approved on its own". The plan below is what
  that approval would be asked to cover.
- **[settled]** **One chain per device, plus temporary segments.** Extending the chain's *semantics*, not
  its formula: a segment's head carries a **cross-segment reference** — added **metadata**, with the hash
  formula unchanged.
- **[settled]** `provisional` → fold in (a conflict-free run merges and the mark is cleared) or `fork`
  (both sides stay marked, and it is never a silent merge).
- **[open]** Still undecided, and deliberately so: whether a summary chain needs its own `prev_hash`; how a
  cross-chain reference is verified (by digest, by range, or by both); and how a **conflict** is
  adjudicated when two segments both claim the same act.

## 8. The sandbox plugin interface

- **[settled]** **Out of process, stdio, JSON lines** — isomorphic with `worker` (decisions §3). No
  in-process plugin, no dynamic library, no ABI.
- **[settled]** **A mandatory mechanism layer, an optional semantics layer.** Start / stop / execute /
  output are mandatory; snapshot / fingerprint are optional — and the plugin **declares** which
  capabilities it has (decisions §3).
- **[settled]** **Architecture-independent by design.** Plugin content is not constrained by architecture,
  and kernel code may not assume the guest is RISC-V (decisions §2).
- **[default]** **The capability declaration format** is a draft (§14.11): it is the one part of this
  interface that should be frozen *last*, because it is the part plugins will have to implement.
- **[settled]** **Trust model: capability-constrained.** A plugin's powers are the capabilities it
  declares, checked the way every other capability is checked (decisions §3 reuses the capability model).
- **[settled]** **Preset environments** are four mechanisms, all provided, with the choice left to policy:
  persistent directories (an instance directory plus a shared one), manifest sources (plugin declaration,
  kernel scan and developer-written files, merged), an installation path (a manual shell plus a setup
  script) and AI awareness (system-prompt injection plus a tool query) (decisions §4).
- **[settled]** **The plugin repositories are built in v1.x** — an official index plus community
  repositories, the tool configured with several sources (decisions §5).
- **[settled]** **RISC-V positioning**: a sandbox may be extended by plugin to other architectures, and
  RISC-V stays the substrate and the default implementation (decisions §2).

## 9. The shape of M

- **[settled]** **M is an AI dispatcher, not a static manager.** It decides *what runs where*, and it
  stops there: M does not sit in the data path, does not proxy a task's I/O, and is not in the kernel.
- **[settled]** **Three levels** — node, LAN, cross-region (§5). Every level is the same kind of thing;
  what differs is what it can see.
- **[settled]** **M's permission is the control plane's.** M calls the API directly with its own
  credential — the same surface a person uses — and asks nobody for permission to use it. What M *may* do
  is bounded by its capabilities, not by a gatekeeper inside the kernel.
- **[settled]** **M's loop and M's configuration are separate things.** The loop is a program; the
  configuration is a file. Change the configuration and the loop **restarts** — it does not mutate a
  running loop's beliefs.
- **[settled]** **M's state lives outside M.** The audit chain, the instance table and the pending-approval
  slot are where the truth is; M's in-memory context is a cache of them. This is a **prerequisite, not an
  optimisation**: a design that needs M's memory to be intact to know what is running cannot survive the
  restart above.
- **[settled]** **When M loses contact, M becomes conservative** — it dispatches nothing new, spawns
  nothing, reaps nothing, and tells the user. (Reaping while unable to see is how a supervisor destroys
  work it cannot see.)
- **[settled]** **The way back is a configuration change, not a new entity.** The user fixes the key (or
  the base URL, or the model); M's loop restarts and rebuilds its context **from disk**; the same M
  continues. There is no "new M" to explain to the audit chain — the change is an event in it.

## 10. One settings surface

- **[settled]** **Every setting travels over HTTP**, not over a shell command. A shell command is a
  transport, and a transport that exists only on the desktop is a second way to configure a node.
- **[settled]** **Three paths, one behaviour.** Local desktop, remote desktop and browser client reach the
  same endpoints; "which node" is the only thing that differs.
- **[settled]** **The local desktop keeps an embedded server always on**, bound to `127.0.0.1`.
  `lan_enabled` stops meaning "should the server run" and starts meaning "may the LAN reach it" — which is
  what its warning has said all along.
- **[settled]** **Two kinds of setting, kept apart.** *Node-level* settings travel with the node (it is the
  node that is being configured); *client-level* settings travel with the client (which node am I looking
  at, and what is its token).
- **[settled]** **`host-tauri` becomes thin.** What is left is what only a desktop can do: the window, the
  menu, file dialogs, the keyring shell and the client-level settings.
- **[settled]** **The settings page becomes a debug panel.** Every value shows its **value**, its
  **source** (file, default, or overridden) and its **state**; the underlying JSON can be read and edited
  in place; and it is readable and writable on **every** platform — desktop, remote window and browser
  alike.
- **[settled]** **Every existing tab is converted**, and there is **no separate "advanced" tab**: the debug
  panel is what every tab becomes, because an "advanced" tab is where a normal tab's missing information
  hides.

## 11. Starting the ecosystem

- **[default]** **An SDK** — Rust and TypeScript first, because those are the two languages this repository
  already speaks (the kernel and the front end). See §14.12.
- **[settled]** **The management program moves to its own repository** — `riscdom-adminapp` — at v1.0. The
  v0.9.9 release text already says it ships inside this repository in v0.9 and becomes its own at v1.0.
- **[settled]** Alongside it: the **configuration schema** published as a document; **observability** (what
  a node reports about itself, and where); the **`riscdom-backup`** package (decisions §19: the audit
  store, snapshots and credentials as one movable unit); the **performance budgets** (decisions §18:
  budgets, not measurements); and the **CONTRIBUTING** additions a second repository needs.

## 12. Carried over from v0.9.x

These are open items v1.0 inherits. They are listed here so that no discussion has to rediscover them.

- **[settled]** **D4: the desktop controls.** The browser's control calls refuse today on purpose ("the
  controls arrive with D4"); D4 is the batch that lands them over HTTP.
- **[settled]** **D5: one desktop application.** The desktop's own paths and the browser's converge on the
  same HTTP surface (§10).
- **[open]** **The `task_id` gap.** `POST /v0/agent/run` does not take a `task_id`, so an event frame
  cannot be attributed to the task that produced it when several clients follow one node. The v0.9.9 notes
  name it as "to be resolved before v1.0".
- **[open]** **The `--follow` / `--wait` rough edge**: the stream may still be open when the process exits.
- **[open]** **Python as a guest language** (C, Zig and Rust exist).
- **[open]** **The session database's WAL mode** — deliberately not set in v0.9 (decisions §54).
- **[open]** **A server zip for Linux and macOS** — v0.9.9 shipped the Windows one only.
- **[open]** **The QEMU and gate parallel flakes** — the QMP `10054` family and the port-race hang;
  reported every time, never papered over.
- **[open]** **The GUI switch's click target.** A switch is a bare checkbox about 14 px wide; v1.0 replaces
  it repo-wide with a proper `Toggle` whose hit area is the whole row.
- **[open]** **"The first restart stops the local board."** An observation from the v0.9.9 walk: a first
  relaunch came up on the local board with the remote address gone. It did not reproduce once stale
  developer processes were cleared, and no code path explains it; `manual-acceptance.md`'s layer 9 is what
  settles it.
- **[settled]** **The NetworkTab CPU loop is fixed** — commit `dd599c0`, which this batch pushes: three
  effects depended on the whole store object instead of the store's stable `useCallback` functions, so the
  form was rebuilt on every render, both switches and both fields reverted within milliseconds, and the
  page held about a third of a core at idle.

## 13. Milestones

| # | Milestone | Ends when |
|---|---|---|
| M1 | The freeze level is on disk | §6's six documents exist, and the API stability policy has been tested against §1's red lines |
| M2 | Layer one | several instances of one definition on one node, several LLM configurations, and M dispatching through the control plane |
| M3 | The plugin interface is designed and frozen | §8's interface is frozen — **before** the kernel API, per decisions §3 |
| M4 | Layer two | a workgroup and a cross-region server, with the four roles and the 30-second audit batch |
| M5 | audit v2 | the separate authorisation is obtained, and then the mechanism lands |
| M6 | Layer three | three levels of M dispatching across devices, with cross-chain verification |
| M7 | The ecosystem starts | the SDK, the separate management repository, the backup tool and the budgets |
| M8 | The API freezes, and it ships | the freeze is declared, and v1.0 is released |

- **[settled]** M1 has no dependency and can start immediately.
- **[settled]** M3 comes before the kernel API freeze (decisions §3: "the plugin interface is frozen before
  the kernel API").
- **[open]** M5 depends on an authorisation this document cannot grant; M6 depends on M5.

## 14. Open questions

Fourteen, every one of them **a default that may change**. None of them blocks the milestones above; each
is recorded with the value the discussion landed on, so that changing it is a decision rather than a
drift.

1. **Sandbox reuse.** **[default]** Reuse follows the **capability declaration**: the plugin says what may
   be reused and what may not.
2. **Instance lifetime.** **[default]** Task-scoped, with a configurable retention for what outlives a
   task.
3. **M's reference implementation.** **[default]** The project ships one, under `examples/`, beside the
   reference supervisor that already lives there.
4. **The instance cap's configuration.** **[default]** An extension of `settings.executors` rather than a
   new section.
5. **Where rooms are defined.** **[default]** `rooms.json`.
6. **M-to-M communication.** **[default]** Reuse the cross-device protocol — no protocol of M's own.
7. **The shape of M's `agent_id`.** **[default]** Isomorphic with an executor's, so that one roster
   describes both.
8. **When the API policy lands.** **[default]** At M1, immediately.
9. **Migration mechanism or schema, first.** **[default]** The mechanism first — a schema change without a
   migration path is a schema change nobody can apply.
10. **When the plugin interface freezes.** **[default]** At M3.
11. **The capability declaration format.** **[default]** The draft in §8.
12. **SDK priority.** **[default]** Rust and TypeScript first.
13. **The in-network server's shape.** **[default]** There **is** an in-network server (agreement B); §4's
    workgroup depends on it.
14. **Workgroup discovery.** **[default]** The static table handed down by the in-network server first,
    with UDP broadcast as the supplement.

## 15. Risks

- **audit v2 touches red line 5.** The one item here that is not merely difficult but **not authorised**:
  extending the chain's semantics has to be approved on its own (decisions §33), and if that approval is
  refused, the cross-device half of v1.0 loses its verification story.
- **Timing.** Several AIs collaborating is a frontier rather than a need. v1.0's third layer could be
  correct and still early — and building for a use that has not arrived is how a kernel acquires policy.
- **No reference points.** There is no established shape for "an AI dispatching AIs", so M's design has
  nothing to copy and no way to be obviously wrong. The mitigation is §9's conservatism: when unsure,
  dispatch nothing.
- **Distributed complexity.** Rooms, signing, the relay, temporary centres and cross-chain verification
  are five systems that have never run together here. Each is small; the product of them is not.
- **Time.** The layers are sequential where it matters (L3 behind L1, L2 and audit v2), and there are
  eight milestones.
- **Resources.** Every layer multiplies the number of things that can run at once: instances per node,
  nodes per workgroup, workgroups per region. The budgets of decisions §18 are the only ceiling this plan
  has.

---

*This is a draft. §14 records what is not settled; §7 records what is not authorised.*
