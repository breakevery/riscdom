[中文](upgrade.zh-CN.md) | English

# The upgrade procedure

**Status** v1.0 specification (milestone [M1](roadmap-v1.0.md)) ｜ **Date** 2026-09-28 ｜ **Baseline**
v0.9.9 (`3365970`) ｜ **Audience** distribution integrators — whoever ships or operates an upgrade.

This is the written procedure [decisions §14](decisions.md) names and
[api-compatibility.md §7](api-compatibility.md) points at. The rules live there and in
[decisions §11](decisions.md); this file is the **order of operations**, so an upgrade is a
checklist rather than a reading exercise.

## 1. What an upgrade changes

**[settled]** An upgrade changes two things and no more: the **installation** (overwritten where
it stands) and the **persisted data** (migrated the first time the new build opens it). It never
asks the operator to reinstall, to move a data directory, or to run a migration by hand.

- **In place.** The installer overwrites the installation; nothing is reinstalled and the data
  directory does not move ([decisions §14](decisions.md)).
- **Migration is automatic.** Each format migrates on open, once
  ([decisions §11](decisions.md), [api-compatibility.md §6](api-compatibility.md)). There is no
  separate migration step to remember, which is the point: an upgrade that needs one is an
  upgrade that gets skipped.

## 2. What is versioned, and where the marker lives

**This table is the whole list.** A persisted format that is not here carries no marker this
procedure can act on — and a new one adds its row in the same batch that creates it
([api-compatibility.md §6](api-compatibility.md)).

| Format | Where | Marker today | A file from before the marker |
|---|---|---|---|
| `settings.json` | `<data-dir>/settings.json` | `version` = `SETTINGS_VERSION` (**2** since v1.0 M2b-1) | reads as `1` and is migrated **on open** to `2` |
| Run fingerprint | inside each run's record | `riscdom.run.fingerprint.v1` | a v2 fingerprint is a new marker **value**, never a rewritten v1 record |
| Audit store | `<data-dir>/audit.db` (SQLite) | `PRAGMA user_version` = **1** (v1.0 M2b-3a) | reads as `0`, is migrated **on open** and then stamped |
| Session database | `<data-dir>/sessions.db` (SQLite) | `PRAGMA user_version` = **1** (v1.0 M2b-2) | reads as `0`, is migrated **on open** and then stamped |
| Credential files | `<data-dir>/token` | none (a shape-checked hex line) | a hand-provisioned token is **never rewritten** |
| Node identity | `<data-dir>/node.key`, or the OS keyring | `schema_version` = **1** (v1.0 M4a) | new in v1.0 M4a; a **newer** file is refused |
| Peer table | `<data-dir>/peers.json` | `schema_version` = **1** (v1.0 M4a) | new in v1.0 M4a; a **newer** file is refused, an older one migrates **on open** |
| Room table | `<data-dir>/rooms.json` | `schema_version` = **1** (v1.0 M4c) | new in v1.0 M4c; a **newer** file is refused, an older one migrates **on open** |

Two properties of that table decide the procedure below:

- **A `.bak` is taken for some formats and not others.** `settings.json` and `sessions.db` copy
  their pre-migration bytes aside (`settings.json.bak`, `sessions.db.bak`). The audit database
  deliberately does **not**: it is WAL and may be open in several processes, so a byte copy of
  `audit.db` alone can miss frames still in `-wal`. That is why step 3 of §3 copies the whole
  directory instead.
- **A marker is not a promise about other nodes.** Two nodes on different versions share only
  what the protocol says; the older one refuses the newer one's files rather than guessing.

## 3. The procedure

**[settled]** Six steps, in this order. Steps 1–3 are the operator's; 4–6 are what the new build
does the first time it opens each file.

1. **Read the release notes for the step.** Crossing a major version is stepwise (§5), and the
   step's migration tool ships with the release notes for that step.
2. **Stop the node.** Migration happens on open, so it must not race a running process.
3. **Copy the data directory aside.** The automatic `.bak` files cover `settings.json` and
   `sessions.db`; the audit database is not byte-copied on purpose (§2), so a directory copy is
   what protects it. Migration is **not reversible** — this copy is the way back.
4. **Install the new build in place.** The installer overwrites the installation; no data
   directory moves.
5. **Start it once, and let it migrate.** Each file is migrated the first time it is opened and
   then stamped with the current marker. Nothing is written back for a file that needs nothing.
6. **Check it.** `GET /v0/audit/status` answers whether the chain still verifies and how many
   events it holds, and `GET /v0/status` answers that the node itself came up. An upgrade that
   returns a chain that does not verify is a failed upgrade, and the copy from step 3 is what
   you go back to.

## 4. When a file is refused

**[settled]** Two refusals, both loud, both leaving the file alone.

- **An old build meeting a newer file** refuses it and reports `data_too_new` — it does not read
  it, and it does not write it. Nothing is applied and nothing is downgraded silently
  ([decisions §11](decisions.md)).
- **A file that cannot be migrated** leaves the build running with the file untouched; the
  pre-migration bytes are still in the `.bak` (or in the copy from §3 step 3).

There is no partial migration to clean up: a file is either at the marker the build writes, or
it is at the marker it had, with its backup beside it.

## 5. Crossing a major version

**[settled]** A major version may **not** be skipped. `1.x → 3.x` goes through `2.x`.

- Each step ships the migration it needs, and the step's migration tool is provided with the
  release notes for that step ([api-compatibility.md §7](api-compatibility.md)).
- The stepwise rule is about **data**: each major version's migration assumes the previous major
  version's result is already in place.
- An upgrade from `1.x` straight to `3.x` is **refused**, not attempted
  ([api-compatibility.md §8](api-compatibility.md)).

`0.x → 1.0` is not promised to be painless — `v0.x` promised nothing — but the 1.0 migration is
written and takes a `.bak` first ([api-compatibility.md §8](api-compatibility.md)).

## 6. What an integrator must not do

- **Ship an installer that disagrees with this file.** A package with its own upgrade rules is a
  bug: the macOS / Linux packages and the Windows installers follow this document
  ([api-compatibility.md §8](api-compatibility.md)).
- **Skip a major version**, or let a user believe they can.
- **Make a hand-run migration the normal path.** A migration the operator has to remember is a
  migration that will not happen ([decisions §11](decisions.md)).

## 7. What this file does not cover

- **The schemas themselves.** What each format holds, and what each version marker means, are
  [api-compatibility.md §5 and §6](api-compatibility.md) and [decisions §11](decisions.md).
- **Rollback.** Migration is not reversible; going back means restoring the copy from §3.
- **Anything about another node.** A marker says what *this* installation writes; it promises
  nothing about a peer's.
