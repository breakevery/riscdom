[English](control-plane-client-guide.md) | 中文

# 编写控制平面客户端

> **适用 v0.9（v1.0 前不稳定）。** 本指南写给**写客户端的人**：如何调用查询端点、如何读应答、如何处理错误、以及如何订阅事件流。规范表格在 [control-plane-api.zh-CN.md](control-plane-api.zh-CN.md) 与 [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)；本文是可照着做的走查。

这里全部是普通 HTTP。没有 SDK、没有生成式客户端：接口面就是 HTTP 接口，所以 `curl`、`fetch`、或任何 HTTP 库都够用。

## 1. 第一次调用之前

把 server 指向你要查看的 workspace 启动（参数见 [../server/README.zh-CN.md](../server/README.zh-CN.md)）：

```bash
riscdom-server --bind 127.0.0.1:7821 --workspace ./my-workspace
```

每个请求都带凭证：

```bash
curl -sS http://127.0.0.1:7821/v0/health \
  -H 'Authorization: Bearer <token>'
```

**token 默认必需。** 首次启动时服务端把 32 字节随机值写入 `<data-dir>/token`（仅属主可读），此后不带它的请求一律拒绝，所以请读出该文件并原样发出：

```bash
export RISCDOM_TOKEN="$(cat /path/to/data-dir/token)"
curl -sS http://127.0.0.1:7821/v0/health -H "Authorization: Bearer $RISCDOM_TOKEN"
```

token **从不被打印或记入日志**——启动行只报文件路径，不报值——所以文件是拿到它的唯一地方。部署方也可以自行放置该文件。`--no-auth` 取消该要求（会打印警告）：仅用于本地调试，因为控制端点里含破坏性操作。`401` 表示凭证缺失或错误；`403` 表示凭证已认，但它的 actor 不持有该端点的 capability（见下）。

两个无需参数的调用是存活检查与概要：

```bash
curl -sS http://127.0.0.1:7821/v0/health
# {"status":"ok","version":"0.8.0","uptime_ms":1971}

curl -sS http://127.0.0.1:7821/v0/status
# {"agents":1,"agent_id":"local-17480-1","connections":1,"sse_subscribers":0,
#  "status":"ok","uptime_ms":1997,"version":"0.8.0"}
```

`connections` 是打开的 TCP 连接数；`sse_subscribers` 是存活的事件流数；`agents` 是本宿主知道的 agent 数。

### capability：一份凭证能做什么

认证与授权是两个决定。`401` 表示服务端不接受该凭证；`403` 表示它接受了，但它解析出的 actor 不被允许做**这件事**。

每个端点要求一个 capability，名字见 API 文档 §5 表格——共 32 个，例如 `agent.run`、`audit.read`、`runs.control`、`settings.write`、`vm.control`。服务端在运行处理器前检查，客户端因此可以事先规划，而不是撞上才知道：

```bash
# token 持有者持有全部 32 项，此调用成功。
curl -sS -o /dev/null -w '%{http_code}\n' http://127.0.0.1:7821/v0/status \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
# 200

# 完全没凭证：在考虑 capability 之前就被拒。
curl -sS http://127.0.0.1:7821/v0/status
# {"code":"unauthorized","message":"missing or invalid bearer token","retryable":false,"cause":null}
# 401

# 通过了认证，但 actor 不持有该 capability。
curl -sS -X POST http://127.0.0.1:7821/v0/sessions/clear \
  -H "Authorization: Bearer $RISCDOM_TOKEN"
# {"code":"forbidden","message":"the actor may not session.write","retryable":false,"cause":"capability"}
# 403
```

第三种在 v0.9 默认下不会出现：token 持有者持有全部，`NoAuth`（`--no-auth`）发放同一集合。它出现在分发方自己的 `Authn` 钩子返回更窄 actor 时——也正是「认证通过不等于获准」、客户端要读 `cause` 而不是凭假设的原因。规则是默认拒绝：除非凭证的 actor 恰好持有该路由声明的 capability，该端点一律拒绝。

### 一个安全的部署

回环默认不是走过场：这个构建说的是明文 HTTP。再往外走一步的部署里，有三件事属于它。

1. **token 文件只给属主。** `<data-dir>/token` 在 Unix 上以 `600` 写入，在 Windows 上带仅属主 ACL；收紧不了时服务端拒绝启动。备份、拷贝、挂载时保持这些权限，也让该值远离共享的 shell 历史（`$(cat …)` 同时让它不进 `ps`）。
2. **绑得窄，再在前面加 TLS。** 保持 `--bind 127.0.0.1:7821`，让反向代理负责对外。终止 TLS 是代理的事；服务端没有 TLS。
3. **不要把 `--no-auth` 与非回环绑定放在一起。** 那等于「谁能连上这个端口，谁就能删会话、停 VM」。

一份最小 nginx 前置示例（示意——本构建不带代理）：

