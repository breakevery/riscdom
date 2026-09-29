[中文](multi-repo.zh-CN.md) | English

# Working across the three repositories

**Status** v1.0 specification (M7i) ｜ **Date** 2026-09-29 ｜ **Audience** contributors, and whoever prepares the
repositories.

**What this document is.** [roadmap §11](roadmap-v1.0.md) says v1.0 needs "the **CONTRIBUTING** additions a
second repository needs", and does not list them. This document is those additions: how the other
repositories — the management program `riscdom-adminapp` and the control-plane program `riscdom-server` —
stand next to this one, how they get the kernel, what they inherit and what they have to own. It is a
**specification** for the split; the split itself is [M7a](roadmap-v1.0.md), which runs after v1.0 (§7).

## 1. Three repositories, one kernel

- **This repository is the kernel and the control plane**: the sandbox, the audit chain, the agent loop, the
  network layer, the host core and the HTTP control plane — the crates in the workspace's `members` except
  `host-tauri`: `cli`, `host-core`, `sandbox`, `audit`, `agent`, `worker`, `server`, `net`, `backup` and
  `sdk/rust`.
- **`riscdom-adminapp` is the management program**: the Tauri shell and its front end (`host-tauri` and
  `ui`, which embeds `server` for the control plane a phone reaches). [RELEASE_NOTES.md](../RELEASE_NOTES.md)
  says it plainly — the program ships **inside this repository** in v0.9 and **becomes its own repository at
  v1.0** (that is, after the v1.0 release, §7); [decisions §9](decisions.md)'s impact says the same.
- **`riscdom-server` is the control plane as a program**: the `server` crate (the HTTP + SSE control plane)
  as its own repository, so a host can install and run it without the desktop program. It stays in this
  repository for now (§7).
- **The relationship is the kernel's, not a fork's.** [architecture-evolution §12](architecture-evolution.md)
  calls a split repository "a separate repository, **maintained from the same source** (like Linux's
  coreutils / iproute2)". Each program is a **kernel-level tool**: it is one consumer of a kernel it does not
  own, advancing in step with the kernel's features — which is why §12 also says every kernel capability must
  have a management API, and a capability a program cannot reach is decoration.
- **The split waits for v1.0.** Both programs consume the kernel as a dependency pinned to a **tag** (§2),
  and this repository has no v1.0 tag yet, so the split runs after that release; §7 records the timing and
  the order.

## 2. How the second repository gets the kernel

**A git dependency pinned to a tag.** The second repository's `Cargo.toml` names the crates it uses
**directly**, each as the same `{ git = …, tag = … }`:

```toml
[dependencies]
# One line per crate the management program actually uses. No more.
host-tauri = { git = "https://github.com/breakevery/riscdom", tag = "v1.0.0" }
server     = { git = "https://github.com/breakevery/riscdom", tag = "v1.0.0" }
```

- **The tag is cut at v1.0, and there is one today only for v0.9.x.** `v1.0.0` above is the tag this
  repository will cut at its v1.0 release (§7); until it exists there is nothing to pin, so **the split does
  not execute before then**. A repository that pinned today would have to pin `v0.9.9`, which predates the
  connection layer, the backup tool and the SDK — an older kernel than the programs that consume it. The
  list is one line per crate: `riscdom-adminapp` names `host-tauri` and `server` (it embeds the control
  plane); `riscdom-server` names `server` alone.
- **The indirect crates come along, at the same revision.** A kernel crate's own dependencies are `path`
  dependencies inside this repository (`host-tauri` → `host-core` / `net`; `ui` → `host-tauri` / `server`),
  and Cargo resolves a path dependency that lives inside the same git repository against **that same
  checkout**. So when the second repository names `host-tauri` at `tag = "v1.0.0"`, it also gets the
  `host-core` and `net` that `v1.0.0` pins — one tag, one revision, no second list to keep in step.
- **`Cargo.lock` is committed, and it is the real pin.** A lock entry records the resolved commit, not just
  the tag:

  ```text
  [[package]]
  name = "host-core"
  version = "0.9.9"
  source = "git+https://github.com/breakevery/riscdom?tag=v1.0.0#<commit>"
  ```

  A tag can be moved; a committed lock cannot be changed without a deliberate edit, so the build stays on the
  revision that was tested even if the tag later points elsewhere. **Commit the lock** — that is what makes
  the pin a pin.
- **The tag is cut here, at this repository's release.** Tags are `vX.Y.Z` (`v0.9.9` today), created when this
  repository is released; the second repository moves to a new tag when it chooses to. It never tracks
  `main`: an unstable kernel under a product is exactly what a tag exists to avoid.
- **Why not crates.io — yet.** Publishing to a registry is a commitment (a name, a stability promise, a
  yank story) and it is **not needed** for the split. Keeping the option open costs nothing and is additive:
  moving from `{ git = …, tag = … }` to a versioned dependency later changes one line per crate and nothing
  else. The decision is deferred to when v1.0's commercial side starts ([decisions §9](decisions.md) makes
  the management program the commercial edge, so the kernel's publication is a commercial question).
- **Why not a vendored subtree.** Copying the kernel into the second repository breaks §12's "maintained from
  the same source": a copy drifts, and two kernels pretending to be one is the state the split exists to
  avoid.

## 3. What the second repository inherits

Inherited means: the same discipline applies, and its own tooling must be built to keep it.

