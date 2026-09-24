[中文](PROJECT_CONSTITUTION.zh-CN.md) | English

# PROJECT_CONSTITUTION.md — the full RiscDom project constitution

> This file extends `AGENTS.md`: it is the complete governance and architecture description.
> `AGENTS.md` is the core summary injected every turn; this file is the searchable full
> version. It must not contradict `AGENTS.md`; where they conflict, `AGENTS.md` wins.
>
> **If you are taking this project over, read [docs/handoff.md](docs/handoff.md) first** — the
> cross-conversation handoff: a volatile snapshot of where the project stands, then the stable
> constraints (remote-operation rules, the gate, the audit invariants, the CLA, the release
> gate and what v0.6 starts from).

## 1. Project identity

- Chinese name: 智芯城
- English name: RiscDom
- What it is: a desktop application. Inside a RISC-V virtual sandbox the AI holds virtual
  kernel-level privilege and can write C/assembly and drive virtual hardware.
- Everything is auditable and rollback-able. Humans keep root privilege. Edge capabilities
  are plugins.

## 2. Motto

Freedom inside boundaries, audit outside the AI, root privilege with humans.

## 3. Project constitution (non-negotiable)

1. The host monitoring layer must not be modifiable by the AI.
2. The audit log lives outside the AI: append-only, cannot be disabled.
3. Capabilities are denied by default; plugins declare their permissions.
4. Humans always keep the right to pause, roll back, disconnect and terminate.
5. AI democracy is an experiment variable, not an MVP requirement.
6. During MVP the AI inside the sandbox may only generate C and RISC-V assembly.
   *MVP ended at v0.8.0, so this clause's time condition no longer holds; v0.9 F3a lifts the
   Zig ban — see [decisions.md](docs/decisions.md) §47. The principle is untouched: what
   expired is the "During MVP" qualifier the clause carries.*
7. AI access starts with API keys.

## 4. Architecture layers

From the outside in, privilege tightens layer by layer:

1. **Human layer (Human / Root)**
   Root privilege holder. Pauses, rolls back, disconnects and terminates through the UI and
   can interrupt the system at any moment.
2. **Host monitoring layer (Host Supervisor, Rust + Tauri)**
   Not modifiable by the AI. Owns process lifecycle, QEMU control, capability arbitration
   and UI communication.
3. **Capability broker layer (Capability Broker)**
   Deny by default. Every access to host resources must be explicitly declared by a plugin
   and approved by a human.
4. **Audit layer (Audit, Rust)**
   Outside the AI, append-only, built on SQLite + hash chain. Cannot be disabled or tampered
   with by the AI.
5. **Sandbox layer (Sandbox, Rust + QEMU RISC-V)**
   Runs a bare-metal ELF on the QEMU `virt` machine. The AI holds virtual kernel-level
   privilege inside it — and only inside it.
6. **AI agent layer (Agent, Rust)**
   Agent loop + tool calls. During MVP it may only generate C11 and RISC-V RV64GC assembly.
   *Same time condition as §3.6: MVP ended at v0.8.0, and v0.9 F3a lifts the Zig ban
   ([decisions.md](docs/decisions.md) §47).*

## 5. Language limits

During MVP the AI inside the sandbox may only generate:

- C11: `-ffreestanding -nostdlib -march=rv64gc -mabi=lp64d`
- RISC-V RV64GC assembly

Forbidden: C++, Rust, Zig, Python.

*MVP ended at v0.8.0, so the time condition above no longer holds: **Zig is lifted** — v0.9 F3a,
see [decisions.md](docs/decisions.md) §47. **C++, Rust and Python stay forbidden**: Rust waits for
F3b, Python for a Linux sandbox (v1.x), and C++ is still out of scope.*

## 6. Audit event types

Every event is written to the append-only log; fields include at least: `id`, `timestamp`,
`actor`, `kind`, `payload`, `prev_hash`, `hash`.

- `vm.start` / `vm.stop` — virtual machine start/stop
- `vm.snapshot.save` / `vm.snapshot.load` — snapshot save/rollback
- `vm.serial.write` / `vm.serial.read` — serial console I/O
- `agent.prompt` / `agent.completion` — LLM request/response
- `agent.tool_call` — AI tool call
- `capability.request` / `capability.grant` / `capability.deny` — capability request/grant/deny
- `sandbox.file.write` / `sandbox.file.read` — file operations inside the sandbox
- `human.pause` / `human.resume` / `human.rollback` / `human.terminate` — human intervention
- `system.config.change` — configuration change
- `audit.verify` — audit chain verification

