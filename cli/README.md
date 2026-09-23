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

### Confirmation

Five commands destroy state — `vm stop`, `snapshots resume`, `snapshots delete`,
`sessions delete`, `sessions clear-all` — and each one asks before it does:

- `--yes` answers the question up front.
- On a terminal the CLI asks and reads the answer: `y` or `yes` continues, anything
  else declines.
- **Not** on a terminal — a script, a pipe, an AI — there is nobody to ask, so the
  command is refused and exits `2`. Silence is not consent.

The one non-destructive control command is `runs abandon-stale`: it marks runs
whose process is gone, and running it twice is the same as running it once.

## Options

| Option | Meaning |
|---|---|
| `--json` | print the control plane's JSON, unchanged |
| `--yes`, `-y` | answer a destructive command's confirmation up front |
| `--follow`, `-f` | `run` only: print the event stream while the run is going |
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

## Exit codes

| Code | Meaning |
|---|---|
| `0` | success (a 2xx answer), or `--help` / `--version` |
| `1` | a local failure: no connection, no token file, no workspace, no runtime |
| `2` | a usage error, or the control plane rejected the request (`400`) |
| `3` | the control plane refused or failed (`404` / `405` / `409` / `5xx`) |
| `4` | authentication failed (`401` / `403`) |

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
```

## Relationship to `riscdom-server`

`riscdom-server` is the control plane as a long-lived process; `riscdom` is the
client. For a one-off command the CLI starts a control plane for the duration of
the call. For a daemon — something else will connect later — start
`riscdom-server` and point the CLI at it with `--remote`.
