[中文](known-issues.zh-CN.md) | English

# RiscDom — known issues, unfinished work, and unverified ground

> **Where this comes from.** Nothing here is a fresh opinion. Each entry is lifted from the
> project's own records — [handoff.md](../handoff.md) §1, [roadmap-v1.0.md](../roadmap-v1.0.md)
> §12 and §15, [cross-chain-verification.md](../cross-chain-verification.md) §6–§7, and
> [README.md](../../README.md)'s known limitations. If a fact is uncertain, this page marks
> it uncertain rather than smoothing it over. The version is `v1.0.0`.

## 1. Not finished

- **M8-4 is not done.** The control plane was extracted to its own repository in **M8-4a**
  (`riscdom-server`), but three batches remain: **M8-4b** (the desktop repository), **M8-4c**
  (this repository's close-out — deleting `server/`, trimming the packaging scripts to the
  relay only, renaming the CI job, redirecting the docs), and **M8-4d** (reconciliation of
  the three repositories' lock files). Consequence today: **`server/` is still a crate in
  this repository**, because `ui/src-tauri` depends on it by path and `ui/` does not leave
  until M8-4b.
- **The cross-region level of M is deferred to v1.x** (roadmap §13; decisions §157). The
  reference M under `examples/` reads one **workgroup** (`--level lan`); the level above it
  — a list of LAN Ms and a `--config` file — is out of v1.0's scope.
- **The `--follow` / `--wait` rough edge** (roadmap §12): the stream may still be open when
  the process exits. `--follow` terminates when the run does, but the seam is not closed.
- **The key-event push has no reader** (cross-chain §7): since M4e-2 a chain fork is *also*
  pushed to the server as a key event, so the aggregation role hears it at once — and **no
  route and no command read that log**. Today the push reaches nobody outside the server's
  memory, and the only surface a deployer has is the node's own chain. Recorded as a known
  gap, explicitly **not** a working notification path.
- **Cross-chain pieces deliberately absent** (cross-chain §2, §6): **no conflict rule**, **no
  range proof** on the centre's own summary chain (a row per change, not a second chain),
  and **no endpoint lists segments at all** — a fork is read out of the chain like every
  other fact. `ChainDigest` stays a commitment to a head.
- **Observability is specification only** (handoff §1, M7g / decisions §115):
  [observability.md](../observability.md) writes down the structured-log fields and the
  `/metrics` route, but **the writer and the route land later** — the route arrives with the
  first consumer. There is no metrics endpoint at `v1.0.0`.
- **The liveness protocol defines no removal** (connection.md §6.7): a judgement records
  `judged_at_ms` and writes `peer_offline` / `peer_recovered`, but **the kick is the
  deployer's** — the protocol itself has no removal step.
- **Python as a guest language is absent** (roadmap §12): C, Zig and Rust exist.
- **The session database's WAL mode is deliberately not set** (decisions §54), unchanged in
  v1.0.
- **The GUI switch's click target** (roadmap §12): a switch is a bare ~14 px checkbox; the
  planned replacement with a proper whole-row `Toggle` is still open.

## 2. Known flaky

- **The QEMU and gate parallel flakes** are the project's oldest open item (roadmap §12):
  the QMP `10054` family and the port-race hang. They are **reported every time and never
  papered over** — which is why this page says so instead of hiding it.
- **In the gate run behind [test-evidence.md](test-evidence.md)**, the `net` relay test
  `a_frame_reaches_the_destination_through_the_server` reported `got Direct`: the frame took
  the **direct** path where the test expected the relay. That is a timing/port race in the
  fixture, not a functional change — the same family as the item above.

## 3. Not verified on real machines

- **macOS and Linux packages are built by CI but unsigned and never walked.** README's known
  limitations say so plainly: QMP over a Unix socket is **still not implemented** (TCP only).
  The golden path is verified on **Windows only**.
- **The clean-machine walk has not been done for v1.0.** `manual-acceptance.md` asks for
  somebody who did not write the code, on a machine that has never had RiscDom; that walk is
  not recorded.
- **Layer 8 (the board on a phone) and layer 9 (a desktop connected to another node) cannot
  be walked without a phone and a second node.** `manual-acceptance.md`'s pass criteria say
  layers 1–7 must pass, and treat 8–9 as new.
- **"The first restart stops the local board"** — an unexplained observation from the v0.9.9
  walk: a first relaunch came up on the local board with the remote address gone. It did not
  reproduce once stale developer processes were cleared, and **no code path explains it**.
  Layer 9 is what settles it.
- **The `v1.0.0` tag itself** was cut on Windows; the release assets (installers, server
  archives) are produced by CI but only the Windows walk is recorded.

## 4. Technical debt

- **Three CLI end-to-end tests run at reduced coverage** (v1.0 batch DT / M8-4a). The CLI
  became a pure client and starts no server of its own, so `cli/tests/control.rs`'s
  end-to-end tests need a real `riscdom-server` binary; when one is not built beside the
  test binaries (or given via `RISCDOM_SERVER_BIN`) **they skip** rather than fail. **M8-4d
  is where the cross-repository integration returns.** On a machine that has built the
  workspace — as this one did for [test-evidence.md](test-evidence.md) — they do run.
- **`server/` still lives in the kernel repository** until M8-4c (see §1): a consumer
  (`ui/src-tauri`) outlives the crate here, so the crate cannot leave first.
- **A stale documentation line**: `docs/README.md`'s `RELEASE_NOTES.md` row still names
  "v0.9.9" as the newest release while `RELEASE_NOTES.md` has been rewritten for v1.0. Noted
  here rather than silently fixed.
- **The desktop repository (`riscdom-adminapp`) does not exist yet**, and
  `multi-repo.md` §2's example (a management program taking `server` *at the kernel's tag*)
  no longer holds now that `server` left the kernel; the corrected rule is that adminapp
  pins **`riscdom-server`'s own tag**.

## 5. What is *not* an issue

For balance, and to avoid a reader mistaking openness for breakage:

- The **audit chain's core** — `compute_hash`, `verify_chain` and the append-only triggers —
  is untouched across the whole v1.0 line.
- The **API freeze** is a deliberate boundary, not a missing feature: `/v0/` is the v1.0
  path prefix, and change is gated by [api-compatibility.md](../api-compatibility.md).
- The **items above that are `[open]`** in the roadmap are open **on purpose** — recorded
  so a later discussion does not have to rediscover them.
