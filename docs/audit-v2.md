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

**[settled]** **The physical shape is (b): one file per chain, merged later.** A temporary segment keeps its
**own SQLite file** beside the main chain, and when the real centre returns its events are written into the
main chain by **transcription** (M5-2). The main chain therefore stays one linear, verifiable sequence, and
both the formula and `verify_chain` stay where they are. [§8](#8-where-the-temporary-segments-live) says where
the files live and [§9](#9-how-each-chain-is-verified) how each is verified.

The two shapes that were weighed and are **not** the project's. **(a) One file, one chain, segments as
tags** — a temporary centre appending straight into the main `audit_events` under its `segment_id`. That is
not available to a centre standing in *while the real one is unreachable*: the two are different machines,
so the events cannot physically be in one file until they meet, and writing an unreconciled centre's rows
into the trusted chain is the opposite of what `provisional` is for. **(c) A separate chain inside one
file** — which would force `verify_chain` to grow a **segment-aware** mode, i.e. to change on the red-line-4
verification surface.

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

**[settled]** **M5-1b opens and closes one.** `AuditStore::open_segment(kind)` writes the row — with
`head_prev_chain` set to the chain's head **at that moment**, read *before* anything is appended — and then
appends the opening event (§6) to the **main chain**; if that append fails the row is removed, so a segment
row always has its opening event. `AuditStore::close_segment(&segment_id)` sets `state = closed` and
`closed_at_ms`, then appends the closing event, and puts the row back to `open` if that append fails. The
`segments` table has **no append-only trigger** — it is the record *beside* the chain, like `runs` — so its
own rows may be updated; the chain's rows are only ever appended.

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
bears on how a segment's head is shaped now that §2 fixes the physical shape as (b).

## 6. The segment events (reserved)

**[settled]** **The lifecycle events are on the main chain**, and M5-1b writes them:

- `host.audit.segment_opened` — a segment opened. Detail:
  `{ "segment_id": …, "kind": "main" | "temporary", "head_prev_chain": <hash or null> }`.
- `host.audit.segment_closed` — a segment closed. Detail:
  `{ "segment_id": …, "closed_at_ms": <epoch ms> }`.
- `host.audit.segment_merged` — a segment's events were written into the main chain. **Reserved**: §2 fixes
the shape as transcription, and **M5-2** is what implements it; the name is recorded here so the merge has
one to use.

Both are written with `segment_id IS NULL` (the record of the segment's *life* is not one of the segment's
own events), both are ordinary appends, and both carry the words §3 stores (`kind` is `SegmentKind::as_str`).

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
- **The physical shape of a segment's own chain** is **settled as (b)** — see §8.
- **It is not the digest's transport.** What a digest is and how it travels is [connection.md §7](connection.md).

## 8. Where the temporary segments live

**[settled]** **The main chain** is the store the node has always had — `audit.db` in the audit directory
(the one **backup** already carries, `<workspace>/.riscdom/audit.db`). Its rows are the `segment_id IS NULL`
rows, and nothing about it changes.

**[settled]** **A temporary segment is its own SQLite file**: `audit-segments/<segment_id>.db` **inside the
same audit directory**. One file per segment, made by
[`segment_db_path_in`](../audit/src/store.rs) and opened (creating the directory and the file) by
[`AuditStore::open_segment_store`](../audit/src/store.rs). The file is an **ordinary store**: the same
`audit_events` schema, the same append-only triggers, and **its own genesis** (its first row's `prev_hash`
is [`GENESIS_PREV_HASH`](../audit/src/hash.rs)).

The base directory is the caller's — the **audit directory**, not the workspace — so `audit` composes no host
layout of its own; it composes the `audit-segments/<id>.db` part.

**[settled]** **The two are tied together by the `segments` row in the *main* store.** Opening a segment
(§3) registers it there, with `head_prev_chain` = the main chain's head at that moment. The segment's own
store holds its own events; the main store holds the **record** of the segment, not its rows.

**Merging is transcription (M5-2).** When the real centre returns, a batch reads the segment's events and
**appends** them to the main chain as new events, then appends the merged event (§6). Nothing is rewritten:
the main chain only ever grows, and the `segments` row moves to `folded` (conflict-free) or `forked`
(conflicting) — §5's question 3 is what decides which.

## 9. How each chain is verified

**[settled]** **The main chain: `verify_chain`, unchanged.** It walks one store in id order, checking
`prev_hash` linkage and recomputing each hash — and because a temporary segment is a **different file**, the
main store it reads holds the main chain's rows and nothing else. No segment-aware mode exists, and none is
needed.

**[settled]** **A temporary segment: the same function, a different store.** `verify_chain(&segment_store)`
is the whole of it: the segment file is an ordinary store with its own genesis, so the one verifier answers
"is this segment intact" exactly as it answers it for the main chain. There is no second verifier and no
second formula.

**Cross-chain verification is M6.** `head_prev_chain` (§4) is the **anchor** a later batch checks the two
against; §5's question 2 — by digest, by range, or both — is where its shape is settled. Until then the two
chains are each verifiable on their own, and their *relationship* is recorded metadata.