```nginx
server {
    listen 443 ssl;
    server_name riscdom.example.internal;
    ssl_certificate     /etc/ssl/riscdom/fullchain.pem;
    ssl_certificate_key /etc/ssl/riscdom/privkey.pem;

    location /v0/ {
        proxy_pass http://127.0.0.1:7821;
        # bearer token 就是凭证；让它留在这一跳。
        proxy_set_header Authorization $http_authorization;
        proxy_set_header Host $host;
        # SSE：不缓冲、不设空闲超时、用 HTTP/1.1。
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_buffering off;
        proxy_read_timeout 1h;
    }
}
```

`proxy_buffering off` 与较长的 `proxy_read_timeout`，是让 `/v0/events` 保持一条活的流、而不是在第一个心跳迟到时就结束的请求的关键。

## 2. 查询端点

共 31 个，全部 `GET`。应答是宿主的视图类型，字段见 `host-core/src/state.rs`。一律 JSON。

### 审计与运行

```bash
# 事件总数，以及哈希链是否完整。
curl -sS 'http://127.0.0.1:7821/v0/audit/status'
# {"alert_on_failure":true,"chain":{"status":"Intact","length":42},"count":42,"failures":[]}

# 最近的事件，新的在前。这里 limit 是必填。
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10'
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10&actor=local-17480-1'
curl -sS 'http://127.0.0.1:7821/v0/audit/events?limit=10&action_prefix=agent.tool'

# 派生索引里的运行（limit 默认 20）。
curl -sS 'http://127.0.0.1:7821/v0/runs?limit=20'

# 单个运行；本日志从未见过它时回 null。
curl -sS 'http://127.0.0.1:7821/v0/runs/run-17480-3'

# 两次运行的配置指纹，逐字段对比。
curl -sS 'http://127.0.0.1:7821/v0/runs/diff?run_a=run-17480-3&run_b=run-17480-4'
```

`RunView` 携带 `run_id`、`status`（`open` / `ok` / `failed` / `interrupted` / `abandoned`）、`fingerprint`（64 位十六进制）与 `fingerprint_short`（前 16 位）、`parent_run_id`、`session_id`、`resumed_from_snapshot`、`started_at_ms` 与 `ended_at_ms`。

> **`/v0/audit/status` 不消费失败队列。** 桌面端命令会取走待报的审计失败；这个 `GET` 只报告、不取走，因此在这里轮询不会偷走别的客户端的告警。

> **`/v0/runs/diff` 遇到不存在的 run 回 `500 internal`。** 宿主把「找不到 run」报成消息而不是有类型的 not-found。需要区分「没有这个 run」与「diff 失败」时，用 `/v0/runs/{id}` 先确认。

### LLM 配置

这里任何响应都不会返回 key。

```bash
curl -sS 'http://127.0.0.1:7821/v0/llm/provider-presets'   # 下拉框的数据
curl -sS 'http://127.0.0.1:7821/v0/llm/config'             # {"configured":false,...}
curl -sS 'http://127.0.0.1:7821/v0/llm/readiness'          # {"ready":false,"reason":"no_config",...}
curl -sS 'http://127.0.0.1:7821/v0/llm/local-probe'        # 回环上的 OpenAI 兼容服务
curl -sS 'http://127.0.0.1:7821/v0/llm/stored-key?provider_id=deepseek'
# {"present":false}
```

### 会话、快照、VM

```bash
curl -sS 'http://127.0.0.1:7821/v0/sessions?limit=20'   # limit 必填
curl -sS 'http://127.0.0.1:7821/v0/sessions/current'
# {"session_id":"s-17480-1"}   （或 null）

curl -sS 'http://127.0.0.1:7821/v0/snapshots'
# [{"name":"after-blink","size_bytes":1048576,"created_at_ms":1758533002099,"mode":"tcp-relay"}]

curl -sS 'http://127.0.0.1:7821/v0/vm/running'   # {"running":true}
curl -sS 'http://127.0.0.1:7821/v0/vm/status'    # {"running":true,"since_ms":1758533002050}
```

快照的 `mode` 为 `tcp-relay`（真实迁移流）或 `reboot-fallback`（JSON 兜底）。

### 工具链、QEMU、预检

```bash
curl -sS 'http://127.0.0.1:7821/v0/toolchain'
# {"found":true,"path":"...","source":"Path","diagnostics":"..."}
curl -sS 'http://127.0.0.1:7821/v0/toolchain/download'
# {"in_progress":false,"last_event":null}
curl -sS 'http://127.0.0.1:7821/v0/qemu'
curl -sS 'http://127.0.0.1:7821/v0/qemu/status'   # 同一个视图
curl -sS 'http://127.0.0.1:7821/v0/preflight'
# {"ran":true,"checked":true,"ok":true,"rows":[...],"failed_step":null,...}
```

