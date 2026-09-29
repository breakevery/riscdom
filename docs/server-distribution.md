[中文](server-distribution.zh-CN.md) | English

# Distributing the two server programs

**Status** v1.0 specification (M7b-1) ｜ **Date** 2026-09-29 ｜ **Audience** whoever builds, ships or
unpacks a RiscDom server package.

**What this document is.** RiscDom ships two programs that a **deployer** runs: the
**single-node control plane** (`riscdom-server`) and the **connection layer's server**
(`riscdom-relay`). This document says what each **package** holds, how one is built, and what neither
package carries. It is a **specification**; the packaging script lands with it (M7b-1), while CI
packaging and the release act are later batches (M7b-2, M7b-3).

## 1. The single-node control plane

`riscdom-server-<version>-<platform>` holds:

| Entry | What it is |
|---|---|
| `riscdom-server` (`.exe` on Windows) | The control plane: HTTP + SSE over the host's kernel facade ([server/README.md](../server/README.md)). |
| `web/` | The built Web UI, served at `/` by `--web-root web` (unauthenticated; the API is not). |
| `README.md` | [server/README.md](../server/README.md) — how to build, run, the endpoints, authentication. |
| `settings.example.json` | A minimal, valid settings document (`{"version": 2}`), for a first run. |

**Run it** with `riscdom-server --bind <addr> --workspace <dir> --data-dir <dir> --web-root web`. The
full flag set is the program's own `--help` (`--bind`, `--workspace`, `--data-dir`, `--web-root`,
`--heartbeat-ms`, `--auth` / `--no-auth`, `--log-level`; exit codes `0`/`1`/`2`).

## 2. The connection layer's server

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

## 3. What neither package carries

- **No credential, ever.** The control plane's bearer token is generated into `<data-dir>/token` on its
  first start, and the relay's Ed25519 key into `<data-dir>/node.key`; both are **minted locally**, and a
  package carries neither. A packer must never copy a data directory in.
- **No data directory at all** — no `settings.json`, `peers.json`, `sessions.db` or `audit.db` travels
  with the software. `settings.example.json` and the two `examples/` files are examples, not state.
- **No QEMU, and no RISC-V GCC.** Both programs are plain Rust binaries. (A control plane still *needs*
  QEMU and a RISC-V GCC to boot guests — but that is the operator's installed toolchain, the same one the
  desktop app uses, not something a package would bundle: see [qemu-setup.md](qemu-setup.md).)
- **No web root in the relay package.** Only the control plane serves pages.

## 4. Building a package

`scripts/pack.sh` (unix) and `scripts/pack.ps1` (Windows) are twins — the same split `gate` and `commit`
keep, so the platform the project verifies on has a native implementation rather than a dependency on an
external `zip`.

```
scripts/pack.sh  [--output-dir <dir>] [--skip-ui-build]
scripts\pack.ps1 [-OutputDir <dir>]  [-SkipUiBuild]
```

The script:

1. builds the release binaries (`cargo build --release -p server --bin riscdom-server` and
   `-p net --bin riscdom-relay`) — a package is always built as **release**;
2. builds the front end (`npm run build` in `ui/`), unless `--skip-ui-build` / `-SkipUiBuild` reuses an
   already-built `ui/dist/app`. It runs `npm run build`, **not** `npm ci`: the gate assumes the frontend
   dependencies are installed, and a packaging script that reached the network would be something else;
3. assembles the two trees and writes **one archive per product** into the output directory;
4. prints both paths and their sizes.

The **version** comes from `[workspace.package] version` in the root `Cargo.toml` — the one place a
release bumps — so a package cannot disagree with the binaries inside it. The **platform** is the host's
own (`win-x64`, `linux-x86_64`, `macos-aarch64`, …).

The default output directory is **`target/dist/`**, which is ignored (`**/target` in `.gitignore`), so
artifacts never enter the repository. **Nothing is signed**, and nothing is published: the script is
offline apart from the build itself.

## 5. Platforms

| Platform | Format | Notes |
|---|---|---|
| Windows | `.zip` | The verified platform; the format the v0.9.9 release used. |
| Linux | `.tar.gz` | Built by the script on a Linux host; CI packaging is a later batch. |
| macOS | `.tar.gz` | Same. |

**CI does not build these packages yet**, and **CI has no Windows runner**: the v0.9.9
`riscdom-server-0.9.9-win-x64.zip` was built on a machine by hand and attached to the release. Turning
that into a CI job — macOS and Linux first, Windows when a runner exists — is
[M7b-2](roadmap-v1.0.md), and it is the batch that closes roadmap §12's open item ("a server zip for
Linux and macOS — v0.9.9 shipped the Windows one only").

## 6. What is not signed

**Nothing here is signed.** There is no Authenticode signature on the Windows binaries, no
notarization on macOS and no package signing anywhere — the same state the desktop installers are in
(RELEASE_NOTES records that the macOS and Linux packages are unsigned and unlaunched). An operator who
needs a signature gets one by building from source and signing it themselves. A signing story would be
its own decision, with its own credentials, and is not assumed here.

## 7. What is not covered

- **CI packaging** — [M7b-2](roadmap-v1.0.md): a tag-gated job that builds these archives on macOS and
  Linux (Windows needs a runner the project does not have yet).
- **The release act** — [M7b-3](roadmap-v1.0.md): cutting a `v*` tag and attaching the archives to a
  release. It needs its own authorisation, and it is what makes a package public.
- **A `riscdom-backup` package inside these** — the portability tool has its own spec
  ([backup.md](backup.md)); it is not bundled with the servers.
- **The desktop application** — it is distributed as the Tauri bundles the CI `bundle` job produces
  (`.dmg`, `.deb`, `.rpm`, `.AppImage`, and the Windows installers built by hand).
