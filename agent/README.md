[中文](README.zh-CN.md) | English

# agent

The RiscDom **AI agent runtime**: natural language drives an LLM to write C / assembly,
compile, run, read the serial console and iterate inside a RISC-V virtual sandbox.

Dependency direction: `agent → sandbox`, `agent → audit`.

## Modules

- `config` — `AgentConfig` (read from environment variables; `Debug` masks the API key)
- `message` — `ChatMessage` / `ChatRequest` / `ChatResponse` / `ToolCall` (DeepSeek-compatible)
- `llm` — the `LlmClient` trait (`chat` + `chat_stream`) + `OpenAiCompatClient` (an
  OpenAI-compatible HTTP client; `DeepSeekClient` is a backwards-compatible alias) + `MockLlm`
  (scripted test double)
- `audit_hook` — shared audit-writing helpers
- `policy` — `WorkspacePolicy` (capability policy, deny by default)
- `compiler` — `compile_freestanding` (C → bare-metal ELF, injecting crt0)
- `tools` — the tool set and `execute_tool`
- `prompt` — system prompt construction
- `agent` — `AgentLoop` / `AgentOutcome`

## Provider presets

The `presets` module is **pure data**: it describes how to reach an OpenAI-compatible endpoint
and contains no provider-specific logic.

| id | name | base_url | default model | needs key | local |
| --- | --- | --- | --- | --- | --- |
| `deepseek` (default) | DeepSeek | `https://api.deepseek.com` | `deepseek-chat` | yes | no |
| `openai` | OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` | yes | no |
| `ollama` | Ollama (local) | `http://localhost:11434/v1` | `qwen2.5-coder` | no | yes |
| `lmstudio` | LM Studio (local) | `http://localhost:1234/v1` | (user fills) | no | yes |
| `custom` | custom | (user fills) | (user fills) | no | no |

- `DEFAULT_PRESET_ID = "deepseek"`; `find_preset(id)` looks one up; `builtin_presets()` returns
  all of them.
- `AgentConfig::from_preset(preset, api_key)`: fills `base_url` / `model` from the preset;
  `requires_key=true` without a key → **an explicit error** (never silent); local presets
  accept an empty key; `validate()` runs last.
- "custom" lets users point at **any** OpenAI-compatible service (including a self-hosted
  gateway).

## Configuration

The LLM client speaks the **OpenAI-compatible protocol** (`OpenAiCompatClient`): point
`base_url` / `model` at any compatible provider (DeepSeek is the default preset). `api_key`
may be empty for a local address (localhost / 127.0.0.1) and is required for a cloud address.

> The v0.2 OpenAI-compatible refactor is complete (this stage); the provider dropdown, local
> models (Ollama / LM Studio) and the key-less experience are tracked in
> [PROJECT_CONSTITUTION.md §10](../PROJECT_CONSTITUTION.md).

| environment variable | required | default |
| --- | --- | --- |
| `DEEPSEEK_API_KEY` | yes | — |
| `DEEPSEEK_BASE_URL` | no | `https://api.deepseek.com` |
| `DEEPSEEK_MODEL` | no | `deepseek-chat` |
| `RISCDOM_MAX_ITERATIONS` | no | `10` |
| `RISCDOM_REQUEST_TIMEOUT` | no | `120` |
| `RISCDOM_RISCV_GCC` | no | `D:\tools\...\riscv64-unknown-elf-gcc.exe` |
| `RISCDOM_QEMU` | no | `C:\Program Files\qemu\qemu-system-riscv64.exe` |

PowerShell:

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."
```

**The API key is never written** into code, the repository, audit events, error messages or
Debug output (only the first 4 and last 4 characters are ever shown).

## Running the tests

Mock (no network / no key):

```text
cargo test -p agent
```

Real DeepSeek end-to-end (needs a key + network + QEMU + the toolchain):

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."
cargo test -p agent -- --ignored --nocapture
```

That test's audit backend is **file SQLite** (`%TEMP%\riscdom-audit-real-*.db`): after the run
a **new handle** reopens the same database and `audit::verify_chain` must return
`ChainStatus::Intact` (with `length > 0`), plus at least one each of `agent.llm.request` /
`agent.tool.call` / `agent.tool.result`. The temp database is removed by a `Drop` guard (it is
deleted on failure and panic too).

## Tools

| tool | description |
| --- | --- |
| `write_source(path, content)` | write a C / assembly source file (`.c/.h/.S/.s` only) |
| `compile(source_path, output_elf)` | compile to a bare-metal ELF (load address `0x80000000`) |
| `start_vm(elf_path)` | start QEMU through the `sandbox` crate |
| `read_serial()` | read the current serial buffer |
| `stop_vm()` | stop QEMU |
| `list_workspace()` | list workspace files |
| `request_sandbox(action, sandbox?, reason?)` | leave a sandbox request for someone who may switch (v0.9 sandbox F2c) |
| `sandbox_status()` | what the node runs now, and what is waiting |

## The audit events the agent writes

One row per thing that happened, through `audit_hook`:

