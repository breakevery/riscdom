[中文](cross-chain-verification.zh-CN.md) | English

# Cross-chain verification

> **Status** v1.0 specification (M6-5-1, M6-5-2a, M6-5-2b, M6-5-3a) ｜ **Date** 2026-09-30 ｜ **Audience** kernel developers, and whoever
> deploys more than one node.

[roadmap §7](roadmap-v1.0.md) left **three questions open** and said so on purpose. [audit-v2 §5](audit-v2.md)
recorded them without settling them. This document is where they are settled — in **pieces**, because the
mechanism they need is the one M5 finished, and each piece is a batch.

## 1. The three questions, and what stands behind them

1. **Whether a summary chain needs its own `prev_hash`.**
2. **How a cross-chain reference is verified** — by digest, by range, or by both.
3. **How a conflict is adjudicated** when two segments both claim the same act.

Behind all three is one fact: **two devices keep two chains, each with its own genesis.** §4's
`head_prev_chain` is the only thing that joins them — a hash on the *sender's* chain — and the receiver
**cannot see the sender's chain**. A digest (M4e-1) is a commitment to a **head**; the reference names an
**interior point**. A commitment to the head cannot verify an interior point.

**What M5 already put in place** (and what it deliberately did not):

| Already there | Still missing |
|---|---|
| `head_prev_chain` on the segment row (`segments.head_prev_chain`, M5-1b) | a **verifier** — nothing reads it |
| the anchor travels to the centre with the end frame (M5-3c-2) | the anchor's **length** |
| delivery, rebuild and `merge_segment` (M5-3c-2) | any check that a segment **really continues** from its anchor |
| `ChainDigest { chain, length }` (M4e-1), held per node by the server | a digest of a **past** point |

## 2. The choice: by digest, and the anchor grows a second half

**[settled]** **Answer to (2): by digest.** A verified anchor is a `(chain, length)` pair — the head hash of
the sender's chain **at the moment the segment opened**, and **how long that chain was there**. The pair is
what M4e-1 already calls a commitment to a point; what was missing is that the anchor carried only its first
half.

- **Not by range.** A range proof ("everything from genesis to the anchor") would mean a second digest
  structure — a Merkle tree or a rolling hash — beside the one formula §127 fixes. The owner declined it for
  the first piece.
- **Not by summary chain.** A chain over the digests would let the centre prove what *it* was told, in
  order; it would still say nothing about a node's interior point, and it would make the centre a holder of
  history, which §6.2 explicitly avoids ("holds digests rather than messages").

**M6-5-1 lands the evidence, not the check.** This batch adds:

- **`segments.head_prev_length`** — an `INTEGER` column beside `head_prev_chain`, `NULL` for a row written
  before it existed. It is read at the same moment as the head (before the segment's opening event is
  appended), so it names the **same** point. The column is beside the chain, so `AUDIT_SCHEMA_VERSION` stays
  **1** and nothing is rewritten.
- **`anchor_digest` / `anchor_length`** on the `segment_done` frame. `anchor_digest` and `head_prev_chain`
  are **one value under two names** — the M5-3c-2 field keeps its name so every existing reader keeps
  working, and the anchor vocabulary gets its own name for the same string. Both are written from the same
  row field, so they cannot drift; a reader prefers `anchor_digest` and falls back to `head_prev_chain`, so a
  sender that predates this batch still reads.
- **`adopt_segment` records both halves.** The centre writes what the owner reported — the name, the reference
  and its length. **It verifies none of it.**

## 3. What the receiver checks — and what it cannot (M6-5-2a)

**[settled]** **The centre checks the delivery, refuses what does not hold together, and says so.** Since
v1.0 M6-5-2a, `receive_segment` verifies two things between rebuilding the segment and merging it, and
records which happened:

| What is checked | How | What it answers |
|---|---|---|
| **the envelope** | the events against the end frame that closed the stream | did every position `0..total` arrive exactly once, and was the stream addressed to this node? |
| **the rebuild** | `verify_chain` on the store the centre just wrote | is the store intact? |

- **`host.audit.chain_verified`** — the delivery held. The detail carries `segment_id`, `from`,
  `anchor_digest`, `anchor_length`, `events` and `checked` (`"delivery"`, or `"skipped"` for a segment whose
  anchor has no length). The merge then runs **unchanged**.
- **`host.audit.chain_rejected`** — the delivery did not hold. The detail carries the same fields plus
  `reason`. **`merge_segment` is not called**: nothing of the segment reaches the main chain, and the row is
  neither `folded` nor `forked` — a fork is "both sides are real and nobody has said which is right", and
  this is "the delivery did not hold together". The segment's own file **stays**: a refused delivery is
  still evidence.
