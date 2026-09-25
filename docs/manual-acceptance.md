[中文](manual-acceptance.zh-CN.md) | English

# Manual acceptance checklist

> **Why this file exists.** v0.9.0 shipped with a P0 nobody automated: the desktop application opened
> on a login screen no input could pass. Every test was green, because the automated checks run in
> Node, where the "am I the desktop?" answer is always *no* — the desktop branch had never been
> exercised by anything. The walk this repository relies on lived in people's heads. This is that walk,
> on disk.

**What it is not.** It is not the golden-path form. [golden-path-checklist.md](golden-path-checklist.md)
is the record a walker fills in for steps 1–7 of the golden path (machine, versions, fingerprints,
the exported record). This file is the *order of work* around it: install, launch, drive, verify, and
what to send back when something breaks. Fill in both when you walk a release.

**Who walks it.** Somebody who did not write the code, on a machine that has never had RiscDom on it.

## Prepare

- A **clean machine** (or a machine with no prior RiscDom install), ideally one per platform.
- An **API key** of your own for a model provider (or a local provider such as Ollama / LM Studio).
- **QEMU** installed by you: `qemu-system-riscv64` (Windows: `winget install
  SoftwareFreedomConservancy.QEMU`; macOS: `brew install qemu`; Linux: your distribution's package).
  RiscDom never bundles or downloads QEMU, by decision (§7).
- **A phone on the same local network** for layer 6.
- Somewhere to write: the filled [golden-path-checklist.md](golden-path-checklist.md), plus a note of
  anything that failed.

## Layer 1 — install

Take the package for your platform from the release page and install it. **Every package is unsigned**,
so this is the expected experience, not a bug:

| Platform | Package | What the OS does |
|---|---|---|
| Windows 10/11 | `RiscDom_<version>_x64_en-US.msi` or `RiscDom_<version>_x64-setup.exe` | SmartScreen warns: *More info → Run anyway* |
| macOS (Apple Silicon) | `RiscDom_<version>_aarch64.dmg` | Gatekeeper blocks the first launch: right-click the app → *Open*, or `xattr -dr com.apple.quarantine /Applications/RiscDom.app` once |
| Linux (amd64) | `RiscDom_<version>_amd64.deb`, `RiscDom-<version>-1.x86_64.rpm` or `RiscDom_<version>_amd64.AppImage` | installs with your package manager, or runs in place |

- [ ] The package installs and the application appears (menu / Applications / `riscdom` in the path).
- [ ] Uninstall removes it cleanly. Say what it left behind, if anything.

## Layer 2 — launch

**This is the layer v0.9.0 broke.** Open the application.

- [ ] **The main shell comes up — not a login screen.** If you see a token prompt, you have found a
      blocker: the desktop runs the node itself and has no control plane to sign in to (P0).
- [ ] The shell shows the chat panel and the serial panel.
- [ ] *Settings* (the gear, top right) opens, and **all six tabs** switch: Model / Toolchain /
      Snapshots / Audit / Plugins / Appearance.
- [ ] *Settings → Appearance*: switching the **language** (follow the system / English / 中文) takes
      effect at once, and switching the **theme** (light / dark / follow the system) does too.
- [ ] Close and reopen: the language and theme survived (they are written to
      `<workspace>/.riscdom/settings.json`).

## Layer 3 — the golden path

The full record goes in [golden-path-checklist.md](golden-path-checklist.md); this is the driving.

- [ ] *Settings → Model*: pick a provider and set the **API key** (stored in the OS keyring if you
      tick the box), base URL and model. The status line says the configuration is active.
- [ ] *Settings → Toolchain*: the **RISC-V GCC** is found or installed — the in-app download is one
      button; the manual path picker works too.
- [ ] *Settings → Toolchain*: the **QEMU** probe finds `qemu-system-riscv64`.
- [ ] *Settings → Toolchain → Environment preflight*: run it. All steps pass, or the failure names the
      step and you can override it with a reason.
- [ ] In the chat box, ask for something end to end: *"Write a RISC-V bare-metal hello, compile it,
      run it, and print hello over the serial console."* The agent compiles, boots the guest and reads
      the serial output back.
- [ ] **The serial panel prints the guest's `hello`** (the terminal fills in as the run goes).
- [ ] *Settings → Audit* lists the events of that run.

## Layer 4 — the audit chain

- [ ] *Settings → Audit*: the event list loads, newest first, and the header says the chain is intact.
- [ ] Export it, and check the file: `riscdom export audit-jsonl --out audit.jsonl`, or per run
      `riscdom export run-audit <run_id>` (the path is resolved against the workspace).
- [ ] The independent checker agrees: `audit-verify <workspace>/.riscdom/audit.db --runs` →
      `Intact { length: N }` and `RunIndex { findings: 0 }`.
- [ ] Now break a **copy**: copy `audit.db` somewhere else and, in the copy,
      `DROP TRIGGER audit_no_update; UPDATE audit_events SET action = 'evil' WHERE id = 2;`
      (the triggers refuse this on the real file — that is the point of them).
      `audit-verify <copy> --runs` → **`Broken`**, naming the id. The real file is untouched.

## Layer 5 — the command line

- [ ] Start a control plane in a console: `riscdom-server --workspace <dir> --log-level info`. It
      prints the address it bound and **generates `<data-dir>/token`** on first start.
- [ ] In another console: `riscdom --remote 127.0.0.1:7821 health` → the node's status line.
- [ ] `riscdom --remote 127.0.0.1:7821 status` → connections, subscribers, agents, `agent_id`.
- [ ] `riscdom --remote 127.0.0.1:7821 sandboxes list` → the merged registry, with `current` and
      `default`.
- [ ] **A wrong token is refused**: `riscdom --remote 127.0.0.1:7821 --token wrong health` exits
      **`4`** (and prints the control plane's own refusal, not a guess).
- [ ] `riscdom health --json` passes the JSON through unchanged.

## Layer 6 — the board on a phone

- [ ] Serve the built front end on the LAN: `riscdom-server --workspace <dir> --bind 0.0.0.0:7821
      --web-root ui/dist` (the build is `npm run build` in `ui/`; the server serves `index.html` at
      `/` and hashed assets at `/assets/*`).
- [ ] Open `http://<this machine's LAN address>:7821` on the phone. The **login gate** appears.
- [ ] Sign in with the token from `<data-dir>/token` (that file is also on this machine). The node page
      opens with its three tabs: **Status / Executors / Sandboxes**.
- [ ] Start something on the desktop application while the phone watches: within a few seconds the
      board refreshes on its own (the event stream).
- [ ] The board is **read-only**: the controls that belong to the desktop are absent, except the two
      display preferences (theme and language), which work.

## Layer 7 — the other two languages (optional, bonus)

- [ ] *Settings → Toolchain*: install **Zig**, then ask the agent for a Zig hello that boots.
- [ ] *Settings → Toolchain*: install the **Rust** sysroot, then ask for a Rust hello that boots.

## Layer 8 — the board on the network (v0.9.9)

- [ ] *Settings → Network*: switch **serve this node's board on the network** on, and **allow other
      devices on this network** (the warning appears — read it, and keep the token to yourself).
- [ ] Save. **Windows may ask whether to let the app use the network — choose Allow**, or nothing on
      the LAN will reach it. macOS may ask through its own firewall; Linux has no prompt here.
- [ ] The page now shows a **board address** (`http://<this machine's address>:7821`) with the state
      **serving**, and the bound address in brackets (`0.0.0.0:7821`).
- [ ] On a **phone on the same network**: open that address. The **login gate** appears; take the token
      from *Show token → Copy* and sign in. The node page opens with its three tabs.
- [ ] Start something on the desktop while the phone watches: the board refreshes by itself.
- [ ] Switch the board **off** and save: the phone's connection stops working within a few seconds.
- [ ] Close the app and open it again: the board comes back on its own (the settings remember it) — or
      stays off, if you switched it off.

## Pass criteria

**Layers 1–7 must pass** for a release to be walked successfully. **Layer 8 is the newest one**, and a
phone is the only thing that can check it — walk it when you have one, but a release is not failed for
its absence.

## Reporting back

Open an issue at <https://github.com/breakevery/riscdom/issues>, or put the filled
[golden-path-checklist.md](golden-path-checklist.md) in `walkthroughs/`. For every problem give:

- **Layer** (1–7) and **platform** (OS + build, package used);
- **What you did** — the shortest sequence that shows it;
- **What happened** — the message, verbatim, and a screenshot if the screen is the evidence;
- **What you expected**;
- Anything in the logs: the console the server printed, or the audit export.

## Priority

| Priority | Meaning |
|---|---|
| **P0** | Layers 1–3 fail: the app cannot be installed, opened, or driven. The release is not usable. |
| **P1** | Layers 4–6 fail: the app works, but the audit, the CLI or the LAN board does not. |
| **P2** | Layer 7 fails, or a documented behaviour differs from what the documents say. |
| **P3** | Anything cosmetic: wording, spacing, a tooltip. |
