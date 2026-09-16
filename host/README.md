[中文](README.zh-CN.md) | English

# host

The RiscDom **Tauri host backend**. The frontend talks to it only through Tauri commands and
never touches `agent` / `sandbox` / `audit` directly.

Dependency direction: `host → {agent, sandbox, audit}`; `ui/src-tauri → host`.

## Modules

- `state` — `AppState` (audit store, sink, VM slot, LLM config, workspace, compiler)
- `commands` — Tauri commands
- `events` — event-name constants + `EventSink` (Tauri / recording test implementation)
- `error` — `HostError`

## Commands

| command | returns |
| --- | --- |
| `get_audit_status()` | `{ count, chain }` |
| `list_audit_events(limit, actor?, action_prefix?)` | `StoredEventView[]` (newest first) |
| `set_llm_config(api_key, base_url, model, provider_id?, remember?)` | `()` (`provider_id` defaults to `deepseek`; a preset with an empty base_url/model fills them in; `custom` requires both; `remember` defaults to `true` → written to the OS keyring) |
| `clear_llm_config()` | `()` |
| `get_llm_config_status()` | `{ configured, provider_id, base_url, model, persisted }` (**never the key**) |
| `get_provider_presets()` | `ProviderPresetView[]` (5 built-in presets, pure data) |
| `has_stored_key(provider_id)` | `bool` (does the keyring hold an entry for that provider; **never returns the key**) |
| `load_stored_key(provider_id)` | `()` (reads the key from the keyring into memory; `Err("no_stored_key")` when absent) |
| `run_agent(user_input)` | `AgentOutcomeView` |
| `get_workspace_files()` | `string[]` |
| `read_workspace_file(path)` | `string` (policy-checked) |
| `get_serial_buffer()` | `string` |
| `stop_current_vm()` | `()` (stops and clears the host-owned VM; a no-op on an empty slot) |
| `vm_is_running()` | `bool` (does the host hold a resident VM) |
| `save_snapshot_real(name)` | `u64` (real snapshot bytes; `Err("no running vm")` when there is no VM) |
| `resume_from_snapshot_real(name)` | `()` (stops the current VM first, then restores with `-incoming`) |
| `export_audit_jsonl(path)` | `usize` (written inside the workspace) |

## Events (host → frontend)

- `agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final`
- `agent:stream:delta` / `agent:stream:done` (LLM streaming deltas, see below)
- `serial:chunk`
- `vm:state`

## Provider presets

`host` exposes `agent::presets::builtin_presets()` (pure data) to the frontend through
`get_provider_presets`; `set_llm_config` accepts `provider_id` and fills `base_url` / `model`
from the preset when they are empty; `provider_id = "custom"` requires both. The 5 built-in
presets: `deepseek` (default) / `openai` / `ollama` (local, no key) / `lmstudio` (local, no
key) / `custom`. See `agent/README.md`.

## Toolchain (stages 24b/24c)

`probe_toolchain()` returns `{ found, path, source, diagnostics }`, where `source` is
`EnvVar` / `KnownPath` / `Path` / `Manual`. `set_toolchain_path(path)` validates the file with
`--version` before storing it, `clear_toolchain_path()` returns to auto-discovery, and
`run_agent` refuses early with a `toolchain_missing` error (followed by the diagnostics) when no
usable compiler exists. Audit events: `host.toolchain.set` / `host.toolchain.clear`.

### One-click download (v0.3 #3)

- `start_toolchain_download()` — fetches the official xPack RISC-V GCC for this platform
  (SHA-256 pinned), verifies it, extracts it under `<app data>/toolchain/<version>/` and adopts
  it as the active toolchain (which also records it in `settings.json`). A second call while one
  is running fails with `download already in progress`; an already-installed version is a no-op
  (nothing is re-downloaded).
- `cancel_toolchain_download()` — asks the running download to stop; the temporary archive is
  removed and the status returns to idle.
- `toolchain_download_status()` — `{ in_progress, last_event }`.
- Progress is streamed to the UI as `toolchain:download` events carrying a `DownloadEvent`
  (`started` / `progress` / `verifying` / `extracting` / `done` / `failed` / `cancelled`).
- Audit: `host.toolchain.download.start` / `.done` / `.failed` / `.cancelled` (the detail holds
  the version, plus the final path on success).
- The checksum is mandatory — there is no skip-verification switch — and archive entries that
  would escape the install directory are refused (Zip Slip guard).

Manual steps (Windows):

The manual QEMU path is now fully wired: setting it in **Settings → Toolchain → QEMU** takes
effect on the next run (the agent loop receives it; when unset, the sandbox falls back to
auto-discovery).

1. Open the app and go to **Settings → Toolchain**. A red banner means nothing was found.
2. Expand **diagnostics** to see every path that was tried.
3. Click **set path** and paste the full path to `riscv64-unknown-elf-gcc.exe`; the row must
   switch to a green dot with the `Manual` badge.
