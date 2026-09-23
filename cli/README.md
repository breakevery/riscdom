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

Everything here is read-only. The control commands (`run`, `vm stop`, …) and
`--follow` arrive in a later batch.

## Options

| Option | Meaning |
|---|---|
| `--json` | print the control plane's JSON, unchanged |
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
```

## Relationship to `riscdom-server`

`riscdom-server` is the control plane as a long-lived process; `riscdom` is the
client. For a one-off command the CLI starts a control plane for the duration of
the call. For a daemon — something else will connect later — start
`riscdom-server` and point the CLI at it with `--remote`.
