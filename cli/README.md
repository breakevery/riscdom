[中文](README.zh-CN.md) | English

# riscdom — the command-line control-plane client

`riscdom` drives the RiscDom control plane from a shell: the same HTTP + SSE
interface the desktop app and an AI supervisor use. It is a **client**, not a
second way into the kernel — every command goes through the control plane, and the
local mode simply starts the control plane inside this process on a loopback port
the OS picks.

```text
riscdom [options] <command> [args]
```

## Commands

| Command | Asks for | Answers |
|---|---|---|
| `health` | `GET /v0/health` | liveness, version, uptime |
| `status` | `GET /v0/status` | connections, subscribers, agents, `agent_id` |
| `agents` | `GET /v0/status` | the agent count and identity (a derived view) |
| `runs list [--limit <n>]` | `GET /v0/runs` | the run index, newest first (the server defaults to 20) |
| `runs get <run_id>` | `GET /v0/runs/<run_id>` | one run |
| `audit status` | `GET /v0/audit/status` | event count, chain verdict, pending failures |
| `audit events [--limit <n>]` | `GET /v0/audit/events` | recent events, newest first (default 20) |
| `snapshots list` | `GET /v0/snapshots` | stored snapshots |
| `sandboxes list` | `GET /v0/sandboxes` | the merged sandbox registry, with `current` and `default` |
| `sandboxes current` | `GET /v0/sandboxes/current` | the definition a run would use, and the fallback's name |
| `sandboxes candidates` | `GET /v0/sandboxes/candidates` | what is installed here: the two independent lists, unmerged |
| `sandboxes show <name>` | `GET /v0/sandboxes/<name>` | one definition, one `key value` line per field |

Control commands — every one an HTTP `POST`, and every one needs the token:

| Command | Asks for | Answers |
|---|---|---|
| `run <task> [--follow]` | `POST /v0/agent/run` | one agent turn's outcome; `--follow` prints the event stream while it runs |
| `vm stop` | `POST /v0/vm/stop` | confirmation, then `ok` |
| `vm start` | `POST /v0/vm/start` | `501` — reserved: today the VM starts inside a run |
| `snapshots save <name>` | `POST /v0/snapshots/save` | how many bytes were written |
| `snapshots resume <name>` | `POST /v0/snapshots/resume` | confirmation, then `ok` |
| `snapshots delete <name>` | `POST /v0/snapshots/delete` | whether anything was deleted |
| `sessions create <title>` | `POST /v0/sessions/create` | the new `session_id` |
| `sessions open <session_id>` | `POST /v0/sessions/open` | the session's meta and how many messages it holds |
| `sessions rename <id> <title>` | `POST /v0/sessions/rename` | `ok` |
| `sessions delete <session_id>` | `POST /v0/sessions/delete` | confirmation, then `ok` |
| `sessions clear-all` | `POST /v0/sessions/clear` | confirmation, then `ok` |
| `runs abandon-stale` | `POST /v0/runs/abandon-stale` | how many stale runs were abandoned |
| `sandboxes switch <name>` | `POST /v0/sandboxes/switch` | confirmation, then `switched from <old> to <new>` (or `switched to <new>` when nothing was current) |

### Confirmation

Nine commands destroy or replace state — `vm stop`, `snapshots resume`, `snapshots delete`,
`sessions delete`, `sessions clear-all`, `llm clear`, `qemu clear`, `toolchain clear`,
`sandboxes switch` — and each one asks before it does:

- `--yes` answers the question up front.
- On a terminal the CLI asks and reads the answer: `y` or `yes` continues, anything
  else declines.
- **Not** on a terminal — a script, a pipe, an AI — there is nobody to ask, so the
  command is refused and exits `2`. Silence is not consent.

The three clears ask because what they remove cannot be read back out of the host:
the API key has to be typed again, and the path the host auto-detected is no longer
on record.

Nothing else asks. `runs abandon-stale` only marks runs whose process is gone, and
running it twice is the same as running it once.

