[中文](qemu-distribution.zh-CN.md) | English

# Getting QEMU to the user: bundle or download (v0.4 #4)

> **Status: proposal.** No code in this batch. §4 lists licence facts and the obligations the
> licence text attaches to a distributor; **it is not a legal opinion and reaches no conclusion about
> whether any option is compliant.** That call is the project owner's (or a lawyer's).

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
  and QEMU runs as a separate process we start and talk to over stdio/TCP. There is currently **no
  third-party notice file** and no mention of QEMU's (or the downloaded GCC's) licence anywhere in
  the repository — a gap worth closing whichever option is chosen.

## 5. Recommendation

**Download on demand; do not bundle.** The engineering reasons, in order of weight:

1. The machinery already exists and is proven here (§2) — bundling means new packaging work on every
   platform for a payload that is not ours.
2. The installer stays small, and the first-run experience stays honest: the app already tells the
   user exactly what is missing and offers a manual path, which the preflight then verifies.
3. Bundling makes us the distributor of a GPL-2.0 binary, which is a set of obligations we would have
   to take on deliberately; downloading keeps our shipped artifact ours.
4. QEMU versions move faster than our release cadence; a download path lets us follow the pinned
   version by changing one constant.

If bundling is chosen anyway, §4's obligation list becomes a work item, not a footnote: licence text
and notices in the installer, a corresponding-source mechanism for the exact build, and a decision
about how that interacts with our Apache-2.0 terms — reviewed by someone qualified to make that call.

## 6. Phasing

1. **Now (done):** discovery + manual path + the environment preflight. A user with QEMU installed is
   fully served.
2. **Next (proposed for v0.4/v0.5):** a pinned QEMU *download* spec mirroring the GCC one — version,
   per-platform URL + SHA-256, Zip-Slip guard, cancel, progress, install into the app data directory,
   then adopt it like a manual path (and let the preflight confirm it).
3. **Not planned:** bundling QEMU into the installer. Revisit only if a distribution channel makes a
   download impossible (e.g. a store that forbids network fetches), and only with a licence review.
