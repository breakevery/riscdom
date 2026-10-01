[中文](RELEASE_NOTES.zh-CN.md) | English

# RiscDom v1.0

> **This is v1.0 — the API is frozen.** The **macOS and Linux packages are built by CI and have never been
> launched on a real machine**, and they are **unsigned** (macOS Gatekeeper blocks a first launch; Windows
> SmartScreen warns on the installer). And **Windows remains the platform the golden path has been verified
> on**: the clean-machine walk by somebody else still has not happened.

**v1.0 in one sentence:** RiscDom stops moving where it promised to — **the kernel API is frozen**, and the
three layers the [roadmap](docs/roadmap-v1.0.md) set out are on disk: **several sandboxes on one device**, **a
workgroup of devices that sign what they say and reach each other through a deployer-run server**, and **a
dispatcher that hands a task to another node by name** — over an audit chain that now spans devices without
changing its formula.

## What this release adds

- **The freeze (M1, M8-1).** The six documents [roadmap §6](docs/roadmap-v1.0.md) required before the API
  could be called frozen are on disk — the stability policy (`docs/api-compatibility.md`), data migration
  (`api-compatibility.md` §6), the error model (`docs/error-model.md`), credentials and keys
  (`docs/security-model.md`), the upgrade path (`docs/upgrade.md`) and the disclosure policy (`SECURITY.md`)
  — and the stability policy has been **tested against roadmap §1's four red lines** (`api-compatibility.md`
  §9). **`/v0/` is the path v1.0 ships with**: the freeze pins what the paths *mean*; the prefix moves to
  `/v1/` at the next protocol-breaking change, not here.
- **Layer one (M2): several sandboxes on one device.** A definition can be derived into **instances**, with an
  instance table, its five endpoints and the audit rows that name them; each executor gets its **own model
  configuration** (`settings.json` migrates v1 → v2 on open, keeping a `.bak`); and the reference **M**
  (`examples/python`) dispatches through the control plane.
- **The plugin interface (M3).** `docs/plugin-interface.md`: **out of process, stdio, JSON lines** — a
  mandatory mechanism layer (start / stop / execute / output), an optional semantics layer (snapshot /
  fingerprint), and a capability declaration. No in-process plugin, no ABI.
- **Layer two (M4): a workgroup and a cross-region server.** Node identity and signing (`node.key`, one JWK,
  Ed25519, six verification steps, one JSON line over TCP), discovery (a peer table handed down by the
  in-network server, with UDP broadcast as a supplement), **rooms** (`rooms.json`), the **cross-region
  server** (a deployment the deployer runs, four roles, no new credential), and **chain digests** for the
  30-second audit batch.
- **audit v2 (M5).** **One chain per device, plus temporary segments.** A stand-in opens a segment, declares
  it, and on the centre's return the segment is delivered, rebuilt and **merged by transcription**; a
  conflict is recorded on **both** sides and is never a silent merge. **The hash formula is unchanged.**
- **Layer three (M6): dispatch across devices.** `POST /v0/tasks`'s **`node`** parameter, the runtime table
  (`GET /v0/online`), the chain digest rows the centre writes, **`task_id`** carried end to end, the stream's
  **`task_id` filter**, a request left on another node reaching the centre's queue, the centre's **decision
  travelling back** down the asker's own session, and **cross-chain verification** (the anchor's second half).
- **The ecosystem (M7).** The **configuration schema** (`docs/config-schema.md`), the **observability**
  contract (`docs/observability.md`), the **performance budgets** (`docs/performance-budget.md`), the
  multi-repository plan (`docs/multi-repo.md`), **`riscdom-backup`** (a whole node — both roots, the audit
  store and the credentials — as one movable unit), and the **Rust and TypeScript SDKs** for the whole
  control plane.

## Install by platform

These are the artifacts the `v1.0.0` tag's CI jobs build and attach to the release:

- **Windows 10/11** — `RiscDom_1.0.0_x64_en-US.msi` or `RiscDom_1.0.0_x64-setup.exe`. The installers are
  **unsigned**, so SmartScreen warns the first time ("More info → Run anyway"). QEMU and a RISC-V bare-metal
  GCC are not bundled: *Settings → Toolchain* guides you to
  `winget install SoftwareFreedomConservancy.QEMU` (or the official page) and can download the xPack GCC
  itself.
- **macOS** — `RiscDom_1.0.0_aarch64.dmg` (Apple Silicon) and the `.app` inside it. **Unsigned**: right-click
  the app → *Open*, or run `xattr -dr com.apple.quarantine /Applications/RiscDom.app` once. **QEMU is not
  bundled**: `brew install qemu`. **Nobody has launched these packages on a real Mac yet.**
- **Linux** — `RiscDom_1.0.0_amd64.deb`, `RiscDom-1.0.0-1.x86_64.rpm` or `RiscDom_1.0.0_amd64.AppImage`.
  **QEMU is not bundled**: install your distribution's `qemu-system-riscv64` (for example
  `sudo apt install qemu-system-misc`, `sudo dnf install qemu-system-riscv`). **Nobody has launched these
  packages on a real Linux machine yet.**
- **Servers** — `riscdom-server-1.0.0-linux-x64.tar.gz` and `riscdom-server-1.0.0-macos-*.tar.gz` from the
  tag's **`server-bundle`** job, and the Windows `riscdom-server-1.0.0-win-x64.zip`. Each holds the binary,
  its `web/`, a README and a `settings.example.json`; no credential and no data directory travels. The
  dedicated relay ships beside it (`riscdom-relay-*`), and — by design — **it has no HTTP face at all**.

## What was verified — and what was not

**Verified**