Actor classification: `human`, `host`, `agent`, `sandbox`, `system`.

Implementation note (updated 2026-09-14): the events above are implemented by the `audit`
crate — append-only SQLite (hard guarantees via `BEFORE UPDATE` / `BEFORE DELETE` triggers)
plus a SHA-256 hash chain, with no UPDATE / DELETE API and no switch to turn auditing off.
`sandbox` writes through `audit::AuditSink`, so the dependency direction is
`sandbox → audit`. `audit::FileAuditSink` is kept as an example implementation only.

## 7. Development discipline

- Every action writes an audit event.
- Touch only the directories you were given; do not cross boundaries.
- Show tests and diffs.
- Prefer slow over wrong: every step must be verifiable and rollback-able.

## 8. Red lines

- Never exfiltrate private data.
- Never run destructive commands without asking.
- Before changing configuration, inspect the current state; preserve/merge by default.
- Prefer trash over rm.
- When in doubt, ask first.

## 9. v0.1 status

- [DONE] The host monitoring layer (`host-core` / `host-tauri`) cannot be modified by the AI; the frontend can
  only reach it through Tauri commands.
- [DONE] The audit log is outside the AI, append-only and cannot be disabled (`audit`:
  SQLite triggers + hash chain + `audit-verify`).
- [DONE] Capabilities denied by default (`agent::WorkspacePolicy`: traversal guard +
  extension allowlist).
- [DONE] Humans can pause / roll back / terminate (VM lifecycle under control; the VM is
  host-owned and snapshots support real save/restore).
- [DONE] AI access uses API keys (`DEEPSEEK_API_KEY`, memory only, never on disk or in audit).
- [DONE] Inside the sandbox the AI only generates C11 and RV64GC assembly
  (`agent::tools` allowlist + compiler flags).
- [DONE] Every action is audited (sandbox / agent / host all emit events).
- [DONE] Real snapshots (TCP migration + local file relay): save/restore work; the old
  reboot fallback is kept for compatibility.
- [DONE] Serial events are pushed by the sandbox; serial subscriptions **survive across
  runs** (20b).

## 10. Roadmap

### v0.2 additions — multi-model access and key security

a. **LLM client refactor: `DeepSeekClient` → `OpenAiCompatClient`**
   - `base_url` / `api_key` / `model` all user-configurable
   - Keep the OpenAI-compatible protocol; DeepSeek becomes one default preset
   - Rationale: DeepSeek is OpenAI-compatible, so the change is cheap and unlocks every
     compatible provider

b. **Built-in provider presets**
   - DeepSeek (default): `https://api.deepseek.com`, `deepseek-chat`
   - OpenAI: `https://api.openai.com/v1`, `gpt-4o-mini`
   - Ollama (local): `http://localhost:11434/v1`, `qwen2.5-coder`, no key needed
   - LM Studio (local): `http://localhost:1234/v1`, user-specified
   - Custom: user fills `base_url` and `model`
   - UI: provider dropdown; picking a preset fills `base_url` / `model`

c. **Local offline model support**
   - Ollama / LM Studio are OpenAI-compatible and reuse the same client
   - Offline mode = QEMU + RISC-V GCC + audit + sandbox + local LLM, with no network at all
   - Valuable for privacy-sensitive, educational and air-gapped scenarios

d. **Key-less degradation**
   - Without a key the app does not crash; the UI guides you to configure a model
   - Probe `localhost:11434` and offer "a local model was detected — use it?"
   - After going public, a new user's first launch must not simply error out

e. **API key persistence: OS keyring**
   - Windows Credential Manager / macOS Keychain / Linux Secret Service
   - Use the Rust `keyring` crate
   - Never `localStorage` / plain files / `.env`
   - The current "memory only" behaviour degrades to a fallback when the keyring is unusable

f. **Pre-launch security checklist**
   - `.env.example` holds placeholders only
   - `.gitignore` covers `.env` / `*.db` / `*.jsonl`
   - CI runs secret scanning (gitleaks or GitHub native)
   - The README states plainly that no API key is provided — bring your own
   - Audit records of LLM requests store hashes and token counts only (already done)

### v0.2 other