`source` 取 `EnvVar` / `KnownPath` / `Path` / `Manual` 之一。预检应答是当前配置的缓存结果：端点只报告，不跑检查。

### 设置、workspace、串口

```bash
curl -sS 'http://127.0.0.1:7821/v0/settings/theme'      # {"theme":"system"}
curl -sS 'http://127.0.0.1:7821/v0/settings/language'   # {"language":"system"}
curl -sS 'http://127.0.0.1:7821/v0/workspace/root'      # {"root":"/abs/path/to/workspace"}
curl -sS 'http://127.0.0.1:7821/v0/workspace/files'     # ["src/main.c", ...]
curl -sS 'http://127.0.0.1:7821/v0/workspace/file?path=src%2Fmain.c'
# {"content":"int main(void) { return 0; }\n"}
curl -sS 'http://127.0.0.1:7821/v0/serial'              # {"buffer":"hello from riscv\n"}
```

`?path=` 会做百分号解码，所以 `src%2Fmain.c` 与 `src/main.c` 是同一个请求。读取会过 workspace 策略检查：越出根目录的路径属于调用方参数不可用，即 `400`，`cause` 为 `"path"`。

### 沙箱

```bash
curl -sS 'http://127.0.0.1:7821/v0/sandboxes'
# {"sandboxes":[{"name":"blink","source":"manual","runnable":true,"shadowed":false,
#   "memory_mb":256,"toolchain_path":"...","qemu_exe":"...","kernel":null,
#   "display_name":"Blink","notes":null},
#  {"name":"default","source":"discovered","runnable":true,"shadowed":false,...}],
#  "current":"blink","default":"blink"}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/current'
# {"current":"blink","default":"blink"}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/candidates'
# {"toolchains":[{"kind":"toolchain","version":"15.2.0-1","origin":"installed",...}],
#  "qemus":[{"kind":"qemu","version":"11.1.0","origin":"installed",...}]}
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/blink'   # 单项，与列表行同形
```

列表是**合并后的注册表**：先是手写在 `settings.json` 里的定义，然后是扫描所得，最后是内置的 `default`。`source` 为 `manual` 或 `discovered`；`runnable` 每次读取时现算（QEMU 存在且 `--version` 能跑、工具链存在、内核存在或可编译），所以资源被卸载的定义仍会列出来，只是答 `runnable: false`。同名时手写者胜，而被遮的扫描项**留在列表里**并标 `shadowed: true`。

`/v0/sandboxes/candidates` 答的是原始扫描——两个互相独立的列表，不合并、也不写回——而 `/v0/sandboxes/{name}` 在没有这个名字的定义时答 `404` 并在 `cause` 指出参数。字面子路径（`current`、`candidates`，以及 `requests` / `switch` / `assemble`）永不被当作名字读。

### 项目：出去，再回来

workspace 就是项目，而一个项目以一个归档的形式行走。导出答的是**字节**——除事件流之外这里唯一的非 JSON body——导入收的也是字节。

```bash
# 出去。`curl -o` 写归档；空 workspace 导出的也是合法归档。
curl -sS -X POST http://127.0.0.1:7821/v0/workspace/export \
  -H "Authorization: Bearer ***" -o project.tar.gz

# 回来。`--data-binary` 很重要：会删换行或重编码的工具会把归档弄坏；
# Content-Type 选读法（zip / gzip / tar）。
curl -sS -X POST 'http://127.0.0.1:7821/v0/workspace/import' \
  -H "Authorization: Bearer ***" -H 'Content-Type: application/gzip' \
  --data-binary @project.tar.gz
# {"files":17,"bytes":48211}

# 已有同名文件就是 409，除非你说要换掉。
curl -sS -X POST 'http://127.0.0.1:7821/v0/workspace/import?force=true' \
  -H "Authorization: Bearer ***" -H 'Content-Type: application/gzip' \
  --data-binary @project.tar.gz
```

一条 entry 不允许做的事在读取时就被检查，每种拒绝都自己报名：逃出 workspace、符号链接或硬链接、`.riscdom/` 下的东西（宿主自己的状态——审计库、快照、预检缓存）、或一个不是可读归档的 body，都是 `400`、`cause: "archive"`；workspace 里已有同名文件是 `409`、`cause: "exists"`；超过 64 MiB 是 `413`。导入需要 `workspace.write`，导出需要 `workspace.read`。

同样两趟，走 CLI：

```bash
riscdom workspace export --out project.tar.gz   # 或 `> project.tar.gz`：计数走 stderr
riscdom workspace import project.tar.gz         # 拒绝替换已有的东西
riscdom workspace import project.tar.gz --force # 替换它，并报出落了多少
```

### 沙箱申请：提出与裁决

**申请**是不能切换的 actor 表达它所想的方式。AI 的 `request_sandbox` 工具会落一条；`POST /v0/sandboxes/requests` 在 HTTP 上做同一件事，需要 `agent.run`。

