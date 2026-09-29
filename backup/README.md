[中文](README.zh-CN.md) | English

# backup

`riscdom-backup` — the **portability tool** (v1.0 M7e), as [docs/backup.md](../docs/backup.md)
freezes it. [decisions §19](../docs/decisions.md) settles why it exists: *being able to leave is what
makes it safe to stay* — the package it writes **is** the unit of portability.

**What is here today: the whole node, sealed into one file (batches AX/AY, AV-1 + AV-2).** `export`
reads both roots of [backup.md](../docs/backup.md) §1 and seals them into **one encrypted package**:

- **the data directory** — `settings.json`, `sessions.db`, `token`, `node.key`, `peers.json`,
  `rooms.json` (§1.1);
- **the workspace's `.riscdom/`** — the **audit store** and the **snapshots** (§1.2). The audit store
  is taken through **SQLite's own consistent path** (`VACUUM INTO`), never a byte copy: `audit.db` is
  WAL and multi-process, and a copy of the file alone can miss frames still in `-wal`. The snapshots
  are walked whole, tree and all;
- **the keyring's credentials** (§1.4), **derived** rather than enumerated — the OS keyring cannot be
  listed (`keyring` v3 looks a credential up by service and account), so the account names come from
  `settings.json`: `llm-api-key:<executor_id>:<provider_id>`, the legacy `llm-api-key:<provider_id>`,
  and `remote-token:<host>` from `network.remote_url`.

**Nothing is dropped quietly.** Whatever cannot be named — an account `settings.json` implies but the
keyring does not hold, a `settings.json` this build cannot read — goes into the manifest's
**`not_derived`** list, and the command prints those lines. The list also carries one standing note:
the keyring cannot be listed, so a credential whose executor or host is gone from `settings.json`
**cannot be found**, and [backup.md](../docs/backup.md) §1.4 calls that the package's one declared
outside dependency.

**The archive is sealed** with **AES-256-GCM**, keyed from the operator's passphrase with
**PBKDF2-HMAC-SHA256**; the header (magic, salt, nonce, rounds) is the AEAD's additional data, so
editing it makes the package unreadable rather than merely wrong. A wrong passphrase and a tampered
file give the same answer, because an authenticated cipher cannot tell them apart.

**The passphrase never travels the wrong way.** It is not accepted on a command line, is never written
to disk by this tool, and is never printed. It comes from `--passphrase-from-env <VAR>`, or from
**stdin when it is piped**; a terminal prompt is a last resort that prints a warning, because a
terminal echoes what is typed.

## Use

```
riscdom-backup export --data-dir <dir> --workspace <dir> --output <path> [--passphrase-from-env <VAR>] [--force]
```

- `--data-dir` — the directory the node keeps its files in (the one `riscdom-server --data-dir` is
  given, or the desktop's app-data directory).
- `--workspace` — the directory the node works in; its `.riscdom/` holds the audit store and the
  snapshots. Both roots are required: a backup of one half is not the unit §19 defines.
- `--output` — where the package goes, conventionally
  `riscdom-backup-<node_id>-<timestamp>.rdbak`. It refuses to replace an existing file without
  `--force`.
- `--passphrase-from-env <VAR>` — the non-echoing way to hand over the passphrase. Piping it on
  stdin works too: `riscdom-backup export … < passphrase-file`.

Exit codes: `0` success, `2` a usage mistake, `1` a failure (a missing directory, an existing
`--output`, an audit store SQLite will not export, a wrong passphrase on read). The command prints the
file count, the output path, and every `not carried:` line — never the passphrase, and never a file's
contents.

## Layout

- `src/lib.rs` — the export: collect the two roots, read each file's marker, derive the credentials,
  build the tar, gzip it, seal it. `export`, `export_with` (an injected keyring), `decrypt` and
  `read_manifest` are the public surface.
- `src/main.rs` — the command line, including the passphrase's sources.
- `tests/export.rs` — the end-to-end view: a real data directory and workspace, one sealed package,
  the manifest read back out.

The crate depends on `host-core` (its data-directory paths, its settings types, its keyring) and
`agent` (the node's device name, which is its `node_id`). The audit store needs `rusqlite`, already in
`Cargo.lock`, so no new package is added. It does **not** depend on `net`, `server` or `cli`, so the
file names it repeats from those crates are documented next to their definitions.

**Not here yet:** `import` (restoring a package) is a later batch.
