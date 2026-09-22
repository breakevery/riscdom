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
- **several processes, one `audit.db`**: the connection runs in WAL with a 5 s `busy_timeout`,
  and **both** the open sequence and every append are retried on a locked database
  (`OPEN_MAX_ATTEMPTS` / `APPEND_MAX_ATTEMPTS`, exponential backoff from 20 ms). The open
  retry exists because `PRAGMA journal_mode = WAL` answers `SQLITE_BUSY` *without* consulting
  the busy timeout, so two processes creating one fresh `audit.db` at the same moment used to
  fail one of them. A lock that outlasts either budget is an error — never a silent drop.

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
the triggers / query filtering / JSONL export / CLI exit codes / concurrent appends /
concurrent **opens** (8 threads racing one fresh file, and one already in WAL).

## v0.2 TODO

- audit log sharding (roll over by time/size while keeping the chain continuous)
- remote backup (append-style mirror that preserves append-only semantics)