4. Send a chat message — it should compile and boot instead of refusing with
   `toolchain_missing`.
5. Click **clear manual path** — the source badge goes back to `KnownPath` / `Path`.

## OS keyring

Keys are persisted in the OS credential store (Windows Credential Manager / macOS Keychain /
Linux Secret Service) under the service name `com.breakevery.riscdom` and the account name
`llm-api-key:<provider_id>`.

- Only **host** depends on the `keyring` crate; `agent` does not.
- A failed write **degrades silently**: it records `host.keyring.save_failed`, returns success
  with `persisted = false`, and never panics or blocks startup.
- Related audit events (the detail records only `provider_id`, **never the key**):
  `host.keyring.save` / `host.keyring.save_failed` / `host.keyring.delete` / `host.keyring.load`.
- `clear_llm_config` deletes only the current provider's entry, leaving other providers alone.
- The `DEEPSEEK_API_KEY` environment variable is adopted into **memory** at startup (a
  development convenience) and is **never** written to the keyring.

### Manual verification steps (13c)

a. First launch, no configuration → the banner shows `no_config`.
b. Fill in the DeepSeek key and tick "save to the OS keyring" → save → the status reads
   "configured · DeepSeek · saved to the OS keyring".
c. Close and restart the app → the status is restored to "configured" (read from the keyring).
d. Clear the configuration → restart → the status is unconfigured (the keyring entry is gone).
e. Leave "remember" unticked → after saving the status is "this session only"; a restart
   requires refilling it.

## Snapshots

- **Real snapshots (`.mig`)**: persisted by the sandbox via QMP `migrate` + a local TCP relay
  (see `sandbox/docs/snapshot-experiment.md`), because `migrate` → `file:` is unusable on
  Windows.
- **Reboot fallback (`.json`)**: the old scheme, kept for compatibility.
- host commands: `list_snapshots` / `delete_snapshot` / `save_snapshot_real` /
  `resume_from_snapshot_real` (they scan `<workspace>/.riscdom/snapshots`; audit events
  `host.snapshot.delete` / `host.snapshot.save` / `host.snapshot.resume`).
- Save: takes the host-owned VM from `AppState::vm_slot` and calls the sandbox's
  `save_snapshot_real`; snapshot names are restricted to `[A-Za-z0-9_-]{1,64}` (traversal
  guard) and an existing name is **refused, never overwritten**.
- Restore: `stop_current_vm()` first, then `-incoming tcp:` plus the local relay to feed the
  stream; the restored VM stays in the slot for later runs. `-kernel` comes from the **newest
  `*.elf`** in the workspace (the migration stream overwrites memory; the kernel only lets
  QEMU boot). A workspace with no ELF is an explicit error.

### Verifying on Windows (Credential Manager)

Since v0.2.2 the Windows build opts into keyring's `windows-native` backend, so keys really
land in Credential Manager. keyring stores the OS entry as `<user>.<service>`, i.e.
`llm-api-key:deepseek.com.breakevery.riscdom`; `cmdkey /list:<filter>` only matches from the
start of that name, so use the full name or list everything and filter:

```text
cmdkey /list:llm-api-key:deepseek.com.breakevery.riscdom
cmdkey /list | findstr breakevery
```

`cargo test -p host --test keyring_os -- --ignored --nocapture` exercises the real store with a
throwaway `com.breakevery.riscdom.test` entry and cleans up after itself.

## Session persistence

Conversations are stored in `sessions.db` (SQLite, reusing `rusqlite`, no new dependency) in
the **app data directory**:

- Location: `app_data_dir/sessions.db` (registered by `ui/src-tauri` during setup);
  `RISCDOM_SESSION_DB_PATH` can override it; outside Tauri it falls back to
  `<temp>/riscdom/sessions.db`.
- Commands: `list_sessions` / `create_session` / `open_session` / `rename_session` /
  `delete_session` / `clear_all_sessions` / `get_current_session_id`.
- Audit events: `host.session.create` / `.open` / `.rename` / `.delete` (message content is
  **not** recorded).
- **Never persisted**: API keys, the raw system prompt, streaming intermediate state, audit
  events; deleting a session cascades to its messages.
- Restoring: `open_session` returns the history messages and `run_agent` injects them with
  `push_history` before starting a new `AgentLoop` (tool calls are not replayed).

## Security

- **The API key lives in memory only** (`AppState::llm_config`). It never reaches disk, the
  audit log, the logs or `get_llm_config_status`; its `Debug` shows only the first and last 4
  characters.
- File reads/writes go through `agent::WorkspacePolicy` (deny by default + traversal guard).
- The frontend cannot bypass host to call sandbox / agent.

## Event bridging and serial (current implementation)

Each `run_agent` starts two background threads (the serial forwarder is long-lived now, see
item 2):

1. **AuditBridge** (200 ms audit polling) → only handles `agent:iteration` /
   `agent:tool_call` / `agent:tool_result`, plus `vm.start` / `vm.stop` / `vm.snapshot.save`
   → `vm:state`.
