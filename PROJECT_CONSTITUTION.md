[中文](PROJECT_CONSTITUTION.zh-CN.md) | English

# PROJECT_CONSTITUTION.md — the full RiscDom project constitution

> This file extends `AGENTS.md`: it is the complete governance and architecture description.
> `AGENTS.md` is the core summary injected every turn; this file is the searchable full
> version. It must not contradict `AGENTS.md`; where they conflict, `AGENTS.md` wins.

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

## 5. Language limits

During MVP the AI inside the sandbox may only generate:

- C11: `-ffreestanding -nostdlib -march=rv64gc -mabi=lp64d`
- RISC-V RV64GC assembly

Forbidden: C++, Rust, Zig, Python.

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

- [DONE] The host monitoring layer (`host`) cannot be modified by the AI; the frontend can
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

## 10. v0.2 roadmap

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