| This repository's rule | Applies to the second repository? |
|---|---|
| **Bilingual docs** ([CONTRIBUTING §Documentation](../CONTRIBUTING.md)) — every `X.md` paired with `X.zh-CN.md`, a switcher line on the first line | **Yes, as a convention to adopt.** The rule exists because this project publishes both languages; the second repository's documents are its own, so it must maintain the same pairing and switcher, and it needs its own bilingual check. |
| **The gate** — one source of truth, `scripts/gate.sh`, and nothing checked only in CI | **Yes as a shape, no as a script.** It cannot run this repository's gate (that gate checks this workspace); it must have its own gate that mirrors the discipline — one command list, CI runs the same one. |
| **Commit discipline** — gated commits, ASCII commit messages (the `-m` path is lossy on Windows) | **Yes, and it needs its own scripts.** The rules are conventions of the `scripts/commit.*` and `scripts/preflight.*` this repository ships; the second repository reproduces them rather than reaching across the repository boundary. |
| **Encoding rule** — never read or write a source file with PowerShell; UTF-8 without a BOM | **Yes.** The damage the rule prevents is a property of Windows PowerShell, not of this repository. |
| **Secret scanning** — a CI job that scans the full history | **Yes.** Its own repository needs its own `secrets` job; a shared history is not scanned by one repository's job. |

**What it does *not* inherit:** this repository's `Cargo.lock`, its `Cargo.toml` workspace, its `.gitignore`,
its licences as files, and its gate's crate list. The kernel is consumed as a **dependency**, and the kernel
is checked by **this** repository's gate — the second repository checks its own crates, not a copy of the
kernel's.

## 4. What the second repository needs of its own

1. **Its own gate** — a `gate.sh` (and a `.ps1` twin, if it keeps Windows as the verified platform) with one
   command list, invoked identically by a CI `gate` job.
2. **Its own build and bundle** — the Tauri bundling this repository's CI `bundle` job already models
   (dmg on macOS; deb / rpm / AppImage on Linux; admission by a version tag or a manual dispatch).
3. **Its own release flow** — it publishes the management program, on its own schedule, against a pinned
   kernel tag (§2).
4. **Its own CLA provisioning** — see §5.
5. **Its own documentation map** — a `docs/README.md` of its own, describing its own documents; the switcher
   and pairing rules of §3 apply to it.
6. **Its own secret-scanning job** (§3).

## 5. The CLA across repositories — [open]

**The CLA is written for one repository, and that is deliberate.** [CLA.md](../CLA.md) §1 defines "Project"
as "**the RiscDom repository** and the work distributed from it" — singular — and the signature store
([`signatures/version1/cla.json`](../signatures/version1/cla.json)) and the CLA Assistant workflow
([`.github/workflows/cla.yml`](../.github/workflows/cla.yml)) both live here. [CONTRIBUTING.md](../CONTRIBUTING.md)
already says so out loud: contributions "may be taken in somewhere other than this repository in the future,
so this section speaks only for the flow that exists here today".

**What is undecided, and when it is decided.** Whether each new repository installs its own CLA Assistant
and signature store, shares this one, or the CLA text is amended to name all three is **not decided here** —
it is decided with the split, in [M7a](roadmap-v1.0.md), which runs after v1.0 (§7). Until then the rule for
a contributor is unchanged: **a contribution to this repository is covered by this repository's CLA**, and
nothing in this document extends it.

## 6. What this document is not

- **It is not the split runbook.** Creating the repositories, moving the crates, cutting the first tag
  and wiring the CI is [M7a](roadmap-v1.0.md) — which runs after v1.0 (§7) — and needs its own
  authorisation (it writes to new remotes).
- **It is not the release mechanics.** How this repository's server and packages are released, and how the
  second repository's are, is [M7b](roadmap-v1.0.md) and the release notes, not this.
- **It is not an SDK contract.** The types a third-party consumer sees are [M7c / M7d](roadmap-v1.0.md)'s
  job; the git dependency in §2 is the kernel's own crates, not a published interface.
- **It does not restate the CLA or CONTRIBUTING.** Where those documents already say something, this one
  points at them rather than copying them, so there is one place to change.

## 7. The three repositories, and when they split

**M7a — the split — is deferred to after v1.0.** [roadmap §11](roadmap-v1.0.md) settles that the management
program moves to its own repository at v1.0, and [M8](roadmap-v1.0.md) is "the API freezes, and it ships":
the freeze is declared and v1.0 is released. The order is **the v1.0 release first, then `riscdom-server`,
then `riscdom-adminapp`** — each new repository pins a kernel tag (§2), so the kernel has to have one, and a
program split out before the freeze would be pinned to a kernel still moving under it.

What each new repository is for (recorded here; neither is implemented in v1.0):

- **`riscdom-adminapp` — the management program.** Today a **desktop** application (Tauri + React + Vite,
  `host-tauri` + `ui`). It goes on to **mobile (Android / iOS)** and keeps its **browser** mode; all three
  connect to both a **RiscDom node** and a **`riscdom-server`**.
- **`riscdom-server` — the control plane as a program.** Today a **command line** (`riscdom-server`, plus the
  connection layer's `riscdom-relay`). It gains a **web status page** and ships for **Windows and Linux**. It
  serves **RiscDom only**: it is not a general-purpose server-management panel.

**The relay stays here.** `net/src/bin/riscdom-relay.rs` depends only on `net` — it imports `net`'s types and
nothing from `server` — and `net` stays in this repository, so the relay binary splits with neither new
repository: it is a connection-layer program, not a control-plane one.