```bash
# 落一条申请。201 答出 id；申请本身不切换任何东西。
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/requests \
  -d '{"action":"switch","sandbox":"big","reason":"the guest needs more memory"}'
# {"id":"req-4711-1"}

# 读队列（新的在前），或只看还在等的。
curl -sS 'http://127.0.0.1:7821/v0/sandboxes/requests?status=pending'
# {"requests":[{"id":"req-4711-1","requester_agent_id":"local-4711-1","action":"switch",
#   "sandbox":"big","definition":null,"reason":"the guest needs more memory",
#   "requested_at_ms":1758533002110,"status":"pending","decided_by":null,"decided_at_ms":null}]}

# 裁决。批准只改记录，别的什么都不做——切换是它自己那次调用。
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/requests/req-4711-1/approve
# {"id":"req-4711-1",...,"status":"approved","decided_by":"operator","decided_at_ms":1758533009999}
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/switch -d '{"name":"big"}'   # 现在才真的动
```

两条决策都需要 `sandbox.read`——决策者先要看得见队列——然后在处理器内部需要该请求自己的 `action` 所隐含的 capability：`switch` 要 `sandbox.switch`，`define` / `assemble` 要 `sandbox.assemble`。只持 `sandbox.read` 的 actor 能读队列，做决策时拿到 `403`、`cause: "capability"`（并指名它想要的哪一个）。id 未知是 `404`、`cause: "id"`；对已决请求再决是 `409`——决策不可逆。`?status=` 接受 `pending` / `approved` / `rejected`（`expired` 为预留：v0.9 没有 TTL，pending 请求一直等到有人决它）。

### 预留的聚合端点

```bash
curl -sS 'http://127.0.0.1:7821/v0/resources'
# {"code":"not_implemented","message":"resource accounting is reserved ...","retryable":false,"cause":"resources"}
```

## 3. 参数

- 必填参数缺失或无法解析即 `400`，`cause` 为该参数名：

```json
{ "code": "bad_request", "message": "missing required parameter \"limit\"", "retryable": false, "cause": "limit" }
```

- `limit` 在宿主命令要求处为必填：`/v0/audit/events` 与 `/v0/sessions`。`/v0/runs` 可选，默认 20。
- v0.9 没有任何 offset 或游标：调大 `limit`，在客户端侧过滤。

## 4. 处理错误

每个非 2xx 应答都是一个对象：

```json
{ "code": "not_found", "message": "no endpoint GET /v0/nope", "retryable": false, "cause": null }
```

把 `code` 当契约、`message` 当给人看的文本。客户端应对 `code` 分支，最多再读 `cause` 定位出错的字段。

| 状态 | `code` | 该怎么做 |
|---|---|---|
| 400 | `bad_request` | 改请求；`cause` 指出参数名——包括 workspace 策略拒绝的路径（`cause: "path"`）。 |
| 401 | `unauthorized` | 带有效凭证。 |
| 403 | `forbidden` | 不被允许：`cause` 为 `"capability"` 表示 actor 缺少该端点的 capability。这个状态只用于认证与授权。不要重试。 |
| 404 | `not_found` | 端点或资源不存在。 |
| 405 | `method_not_allowed` | 用 `message` 里指出的方法。 |
| 409 | `conflict` | 状态冲突；重读状态再决定。 |
| 500 | `internal` | 宿主失败；`message` 是宿主自己的文本。 |
| 501 | `not_implemented` | 预留；后续批次补齐。 |
| 503 | `unavailable` | 依赖未就绪（无 LLM、无 QEMU、无工具链）。 |

仅在 `retryable` 为 `true` 时重试——异步工作由控制类（`202` + 事件流）自行报告，查询面里没有这种情况。

```bash
# 缺少必填参数。
curl -sS -o - -w '\n%{http_code}\n' 'http://127.0.0.1:7821/v0/audit/events'
# {"code":"bad_request","message":"missing required parameter \"limit\"","retryable":false,"cause":"limit"}
# 400

# 已服务的路径上方法不对。
curl -sS -o - -w '\n%{http_code}\n' -X POST 'http://127.0.0.1:7821/v0/snapshots'
# {"code":"method_not_allowed","message":"POST is not allowed on /v0/snapshots; use GET","retryable":false,"cause":"method"}
# 405
```

## 5. 订阅事件流

`GET /v0/events` 是 `text/event-stream`。它会一直开着，所以用会流式读的客户端：

```bash
curl -sS -N http://127.0.0.1:7821/v0/events \
  -H 'Authorization: Bearer <token>' \
  -H 'Accept: text/event-stream'
```

第一帧恒为 `hello`；其后是宿主的事件，一事件一帧：

