[中文](audit-v2.zh-CN.md) | English

# audit v2 — the main chain and its temporary segments

**Status** v1.0 specification (M5-1a) ｜ **Date** 2026-09-30 ｜ **Audience** kernel developers, and
whoever implements M5.

## 1. What this document is

[roadmap §7](roadmap-v1.0.md) extends the audit chain's *semantics* to "the main chain + temporary
segments" and says a segment's head carries a **cross-segment reference**, added as **metadata**, with the
hash formula unchanged — and [decisions §127](decisions.md) is the owner's authorisation of that extension.
This document is the extension **written down**: what a segment is, how a segment's head refers to the chain
it continues, and which questions are still deliberately open.

**It is the audit subsystem's specification, not the connection layer's.** [connection.md §7](connection.md)
is the *transport* of a chain's digests — a commitment to a point on the chain, reported upward on a timer —
and it points here for what a segment is. Where the two touch, this document is the one that says what the
chain means and connection.md's §7 says how a digest of it travels.

**M5-1a is what this document's batch lands**: the **schema** (the `segments` table and the
`audit_events.segment_id` column) and this specification. It writes no segment and opens no segment — that is
M5-1b.

## 2. The main chain and temporary segments

**[settled]** **One chain per device, plus temporary segments.** The split is **semantic, not a second
formula**: the device's chain is one linear, verifiable sequence of events, and a **segment** is a **span**
of that sequence — the events written while a temporary centre stood in for the real one. What changes is
*which span an event belongs to*, never *how an event is hashed*.

**[settled]** **A segment's head carries a cross-segment reference** — metadata, at the head of the span,
saying which chain position the segment continues from. It is **not** an input to
[`compute_hash`](../audit/src/hash.rs) (the formula is `prev_hash | ts | actor | action | detail_json` and
nothing else), so the chain verifies exactly as it did before (decisions §127 point 2).

**[settled]** **The tag is `audit_events.segment_id`.** `NULL` means **the main chain** — which is exactly
what every event written before this column existed is, so an old log reads correctly with **no rewrite**.
A non-`NULL` value names a row in the `segments` table (§3).

**[settled]** **The `segments` table is not part of the hash chain.** Like `runs`, it records what the
chain's own rows imply; a chain verifies whether or not a segment row exists.

**The physical shape of a temporary segment's own chain is [not decided here](#5-the-questions-still-open).**
The schema above supports all of the candidates the discussion has named; which one is chosen is the owner's
to settle, and it is listed as an open question in §5 rather than assumed here. The candidates:

- **(a) One file, one chain, segments as tags** — a temporary centre writes into the same `audit_events`
  chain under its `segment_id`. `verify_chain` needs no change (one chain, walked in id order).
- **(b) One file per chain, merged later** — a temporary centre keeps its own store, and when the real
  centre returns the run is folded in. The merge is M5-2's.
- **(c) A separate chain inside one file** — the shape that would make `verify_chain` need to grow a
  **segment-aware** mode, which touches the red-line-4 verification surface. **This one needs an explicit
  decision, and the decision is not made here.**

## 3. The segment record

The `segments` table (created by M5-1a; written by M5-1b):

| Column | Type | Meaning |
|---|---|---|
| `segment_id` | `TEXT` PRIMARY KEY | The segment's own name — the value the events carry in `audit_events.segment_id`. |
| `kind` | `TEXT` NOT NULL | `main` or `temporary` ([`audit::SegmentKind`](../audit/src/segment.rs)). |
| `head_hash` | `TEXT` | The segment's last hash once it has events; `NULL` until then. |
| `head_prev_chain` | `TEXT` | The **cross-segment reference** (§4): the main chain's head when the segment opened. |
| `opened_at_ms` | `INTEGER` | When it opened. |
| `closed_at_ms` | `INTEGER` | When it closed; `NULL` while it is open. |
| `state` | `TEXT` NOT NULL | `open` / `closed` / `folded` / `forked` ([`audit::SegmentState`](../audit/src/segment.rs)). |
| `note` | `TEXT` | A human note. Free-form, stored beside the chain like everything else here. |

`main` / `temporary`, `open` / `closed` / `folded` / `forked` are the **words stored on disk**; the
reader is [`SegmentKind::parse`](../audit/src/segment.rs) / [`SegmentState::parse`](../audit/src/segment.rs),
which answers `None` for a word it does not know rather than guessing.

## 4. Cross-segment references

**[settled]** **A segment's head records where it continues from, and that record is metadata.** The field
is `head_prev_chain`: the main chain's head hash at the moment the segment opened. A verifier that holds both
the segment and the chain can check the segment really continues where it says it does **without recomputing
any hash differently** — the reference sits beside the events, and
[`compute_hash`](../audit/src/hash.rs) never reads it.

**Why not fold it into the hash.** Any input added to `compute_hash` would change every hash after it, i.e.
a different formula — which is exactly what decisions §127 point 2 forbids ("the hash formula does not
change"). Keeping the reference in a column is what lets the chain's guarantee stay its guarantee.

## 5. The questions still open

[roadmap §7](roadmap-v1.0.md) leaves three questions open, and this document does **not** settle them — it
records them so the implementation meets them deliberately:

1. **Whether a summary chain needs its own `prev_hash`.**
2. **How a cross-chain reference is verified** — by digest, by range, or by both.
3. **How a conflict is adjudicated** when two segments both claim the same act.

All three are marked `[open]` in roadmap §7, and (2) and (3) are inputs the merge stage (M5-2) needs; (1)
bears on how a segment's head is shaped in (a)/(b)/(c) of §2.

## 6. The segment events (reserved)

These names are **recorded here and implemented by M5-1b** — this batch writes no event:

- `host.audit.segment_opened` — a segment opened.
- `host.audit.segment_closed` — a segment closed.

They belong to the **`host.audit.`** family, which is the audit subsystem's own (the connection layer's facts
stay in `host.connection.*`: `key_minted`, `data_too_new`, `peer_offline`, `peer_recovered`). They are
**audit event names**, not stream names: the twenty `control-plane-events.md` names are a different
vocabulary (`agent:*`, `vm:*`, `m:*`, …) and this does not touch them.

## 7. What is not here

- **`provisional` and `fork`** — the marking, the fold and the conflict rule — are **M5-2**, and they need
  §5's questions answered first.
- **The temporary centre** — who takes over, the three suppression layers, and the return — is **M5-3**.
- **The immediate push of a key event** (an ejection, a fork, a takeover) is **M4e-2**; [connection.md
  §7](connection.md) covers the batched digest it is beside.
- **The physical shape of a segment's own chain** (§2) is the owner's to settle.
- **It is not the digest's transport.** What a digest is and how it travels is [connection.md §7](connection.md).
