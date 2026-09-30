[中文](cross-chain-verification.zh-CN.md) | English

# Cross-chain verification

> **Status** v1.0 specification (M6-5-1) ｜ **Date** 2026-09-30 ｜ **Audience** kernel developers, and whoever
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

**M6-5-2b is where the anchor continuity would be checked**, and it needs one more piece of evidence first:
the events' own `hash` (and `prev_hash`) on the wire, which would let the centre recompute the sender's
linkage from `anchor_digest` and refuse a stream that does not continue from it.

## 4. The other two questions

**Answer to (1): the summary chain is not needed for the anchor.** §2's choice answers (2) without one. A
summary chain of its own `prev_hash` would answer a different question — what the *centre* was told, in order
— and it is **M6-5-4**, out of the first piece.

**Answer to (3): the kernel gives observability and tools, not a rule.** M5-2 already does the honest half:
the narrowest possible test (exact equality of `actor`, `action` and cleared detail), a **fork** rather than a
merge, and both sides kept ([audit-v2 §11](audit-v2.md)). What is left is what to *do* about a fork, and
§33's majority rule does not transfer: it settles "what a node knows" (liveness, already M5-3a's witness
rule), not "which of two acts is right". A **rule** would be policy — the caller's, not the kernel's
(roadmap §1). So M6-5-3 is **observability plus a manual tool**: a fork is visible and can be exported for a
person to judge. No automatic adjudication, **M6**.

## 5. What is not here yet

- **No anchor-continuity check.** The events' hashes do not travel, so the centre cannot prove a segment
  really continues from `anchor_digest`; §3's checks are what the delivery itself can be asked (M6-5-2b).
- **No new wire field, and no new frame.** M6-5-2a reads the frames M5-3c-2 already sends.
- **No summary chain, no `prev_hash` of its own, no range proof** (M6-5-4).
- **No conflict rule** (M6-5-3), and no change to M5-2's exact test.
- **No change to `ChainDigest`.** It stays a commitment to a head.
- **`compute_hash`, `verify_chain`, the append-only triggers, the route table and the 33-name capability
  vocabulary are untouched.** An added column beside the chain and two added members of an existing frame's
  body are not a format change: the version stays where it is.

**Frozen**: the anchor is a `(chain, length)` pair; `segments.head_prev_length` exists and is `NULL` for older
rows; `anchor_digest` ≡ `head_prev_chain` (one value, two names, the old one never dropped); a delivered
segment is checked **against its own end frame**, a segment whose anchor has no length is recorded as
**skipped**, and a delivery that fails is **recorded and not merged** while its file stays; a conflict is
forked, never adjudicated automatically. **Not frozen**: whether the events' hashes ever travel (M6-5-2b),
whether the centre ever caches past digests (today it keeps only the newest), whether a range proof is ever
wanted, and the shape of M6-5-3's tool.