- The gate, **twenty-six steps** (v0.9.9's eighteen, plus the `net` layer's six example self-tests, the remote
  executor example and the packaging-script syntax check): `cargo fmt`, `cargo clippy -D warnings` over every
  crate on every platform, `cargo check`, the full `cargo test`, `npm run build`, the **sixteen** UI probes,
  the mirror guard, the encoding scan, the tool-schema check, the example self-tests, the wix-version guard,
  the UI string registry, the packaging syntax and the bilingual-documentation check. Green locally; CI runs
  the same script on `ubuntu-latest`.
- The test suite: **958 tests in 146 suites** on the workspace run (v0.9.9 reported 688 in 118; the three
  layers' own suites are what the growth is). A handful need an API key or write to the OS keyring and are
  excluded from the gate's run; the rest run here, where QEMU and a RISC-V GCC are present.
- The end-to-end walk of golden-path steps against a real QEMU guest, unchanged since v0.5.0.
- **The connection layer, hand-walked**: two nodes on one machine, each with its own data directory and
  therefore its own key and token, registering, beating and reading each other's registrations through an
  in-network server; and the stream and dispatch surfaces the three layers added.

**Not verified**

- **The macOS and Linux packages themselves.** They compile and bundle; nobody has installed or launched them
  on a real Mac or Linux machine. That is the next walk.
- **A clean-machine walk.** Still the strengthening item it was:
  [docs/golden-path-checklist.md](docs/golden-path-checklist.md) is the form, `walkthroughs/` is where a
  filled one goes.
- **The management program has not been split out.** v0.9.9's notes said it "becomes its own repository at
  v1.0". The split (**M7a**) is **deferred to after v1.0** — each new repository pins a v1.0 tag, and that tag
  ships with this release — so the desktop and its front end still live **inside this repository**. The plan
  is written down in [docs/multi-repo.md](docs/multi-repo.md).
- **Multi-agent collaboration is still an interface.** The roster, the dispatch endpoint, the request queue
  and the decision that travels back are in place and tested; which AI divides a composite task, and how the
  parts are shared out, remains the user's to decide.
- **The installers and packages are unsigned** — SmartScreen on Windows, Gatekeeper on macOS. Signing and
  notarization remain commercialisation-layer items.

## What a tester needs

- A **model provider** and **an API key** of your own (or a local provider such as Ollama / LM Studio),
  **QEMU** (`qemu-system-riscv64`, installed by you — see the platform notes above), and **a RISC-V bare-metal
  GCC** (xPack `riscv-none-elf-gcc` or an equivalent `riscv64-unknown-elf-gcc`; the app can download the xPack
  one). Zig and Rust are optional: *Settings → Toolchain* can install both.
- For the network face: a **second node**. A second `riscdom-server` with its **own data directory** on the
  same machine is enough for "connect out", and any second device with a browser is enough for "serve in"; a
  real second machine is better. `riscdom-server --help` lists the flags, and
  [docs/control-plane-client-guide.md](docs/control-plane-client-guide.md) walks the API.

## How to report back

1. Walk the steps with **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** open and fill it in
   as you go. On macOS or Linux, say which package you used and whether it opened at all.
2. Open an issue at <https://github.com/breakevery/riscdom/issues> and **paste the filled checklist**; put it
   in `walkthroughs/` if you prefer the repository.
3. A walk nobody wrote down is a walk nobody can check — the filled template is the report.

## Known limitations

- **The cross-region level of M is v1.x.** The reference M (`examples/python`) reads one **workgroup**
  (`--level lan`: `/v0/online`, `/v0/peers`, and each node named with its token in a file). The level above it
  — a list of LAN Ms, and a `--config` file — is **out of v1.0's scope** (decisions §157, roadmap §13).
- **`/v1/` is not the path.** v1.0 ships `/v0/`; the prefix moves at the next protocol-breaking change, not
  here. Four documents used to say otherwise and were corrected in the same batch as the freeze declaration
  (decisions §160).
- **The embedded `--follow` / `--wait` paths have a rough edge**: the stream may still be open when the
  process exits. It is a known rough edge, reported and not silently worked around.
- **Python is not a guest language.** C, Zig and Rust are; Python waits for a Linux sandbox (v1.x).
- **The session database's WAL mode is deliberately not set** (decisions §54).
- **One VM at a time** per node, and the audit log has **no retention policy yet**: it grows with use and is
  never pruned.
- **Windows is the verified platform.** macOS and Linux build, but the golden path has not been walked there;
  their packages are unsigned and unlaunched.
- **The release walk has not happened**: the clean-machine walk by somebody else, with a real API key, is
  still outstanding.
- **Windows server archives are built by hand.** CI builds the Linux and macOS archives on a `v*` tag; there
  is no Windows runner, so the Windows `riscdom-server-*.zip` is assembled on a machine (as it was for
  v0.9.9).

## Security

This project ships **no** API key: all model access is bring-your-own-key. Keys never leave your machine, the
audit log lives outside the AI workspace and is append-only, and nothing is uploaded anywhere. The control
plane binds to loopback by default and requires a token unless it is started with `--no-auth`; the browser
board reads the same token from the data directory. A node's identity is an Ed25519 key in `<data-dir>/node.key`
or the OS keyring, and every frame between nodes is signed. **An in-network server's token is a credential and
is not written to `settings.json`**: it lives in the OS keyring, per host, and the agent that files it never
logs it. Serving the board to the network is off until you switch it on, it is loopback until you allow other
devices, and the warning on that switch says what it means. QEMU and the RISC-V toolchain are separate programs
under their own licences; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The full statement and the
reporting process are in [SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE). Contributions need the [CLA](CLA.md) — see [CONTRIBUTING.md](CONTRIBUTING.md).
