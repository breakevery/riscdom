[中文](server-distribution.zh-CN.md) | English

# Distributing the relay

**Status** v1.0 specification (M7b-1; relay-only since v1.0 M8-4c) ｜ **Date** 2026-09-29 ｜ **Audience**
whoever builds, ships or unpacks the RiscDom relay package.

> **The control plane is not here.** `riscdom-server`, the single-node control plane, left this
> repository in **v1.0 M8-4a** and is packaged in its own repository —
> <https://github.com/breakevery/riscdom-server>, whose `docs/server-distribution.md` is the authority
> for that package. This document now covers the one program this repository still ships: the
> **connection layer's server**.

**What this document is.** RiscDom ships a program that a **deployer** runs: the **connection layer's
server** (`riscdom-relay`). This document says what its **package** holds, how one is built, and what
it does not carry.

## 1. The connection layer's server

`riscdom-relay-<version>-<platform>` holds:

| Entry | What it is |
|---|---|
| `riscdom-relay` (`.exe` on Windows) | The cross-region server ([net/README.md](../net/README.md), [connection.md](connection.md) §6). |
| `README.md` | [net/README.md](../net/README.md) — the connection layer it belongs to. |
| `examples/peers.example.json` | An empty peer table (`{"schema_version": 1, "peers": []}`). |
| `examples/rooms.example.json` | An empty room set (`{"schema_version": 1, "rooms": []}`). |

**Run it** with `riscdom-relay --data-dir <dir> --bind <addr> --node-id <name>`. There is **no default
bind**, deliberately: naming one would be the project naming where a server is
([connection.md](connection.md) §6.1). A data directory with neither file is a server that knows nobody
and publishes nothing — honestly, rather than conveniently.

## 2. What the package does not carry

- **No credential, ever.** The relay's Ed25519 key is minted into `<data-dir>/node.key` on its first
  start; a package never carries it, and a packer must never copy a data directory in.
- **No data directory at all** — no `settings.json`, `peers.json`, `sessions.db` or `audit.db` travels
  with the software. The two `examples/` files are examples, not state.
- **No QEMU, and no RISC-V GCC.** The relay is a plain Rust binary; it boots no guest.
- **No web root.** The relay serves no pages. (The control plane does — that package is
  `riscdom-server`'s, see the note at the top.)

## 3. Building a package

`scripts/pack.sh` (unix) and `scripts/pack.ps1` (Windows) are twins — the same split `gate` and
`commit` keep, so the platform the project verifies on has a native implementation rather than a
dependency on an external `zip`.

```
scripts/pack.sh  [--output-dir <dir>]
scripts\pack.ps1 [-OutputDir <dir>]
```

The script:

1. builds the release binary (`cargo build --release -p net --bin riscdom-relay`) — a package is
   always built as **release**;
2. assembles the tree and writes **one archive** into the output directory;
3. prints the archive's path and its size.

The **version** comes from `[workspace.package] version` in the root `Cargo.toml` — the one place a
release bumps — so a package cannot disagree with the binary inside it. The **platform** is the host's
own (`win-x64`, `linux-x86_64`, `macos-aarch64`, …).

The default output directory is **`target/dist/`**, which is ignored (`**/target` in `.gitignore`), so
artifacts never enter the repository. **Nothing is signed**, and nothing is published: the script is
offline apart from the build itself.

## 4. Platforms

| Platform | Format | Notes |
|---|---|---|
| Windows | `.zip` | The verified platform. |
| Linux | `.tar.gz` | Built by the script on a Linux host; the tag-gated CI job builds it too. |
| macOS | `.tar.gz` | Same. |

**The tag-gated CI job builds this archive** — `relay-bundle` in
[`.github/workflows/ci.yml`](../.github/workflows/ci.yml), which runs the packer when a `v*` tag is
pushed (batch BF; renamed from `server-bundle` in v1.0 M8-4c, when the control plane left). **CI has no
Windows runner**, so the Windows `.zip` is built on a machine by hand — that is M7b-4.

## 5. What is not signed

**Nothing here is signed.** There is no Authenticode signature on the Windows binary, no notarization
on macOS and no package signing anywhere — the same state the desktop installers are in (RELEASE_NOTES
records that the macOS and Linux packages are unsigned and unlaunched). An operator who needs a
signature gets one by building from source and signing it themselves. A signing story would be its own
decision, with its own credentials, and is not assumed here.

## 6. What is not covered

- **The control plane's package** (`riscdom-server-<version>-<platform>`, with its built front end) — it
  belongs to <https://github.com/breakevery/riscdom-server> and is documented there.
- **Windows CI packaging** — [M7b-4](roadmap-v1.0.md): no Windows runner is wired up yet.
- **The release act** — cutting a `v*` tag and attaching archives to a release needs its own
  authorisation, and it is what makes a package public.
- **A `riscdom-backup` package inside this one** — the portability tool has its own spec
  ([backup.md](backup.md)); it is not bundled with the relay.
- **The desktop application** — it is distributed as the Tauri bundles
  [`riscdom-adminapp`](https://github.com/breakevery/riscdom-adminapp) produces (`.dmg`, `.deb`, `.rpm`,
  `.AppImage`, and the Windows installers built by hand).
