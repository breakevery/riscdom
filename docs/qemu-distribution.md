[中文](qemu-distribution.zh-CN.md) | English

# Getting QEMU to the user: bundle or download (v0.4 #4)

> **Status: decision recorded in §5; no code in this batch.** §4 lists licence facts and the
> obligations the licence text attaches to a distributor; **it is not a legal opinion.** What §5
> decides is the *distribution model*, not the licence question.

## 1. Where users stand today

- **You install QEMU yourself.** The app discovers it — `RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64` →
  well-known install locations → `PATH` — or you point it at a specific binary in *Settings →
  Toolchain*, which is remembered in `settings.json`. Documented in `docs/qemu-setup.md`.
- **The preflight checks it really works** (v0.4 batch 3): GCC runs and compiles a four-line guest,
  QEMU runs, the guest boots and its banner arrives. That covers "is it there, and does this pair
  actually run" — including the failures people hit (long/space-bearing paths, a broken binary).
- **What is not covered**: nothing downloads QEMU, nothing tells a user which QEMU to install beyond
  "verified with 11.1.0", and there is no version-compatibility matrix (deliberately — batch 3 shows
  why: none exists in this repository to rely on).

## 2. The pattern we already have for GCC

`host/src/toolchain_download.rs`, shipped in v0.3.0, is a complete, reusable template:

- a **pinned upstream version** (`XPACK_RISCV_GCC_VERSION = "15.2.0-1"`) and a pinned release base URL;
- **per-platform** asset names and **pinned SHA-256** for each (win32-x64, darwin-x64, darwin-arm64,
  linux-x64, linux-arm64);
- archive handling with the platform's format (zip on Windows, tar.gz elsewhere), **Zip-Slip guarded**
  extraction, a cancellation flag, progress events, and a re-run that is idempotent;
- installation into the app data directory, followed by adopting the result as the active toolchain.

Everything QEMU would need on the *download* side is a variation of this file: another pinned version,
another set of per-platform URLs and digests, the same guards. That is a fact about the codebase, and
it is the main input to the recommendation in §5.

## 3. The two options, technically

| | Bundle QEMU in the installer | Download QEMU on demand |
|---|---|---|
| **Installer size** | Grows by the size of the QEMU build(s) for every platform we ship. The figure must be measured per build; it is much larger than our current few MB (our GCC download is described in-app as ~200 MB, which is the scale of the *optional* payload today). | Unchanged. The payload is fetched from upstream when the user asks for it. |
| **First-run experience** | Works offline out of the box; nothing to install. | Needs one download (network) before the first boot; until then the app says what is missing (as today). |
| **Offline / air-gapped use** | Best. | Needs a one-time download (or a manual install, which is supported today and stays supported). |
| **Update velocity** | Bumping QEMU means shipping a new app release (and every platform build). | Bumping QEMU means changing one pinned version + digests in our code, still an app release, but no re-packaging of binaries. |
| **Platform coverage** | Each platform needs its own QEMU build in the installer; we currently support Windows only. | Same coverage question, but the payload is per-platform at fetch time. |
| **What we ship** | A GPL-2.0 binary (see §4). | Our own code; the binary comes from upstream to the user's machine. |
| **Failure modes** | A broken/mismatched QEMU inside our installer becomes *our* bug report. | Upstream availability and checksum drift become the failure modes; both are handled by pinning + verification, as the GCC download already does. |

A third option — neither bundling nor downloading, but pointing the user at an install they run
themselves — is what §5 decides.

## 4. Licence facts (no conclusions)

Facts, each of which should be verified against the exact build before acting:

- **QEMU is distributed under GPL-2.0**, with some components under other licences (its own
  `COPYING`/`LICENSE` files are the authority for a given build).
- **GPL-2.0 attaches obligations to whoever *distributes* the binary.** The licence text's own
  requirements for distributing a verbatim binary include, as facts about the text: shipping the
  licence text and conspicuous notices; accompanying the binary with the **corresponding source**
  (or a written offer to provide it); not imposing further restrictions on the recipient's rights
  under the licence; and the warranty/liability disclaimer. Distributing a *modified* QEMU would
  additionally require those modifications to be licensed under the GPL.
- **If we bundled QEMU, our installer would contain that binary**, so those obligations would be
  attached to the act of distributing our installer. The specifics — what counts as the
  "corresponding source" for a Windows build, how bundling interacts with an Apache-2.0 application
  licence, and how a separate-process (not linked) relationship is classified — are questions this
  document does not answer.
- **If we download at runtime instead**, our installer contains no QEMU bits: the binary goes from
  upstream to the user's machine. Whether that is "distribution" by us, and therefore whether any of
  the obligations above attach, is a legal classification, not a technical one.
- **Our own position today**: the repository is Apache-2.0 (`LICENSE`), the app links no QEMU code,
  and QEMU runs as a separate process we start and talk to over stdio/TCP. Those facts are now written
  down where a user can find them: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) states the
  licences of QEMU and of the downloaded RISC-V GCC, and how we do — and do not — use them.

## 5. Decision

**Guide, do not download: send the user to `winget` or the official download page.**

The three ways to get QEMU onto a user's machine, and where each one stands:

- **Bundle it in the installer — rejected.** New packaging work on every platform for a payload that
  is not ours, a bigger installer, and the obligations of distributing a GPL-2.0 binary (§4).
- **Download it at runtime — dropped while implementing v0.4 #4.** A download spec is a URL *plus* a
  SHA-256, and neither may be guessed. The upstream release publishes **source**; the official
  download page this repository already sends Windows users to
  (`https://www.qemu.org/download/#windows`) offers Microsoft's `winget` package and third-party
  installers. Pinning a third-party packager's installer would put that packager into our supply
  chain silently, and a digest nobody can reproduce is worse than no download at all.
- **Guide the user — chosen.** RiscDom says what to install and how — `winget install
  SoftwareFreedomConservancy.QEMU` when `winget` is present, the official download page when it is
  not — and then does what it already does: find the install, remember a manual path, and let the
  preflight prove the toolchain × QEMU pair really boots a guest.

Why guiding, and not the other two:

1. **Upstream publishes no Windows binary**, so "download it from the official release" has nothing
   to point at. That is what v0.4 #4 ran into.
2. **A third-party packager is a supply-chain link.** Adding one by pinning its installer is a
   decision a build script should not make on its own.
3. **Building QEMU ourselves would make us the distributor of a GPL-2.0 binary** (§4), with the
   obligations that follow.
4. It is the smallest change: discovery, the manual path and the preflight already exist, and the
   user ends up with a QEMU that their own package manager keeps updated.

The downloader built for v0.4 #4 stays in the tree, unwired and refusing to run
(`host/src/qemu_download.rs`, spec table empty — see `docs/qemu-setup.md` §3). It is kept because the
machinery is written and tested, and a future distribution channel might justify it; nothing calls
it today.

Bundling and mirroring stay unplanned. Revisit bundling only if a distribution channel makes a
user-run install impossible (e.g. a store that forbids it), and only with a licence review.

## 6. Phasing

1. **Now (done):** discovery + manual path + the environment preflight, plus guided install (§5). A
   user with QEMU installed is fully served, and a user without it is told exactly what to run.
2. **Kept, unwired:** the downloader machinery (`host/src/qemu_download.rs`) — written and tested
   against a local server, spec table empty because no build may be pinned (§5). Nothing calls it.
3. **Not planned:** bundling QEMU into the installer, and mirroring the archives ourselves.
