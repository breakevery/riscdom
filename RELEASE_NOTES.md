[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v0.9.9

> **This is a release, not a preview — with the same two caveats as v0.9.0.** The **macOS and Linux
> packages are built by CI and have never been launched on a real machine**, and they are **unsigned**
> (macOS Gatekeeper blocks a first launch; Windows SmartScreen warns on the installer). And **Windows
> remains the platform the golden path has been verified on**: the clean-machine walk by somebody else
> still has not happened.

**v0.9.9 in one sentence:** the desktop application joins a network — it can **connect out** to an
in-network RiscDom server and show *that* node's board, and it can **serve in**, exposing its own board
to phones and other devices on the LAN — while the remote credential lives in the OS keyring instead of
a settings file, and the fifth "green locally, red in CI" mechanism is fixed at the root.

## What this release adds

- **The desktop application can connect out to another node.** *Settings → Network* takes a server
  address and the token that server wants; the address is a preference in `settings.json`, and the
  token is a **credential in the OS keyring**, filed under `remote-token:<host>`
  (`NetworkSettings` has no token field at all). The mode is settled **once, at startup** — which host
  this window talks to is a configuration fact, not a keystroke — so connecting (or leaving) is applied
  by restarting the app, and the page says so. From there the desktop is a client of the other node:
  the same gate the browser uses, a top bar that names the host on screen, and a settings page filtered
  by mode, so the screens that configure *this* machine are not offered while another machine's node is
  on screen. The gate carries the way back — **"Disconnect and use this machine"** — and the four
  commands behind it (the keyring's three and the restart) deliberately act on **this** machine in every
  mode, because a window whose server is unreachable still has to be able to stop being that window.
- **The desktop application can serve its own board to the network.** *Settings → Network* also holds
  the other direction: a switch, a bind address (loopback by default), and an *allow other devices on
  this network* switch that raises its warning the moment it is ticked. The board is the **node this
  window runs** — the embedded control plane started over the app's own `Arc<AppState>`, never a copy,
  because a copy would have its own VM slot and a board that can start a second QEMU is worse than no
  board. It binds **loopback unless you say otherwise**, mints its token in `<data-dir>/token` on first
  start, serves the built front end at `/` and its hashed assets under `/assets/*`, rebinds when a
  setting changes, and stops with the window (no socket outlives it). The built front end travels with
  the package as a Tauri resource, resolved by one helper that also serves `tauri dev`.
- **One page for both directions, and a token that is readable without being created.** The network tab
  shows the board's real state (running, what it bound, and the address a phone has to type), and
  "show token" reads `<data-dir>/token` and **never creates it** — opening a settings page must not
  bring a credential into existence. The browser is offered none of this page: it can look at a node, it
  cannot rewire one.
- **Engineering: the fifth "green locally, red in CI" mechanism, and a settings file that stays
  non-secret.** Declaring `bundle.resources` turned `ui/dist` — a gitignored build artifact — into a
  **compile-time prerequisite**, so a fresh checkout could no longer run `cargo clippy ui/src-tauri`.
  The front end now builds into `ui/dist/app/` under a stable, tracked parent, `emptyOutDir` keeps its
  default, and the property the B-2 batch recorded (a build artifact is not a build prerequisite) holds
  again. It is the first of the five that our own change introduced rather than the environment, which
  is why it is written down as a rule. The same pass removed the plaintext remote-token field before it
  ever shipped writing one.

## Install by platform

- **Windows 10/11** — `RiscDom_0.9.9_x64_en-US.msi` or `RiscDom_0.9.9_x64-setup.exe` from the release
  assets. The installers are **unsigned**, so SmartScreen warns the first time ("More info → Run
  anyway"). QEMU and a RISC-V bare-metal GCC are not bundled: *Settings → Toolchain* guides you to
  `winget install SoftwareFreedomConservancy.QEMU` (or the official page) and can download the xPack GCC
  itself.
- **macOS** — the CI `bundle` job builds `RiscDom_0.9.9_aarch64.dmg` (Apple Silicon) and the `.app`
  inside it. They are **unsigned**, so Gatekeeper blocks the first launch: right-click the app → *Open*,
  or run `xattr -dr com.apple.quarantine /Applications/RiscDom.app` once. **QEMU is not bundled**:
  `brew install qemu`. **Nobody has launched these packages on a real Mac yet.**
- **Linux** — from the same job: `RiscDom_0.9.9_amd64.deb`, `RiscDom-0.9.9-1.x86_64.rpm` or
  `RiscDom_0.9.9_amd64.AppImage`. **QEMU is not bundled**: install your distribution's
  `qemu-system-riscv64` (for example `sudo apt install qemu-system-misc`, `sudo dnf install
  qemu-system-riscv`). **Nobody has launched these packages on a real Linux machine yet.**

## What was verified — and what was not

**Verified**

- The gate, **eighteen steps** (two of them optional): `cargo fmt`, `cargo clippy -D warnings` over every
  crate on every platform, `cargo check`, the full `cargo test`, `npm run build`, the **sixteen** UI
  probes, the mirror guard, the encoding scan, the tool-schema check, the two example self-tests, the
  wix-version guard, the UI string registry and the bilingual-documentation check. Green locally; CI runs
  the same script on `ubuntu-latest`.
- The test suite: **688 tests in 118 suites**. Three of them need an API key or write to the OS keyring
  and are excluded from the gate's run; the rest run here, where QEMU and a RISC-V GCC are present.
- The **LAN board**, on a real machine: the embedded server starts over the app's own state, binds where
  the settings say (loopback unless the allow-LAN switch is on), mints its token in `<data-dir>/token`,
  serves the built front end at `/` and its hashed files under `/assets/*`, answers the API with the
  token and refuses it without one, and stops when the app does.
- **Remote mode, walked by hand on Windows** with a second node on the same machine — its own data
  directory, therefore its own token, so the check is real. Pointed at that address, the desktop comes
  up on the **login gate** (a desktop talking to its own host never shows one), and pressing
  **"Disconnect and use this machine"** clears the address, deletes the keyring entry and restarts the
  app back into its own board.
- The **compatibility case**: a real `settings.json` written before the token field was removed still
  loads, and the field is dropped on the next write.
- The end-to-end walk of golden-path **steps 3–7** against a real QEMU guest, unchanged since v0.5.0.

**Not verified**

- **The macOS and Linux packages themselves.** They compile and bundle; nobody has installed or launched
  them on a real Mac or Linux machine. That is the next walk.
- **A clean-machine walk.** Still the strengthening item it was:
  [docs/golden-path-checklist.md](docs/golden-path-checklist.md) is the form, `walkthroughs/` is where a
  filled one goes.
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
- For the network face: a **second node**. A second `riscdom-server` with its **own data directory** on
  the same machine is enough for "connect out", and any second device with a browser is enough for
  "serve in"; a real second machine is better. `riscdom-server --help` lists the flags.

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
- **Part of the network face is unwalked** (new). Typing the remote server's token at the gate, the board
  then showing *that* node's data, and what a window does when the remote node is stopped were **not**
  walked here: they need a credential, and no credential was put on a command line to make the walk
  possible. They are written up as **layer 9** of
  [docs/manual-acceptance.md](docs/manual-acceptance.md), together with the LAN steps.
- **One unexplained observation** (new). During the walk above, a first relaunch came up on the **local**
  board with the address gone from the settings file; it did not reproduce once stale developer processes
  were cleared, and no code path is known to explain it. It is recorded rather than explained — in
  [docs/handoff.md](docs/handoff.md) §1 and as a step of layer 9 — and layer 9 is what settles it.

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your machine,
the audit log lives outside the AI workspace and is append-only, and nothing is uploaded anywhere. The
control plane binds to loopback by default and requires a token unless it is started with `--no-auth`;
the browser board reads the same token from the data directory. **An in-network server's token is a
credential and is not written to `settings.json`**: it lives in the OS keyring, per host, and the agent
that files it never logs it. Serving the board to the network is off until you switch it on, it is
loopback until you allow other devices, and the warning on that switch says what it means. QEMU and the
RISC-V toolchain are separate programs under their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full statement and the reporting process are in
[SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see
[CONTRIBUTING.md](CONTRIBUTING.md).
