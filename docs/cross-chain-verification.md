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

**M6-5-2 is where the comparison happens**: the centre takes the anchor it was handed and the digest it holds
for that sender, and either accepts the merge or records `host.audit.chain_rejected` and refuses it. That
event name belongs to M6-5-2 and does not exist yet.

## 3. The other two questions

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

## 4. What this batch does not do

- **No verification.** Nothing compares the anchor with anything (M6-5-2).
- **No `host.audit.chain_verified` / `chain_rejected`.** Those are M6-5-2's, and until a check exists the
  names would describe nothing.
- **No summary chain, no `prev_hash` of its own, no range proof** (M6-5-4).
- **No conflict rule** (M6-5-3), and no change to M5-2's exact test.
- **No change to `ChainDigest`.** It stays a commitment to a head.
- **`compute_hash`, `verify_chain`, the append-only triggers, the route table and the 33-name capability
  vocabulary are untouched.** An added column beside the chain and two added members of an existing frame's
  body are not a format change: the version stays where it is.

**Frozen**: the anchor is a `(chain, length)` pair; `segments.head_prev_length` exists and is `NULL` for older
rows; `anchor_digest` ≡ `head_prev_chain` (one value, two names, the old one never dropped); verification is
"the centre compares against the digest it holds"; a conflict is forked, never adjudicated automatically.
**Not frozen**: whether the centre ever caches past digests (today it keeps only the newest), whether a range
proof is ever wanted, and the shape of M6-5-3's tool.