- **A stream that carried no event is answered.** `total: 0` and nothing delivered is consistent, so an
  **empty segment is checked and folded** — M5-3c-2 dropped that case on the floor, so a legitimately empty
  segment never merged and a sender could believe it had delivered one the centre never heard of. `total: n`
  with nothing delivered is a gap, and is refused.
- **A segment whose `anchor_length` is `None` is recorded as `skipped`** — it was opened before the length
  column existed, so there is nothing to check against. It is merged, and the row says what was not checked
  rather than pretending.

**What it deliberately does not check — and this is the honest half.** The centre **cannot** say that those
events were ever on the sender's chain, nor that the segment really continues from `anchor_digest`: the
stream carries the events' five transcription fields and **not their hashes** (M5-3c-2 chose that on purpose —
the ids and hashes belong to the sender's chain). A commitment to a head cannot verify an interior point
(§1), and there is no digest history to compare a past point against. So a passing check is recorded as
**checked**, never as **proven**.

## 4. The stream's own chain — and the link that is missing (M6-5-2b)

**[settled]** **Each event now carries the two hashes it was written with, and the centre recomputes them.**
Since v1.0 M6-5-2b a `segment_event` body carries `hash` and `prev_hash` (the source row's own values, moved
across, never re-derived by the sender), so the receiver can ask two more questions:

- **does every event recompute** to the hash it claims — with `audit::compute_hash` **called, never
  changed**, and the detail rendered exactly as the sender's `append` rendered it?
- **does every event continue** from the one before it (`prev_hash` of *n* is the `hash` of *n-1*)?

The last event needs nothing extra: recomputing it *is* the "its own hash is self-consistent" check. The
verdict travels in the same two words as before, with one new value each:

| what happened | `checked` | `linkage` |
|---|---|---|
| the stream recomputed and linked | `delivery+chained` | `ok` |
| it did not (the merge is refused) | `delivery` | `broken` |
| the sender carried no hashes (the **cross-version window**; it merges) | `delivery` | `skipped` |
| the anchor had no length (it merges) | `skipped` | `skipped` |

**[settled] The anchor itself is still not checked, and with what travels it cannot be — this is a remaining
goal, not an oversight.** The anchor (`head_prev_chain`) is the chain position **before** the
`segment_opened` marker, and a segment's delivered events are the span **after** that marker. So the first
delivered event's `prev_hash` is the **marker's** hash, one link away from the anchor, and the centre never
holds the marker: it is the record of the segment's *life*, not one of its events. An earlier plan assumed
`events[0].prev_hash == anchor_digest`; it is **false by construction** — the two differ by exactly the
`segment_opened` row. What would make it real is one more piece of evidence (the marker's own hash, or a
redefinition of the anchor to *be* that hash), and that is recorded as **work still owed by M6-5**, not as
something this batch delivers.

**So the linkage check proves the stream is a chain, not that it is a piece of the sender's chain.** It
catches an edited detail, a reordered or dropped event, and a stream stitched together from other streams —
and it does not claim the anchor.

## 5. The other two questions

**Answer to (1): settled — the centre's own chain (v1.0 M6-5-4).** A summary chain of its own `prev_hash`
would be a second genesis, a second format and a second verification story, and it would make the centre a
holder of history, which §6.2 avoids. What "what was the centre told, in order" needs is **a row**, and the
centre has a chain already: a digest it is told is recorded there as `host.audit.digest_received`
(`{node_id, chain, length}`), **once per change** — a report that repeats itself writes nothing, so a
thirty-second cadence does not become thirty seconds of chain. §2's choice already answered (2) without it.

**Answer to (3): the kernel gives observability and tools, not a rule.** M5-2 already does the honest half:
the narrowest possible test (exact equality of `actor`, `action` and cleared detail), a **fork** rather than a
merge, and both sides kept ([audit-v2 §11](audit-v2.md)). What is left is what to *do* about a fork, and
§33's majority rule does not transfer: it settles "what a node knows" (liveness, already M5-3a's witness
rule), not "which of two acts is right". A **rule** would be policy — the caller's, not the kernel's
(roadmap §1). So M6-5-3 is **observability plus a manual tool**: a fork is visible and can be exported for a
person to judge. No automatic adjudication, **M6**.

## 6. What is not here yet