```text
id: 1790074876659-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"local-17480-1","task_id":null,"ts":1790074876659,"payload":{"buffer":{"from":0,"to":0},"filters":{"event":[],"agent_id":null,"task_id":null}}}

id: 1790074877033-1
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"local-17480-1","task_id":null,"ts":1790074877033,"payload":{"name":"write_source","arguments":{"path":"src/main.c"}}}

id: 1790074877100-2
data: {"version":1,"kind":"event","event":"serial:chunk","agent_id":"local-17480-1","task_id":null,"ts":1790074877100,"payload":{"chunk":"hello from riscv\n"}}

```

客户端可以依赖的规则：

- **帧只用 `id:` 与 `data:`。** 没有 SSE 的 `event:` 字段，所以每帧都进同一个 `onmessage`；按 envelope 的 `event` 路由。
- **每帧以空行结束。**
- **心跳是注释行**，每 15 秒一次：`: keep-alive`。以 `:` 开头的行忽略。
- **`id:` 不透明。** 打算重连就存下它；不要解析。
- **envelope 的键在 v0.9 内稳定**：`version`、`kind`、`event`、`agent_id`、`task_id`、`ts`、`payload`。忽略不认识的 payload 键。

```js
const source = new EventSource("/v0/events"); // 已建立会话
source.onmessage = (e) => {
  const frame = JSON.parse(e.data);
  if (frame.kind !== "event") return;          // hello（以及日后的 gap）
  switch (frame.event) {
    case "vm:state":
      // `name` 现在恒存在：非快照时为 null。
      badge(frame.payload.running, frame.payload.since_ms, frame.payload.name);
      break;
    case "serial:chunk":
      terminal.write(frame.payload.chunk);
      break;
    case "audit:failed":
      warn(frame.payload.message);              // v0.9 之前叫 `error`
      break;
  }
};
```

**重连。** 每帧都带 `id: <ts>-<seq>`。记住你见过的最后一个，并把它作为 `Last-Event-ID` 回传；服务端会从内存缓冲（最近 1024 帧）补发其后的帧：

```bash
curl -sS -N http://127.0.0.1:7821/v0/events \
  -H "Authorization: Bearer $RISCDOM_TOKEN" \
  -H 'Last-Event-ID: 1790074877033-7'
```

若游标已掉出缓冲，事件流会先发一个 `gap` 帧——`payload.lost_after` 给出仍持有的最旧 id——随后是从那里起的帧。无论哪种情况，都请用上面的查询重新读取状态，而不要假定事件流是完整的。

## 6. 控制端点

API 表的 §5.2 控制类端点，加上沙箱切换、申请端点与两条项目进出端点，全部是 `POST`。它们全部需要 token（这正是引入它们的那一批的要点：其中包含破坏性操作）。

```bash
# 跑一轮 agent。进度以 `agent:*` 事件抵达事件流。可选的 `sandbox` 是**声明**、不是切换：
# 这次运行用它（工具链、QEMU、内存），节点原地不动。
curl -sS -X POST http://127.0.0.1:7821/v0/agent/run \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"user_input":"compile the blink example","sandbox":"blink"}'
# 404 cause "name"    —— 没有叫这个名字的定义（拼写错误不当兕底）
# 409 cause "sandbox" —— 来自另一个定义的 VM 正在跑：停掉它，或
#   `POST /v0/sandboxes/switch` 切到 blink。一次运行从不自己切换节点。

# 会话。
# 把本节点切到另一个沙箱定义。校验发生在动到正在跑的 VM 之前，应答会说明来自哪里：
curl -sS -X POST http://127.0.0.1:7821/v0/sandboxes/switch \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"blink"}'
# {"from":null,"to":"blink"}

# 它会以什么理由拒绝：没有这个名字的定义（`404`，`cause: "name"`）；运行中切换（`409`，
# `cause: "run"`）；另一次切换进行中（`409`，`cause: "sandbox"`）；定义不能跑（`503`，`cause`
# 就是原因码）；或者 VM 起不来（`500`，`cause: "sandbox_start_failed"`——此时节点是已停，不是半切换）。

curl -sS -X POST http://127.0.0.1:7821/v0/sessions/create \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"title":"blink"}'
# {"session_id":"sess-1a2b3c4d-0"}

curl -sS -X POST http://127.0.0.1:7821/v0/sessions/clear \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'

# 快照。
curl -sS -X POST http://127.0.0.1:7821/v0/snapshots/save \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"after-blink"}'
# {"bytes_written":1048576}

curl -sS -X POST http://127.0.0.1:7821/v0/snapshots/resume \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"name":"after-blink"}'

# VM、设置、导出。
curl -sS -X POST http://127.0.0.1:7821/v0/vm/stop \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' -d '{}'

curl -sS -X POST http://127.0.0.1:7821/v0/settings/theme \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"theme":"dark"}'

curl -sS -X POST http://127.0.0.1:7821/v0/audit/export \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"path":"/abs/path/inside/the/workspace/audit.jsonl"}'
# {"events_exported":42}

# 把一条任务派给*已配置的执行器*——这是上面 run 的兄弟，不是它的同义词：
# run 在本节点上干活，任务则按 target 路由。
curl -sS http://127.0.0.1:7821/v0/executors -H "Authorization: Bearer $RISCDOM_TOKEN"
# {"executors":[{"agent_id":"executor-0"}]}——什么都没配就是 []

curl -sS -X POST http://127.0.0.1:7821/v0/tasks \
  -H "Authorization: Bearer $RISCDOM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"target":"executor-0","input":"say hi"}'
# {"task_id":"task-4711-1","agent_id":"device-4711-9","outcome":{"Final":{...}}}
# 404 cause "target"——本节点谁都不叫这个名字（包括它自己：给本节点的任务走
#   `POST /v0/agent/run`）。同步，和 run 一样：应答*就是*结果，没有任务可轮询。
#   `id` 可选（服务端补一个）；一次只是*跑失败*的任务仍是 `200`——它的 `outcome`
#   会说 `Failed`。应答里的 `agent_id` 是执行者自己宣告的身份，不是任务的收件标签。
```

