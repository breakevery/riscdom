[中文](backup.zh-CN.md) | English

# Backup and portability

**Status** v1.0 specification (M7e) ｜ **Date** 2026-09-29 ｜ **Audience** operators, and whoever has to move a
node — or to prove it can be moved.

**What this document is.** [decisions §19](decisions.md) settles it: a **`riscdom-backup`** CLI exports the
audit store, snapshots and credentials **as one movable package**, and "the package is the unit of
portability; nothing outside it is required to restore a node's history and identity". This document writes
down what that package holds, how it is made, how it is restored, and what it deliberately does not do. It
is a **specification**; the tool is a later batch.

**Why this is a v1.0 item.** §19's own reason: *being able to leave is what makes it safe to stay.* A node
that cannot be moved is a node an operator is hostage to — so the ability to walk away with the whole thing,
in one file, is a product property, not an afterthought.

## 1. What a node is made of

A node's persistent state is a small, closed set: the files in its data directory plus the entries this
project keeps in the **OS keyring**. Everything a restore needs is on this list; anything not on it is
runtime state that a node rebuilds by starting ([config-schema.md](config-schema.md) §5).

| State | Where it lives | Marker |
|---|---|---|
| Settings | `<data-dir>/settings.json` | `version` = **2** |
| Audit store | `<data-dir>/audit.db` (SQLite, **WAL**) | `PRAGMA user_version` = **1** |
| Session store | `<data-dir>/sessions.db` (SQLite) | `PRAGMA user_version` = **1** |
| Control-plane token | `<data-dir>/token` | none — one line of hex, shape-checked |
| Node identity | `<data-dir>/node.key`, **or** the OS keyring | `schema_version` = **1** |
| Peer table | `<data-dir>/peers.json` | `schema_version` = **1** |
| Room table | `<data-dir>/rooms.json` | `schema_version` = **1** |
| Snapshots | `<data-dir>/snapshots/` | — |
| Provider API keys | the **OS keyring**, one account per executor | — |
| In-network server tokens | the **OS keyring**, one account per host | — |

[api-compatibility.md §6](api-compatibility.md) is the authority for the markers; this table is the same set
seen as *what a backup must carry*. **The keyring is why the package is more than the data directory**: the
API keys and server tokens are not files, and §19's "nothing outside it" means they travel **inside** the
package.

## 2. Export

**One command, one file.** `riscdom-backup export` reads the state in §1 and writes **a single file**,
conventionally `riscdom-backup-<node_id>-<timestamp>.rdbak`.

- **The package is encrypted, and the tool never holds the key.** Export takes a **passphrase from the
  operator** and writes the package under it (an authenticated cipher over the archive). The passphrase is
  **never written to disk by the tool, never placed on a command line, and never printed** — the same rule
  the rest of the project keeps for credentials. An unencrypted package is not offered: it would put an
  Ed25519 private key and a set of API keys in one plain file, which is the opposite of what a backup is for.
- **The package holds, under the encryption:** every file in §1 that exists, laid out as the data directory
  is (so a restore is a matter of putting things back where they came from), plus the keyring entries §1
  names. **A manifest** at the package's root lists every entry with its size and a content hash, and records
  the node's `node_id`, the moment of export, and each format's marker — so a restore can say what it is
  holding before it writes anything.
- **The audit store is copied consistently, not byte-for-byte.** `audit.db` is **WAL** and opened by several
  processes at once, so a byte copy of the file alone can miss frames still in `-wal`
  ([api-compatibility.md §6](api-compatibility.md) says exactly this, and is why the audit store has no
  `.bak`). Export therefore takes the store through SQLite's own consistent path (a backup image or
  `VACUUM INTO`), never a raw file copy. **A package whose audit store is torn is worse than no package**,
  because it looks like history and is not.
- **Migration escape hatches are not carried.** `settings.json.bak` and `sessions.db.bak` are the bytes a
  migration kept for the moment it went wrong; they are not state a restore needs, and the package leaves
  them out.
- **Nothing is read that a node would not read.** Export reads the data directory and the keyring and nothing
  else; it does not dial out, and it needs no network.

## 3. Import

**The package is restored into a data directory**, and the node then starts on it.
`riscdom-backup import --from <file> --into <data-dir>`.

- **It refuses to overwrite silently.** Import into a directory that already holds a node's state is refused
  unless the operator says so explicitly; §19's package is a **move**, and a move that quietly replaces a
  node is how two histories become one.
- **A newer format is refused, not guessed.** Restoration targets a node **of the same version or older**:
  each format's marker is checked exactly as [api-compatibility.md §6](api-compatibility.md) prescribes, and
  a file from *after* the running node is refused with `data_too_new` — nothing read, nothing written. That
  rule is why the manifest records the markers: a restore can tell the operator "this package is from a
  newer node" before it touches anything.
- **Credentials go back to the keyring.** The API keys and server tokens in the package are re-entered into
  the OS keyring on the target machine, under the same account names §1 names; they are never written into
  the data directory as files (which is the mistake [decisions §6](decisions.md)'s keyring rule exists to
  prevent). `node.key` is written to the data directory if that is where the source node kept it, and to the
  keyring if that is where it kept it.
- **The node's identity comes back with it.** Because `node.key` travels in the package, a restored node is
  **the same node** the network knew — the same `node_id`, the same fingerprint — not a new one that merely
  holds old data.

## 4. The portability unit is the whole node

§19's impact is absolute on purpose: **the package is all-or-nothing**. There is no selective export ("just
the audit log", "just the settings"), because a partial package would violate "nothing outside it is
required" — a node restored from part of itself is a node with a history that does not match its identity.
An operator who wants a piece of the data uses the tools that already exist for reading it
([audit/README.md](../audit/README.md), the control plane's read routes), not a half-backup.

## 5. How a backup is checked

- **The manifest is the check.** Its per-entry hashes let an import verify the package is intact before it
  writes anything, and its recorded markers let it verify the package is not from a newer node. A failure is
  reported, never worked around.
- **The audit chain verifies itself once restored.** The history's own integrity is the chain's business:
  `verify_chain` locates the first broken event ([audit/README.md](../audit/README.md)), so a restore can end
  by confirming the chain is whole — an independent check the package's own hashes cannot give.
- **A dry-run first.** Import can report the manifest, the node identity and the format markers **without
  writing**, so an operator sees what they are about to restore before they restore it.

## 6. What is not covered

- **No incremental or differential backup.** One package is the whole node (§4); a second package is another
  whole node. Anything smarter is a later decision, not an assumed feature.
- **No scheduling, and no destination.** §19 describes a **tool the operator runs**, producing a file the
  operator puts somewhere. The node never backups itself on a timer and never uploads anywhere — sending a
  node's credentials to a service would be the project operating a service, which [roadmap §1](roadmap-v1.0.md)'s
  red line forbids.
- **No key escrow, and no passphrase recovery.** The passphrase is the operator's; the tool cannot recover
  it, and a lost passphrase means an unreadable package — which is the point of encrypting it.
- **No promise about a running node's live state.** §2 makes the *stores* consistent; it does not freeze a
  VM mid-instruction or capture in-flight work. Snapshots travel as the files they are; a live VM's memory is
  the sandbox's business, not the backup's (see [performance-budget.md](performance-budget.md) for the
  snapshot's own budget).
- **The tool itself** — its exact flags, its cipher choice and its file extension — is the implementation
  batch's, written against this document.
