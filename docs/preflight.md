[中文](preflight.zh-CN.md) | English

# Environment preflight

RiscDom checks that the toolchain and QEMU you configured **actually work together** — by
running them, not by comparing version numbers. The four steps are:

1. the configured RISC-V GCC starts and answers `--version`;
2. it compiles a four-line guest, on the real paths;
3. the configured QEMU starts and answers `--version`;
4. it boots that guest, and the guest's banner arrives on the serial console.

The steps are fail-fast: a later step needs the earlier ones, so a failure names the first
thing that did not work.

## Why not a version matrix

There is no QEMU × GCC compatibility matrix in this repository, and none is guessed at: a
version rule would have to be invented, and a wrong rule is worse than no rule (see
`PROJECT_CONSTITUTION.md` §10, v0.4 item 5). Running the real pair once answers the question
that actually matters — "does this configuration run here" — and it catches the failures
people really hit: a toolchain under a long or space-bearing path, a QEMU that cannot be
spawned, a combination that compiles but never boots.

## When it runs

- after you set a manual toolchain or QEMU path;
- on the first run after the configuration changed — the cached result is bound to the
  configuration fingerprint, so any change invalidates it;
- when you press **Settings → Toolchain → 环境预检 → 重新预检**.

It never runs on every start, and it never blocks the UI: the panel shows each step as it
happens (`preflight:progress`).

## What it does not do

- It never writes to the audit chain: a preflight is an environment check, not a run.
- It never fails a run. A failing preflight is a warning carrying the raw output of the
  failing step and a suggestion; the run itself decides what to do.
- It does not touch the host-owned VM: the preflight guest is booted separately, on its own
  ports, and stopped immediately.

## The escape hatch

When a check fails you can accept the configuration anyway (**仍要继续**). That choice is
recorded in `settings.json` for that configuration, so the warning stops until the
configuration changes again. Accepting does not change the facts: the panel still shows what
failed, and the record says you chose to continue.

## Where it is implemented

- `host/src/preflight.rs` — the steps' vocabulary, the cache shape, and the banner wait.
- `host/src/state.rs` — `preflight_status` / `ensure_preflight` / `acknowledge_preflight`
  (the runner).
- `host/src/commands.rs` — `preflight_status` / `run_preflight` / `acknowledge_preflight`,
  plus the background run triggered by a path change.
- `ui/src/lib/preflightView.ts` — the wording the settings tab shows.