客户端应当知道的几点：

- **多数控制端点回 `204`**（无内容），返回值的回 `200`，而后台开工的（`agent/run`、`preflight/run`、`toolchain/download`）回 `202`——这些请盯事件流。
- **两条审计导出答的是事件数**（`events_exported`），因为写进去的就是事件；`/v0/serial/export` 答 `bytes_written`，因为写进去的就是字节。
- **`POST /v0/qemu/download` 今天在每个平台上都按决定拒绝。** RiscDom 引导用户自己安装 QEMU
  （`docs/qemu-distribution.md` §5）、不 pin 发布版，因此该端点答 `503 unavailable`、
  `cause: "qemu"`，`message` 里是安装指引——且不会占下载槽。它的 `GET`（状态）与
  `/v0/qemu/download/cancel` 都是活的，所以客户端现在就可以像对着工具链那对一样写代码：

  ```bash
  curl -sS http://127.0.0.1:7821/v0/qemu/download -H "Authorization: Bearer ***"
  # {"in_progress":false,"last_event":null}

  curl -sS -X POST http://127.0.0.1:7821/v0/qemu/download -H "Authorization: Bearer ***" -d '{}'
  # {"code":"unavailable","message":"no QEMU download is pinned for windows-x86_64: …","cause":"qemu"}
  # 503
  ```

- **参数会被校验**：缺失或不可用即 `400`，`cause` 指出参数名。
- **状态冲突回 `409`**（没跑 VM 时 `save`、`resume` 未知快照回 `404`、没有下载在跑时 `toolchain/download/cancel`）。
- **`POST /v0/toolchain/download` 会真的下载**固定的 RISC-V GCC 归档。
- **`POST /v0/vm/start` 回 `501`**（预留：今天 VM 在运行内部启动），`GET /v0/resources` 同样。

## 7. 用 CLI 驱动控制平面

`riscdom` 是参考客户端，也是「这个控制平面到底答不答本文档所说的东西」最快的核对方式。它是严格意义上的客户端：每条命令都是一次 HTTP 请求，而本地模式只不过是在自己进程内、由内核挑一个回环端口把控制平面起起来。

```bash
riscdom health --json                 # 对着它自己起的控制平面
riscdom --json --remote 127.0.0.1:7821 runs list --limit 5   # 对着已经跑着的那个
```