2. **Serial forwarder (long-lived, 20b)** — created at app startup (`setup`), it **does not
   end with the run**: it owns the receiving end of a broadcast channel
   (`Receiver<Vec<u8>>`, polled with `recv_timeout(200ms)`), turns each frame into a string
   with lossy UTF-8, emits `serial:chunk` and accumulates into what `get_serial_buffer()`
   returns. The senders live in `AppState::serial_senders` and every run injects the same list
   through `AgentLoop::attach_serial()` — so it is **continuous across runs**.
3. **Stream forwarder** (LLM streaming) → reads the `Receiver<StreamEvent>` from
   `AgentLoop::subscribe_stream()`: `Delta` → `agent:stream:delta { text }`, `Done` →
   `agent:stream:done`; `ToolCallDelta` is **not forwarded** (tool calls are still handled by
   `agent:tool_call`). The UI **overwrites** streamed content with `agent:final`'s final
   content so byte differences cannot produce an inconsistency.

Serial is **no longer** derived from the audit log's `read_serial` tool results (the old
`serial_full_text` / `SerialDiff` are gone). Data is fanned out in real time by the sandbox's
serial reader thread through `VMConfig.serial_observer`; the `read_serial` tool semantics are
unchanged (it still returns the VM's whole buffer). The sandbox wraps observer panics in
`catch_unwind` and writes the `sandbox.serial.observer_panic` audit event.

Subscribers **only receive data from after subscribing** (no history replay); call the
`read_serial` tool for the full buffer.

VM lifecycle (20b): the VM is owned by `AppState::vm_slot`. `run_agent` injects that slot with
`AgentLoop::with_vm(...)`; the VM stays in the slot after the run (it is no longer destroyed
with the loop) and the next run reuses the same guest (calling `start_vm` again yields
"already running"). `stop_current_vm()` stops and clears the slot, and `vm_is_running()` lets
the UI decide whether buttons are enabled.

## Manual verification

The full real-API path (needs a key) was **executed and passed** on v0.1.0 (2026-09-14).

### Real DeepSeek end-to-end result (2026-09-14)

```text
cargo test -p agent -- --ignored --nocapture
real_deepseek_writes_and_runs_hello_world ... ok
AgentOutcome: Final, iterations = 6
serial (read_serial tool result):
  wrote 634 bytes to hello.c
  compiled hello.c -> hello.elf (ok)
  VM started (qmp=*****, serial=*****)
  HELLO RISCV
test time: about 7.6s (compile + QEMU boot); about 12.8s total
```

- Does the serial contain `HELLO RISCV`? yes
- Did the model use only allowlisted tools (write_source → compile → start_vm → read_serial →
  stop_vm)? yes
- Note: at the time this record was written the test itself asserted only "serial contains
  HELLO RISCV + not Failed" and had **no** `verify_chain` assertion (a coverage gap; since
  stage 21 the test asserts the chain as well). Chain-integrity evidence is in the mock e2e
  (`cargo test -p host -- --ignored`, Intact length 29).

### Full UI walkthrough

1. `cd ui && npm run tauri dev` to launch the desktop window.
2. Fill Key / Base URL / Model in the settings pane → "save for this session" (the status dot
   turns green and **never echoes the key**).
3. Type "write a RISC-V bare-metal Hello World" in the chat pane and send.
4. Expected: the left pane appends the agent's steps (tool calls as collapsible blocks) →
   then the final answer; the right canvas shows `HELLO RISCV`; the audit event count in the
   middle pane grows.
5. Close the window and restart → `configured == false` in the settings pane (confirming the
   key was not persisted).

### Key-less / local model degradation (12b)

1. Clear the configuration (settings → "clear") → a yellow banner appears at the top of the
   settings pane with `reason = no_config`.
2. Pick DeepSeek but save without a key → the API Key field reports `missing_api_key` (the
   save is rejected, nothing is written).
3. Pick Ollama (local) and save → `ready = true` and the banner disappears (local models need
   no key).
4. If Ollama / LM Studio is running locally, clicking "detect local models" on the banner
   should identify the provider and the model count; clicking "use" switches the preset and
   saves.
5. Sending a message while not ready → a system notice is appended to the message stream
   (`run_agent` is not called).

Supporting interfaces: `get_llm_readiness()`, `probe_local_llm()`. Each probe has a 1.5 s
timeout and fails silently.

The automated equivalent (mock LLM, no key needed):

```text
cargo test -p host -- --ignored --nocapture
```

It asserts: `agent:final` arrives, `serial:chunk` contains `HELLO RISCV`, `verify_chain` is
Intact.

## Tests

```text
cargo test -p host
```

- `tests/commands_smoke.rs`: status/config/files (including key-leak assertions)
- `tests/serial_polling.rs`: incremental emits neither duplicated nor lost; serial source
  filtering