### Export commands

Three `POST`s that hand the *server* a path to write:

| Command | Asks for | Answers |
|---|---|---|
| `export audit-jsonl [--out <path>]` | `POST /v0/audit/export` | how many events were written, and where |
| `export run-audit <run_id> [--out <path>]` | `POST /v0/runs/export` | the same, for one run's self-contained chain |
| `export serial-log [--out <path>]` | `POST /v0/serial/export` | how many bytes were written |

- **`--out` is the server's path, not the CLI's.** It is resolved against the
  workspace root, so a relative name is a workspace file and the host writes it;
  a path that escapes the workspace (a `..`, an absolute path outside) is the
  caller's parameter being unusable, and is refused with `400` and
  `cause: "path"`. The CLI never receives the file's contents.
- **The defaults** are `audit.jsonl`, `run-<run_id>.jsonl` and `serial.log`, all
  relative to the workspace.
- **What the number counts** differs, and the field names say so: the two audit
  exports answer `events_exported`, the serial export answers `bytes_written`.
- **The parent directory has to exist** — an export does not create directories.
- An unknown `run_id` is `404`; a run that is still open has nothing to close its
  record and is refused.

### Configuration commands

| Command | Asks for | Answers |
|---|---|---|
| `llm set --api-key <key> --base-url <url> --model <model> [--provider-id <id>] [--remember]` | `POST /v0/llm/config` | `ok` |
| `llm set --api-key-file <path> …` | the same endpoint | the same, with the key never on the command line |
| `llm clear` | `POST /v0/llm/config/clear` | confirmation, then `ok` |
| `llm load-key <provider_id>` | `POST /v0/llm/stored-key/load` | `ok`, or `404` when nothing is stored for that provider |
| `qemu path <file>` | `POST /v0/qemu/path` | `ok`, or `400` when the file will not run |
| `qemu clear` | `POST /v0/qemu/path/clear` | confirmation, then `ok` |
| `qemu download [--wait]` | `POST /v0/qemu/download` | today `503` with the install guidance — no QEMU release is pinned |
| `qemu cancel` | `POST /v0/qemu/download/cancel` | `409` when nothing is running |
| `qemu status` | `GET /v0/qemu/download` | whether a download is running, and the last event |
| `toolchain download [--wait]` | `POST /v0/toolchain/download` | `download started` (`202`) |
| `toolchain cancel` | `POST /v0/toolchain/download/cancel` | `download cancelling`, or `409` when nothing is running |
| `toolchain path <file>` | `POST /v0/toolchain/path` | `ok`, or `400` when the file will not run |
| `toolchain clear` | `POST /v0/toolchain/path/clear` | confirmation, then `ok` |
| `preflight run [--wait]` | `POST /v0/preflight/run` | `preflight running` (`202`) |
| `preflight ack` | `POST /v0/preflight/ack` | the four steps and the verdict |
| `audit alert set <on\|off>` | `POST /v0/audit/alert` | `ok` |
| `theme set <light\|dark\|system>` | `POST /v0/settings/theme` | `ok`, or `400` naming the three values |
| `language set <system\|en\|zh>` | `POST /v0/settings/language` | `ok`, or `400` naming the three values |

- **`--api-key` warns** exactly the way `--token` does: the key lands in the shell
  history and in `ps`. `--api-key-file` is the shape to prefer, and does not warn.
- **`--remember`** also stores the key in the OS credential store; without it the
  key lives only in the running host, and the next start will not have it.
- **`--wait`** subscribes to the event stream *before* starting the work, prints
  the frames that belong to it (`toolchain:download`, `qemu:download`,
  `preflight:progress`) and closes with `download ok` / `preflight failed`. The exit
  code is the **work's** verdict: a failed download or a failed preflight exits `3`.
  Without `--wait` the command prints the `202` acknowledgement and returns at once.
