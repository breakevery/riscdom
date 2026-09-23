[English](README.md) | 中文

# riscdom —— 命令行控制平面客户端

`riscdom` 让你在 shell 里驱动 RiscDom 控制平面：与桌面应用和 AI 监工用的是同一套 HTTP + SSE 接口。它是一个**客户端**，不是进入内核的第二条路——每条命令都经控制平面，本地模式只不过是在本进程内、由内核挑一个回环端口把控制平面起起来。

```text
riscdom [options] <command> [args]
```

## 子命令

| 子命令 | 请求 | 得到 |
|---|---|---|
| `health` | `GET /v0/health` | 存活、版本、运行时长 |
| `status` | `GET /v0/status` | 连接数、订阅数、agents、`agent_id` |
| `agents` | `GET /v0/status` | agent 数量与身份（派生视图） |
| `runs list [--limit <n>]` | `GET /v0/runs` | 运行索引，新的在前（服务端默认 20） |
| `runs get <run_id>` | `GET /v0/runs/<run_id>` | 单个运行 |
| `audit status` | `GET /v0/audit/status` | 事件总数、链的判定、待报告失败 |
| `audit events [--limit <n>]` | `GET /v0/audit/events` | 最近事件，新的在前（默认 20） |
| `snapshots list` | `GET /v0/snapshots` | 已存快照 |

控制类子命令——全部是 HTTP `POST`，全部需要 token：

| 子命令 | 请求 | 得到 |
|---|---|---|
| `run <task> [--follow]` | `POST /v0/agent/run` | 一轮 agent 的结果；`--follow` 在运行时打印事件流 |
| `vm stop` | `POST /v0/vm/stop` | 先确认，再 `ok` |
| `vm start` | `POST /v0/vm/start` | `501`——预留：今天 VM 在运行内启动 |
| `snapshots save <name>` | `POST /v0/snapshots/save` | 写入字节数 |
| `snapshots resume <name>` | `POST /v0/snapshots/resume` | 先确认，再 `ok` |
| `snapshots delete <name>` | `POST /v0/snapshots/delete` | 是否真的删掉了 |
| `sessions create <title>` | `POST /v0/sessions/create` | 新会话的 `session_id` |
| `sessions open <session_id>` | `POST /v0/sessions/open` | 会话的 meta 与消息条数 |
| `sessions rename <id> <title>` | `POST /v0/sessions/rename` | `ok` |
| `sessions delete <session_id>` | `POST /v0/sessions/delete` | 先确认，再 `ok` |
| `sessions clear-all` | `POST /v0/sessions/clear` | 先确认，再 `ok` |
| `runs abandon-stale` | `POST /v0/runs/abandon-stale` | 标记了多少个遗留运行 |

### 确认

有五条命令会销毁状态——`vm stop`、`snapshots resume`、`snapshots delete`、
`sessions delete`、`sessions clear-all`——每一条动手前都会问：

- `--yes` 提前把问题回答掉。
- 在终端上，CLI 会问并读回答：`y` 或 `yes` 继续，其余都算拒绝。
- **不在**终端上时——脚本、管道、AI——没人可问，于是命令被拒，退出码 `2`。
  沉默不是同意。

唯一不销毁状态的控制命令是 `runs abandon-stale`：它标记进程已消失的运行，
跑一次和跑两次结果一样。

## 选项

| 选项 | 含义 |
|---|---|
| `--json` | 原样打印控制平面的 JSON |
| `--yes`、`-y` | 提前回答销毁类命令的确认 |
| `--follow`、`-f` | 仅 `run`：在运行时打印事件流 |
| `--remote <host:port>` | 连一个已在运行的 `riscdom-server`，而不是自己起一个 |
| `--data-dir <dir>` | settings、会话与 token 所在（默认：本平台宿主数据目录） |
| `--workspace <dir>` | 内嵌控制平面所属的 workspace（默认：当前目录） |
| `--token-file <path>` | 从文件读 bearer token |
| `--token <value>` | 在命令行上传 token——会落入 shell history，CLI 会打印警告 |
| `--limit <n>` | `runs list` / `audit events` 请求多少行 |
| `--help`、`-h` | 打印用法并退出 `0` |
| `--version`、`-V` | 打印版本并退出 `0` |

