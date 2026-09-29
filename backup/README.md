[中文](README.zh-CN.md) | English

# backup

`riscdom-backup` — the **portability tool** (v1.0 M7e), as [docs/backup.md](../docs/backup.md)
freezes it. [decisions §19](../docs/decisions.md) settles why it exists: *being able to leave is what
makes it safe to stay* — the package it writes **is** the unit of portability.

**What is here today: the data directory, sealed into one file (batch AX / AV-1).** `export` reads a
node's data directory — `settings.json`, `sessions.db`, `token`, `node.key`, `peers.json` and
`rooms.json` ([backup.md](../docs/backup.md) §1.1) — writes a **manifest** naming each file with its
size, its SHA-256 and its format's marker, and seals the lot into **one encrypted package**: a gzipped
tar (`manifest.json` plus `data-dir/*`) under **AES-256-GCM**, keyed from the operator's passphrase
with **PBKDF2-HMAC-SHA256**. A wrong passphrase and a tampered file give the same answer, because an
authenticated cipher cannot tell them apart and does not pretend to.

**The passphrase never travels the wrong way.** It is not accepted on a command line, is never written
to disk by this tool, and is never printed. It comes from `--passphrase-from-env <VAR>`, or from
**stdin when it is piped**; a terminal prompt is a last resort that prints a warning, because a
terminal echoes what is typed.

**Not here yet (AV-2): the audit store, the snapshots, and the keyring.** `audit.db` and
`<workspace>/.riscdom/snapshots/` are the **second root** ([backup.md](../docs/backup.md) §1.2), and
the keyring entries are the part of §1.4 that has to be **derived from `settings.json`** (the OS
keyring has no enumeration API) and **reported** when it cannot be. Until AV-2 lands, `export` carries
no history, no snapshot and **no credential at all** — and the manifest says so, in its `not_derived`
list, rather than leaving a reader to assume the package is the whole node.

## Use

```
riscdom-backup export --data-dir <dir> --output <path> [--passphrase-from-env <VAR>] [--force]
```

- `--data-dir` — the directory the node keeps its files in (the one `riscdom-server --data-dir` is
  given, or the desktop's app-data directory).
- `--output` — where the package goes, conventionally
  `riscdom-backup-<node_id>-<timestamp>.rdbak`. It refuses to replace an existing file without
  `--force`.
- `--passphrase-from-env <VAR>` — the non-echoing way to hand over the passphrase. Piping it on
  stdin works too: `riscdom-backup export … < passphrase-file`.

Exit codes: `0` success, `2` a usage mistake, `1` a failure (a missing `--data-dir`, an existing
`--output`, a wrong passphrase on read). The command prints the file count and the output path — never
the passphrase, and never a file's contents.

## Layout

- `src/lib.rs` — the export: collect the data directory, read each file's marker, build the tar,
  gzip it, seal it. `export`, `decrypt` and `read_manifest` are the public surface.
- `src/main.rs` — the command line, including the passphrase's three sources.
- `tests/export.rs` — the end-to-end view: a real data directory, one sealed package, the manifest
  read back out.

The crate depends on `host-core` (its data-directory paths) and `agent` (the node's device name, which
is its `node_id`) and on nothing else of this workspace. It does **not** depend on `net`, `server` or
`cli`, so the file names it repeats from those crates are documented next to their definitions.
