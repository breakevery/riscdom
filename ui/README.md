[中文](README.zh-CN.md) | English

# ui

The RiscDom desktop frontend (Tauri 2 + React + TypeScript + Vite).

## Three-pane layout

- **Left · chat** (`panels/ChatPanel.tsx`): the message list (user / assistant / tool) plus
  the input box at the bottom. Tool calls render as collapsible blocks (tool name +
  arguments + result). While a run is in flight the input box is disabled and shows
  "thinking…".
- **Middle · settings** (`panels/SettingsPanel.tsx`): LLM configuration, workspace, audit
  status and recent events.
- **Right · serial canvas** (`panels/CanvasPanel.tsx`): an xterm.js terminal that shows
  guest serial output live, with a VM status bar, a clear button and serial-log export on
  top. Serial output **accumulates across runs** (it is never cleared automatically; use
  "clear" when you want a blank terminal).

The three panes use CSS Grid with two draggable splitters (`layout/AppShell.tsx`, no
third-party splitter library).

## Running

```powershell
npm install
npm run build        # tsc + vite build
npm run tauri dev    # launch the desktop app (needs the Rust toolchain)
```

`npm run tauri build` produces installers (icons etc. are still MVP-rough).

## Setting the API key (this session only)

Fill in API Key / Base URL / Model in the **settings** pane and click "save for this
session".

- The key only lives in backend memory (`host::AppState::llm_config`);
- it is **never** written to localStorage / sessionStorage / console / the audit log / disk;
- the frontend clears the input box immediately after saving;
- status read-outs echo `base_url` / `model` and **never the key**.

## Architecture

```
frontend (React)  --invoke/listen-->  ui/src-tauri (Tauri shell)  -->  host crate
                                                                        |--> agent
                                                                        |--> sandbox
                                                                        |--> audit
```

- All `invoke` calls are centralised in `src/api/tauri.ts` so they are easy to audit.
- The frontend **never** touches QEMU / gcc / Rust crates directly.
- LLM and serial content is never rendered with `innerHTML` / `dangerouslySetInnerHTML`
  (XSS guard).

## Events

`agent:iteration` / `agent:tool_call` / `agent:tool_result` / `agent:final` /
`serial:chunk` / `vm:state`. See `host/README.md`.

## Tests

End-to-end (mock LLM, needs QEMU + the RISC-V toolchain):

```text
cargo test -p host -- --ignored --nocapture
```

Real-API end-to-end (needs a key):

```powershell
$env:DEEPSEEK_API_KEY = "***"
cargo test -p agent -- --ignored --nocapture
npm run tauri dev   # or drive the UI by hand
```

## Snapshot panel

The "snapshots" section of the settings pane lists snapshots under
`<workspace>/.riscdom/snapshots`:

- labelled "**real**" (`.mig`, a QMP migration stream) or "**reboot**" (`.json`, the old
  fallback);
- refresh and delete are supported (delete asks for confirmation);
- "**save current state**": stores the running host-owned VM as a real snapshot (the button
  is disabled while no VM runs);
- "**restore**" (real snapshots only): after confirmation the current VM is stopped and
  restored from the snapshot (see `host/README.md`).

## Session persistence

Conversations are saved automatically; after a restart you can open / rename / delete them
from the "sessions" panel at the top of ChatPanel.

- Storage: `sessions.db` (SQLite) under the **app data directory** — not in the repository
  and not in the AI workspace.
- The `RISCDOM_SESSION_DB_PATH` environment variable can override the path.
- Clearing: delete entries one by one in the panel, or call `clear_all_sessions` (the UI asks
  for confirmation).
- **Never persisted**: API keys, the raw system prompt, streaming intermediate state, audit
  events.
- Restoring a session injects history messages into `AgentLoop` and **never replays** tool
  calls.

## v0.2 TODO

- keyring persistence (the OS keyring, replacing session-only storage)
- streaming output (SSE, token by token)
- sessions: search / tags / import-export / encryption (all v0.2 follow-ups)
- serial moved to sandbox push callbacks (dropping host polling)
