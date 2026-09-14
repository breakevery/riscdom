[中文](README.zh-CN.md) | English

# audit

The RiscDom **audit layer**. It implements article 2 of the project constitution:

> The audit log lives outside the AI: append-only, cannot be disabled.

## Semantics

- **append-only (hard guarantee)**: the SQLite table `audit_events` has `BEFORE UPDATE` /
  `BEFORE DELETE` triggers, so any rewrite raises
  `RAISE(ABORT, 'audit_events is append-only')`. The Rust API has **no** UPDATE / DELETE and
  no switch to turn auditing off.
- **hash chain**: every event's
  `hash = sha256(prev_hash | "|" | timestamp_ms | "|" | actor | "|" | action | "|" | detail_json)`
  (lowercase hex). The first (genesis) event has a `prev_hash` of 64 `0`s.
- **independently verifiable**: `verify_chain` recomputes the whole chain and **locates the
  first broken event's id**.

## Modules

- `event` — `AuditEvent` / `StoredEvent`
- `store` — `AuditStore` (`open` / `in_memory` / `append` / `last_hash` / `get` / `list` /
  `count` / `export_jsonl` / `all`) + `EventFilter`
- `hash` — `compute_hash` / `verify_chain` / `ChainStatus` / `GENESIS_PREV_HASH`
- `sink` — the `AuditSink` trait + `SqliteAuditSink` (production) + `FileAuditSink` (example)
- `error` — `AuditError`

## audit-verify CLI

```text
cargo run -p audit --bin audit-verify -- <path-to-db>
```

Exit codes:

| code | meaning |
| --- | --- |
| 0 | chain intact: `Intact { length: N }` |
| 1 | chain broken: `Broken { at_id, reason }` |
| 2 | failed to open |

Examples:

```text
$ audit-verify clean.db
Intact { length: 3 }
$ audit-verify tampered.db
Broken { at_id: 2, reason: "hash mismatch: expected 8ff2…, found 467b…" }
```

## Event types (the emitting side decides the actor)

`vm.start` / `vm.stop` / `vm.snapshot.save` / `vm.snapshot.load` / `serial.read` /
`serial.write` (sandbox); `agent.user.input` / `agent.llm.request` / `agent.llm.response` /
`agent.tool.call` / `agent.tool.result` / `agent.policy.deny` / `agent.compile.start` /
`agent.compile.result` (agent).

## Dependency direction

`audit` does **not** depend on `sandbox` / `agent`; they depend on this crate.

## Tests

```text
cargo test -p audit
```

Coverage: empty chain / three-event chain / tamper localisation / UPDATE + DELETE rejected by
the triggers / query filtering / JSONL export / CLI exit codes / concurrent appends.

## v0.2 TODO

- audit log sharding (roll over by time/size while keeping the chain continuous)
- remote backup (append-style mirror that preserves append-only semantics)