选项可以出现在子命令之前、之间或之后。

## 两种模式，同一条代码路径

- **本地（默认）。** CLI 在自己进程内把控制平面起在 `127.0.0.1:0`——由内核挑一个空闲回环端口，因此两次运行永不争端口——然后对它说 HTTP。什么都不留：命令结束进程即退出。
- **远程（`--remote host:port`）。** 连你已经跑着的 `riscdom-server`。适合另一台机器或容器里的 daemon。

两者走同一份客户端代码，所以本地能用的命令远程也能用，反之亦然。

## token

控制平面要求 `Authorization: Bearer <token>`（没有匿名模式；服务端 `--no-auth` 是唯一的例外，且会打印警告）。

- **本地模式**替你读 `<data-dir>/token`。首次运行时该文件还不存在，内嵌控制平面会**生成**它——与 `riscdom-server` 完全同一套逻辑，运维无需手抄 token。
- **远程模式**，按优先级：
  1. `--token-file <path>`——命令行上只有路径；
  2. `RISCDOM_TOKEN`——也不在命令行上；
  3. `--token <value>`——能用，但该值会落入 shell history 与 `ps`，CLI 会打印警告。

token 从不被打印、从不被记录：失败只说**哪个文件**读不到，或凭证被拒，仅此而已。

## 输出

- `--json` **原样透传**控制平面的应答——字段与命名与 `docs/control-plane-api.md` 一致。失败时把文档化的错误体（`{code, message, retryable, cause}`）打到 **stderr**。
- **人类模式**打印表格与短 `key value` 行，例如：

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

- `agents` 是唯一的派生视图：`--json` 仍透传 `/v0/status`（这是规则），人类模式只显示 `agents` 与 `agent_id`。
- 人类模式下失败打印 `code: message (cause: …)`。
- `--follow` 在运行期间每个事件帧打一行——事件名加一小段 payload——然后照旧打结果：

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

  带 `--json` 时，每个帧是原样的 envelope，结果是运行的 JSON。订阅在运行开始**之前**
  就已打开，因此运行产生的东西一个也不会漏。

## 退出码

| 码 | 含义 |
|---|---|
| `0` | 成功（2xx），或 `--help` / `--version` |
| `1` | 本地失败：连不上、token 文件读不到、workspace 打不开、runtime 起不来 |
| `2` | 用法错误，或控制平面驳回了请求（`400`） |
| `3` | 控制平面拒绝或失败（`404` / `405` / `409` / `5xx`） |
| `4` | 认证失败（`401` / `403`） |

脚本可以依赖它们：`riscdom health --json || handle_failure "$?"`。

## 示例

```bash
# 活着吗？
riscdom health --json

# 最近 5 个运行，JSON 输出，连已经跑着的服务端。
riscdom --json --remote 127.0.0.1:7821 runs list --limit 5

# 这个 workspace 的审计链判定。
riscdom audit status

# 不落在 shell history 里的 token。
riscdom --remote box.example:7821 --token-file ~/.riscdom/token audit events --limit 50

# 跑一轮 agent，同时打印事件流。
riscdom run "print hello over the serial console" --follow

# 删快照：终端上会问，脚本里用 --yes。
riscdom snapshots delete after-blink --yes

# 会话重新开始。
riscdom sessions clear-all --yes
```

## 与 `riscdom-server` 的关系

`riscdom-server` 是作为长期进程的控制平面；`riscdom` 是它的客户端。一次性命令由 CLI 在调用期间起一个控制平面；若要有东西稍后再连上来（daemon），就起 `riscdom-server`，再用 `--remote` 指过去。