g. Expose a minimal serial access interface on `AgentLoop` (done, see 15b)
h. **[DONE: A′]** Real QEMU snapshots via **TCP migration + a local file relay**
   (`migrate` → file is unusable on Windows + QEMU 11.1.0, see
   `sandbox/docs/snapshot-experiment.md`);
   **20b/20c/20d complete**: the VM lives in `AppState::vm_slot`, serial subscriptions
   survive across runs, and the UI can save/restore.
   Residual limits: restore takes `-kernel` from the newest `*.elf` in the workspace; no
   multi-VM parallelism (v0.3 evaluation).
i. Streaming LLM responses
j. Session persistence
k. **[DONE]** `real_api` asserts `verify_chain` (stage 21: file SQLite + independent handle)
l. Host serial polling moved to sandbox push callbacks

- gdbstub integration (debugging)
- Unix sockets (macOS / Linux) and virtio devices
- Audit log sharding and remote backup

m. **[DONE]** Bilingual (English/Chinese) docs before going public
   - README / CHANGELOG / PROJECT_CONSTITUTION / AGENTS / release notes in both languages
   - English is the main document (GitHub default); Chinese lives in `*.zh-CN.md`
   - Language switcher at the top
   - Not word-for-word: the English version is terser, the Chinese version keeps its voice
   - LICENSE is not translated; keep the English legal text

### v0.3 status

- [DONE] **Layout rework**: the main view is a two-pane chat + serial layout, settings live on a
  separate tabbed page (Model / Toolchain / Snapshot / Audit / Plugins), and Esc returns to the
  chat.
- [DONE] **One-click RISC-V GCC download** (xPack): SHA-256 verified, Zip-Slip protected,
  cancellable and audited.
- [DONE] **QEMU discovery and a manual path**: `RISCDOM_QEMU` → known paths → `PATH`, plus a
  manual path in *Settings → Toolchain*, persisted to `settings.json` and injected into the
  agent loop so it really takes effect.
- [DONE] **VM status badge** in the top bar, visible across runs.
- [DONE] **Prompt rework**: the system prompt is entirely in English and the AI no longer stops
  the VM on its own (prompt + `stop_vm` tool description + VM badge).
- [DONE] **Auto-scroll**: chat and serial follow the latest output without interrupting a user
  who scrolled up (a "jump to latest" button appears).
- [DONE] **`read_serial` silence window**: it waits for ~150 ms of silence, so the first byte is
  no longer truncated.
- [DONE] **Snapshot relay port retry**: resume retries the relay port on QMP 10054 / bind failure
  (up to 3 attempts).
- [DONE] **Test-gate stability**: port TOCTOU retry in `start_vm`.

### v0.3.1 status

- [DONE] **A snapshot restore honours a manually configured QEMU path**: the restore no longer
  falls back to auto-discovery; the agent run and the restore inject the configured path through
  one helper.
- [DONE] **A QEMU process that has exited is no longer reported as running**: `vm_is_running`
  checks the child process and drops a dead handle (documented as lazy cleanup, not a pure query).
- [DONE] **`read_serial` returns the captured output when the guest never goes quiet**: only a
  genuinely empty buffer reports silence.
- [DONE] **A finished run no longer pulls the chat back to the bottom**: the scroll state is
  respected and the "jump to latest" button is offered instead.
- [DONE] **The two-pane layout stays inside the window**: the drag bound is derived from the
  measured container and the serial column has an adaptive minimum.
- [DONE] **The UI regression probes run in the gate**: `ui/scripts/probe-ui-scroll.mjs` and
  `ui/scripts/probe-ui-width.mjs` (dependency-free modules under `ui/src/lib/`); `scripts/gate.sh`
  and `scripts/gate.ps1` fail when either probe fails.
- Gate: `cargo test` = **181 passed / 0 failed / 7 ignored** across **58 suites** (v0.3.0 was
  178 / 0 / 7 across 55 suites; the +3 are the three new regression tests).

### v0.4 status

1. **Removing the QEMU TCP port dependency, and a unified relay-port lease**: **[DONE] the lease** —
   relay ports come out of one process-wide lease (`sandbox::relay::lease_local_port`), so two parts of
   the app can no longer be handed the same port; **moved to v0.5: removing the port dependency
   itself** (QMP over stdio, the serial on the file variant). Proposal and platform notes:
   [docs/qemu-stdio.md](docs/qemu-stdio.md). The label "option 3" was dropped: no list of options is
   recorded anywhere in this repository, so it named nothing a reader could look up.