| action | when | detail |
| --- | --- | --- |
| `agent.user.input` | a turn starts | the input, hashed and truncated |
| `agent.llm.request` / `.response` | every model call | sizes and a hash, never the raw body — **never the API key** |
| `agent.llm.stream.start` / `.end` | a streamed answer | the same discipline as the block path |
| `agent.tool.call` | before a tool runs | `{id, name, arguments, arguments_len}` — the arguments are **truncated at 4 KiB**, which a source file reaches easily |
| `agent.tool.result` | after a tool runs | `{call_id, ok, result_len, result}` |
| `agent.file.write` | **`write_source` wrote a file** (v0.9 project in/out) | `{path, bytes}` — the workspace-relative path the tool named, and how much landed |
| `agent.compile.start` / `.result` | a build | the compiler, the source and the outcome |
| `agent.policy.deny` | a path the policy refused | the tool, the path, the reason |

`agent.file.write` exists because "which files did the AI write" is a question about the
project, and the answer should not have to be re-derived from `agent.tool.call`'s
truncated arguments. It is an **audit** row, not an SSE event: it belongs on the chain
(where a project's provenance is provable), not on the event stream.

## Capability policy

`WorkspacePolicy` **denies by default**:

1. Any path containing `..` is rejected outright (traversal guard).
2. Resolved to an absolute path it must stay inside the workspace root, otherwise rejected.
3. Writes additionally require an allowlisted extension (`.c/.h/.S/.s`).
4. Every rejection writes an `agent.policy.deny` audit event.

Tools always pass the policy check before executing; the agent never calls QEMU directly, only
through the `sandbox` crate.

## Loop and context

- Each turn: `llm.chat` → if there are `tool_calls`, execute them one by one and feed back
  `tool` messages; otherwise return `Final`.
- **Validation first**: `run()` starts with `config.validate()`; on failure it returns
  `AgentOutcome::Failed { iterations: 0 }` without issuing a single LLM request.
- The cap is `max_iterations` (default 10); reaching it returns `MaxIterations` and never loops
  forever.
- Context trimming: beyond 40 messages it keeps the system message plus the last 30 and inserts
  a truncation marker.
- A single message (user input / tool result) is capped at 8 KB.
- Serial content is treated as **data**, never as instructions (the prompt-injection guard).

## A note on the compiler

Besides the spec-suggested `-Ttext=0x80000000`, `compile` also uses `-mcmodel=medany` and links
a generated linker script plus an injected `crt0`: `crt0` provides `_start` (placed first in
the image), sets up `sp` and then `call main`. The AI therefore only writes
`int main(void) { ... }`.

Why: in this environment the `medlow` model truncates relocations at `0x80000000`, and with
`-bios none` QEMU jumps to `0x80000000`, so the entry must be first in the image. See
`sandbox/README.md` and `ENVIRONMENT.md`.

## Serial subscriptions

`AgentLoop::subscribe_serial() -> std::sync::mpsc::Receiver<Vec<u8>>`: each call creates a new
channel (the sender is kept internally) and returns the receiver. Starting a VM sets
`VMConfig.serial_observer` to a shared observer that fans the sandbox's **real-time** serial
frames out to every subscriber.

- Only data from **after** subscribing arrives (no history replay); call the `read_serial`
  tool when you need everything.
- Once a receiver closes, the next event removes its sender from the list automatically (no
  panic).
- No tokio dependency (pure `std::sync::mpsc`).

## Streaming responses

`LlmClient::chat_stream(req, on_event) -> ChatResponse`: `on_event` receives `StreamEvent`s:

- `Delta(String)` — incremental text
- `ToolCallDelta { index, id, name, args_delta }` — **tool-call fragments** (not rendered
  character by character; buffered per `index` until complete)
- `Done` — the stream finished

- The default implementation **degrades to `chat`** (the whole content arrives as one `Delta`),
  so any `LlmClient` works; `OpenAiCompatClient` overrides it with real SSE (`stream: true` +
  line-by-line `data:` parsing) and `MockLlm` chunks text 8 characters at a time for testing.
- SSE parsing lives in the `sse` module (`parse_sse_line` / `SseAccumulator`) and covers blank
  lines, comments, `data:` with and without a space, multi-line data joining and `[DONE]`.
- Network/JSON errors return `Err`, never panic.
- Real-API streaming test:
  `cargo test -p agent --test stream_real -- --ignored --nocapture`.

### Subscriptions (`AgentLoop`)

`AgentLoop::subscribe_stream() -> std::sync::mpsc::Receiver<StreamEvent>`: each call creates a
new channel (the sender is kept internally) and `run` fans the same `StreamEvent` out to every
subscriber; a closed receiver is dropped automatically.

### Audit events

| action | detail |
| --- | --- |
| `agent.llm.stream.start` | `{ model, base_url_host }` |
| `agent.llm.stream.end` | `{ chunks, duration_ms, has_tool_calls }` |
| `agent.llm.request` / `agent.llm.response` | unchanged (response still records the full response) |

Audit events are **not** recorded per chunk.

## v0.2 TODO

- `OpenAiCompatClient` + local model support: refactor `DeepSeekClient` into a generic
  OpenAI-compatible client with built-in DeepSeek (default) / OpenAI / Ollama (local) /
  LM Studio (local) presets, including key-less local models
- gdbstub integration (debugging)
- multi-turn session persistence (cross-turn context)
- ~~`real_api` asserts `verify_chain`~~ (done, stage 21: file SQLite + independent handle)