| 子命令 | 端点 |
|---|---|
| `riscdom health` | `GET /v0/health` |
| `riscdom status` / `riscdom agents` | `GET /v0/status` |
| `riscdom runs list [--limit <n>]` | `GET /v0/runs` |
| `riscdom runs get <run_id>` | `GET /v0/runs/<run_id>` |
| `riscdom audit status` | `GET /v0/audit/status` |
| `riscdom audit events [--limit <n>]` | `GET /v0/audit/events` |
| `riscdom snapshots list` | `GET /v0/snapshots` |
| `riscdom sandboxes list` / `current` / `candidates` / `show <name>` | `GET /v0/sandboxes` / `/v0/sandboxes/current` / `/v0/sandboxes/candidates` / `/v0/sandboxes/<name>` |
| `riscdom sandboxes switch <name>` | `POST /v0/sandboxes/switch` |
| `riscdom sandboxes requests [--status <s>]` | `GET /v0/sandboxes/requests` |
| `riscdom sandboxes requests approve <id>` / `reject <id>` | `POST /v0/sandboxes/requests/<id>/approve` / `/reject` |
| `riscdom workspace export [--out <file>]` | `POST /v0/workspace/export` |
| `riscdom workspace import <archive> [--force]` | `POST /v0/workspace/import` |
| `riscdom run <task>` | `POST /v0/agent/run` |
| `riscdom executors list` | `GET /v0/executors` |
| `riscdom tasks dispatch --target <agent_id> --input <text>` | `POST /v0/tasks` |
| `riscdom run <task> --sandbox <name>` | `POST /v0/agent/run`（带 `sandbox`） |
| `riscdom vm stop` / `vm start` | `POST /v0/vm/stop` / `/v0/vm/start` |
| `riscdom snapshots save` / `resume` / `delete <name>` | `POST /v0/snapshots/save` / `resume` / `delete` |
| `riscdom sessions create` / `open` / `rename` / `delete` / `clear-all` | `POST /v0/sessions/create` / `open` / `rename` / `delete` / `clear` |
| `riscdom runs abandon-stale` | `POST /v0/runs/abandon-stale` |
| `riscdom export audit-jsonl` / `run-audit <run_id>` / `serial-log` | `POST /v0/audit/export` / `/v0/runs/export` / `/v0/serial/export` |
| `riscdom llm set` / `clear` / `load-key <provider_id>` | `POST /v0/llm/config` / `/v0/llm/config/clear` / `/v0/llm/stored-key/load` |
| `riscdom qemu path <file>` / `clear` | `POST /v0/qemu/path` / `/v0/qemu/path/clear` |
| `riscdom toolchain download` / `cancel` / `path <file>` / `clear` | `POST /v0/toolchain/download` / `/v0/toolchain/download/cancel` / `/v0/toolchain/path` / `/v0/toolchain/path/clear` |
| `riscdom preflight run` / `ack` | `POST /v0/preflight/run` / `/v0/preflight/ack` |
| `riscdom audit alert set <on\|off>` / `theme set <theme>` / `language set <lang>` | `POST /v0/audit/alert` / `/v0/settings/theme` / `/v0/settings/language` |

```bash
# 导出写到哪里由*服务端*决定：--out 相对于 workspace 根解析，回答里是计数，不是文件。
riscdom export audit-jsonl --out audit.jsonl
# exported 1 event to audit.jsonl

# 沙箱注册表，以及一次运行会用哪个定义。
riscdom sandboxes list
# current    blink
# default    blink
#
# NAME                         SOURCE      RUNNABLE  SHADOWED  MEMORY_MB
# blink                        manual      true      false     256
# default                      discovered  true      false     -

riscdom sandboxes current          # 只要那两个名字
riscdom sandboxes show blink       # 一个定义，一行一个字段
riscdom sandboxes candidates       # 这台机器上装了什么（原始扫描）

# 切换会先问（它会停掉正在跑的 VM，且运行中拒绝）；`--yes` 提前回答，非终端 stdin 必须给。
riscdom sandboxes switch blink --yes
# switched to blink

# 申请队列：谁在等裁决，以及去做裁决。两条决策都会先问，理由与切换相同。
riscdom sandboxes requests                    # 新的在前
# no sandbox requests
riscdom sandboxes requests --status pending
riscdom sandboxes requests approve req-4711-1 --yes
# req-4711-1 is now approved

# 配置模型；key 从文件来，所以不会落进 `ps`。
riscdom llm set --api-key-file ~/.riscdom/api-key \
  --base-url https://api.deepseek.com --model deepseek-chat
```

**`--follow` 就是 CLI 版的 §5。** `riscdom run <task> --follow` 先订阅 `/v0/events`，
再发起运行，因此运行产生的事件一到达就打印——与 §5 的客户端读到的是同一批帧——
运行的结果最后出来：

```bash
riscdom run "compile the blink example" --follow
```

或者指定沙箱——它是声明，所以节点不会被切换：

```bash
riscdom run "compile the blink example" --sandbox blink --follow
```

```text
agent:llm.stream.start {"iteration":1}
agent:tool_call {"name":"write_source","arguments":{"path":"src/main.c"}}
serial:chunk {"chunk":"hello from riscv\n"}
agent:final {"kind":"final"}
kind       final
iterations 3
```

**`--wait` 是同一套手法，用在两个异步控制上。** `toolchain download` 与 `preflight run`
答 `202` 并在一条线程上干活；`--wait` 在发请求前先订阅，打印属于该工作的事件族
（`toolchain:download`、`preflight:progress`），到“结束”那一帧就停——下载的 `done` /
`failed`，或预检的最后一步 / 第一个 `failed`（fail-fast 就结束在那里）。退出码是
**工作本身**的判定（失败为 `3`），不是那个 `202`。

```bash
riscdom toolchain download --wait
# toolchain:download {"install_path":"…","state":"done"}
# download ok
```

- **`--json`** 打印的就是控制平面发来的原文——§2、§5 记录的那些字段——因此照着本文档写出的客户端可以用它来调试。失败时把 §4 的错误体打到 **stderr**。带 `--follow` 与 `--wait` 时，每个帧是原样的 envelope。
- **`--api-key` 会像 `--token` 一样警告**（会落入 shell history 与 `ps`）；`--api-key-file`
  是更该用的形状，`--remember` 才让 key 活过重启（宿主把它存进操作系统凭据存储）。