- **The anchor link is closed** (v1.0 M6-5-4). The end frame now carries `anchor_hash` — the hash of the
  sender's `segment_opened` row — so the centre can check that `events[0].prev_hash` **is** that point. A
  sender that predates the member claims nothing and is recorded `anchor: "skipped"`, exactly as a sender
  whose events carry no hashes is.
- **No new frame.** M6-5-2a to M6-5-4 add members to frames M5-3c-2 already sends; nothing new is dialled.
- **A summary chain exists, and it is the centre's own** — a row per change
  (`host.audit.digest_received`), not a second chain; **no `prev_hash` of its own, and no range proof** (M6-5-4).
- **No conflict rule** (M6-5-3), and no change to M5-2's exact test.
- **No change to `ChainDigest`.** It stays a commitment to a head.
- **`compute_hash`, `verify_chain`, the append-only triggers, the route table and the 33-name capability
  vocabulary are untouched.** An added column beside the chain and two added members of an existing frame's
  body are not a format change: the version stays where it is.

## 7. The conflict exit: observation, and the reader that is missing (M6-5-3a)

**[settled]** **A conflict is on the chain and nowhere else.** M5-2b's fork is recorded as one
`host.audit.segment_forked` event — `{ segment_id, kind, forked_at_ms, reason, conflicting_event_id }` — plus
the segment row's `state = forked`. **No endpoint lists segments at all** (the store's `segments()`/`segment()`
are used only inside `audit`), so a fork is read the same way every other fact is: out of the chain.

**[settled] The observation is a filter, not a new surface (v1.0 M6-5-3a).** The CLI's audit read gained
`--action-prefix`, which the server already supported:

```
riscdom audit events --action-prefix host.audit.segment_forked      # this node's forks
riscdom audit events --action-prefix host.audit.chain_rejected     # its refused deliveries
```

The human table names each row's action (that is what the filter selects); the row's **detail** — which
segment, and why — is read with the global `--json`, which passes the control plane through unchanged. **No
new route, no new capability, no new event name.**

**[settled] The kernel does not choose a side.** A fork stays a fork: nothing in the kernel decides which of
two contradicting acts is right (§5's answer to question 3).

**[settled] Marking one resolved is a record, and it is here (v1.0 M6-5-3b).**
`POST /v0/audit/conflicts/{segment_id}/resolve` takes an optional `note` and appends **one**
`host.audit.conflict_resolved` row: `{ segment_id, resolved_by, resolved_at_ms, note }`, where
`resolved_by` is the caller's own identity. The CLI asks for it as
`riscdom audit resolve <segment_id> [--note <text>]`. What it does **not** do is the point:

- **no side is named** as right, and **nothing is transcribed** — the kernel did not decide;
- **the segment row is not touched**: `state = forked` means *unresolved* and is the **index**, while the
  chain is the **record**;
- **the segment is not looked up** — the act records a decision, so refusing it because a row is missing
  would lose a real decision rather than prevent a bad one;
- **no new capability name**: the route declares `settings.write`, the same as the audit alert, because a
  name no other route shares would be vocabulary rather than a power.

**Owed: the key-event push has no reader.** Since M4e-2 a fork is *also* pushed to the server the moment it
happens (`host.audit.segment_forked` as a key event, `key_events_of`, the newest 256 per node, in memory)
precisely so the aggregation role hears it at once — and **no route and no command read that log**. So today
the push reaches nobody outside the server's memory, and the only surface a deployer has is the node's own
chain. That is recorded here as a **known gap**, not as a working notification path.

**Frozen**: the anchor is a `(chain, length)` pair; `segments.head_prev_length` exists and is `NULL` for older
rows; `anchor_digest` ≡ `head_prev_chain` (one value, two names, the old one never dropped); a delivered
segment is checked **against its own end frame**, its events' own hashes are **recomputed and linked**, a
segment whose anchor has no length — or whose sender carried no hashes — is recorded as **skipped**, and a
delivery that fails is **recorded and not merged** while its file stays; a conflict is
forked, never adjudicated automatically. **Not frozen**: whether the `segment_opened` marker's hash ever
travels so the **anchor link** can be checked, whether the centre ever caches past digests (today it keeps
only the newest), whether a range proof is ever wanted, and the shape of M6-5-3's tool.

**The same shape, twice more (v1.0 M6-2a).** This is not the only thing the kernel produces and no one
reads. The connection layer carries two more: a §6.6 registration's `capabilities` are stored on the
server's row and read by nothing, and the §4.1 registry hand-down can be asked for but no production caller
asks. Both are recorded in full in [connection.md §11](connection.md) — the point of noting them here is that
the three gaps are one pattern, and closing one should ask whether the others close with it.
