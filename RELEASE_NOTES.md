[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.9.0

> **This is a release, not a preview — with the same two caveats as v0.8.0.** The **macOS and Linux
> packages are built by CI and have never been launched on a real machine**, and they are **unsigned**
> (macOS Gatekeeper blocks a first launch; Windows SmartScreen warns on the installer). And **Windows
> remains the platform the golden path has been verified on**: the clean-machine walk by somebody else
> still has not happened.

**v0.9 in one sentence:** the project becomes drivable and visible — a control plane with real
authentication and a live event stream, a command-line client that speaks it, a browser board that reads
it — and the sandbox gains two languages (Zig and Rust) beside C, while the multi-agent work arrives as
an **interface** (an executor roster, a dispatch endpoint, a remote handle), not yet as a collaboration
strategy.

## What this release adds

- **The control plane is a real API.** 32 query endpoints and 36 control endpoints, behind a bearer
  token, each route carrying the capability it needs — a caller without it gets a `403`, not a guess. The
  event stream (SSE) replays from `Last-Event-ID` and sends a `gap` frame when the cursor is older than
  the buffer, so a reader knows what it missed instead of silently continuing. A project can be exported
  and imported as one archive, and `POST /v0/tasks` dispatches a task to a configured executor. The
  surface is frozen by two documents whose tables the tests read directly, so an endpoint cannot land
  without its tool definition.
- **A command-line client, `riscdom`.** It speaks either to a running `riscdom-server`
  (`--remote host:port`) or to a control plane started **inside its own process** — one code path, never
  a direct call into the host. `--json` passes the control plane's answer through unchanged, and the
  exit codes are documented (`0` success, `1` local failure, `2` usage, `3` refused or `5xx`,
  `4` unauthenticated or forbidden). `--follow` / `--wait` stream the events of work that is still
  running.
- **Zig and Rust compile in the sandbox, beside C.** `compile` dispatches on the source extension:
  `.zig` through `zig build-exe -target riscv64-freestanding`, `.rs` through
  `rustc --target riscv64gc-unknown-none-elf --sysroot …`. Both are one click away from *Settings →
  Toolchain* — the download wiring an earlier release left open — and the three archive kinds the vendors
  publish (`.zip`, `.tar.gz`, `.tar.xz`) unpack through the same Zip-Slip-guarded extractor. The Rust
  sysroot is the one artifact **pinned to the release that produced it**: the host refuses one whose
  version does not match the machine's `rustc`, before the download starts. Sandboxes became nameable
  objects (define, scan, merge, switch), and an AI can *ask* for a sandbox change that a human decides.
- **The management program.** The desktop shell and the browser share **one built front end**:
  `riscdom-server --web-root ui/dist` serves the same `dist/` the desktop bundles, so the node is visible
  from a phone on the same origin as its API. The browser is a **read-only board** — the only two
  controls it keeps are display preferences (theme and language) — with a login gate, a node page that
  has three tabs (Status / Executors / Sandboxes), and live refresh over the event stream. Nothing in the
  board pretends: a control that belongs to the desktop says so rather than failing when pressed.
- **The interface delivery.** The executor roster is readable (`GET /v0/executors`), a task can be
  dispatched (`POST /v0/tasks`), and the seam `agent::AgentHandle` left open since v0.8 has its other
  half: `HttpExecutorHandle`, a reference remote executor that POSTs a task-shaped body to another node.
  Two tool-schema documents describe, separately, what an executor's model may call and what an AI
  supervisor may call, and each is checked against the code it describes. A reference supervisor in
  Python (`examples/python/dispatch.py`, standard library only) drives a node end to end.
- **Engineering quality.** The gate is the same list on every platform — every crate is linted and
  checked everywhere, the two Tauri crates included — the bilingual rule is enforced by a script, and an
  **encoding guard** now fails on the two classes of Windows code-page damage that no compiler can see.
  Four "green locally, red in CI" mechanisms were found and fixed at the root: a missing system package,
  a capability the local machine happened to have, SIGPIPE on a pipe a test had stopped reading, and
  `ETXTBSY` from a sibling process that had forked but not yet exec'd. The reverse case is recorded too:
  one guest-booting test that only ever flakes on a developer's machine.

## Install by platform

- **Windows 10/11** — `RiscDom_0.9.0_x64_en-US.msi` or `RiscDom_0.9.0_x64-setup.exe` from the release
  assets. The installers are **unsigned**, so SmartScreen warns the first time ("More info → Run
  anyway"). QEMU and a RISC-V bare-metal GCC are not bundled: *Settings → Toolchain* guides you to
  `winget install SoftwareFreedomConservancy.QEMU` (or the official page) and can download the xPack GCC
  itself.
- **macOS** — the CI `bundle` job builds `RiscDom_0.9.0_aarch64.dmg` (Apple Silicon) and the `.app`
  inside it. They are **unsigned**, so Gatekeeper blocks the first launch: right-click the app → *Open*,
  or run `xattr -dr com.apple.quarantine /Applications/RiscDom.app` once. **QEMU is not bundled**:
  `brew install qemu`. **Nobody has launched these packages on a real Mac yet.**
- **Linux** — from the same job: `RiscDom_0.9.0_amd64.deb`, `RiscDom-0.9.0-1.x86_64.rpm` or
  `RiscDom_0.9.0_amd64.AppImage`. **QEMU is not bundled**: install your distribution's
  `qemu-system-riscv64` (for example `sudo apt install qemu-system-misc`, `sudo dnf install
  qemu-system-riscv`). **Nobody has launched these packages on a real Linux machine yet.**

## What was verified — and what was not

**Verified**

- The gate, seventeen steps (two of them optional): `cargo fmt`, `cargo clippy -D warnings` over every
  crate on every platform, `cargo check`, the full `cargo test`, `npm run build`, the thirteen UI probes,
  the mirror guard, the encoding scan, the tool-schema check, the two example self-tests, the wix-version
  guard, the UI string registry and the bilingual-documentation check. Green locally; CI runs the same
  script on `ubuntu-latest`.
- The test suite: **688 tests in 118 suites**. Where QEMU and a RISC-V GCC are absent — the shape CI
  sees — the gate runs **625 passed / 0 failed / 63 ignored**, and every ignored test names the
  prerequisite it lacks; on a machine that has those tools the ignored set runs too, minus the three that
  need an API key or write to the OS keyring.
- The **control plane's surface** end to end: authentication and capabilities, the SSE stream with its
  replay and `gap`, the project export/import round trip, the dispatch endpoint, the sandbox registry,
  the request queue. The doc-locked route tables are read by the tests, not copied.
- The **two new languages** end to end and offline: a Zig and a Rust source compiled for the bare-metal
  target, the archives unpacked and the products adopted, the Rust version refusal. The guest-booting
  tests for both are marked and run where a guest is available.
- The **Web board** against a real server: the login gate, the status page and its three tabs, the SSE
  refresh, and the read-only wrapping (a probe counts the wrapped controls per screen, so a new control
  that forgets to hide itself turns the gate red).
- The end-to-end walk of golden-path **steps 3–7** against a real QEMU guest, unchanged since v0.5.0.

**Not verified**

- **The macOS and Linux packages themselves.** They compile and bundle; nobody has installed or launched
  them on a real Mac or Linux machine. That is the next walk.
- **A clean-machine walk.** Still the strengthening item it was: [docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  is the form, `walkthroughs/` is where a filled one goes.
- **Multi-agent collaboration.** What v0.9 delivers is the interface — the roster, the dispatch endpoint,
  the remote handle, the request queue. Which AI divides a composite task and how the parts are shared
  out is left to the user and to a later release.
- **The installers and packages are unsigned** — SmartScreen on Windows, Gatekeeper on macOS. Signing and
  notarization remain commercialisation-layer items.

## What a tester needs

- A **model provider** and **an API key** of your own (or a local provider such as Ollama / LM Studio),
  **QEMU** (`qemu-system-riscv64`, installed by you — see the platform notes above), and **a RISC-V
  bare-metal GCC** (xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`; the app can
  download the xPack one). Zig and Rust are optional: *Settings → Toolchain* can install both.

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open and fill it
   in as you go. On macOS or Linux, say which package you used and whether it opened at all.
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**; put
   it in `walkthroughs/` if you prefer the repository.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Known limitations

- **`task_id` is not carried into the run.** `POST /v0/agent/run` does not take a `task_id`, and
  `--follow` assumes a single client: with several clients following one node, an event frame cannot be
  attributed to the task that produced it. This is to be resolved before v1.0.
- **The embedded `--follow` / `--wait` paths have a rough edge**: the stream may still be open when the
  process exits. It is a known rough edge, reported and not silently worked around.
- **v0.9 delivers the multi-agent *interface*, not a strategy.** The roster, the dispatch endpoint, the
  remote handle and the sandbox request queue are in place and tested; who decomposes a composite task,
  and how the parts are assigned, is not decided here. The management program also ships **inside this
  repository** in v0.9 — it becomes its own repository at v1.0.
- **The release walk has not happened**: the clean-machine walk by somebody else, with a real API key, is
  still outstanding — the same caveat v0.8.0 carried.
- **Windows is the verified platform.** macOS and Linux build, but the golden path has not been walked
  there; their packages are unsigned and unlaunched.
- **One VM at a time** per agent, and the audit log has no retention policy yet: it grows with use and is
  never pruned.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your machine,
the audit log lives outside the AI workspace and is append-only, and nothing is uploaded anywhere. The
control plane binds to loopback by default and requires a token unless it is started with `--no-auth`;
the browser board reads the same token from the data directory. QEMU and the RISC-V toolchain are separate
programs under their own licences; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full
statement and the reporting process are in [SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