2. **[DONE] Stage 5c-3: end-to-end failure-path diagnostics**: a failed run prints a report
   naming the first failing step, that step's own output, the serial state and the chain
   verdict (`host-core/tests/diagnosis/`; how to read it: [docs/e2e-debugging.md](docs/e2e-debugging.md)).
3. **[DONE] `tauri-plugin-dialog`**: a native file picker for toolchain / QEMU paths, replacing the
   `window.prompt` text input.
4. **[DONE] QEMU distribution decided — guide, do not bundle or download**: the app points the user at
   `winget` / the official download page. Upstream publishes no Windows binary, a third-party packager
   would be an unnamed supply-chain link, and building QEMU ourselves would make us the distributor of
   a GPL-2.0 binary ([docs/qemu-distribution.md](docs/qemu-distribution.md) §5).
5. **[DONE] Environment capability preflight**: compile a minimal guest and boot it on the real toolchain / QEMU paths, then report which step failed (warn-only, cached per configuration, with a recorded "continue anyway"). Version rules were dropped on purpose: this repository records no QEMU × GCC compatibility matrix, and inventing one would be guesswork (batch 3).
6. **[DONE] Theme switching**: light / dark / follow-system, cycled in the Appearance tab and applied
   by `ui/src/lib/theme.ts` — the single writer of `data-theme` — with `ui/scripts/probe-ui-theme.mjs`
   pinning its rules. **Bilingual *code comments* are dropped**: they duplicate the code and rot with
   it, while this repository already keeps its documentation in two languages. A bilingual *interface*
   is what a user actually reads, so it is on the v0.5 roadmap below.

Not in this section but shipped in v0.4 as well: run provenance (every run becomes a first-class audit
record — [docs/run-provenance.md](docs/run-provenance.md)), the shared gate between CI and a developer
machine, commit-time refusal of untracked files, the mirrored-constant guard, the bilingual-link check,
and the temp-directory cleanup.

- Gate: `cargo test` = **261 passed / 0 failed / 7 ignored** across **75 suites** (v0.3.1 was
  181 / 0 / 7 across 58 suites).
- Moved to v0.5, unfinished in v0.4: removing the QEMU TCP port dependency, macOS / Linux support and a
  multi-OS CI matrix, multi-VM parallelism, incremental snapshots and encryption, session encryption /
  export / search, and the multi-AI society with a `Governance` trait.

### v0.5 roadmap

1. **The golden path — the main line for v0.5–v0.6.** A developer who has never seen this repository
   gets from a fresh machine to a compared re-run (proposal and reconnaissance:
   [docs/golden-path.md](docs/golden-path.md)):

   install → create an environment → run an agent task → save a snapshot → get an audit record →
   roll back → change the configuration and run again → **compare the two runs**

   The first seven steps, done by hand, are **v0.5**; the eighth — the automatic comparison — is
   **v0.6**. v0.5 stands on what v0.4 already put on the chain (every run carries an id, a
   configuration fingerprint and an audit interval: [docs/run-provenance.md](docs/run-provenance.md)),
   and it needs one thing that does not exist yet: **audit export**, without which step 5 — the audit
   record — is not something a user can hold on to or hand to someone else.

   How the rest of this roadmap relates to the golden path:

   - **Prerequisites.** *Session encryption, export and search* (item 6) carries the export that step
     5 depends on. *macOS / Linux support and a multi-OS CI matrix* (item 3) decides how far "a
     developer who has never seen this repository" reaches: the path is Windows-only today.
   - **Parallel, off the path.** Removing the QEMU TCP port dependency (item 2), multi-VM parallelism
     (item 4), incremental snapshots and encryption (item 5), the multi-AI society (item 7) and the
     bilingual interface (item 8). Each improves the product; none of them is a step a newcomer has to
     take to reach a compared re-run, and none may be allowed to delay the path.
2. **Removing the QEMU TCP port dependency**: QMP over stdio behind a config switch and the serial on
   the file variant, then the retries retire — [docs/qemu-stdio.md](docs/qemu-stdio.md) §6.
3. **macOS / Linux support and a multi-OS CI matrix**.
4. **Multi-VM parallelism**: more than one host-owned guest at a time.
5. **Incremental snapshots and encryption**.
6. **Session encryption, export and search**.
7. **Multi-AI society and a `Governance` trait** (the constitution's experiment variable).
8. **A bilingual interface**: the UI strings are hard-coded per component (Chinese in the app, English
   in the panels' docs) and there is no language switch. Making the interface translatable is a feature
   of its own.