- **销毁类命令先问**（`vm stop`、`snapshots resume`、`snapshots delete`、`sessions delete`、
  `sessions clear-all`、`llm clear`、`qemu clear`、`toolchain clear`、`sandboxes switch`）：终端上弹提示，`--yes`
  提前回答；stdin 不是终端时直接拒绝（退出码 `2`）——脚本必须显式写 `--yes`。
- **退出码**把 §4 的状态码变成脚本可分叉的东西：`0` 成功、`1` 本地失败（连不上、没有 token）、`2` 用法或 `400`、`3` 被拒或 `5xx`、`4` `401`/`403`（认证与授权；workspace 策略拒绝的路径属于上面的 `400`，故为退出码 `2`）。
- **token**：本地模式取自 `<data-dir>/token`；远程模式按 `--token-file`、`RISCDOM_TOKEN`、`--token` 的顺序取。从不被打印。

完整表格（含人类模式形状）见 [../cli/README.zh-CN.md](../cli/README.zh-CN.md)。

## 8. 用 AI 监工驱动它

上面写的都是给人看的客户端的。同一个表面就是应当交给 **AI 监工**当工具的那一套：全量在 [tool-schema-control-plane.zh-CN.md](tool-schema-control-plane.zh-CN.md)——每个端点一条 `{type:"function", …}`，分节，可直接贴进聊天请求的 `tools[]`。

那份文档讲到的三件事在这里再说一遍，因为监工恰恰在这三处出错：

- **监工不是执行者。** 执行者**内部**的工具是另一套、小得多的集合（[tool-schema-executor.zh-CN.md](tool-schema-executor.zh-CN.md)）；它们不可通过 HTTP 调用，也不是监工交给它模型的东西。
- **HTTP 调用由监工发出。** 一次工具调用说的是**要什么**；随后由进程去发那次 HTTP 请求（比如 `POST /v0/tasks`），再把 JSON 当作工具结果交回去。没有带内 RPC。
- **`GET /v0/events` 不是工具。** 它会一直开着；由监工订阅，把到达的东西当上下文喂给模型，而不是交给它一个永不返回的工具。

一次派发就是最短的完整闭环：`executors`（不知道队伍时先问） → `tasks` → `TaskOutcome` → `run_get` / `audit_events` 看周围发生了什么。若监工还需要**改动**节点（`sandboxes_switch`、`snapshots_save` 等），就应当交给它一个能改的凭据——若本意只是读，就交一个只能读的。v0.9 只有一个持有全部的 token，所以按 capability 分令牌落地之前（v1.0），工具清单是唯一的杆。

那个形状的一个可跑示例是 [`../examples/python/dispatch.py`](../examples/python/dispatch.py)（文档见 [`README`](../examples/python/README.zh-CN.md)）：仅标准库、三个端点、一个用假控制平面自证的 `--self-test`，以及 CLI 的退出码约定。它是骨架，不是产品：一次一条、按顺序。

## 9. 远程执行者句柄

§8 讲的是监工从外面抵达一个执行者。这一节是同一件事，从内核这一侧看：一个 [`AgentHandle`](../agent/src/dispatch.rs)，它的执行者是另一个节点，于是 `LocalDispatcher` 能像持有一个 stdio 或进程内句柄那样持有它。可跑示例是 [`worker/examples/remote_executor.rs`](../worker/examples/remote_executor.rs)（以及 [`worker/README.zh-CN.md`](../worker/README.zh-CN.md) 里的那一节）。

写句柄的人需要的三个事实，而三个都不需要新端点：

- **`POST /v0/tasks` 就是那次派发。** 远程句柄把 `Task` 发到那里，拿回一个**远端**执行者产出的 `TaskOutcome`。`POST /v0/agent/run` 对这件事是错的端点：它在节点自身上跑，答的是 `AgentOutcomeView`，形状不同。
- **target 是远端节点的 label，不是你的。** 一个句柄有两个名字——本地派发器路由用的那个，与对面认识的那个。发自己的 label 就是在向远端节点要一个它未必拥有的执行者，答案是它的 `404`。
- **回来的那个身份胜出。** `TaskOutcome.agent_id` 是远端节点的答案，而你要不要相信它，正是这个 trait 的全部意义：进程内 loop 是它自己的 id，子进程是子进程宣告的 id，远端节点是它上报的 id。

## 10. 还没有的东西

- **细粒度凭证。** 每条路由的 capability 都已强制（见 §1）；v0.9 缺的只是不止一种凭证。单个 token 持有一切，因此没法只授予「只读审计链」的客户端——按能力细分的 token 属 v1.0。
- **`POST /v0/vm/start`** 与 **`GET /v0/resources`** 回 `501`。