- **The two path setters hand the host a file** and it checks that the file exists
  *and runs* (`--version`); that is why `qemu path` / `toolchain path` can answer
  `400` for a path that looks fine.
- **`qemu download` refuses on every platform, by decision**: RiscDom guides the user
  to a QEMU they install themselves (`docs/qemu-distribution.md` §5) and pins no
  release, so the control plane answers `503 unavailable` with the install guidance
  and the CLI prints it. `qemu status` still works (it reports idle), and the three
  commands are wired end to end, so pinning a release later would be a data change.
- **The vocabularies are the server's** (`theme`, `language`): the CLI passes the
  value through and does not second-guess it.

## Options

| Option | Meaning |
|---|---|
| `--json` | print the control plane's JSON, unchanged |
| `--yes`, `-y` | answer a destructive command's confirmation up front |
| `--follow`, `-f` | `run` only: print the event stream while the run is going |
| `--wait`, `-w` | `toolchain download` / `preflight run` only: print progress until the work finishes |
| `--out <path>` | where an export writes (server-side, resolved against the workspace) |
| `--api-key <key>` | the model's API key — it lands in the shell history, so the CLI warns |
| `--api-key-file <path>` | read the API key from a file instead |
| `--base-url <url>` | the model endpoint |
| `--model <model>` | the model's name |
| `--provider-id <id>` | the provider preset `llm set` configures |
| `--remember` | `llm set`: also store the key in the OS credential store |
| `--remote <host:port>` | talk to a running `riscdom-server` instead of starting one here |
| `--data-dir <dir>` | where settings, sessions and the token live (default: this platform's host data dir) |
| `--workspace <dir>` | the workspace the embedded control plane owns (default: the current directory) |
| `--token-file <path>` | read the bearer token from a file |
| `--token <value>` | pass the token on the command line — it lands in the shell history, so the CLI warns |
| `--limit <n>` | how many rows `runs list` / `audit events` ask for |
| `--help`, `-h` | print the usage and exit `0` |
| `--version`, `-V` | print the version and exit `0` |

Options may appear before, between or after the command words.

## Two modes, one code path

- **Local (default).** The CLI starts the control plane inside its own process,
  bound to `127.0.0.1:0` — a loopback port the OS picks, so two runs never fight
  over a port — and then speaks HTTP to it. Nothing is left behind: the process
  exits when the command is done.
- **Remote (`--remote host:port`).** The CLI talks to a `riscdom-server` you are
  already running. Useful for a daemon on another machine or in a container.

Both go through the same client code, so a command that works locally works
remotely, and vice versa.

## The token

The control plane requires `Authorization: Bearer <token>` (there is no anonymous
mode; `--no-auth` on the server is the only opt-out and it prints a warning).

- **Local mode** reads `<data-dir>/token` for you. On the very first run the file
  does not exist yet and the embedded control plane **generates** it, exactly as
  `riscdom-server` does — the operator never has to copy a token by hand.
- **Remote mode**, in order of preference:
  1. `--token-file <path>` — only a path is on the command line;
  2. `RISCDOM_TOKEN` — not on the command line either;
  3. `--token <value>` — works, but the value lands in your shell history and in
     `ps`; the CLI prints a warning when you use it.

The token is never printed and never logged: a failure says *which file* was
unreadable, or that the credential was refused, and nothing more.

## Output

- `--json` passes the control plane's answer through **unchanged** — the same
  fields, the same names as in `docs/control-plane-api.md`. A failure prints the
  documented error object (`{code, message, retryable, cause}`) on **stderr**.
- **Human mode** prints tables and short `key value` lines, e.g.

  ```text
  $ riscdom status
  status          ok
  version         0.8.0
  uptime_ms       1997
  connections     1
  sse_subscribers 0
  agents          1
  agent_id        local-17480-1

  $ riscdom runs list
  RUN_ID                       STATUS     STARTED_MS      ENDED_MS
  local-17480-1                ok                100           200

  $ riscdom sandboxes list
  current    blink
  default    blink

  NAME                         SOURCE      RUNNABLE  SHADOWED  MEMORY_MB
  blink                        manual      true      false     256
  default                      discovered  true      false     -

  $ riscdom sandboxes candidates
  TOOLCHAINS (1)
    KIND       VERSION      ORIGIN     RUNNABLE  PATH
    toolchain  15.2.0-1     installed  true      C:\data\toolchain\15.2.0-1\bin\riscv64-unknown-elf-gcc.exe
  QEMUS (1)
    KIND       VERSION      ORIGIN     RUNNABLE  PATH
    qemu       11.1.0       installed  true      C:\data\qemu\11.1.0\qemu-system-riscv64.exe
  ```

- `agents` is the one derived view: `--json` still passes `/v0/status` through
  (that is the rule), while human mode shows only `agents` and `agent_id`.
- A failure in human mode prints `code: message (cause: …)`.
- `--follow` prints one line per event frame while the run is going — the event
  name and a short payload — and then the outcome, exactly as without the flag:

  ```text
  $ riscdom run "print hello over the serial console" --follow
  agent:llm.stream.start {"iteration":1}
  agent:tool_call {"name":"write_source","arguments":{"path":"src/main.c"}}
  serial:chunk {"chunk":"hello from riscv\n"}
  agent:final {"kind":"final"}
  kind       final
  iterations 3

  Hello from the sandbox.
  ```

  With `--json`, each frame is the envelope verbatim, and the outcome is the JSON
  of the run. The subscription is opened *before* the run starts, so nothing the
  run produces is missed.
- `--wait` prints the same way, restricted to the events that belong to the work
  (`toolchain:download` / `preflight:progress`), and closes with one word:

  ```text
  $ riscdom toolchain download --wait
  toolchain:download {"install_path":"…","state":"done"}
  download ok
  ```

  Under `--json` there is no closing line — the frames are the output, and the
  exit code is the summary.
- An export says what it wrote and where: `exported 1 event to audit.jsonl`,
  `wrote 4096 bytes to serial.log`.
- The three clears and `preflight ack` print `ok` or the preflight's step table.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | success (a 2xx answer), or `--help` / `--version` |
| `1` | a local failure: no connection, no token file, no workspace, no runtime |
| `2` | a usage error, or the control plane rejected the request (`400`) |
| `3` | the control plane refused or failed (`404` / `405` / `409` / `5xx`) |
| `4` | authentication failed (`401` / `403`); a workspace path the policy refuses is the `400` above |

Scripts can rely on these: `riscdom health --json || handle_failure "$?"`.

## Examples

```bash
# Is it alive?
riscdom health --json

# The newest five runs, as JSON, from a server that is already running.
riscdom --json --remote 127.0.0.1:7821 runs list --limit 5

# The audit chain's verdict on this workspace.
riscdom audit status

# A token that does not live in your shell history.
riscdom --remote box.example:7821 --token-file ~/.riscdom/token audit events --limit 50

# Run one agent turn and print the event stream while it goes.
riscdom run "print hello over the serial console" --follow

# Delete a snapshot: a prompt on a terminal, `--yes` in a script.
riscdom snapshots delete after-blink --yes

# Start over on sessions.
riscdom sessions clear-all --yes

# Export the chain and a run's record into the workspace.
riscdom export audit-jsonl --out audit.jsonl
riscdom export run-audit local-17480-3

# Configure the model without putting the key in the shell history.
riscdom llm set --api-key-file ~/.riscdom/api-key --base-url https://api.deepseek.com \
  --model deepseek-chat --remember

# Download the pinned toolchain and watch it finish.
riscdom toolchain download --wait
```

## Relationship to `riscdom-server`

`riscdom-server` is the control plane as a long-lived process; `riscdom` is the
client. For a one-off command the CLI starts a control plane for the duration of
the call. For a daemon — something else will connect later — start
`riscdom-server` and point the CLI at it with `--remote`.
