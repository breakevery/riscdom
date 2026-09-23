[English](control-plane-events.md) | 中文

# 控制平面事件流（设计）

> **适用 v0.9，v1.0 冻结。** 本文是设计文档：它定义客户端照着实现的线格式，不是今天就交付的实现。

**面向读者。** 发行版与集成的开发者：消费事件流的人——管理客户端、官方管理程序、或盯着执行者的 AI 监工。

**范围。** 命令与查询见 [control-plane-api.zh-CN.md](control-plane-api.zh-CN.md)。本文覆盖推送侧：SSE 分帧、所有事件共用的一个 envelope、逐事件 payload、以及过滤。

**事件从哪来。** `host-core/src/events.rs` 定义十二个事件名与一个 `EventSink` trait（`emit(&self, event: &str, payload: serde_json::Value)`）。当前有三个实现：`TauriEventSink`（发往 webview）、`RecordingEventSink`（测试）、`LineEventSink`（`worker`，JSON 行写 stderr）。**每个传输都把要发的东西包进下面的 envelope**——SSE sink、Tauri sink（其 webview 在唯一边界处解包）、以及 worker 的行协议。发射点仍然只传原始 payload，因为 envelope 是传输的事：`EventSink::emit` 保持 `(&str, Value)` 签名，没有任何发射点改形状。**v0.9 批次 3 已实现**，连同 §3 标为「有变」的三种 payload 形状；其余八种与宿主发射时完全一致。

## 1. SSE 协议

- **内容类型。** `text/event-stream; charset=utf-8`，带 `Cache-Control: no-cache` 与 `Connection: keep-alive`。
- **端点。** `GET /v0/events`。流以 `200` 打开；连接保持到客户端关闭或服务端停止。
- **帧只用 `id:` 与 `data:`，不设 `event:` 字段。** *决定与理由：* 设置 SSE 的 `event` 字段会让浏览器的 `EventSource` 派发到具名监听器、不再触发 `onmessage`，从而强迫客户端注册十二个监听器。不设则每帧都进一个 `onmessage` 处理器，由 envelope 的 `event` 字段负责路由。一个处理器，一个路由。
- **心跳。** 每 15 秒一行注释（以 `:` 开头），既防止中间设备关闭空闲流，也让客户端能察觉断开的连接：

```text
: keep-alive

```

- **认证。** 用 API 文档里的 `Authorization: Bearer <token>` 头。浏览器的 `EventSource` 无法设置头，因此浏览器客户端要么先开会话（一个 `POST` 设 cookie，再用 `withCredentials` 的 `new EventSource(...)`），要么在查询串里放一个短时 token。两者都作为客户端集成选项记录在案；查询串 token 不推荐（会落进访问日志），且必须是客户端的显式选择，绝不作为默认。
- **重连。** 断流后用同一组 `(事件过滤)` 加客户端见过的最后一个 id 重连，id 走 `Last-Event-ID` 头（浏览器自动发送；其它客户端须自己发）。
- **`id:` 是重放游标。** 服务端发 `id: <ts>-<seq>`，`ts` 为事件时间戳（epoch 毫秒），`seq` 为**服务端全局**单调帧计数器——必须是全局的，重连后才能用它定位。它对客户端不透明：存下来、回传、不要解析。
- **重放是尽力而为且有界的。** 服务端保留一个有界的近期帧环形缓冲。客户端的 `Last-Event-ID` 若新于缓冲最旧项，服务端补放缺口；若更旧，则无法补齐，此时用 `gap` 帧（§2）如实相告，而不是假装历史完整。

### 1.1 流开启

连接打开后，服务端立即发一个 `hello` 帧描述本流，让客户端能把「暂无事件」与「未连上」区分开：

```text
id: 0-0
data: {"version":1,"kind":"hello","event":null,"agent_id":"server","task_id":null,"ts":1758533001207,"payload":{"buffer":{"from":0,"to":42},"filters":{"event":[],"agent_id":null,"task_id":null}}}

```

## 2. 统一 envelope

每帧都携带同一个对象。这是本批的核心产出：十二个事件各保留自己的 payload，但都装进一个 envelope，字段各有唯一含义。

```json
{
  "version": 1,
  "kind": "event",
  "event": "agent:tool_call",
  "agent_id": "dev-12345-1",
  "task_id": "task-12345-1",
  "ts": 1758533001207,
  "payload": { "name": "write_source", "arguments": { "path": "src/main.c" } }
}
```

| 字段 | 类型 | 必填 | 含义 |
|---|---|---|---|
| `version` | integer | 是 | envelope schema 版本。v0.9 为 `1`。 |
| `kind` | string | 是 | 帧种类：`event` / `hello` / `gap`（见下）。 |
| `event` | string \| null | 是 | 十二个名字之一；`hello` / `gap` 时为 `null`。 |
| `agent_id` | string | 是 | 引发该事件的 agent，`<device>-<pid>-<seq>`。 |
| `task_id` | string \| null | 是 | 所属的派发任务；与任务无关时为 `null`。 |
| `ts` | integer | 是 | epoch 毫秒。 |
| `payload` | object | 是 | 事件专属正文（§3）。 |

v0.9 的 `kind` 取值：

- `event` —— 十二个事件之一；由 `event` 命名。
- `hello` —— 流已开启；`payload.buffer` 与 `payload.filters` 描述它。
- `gap` —— 请求的重放 id 太旧、无法重放；`payload.lost_after` 是服务端仍持有的最旧 id。看到 `gap` 的客户端必须从查询重新同步（见 API 文档），而不是假定什么都没漏。

**已于 v0.9 批次 4 实装。** 重连的客户端带上 `Last-Event-ID`，服务端就从有界的内存缓冲（最近 1024 帧）里补发其后的帧。若游标已掉出缓冲，客户端先收到一个 `gap` 帧——其中给出仍持有的最旧 id——随后是从那里起的帧。

### 2.1 `version` 如何演进

- **增加 payload 字段不升 `version`。** 客户端忽略未知 payload 键。
- **增加新事件名或新 `kind` 取值不升 `version`。** 客户端忽略不认识的帧。
- **改字段含义、改类型、或删字段，升 `version`。** 这个升版是客户端获知「必须改」的唯一信号，所以只留给这三种情况。
- v0.9 内 envelope 顶层字段冻结：新增顶层字段也算升 `version`，因为校验 envelope 的客户端会被迫修改。

## 3. 十二个事件，规范化

这里统一 payload 键名，使得「按本文写的客户端」在发射点被规范化（后续批次）之后依然可用。「有变」指与今日 payload 的映射不是恒等映射；其余保持键名，只是被包进 envelope。

| # | `event` | 今日 payload（v0.8） | envelope `payload`（v1） | 有变 |
|---|---|---|---|---|
| 1 | `agent:iteration` | `{model, messages}` | `{model, messages}` | 否 |
| 2 | `agent:tool_call` | `{name, arguments}` | `{name, arguments}` | 否 |
| 3 | `agent:tool_result` | `{ok, result}` | `{ok, result}` | 否 |
| 4 | `agent:final` | `{kind, content, reason, iterations}` | `{kind, content, reason, iterations}` | 否 |
| 5 | `agent:stream:delta` | `{text}` | `{text}` | 否 |
| 6 | `agent:stream:done` | `{}` | `{}` | 否 |
| 7 | `serial:chunk` | `{chunk}` | `{chunk}` | 否 |
| 8 | `vm:state` | `{state, running, since_ms}`（仅快照时带 `name`） | `{state, running, since_ms, name}`——`name` 恒存在 | **是** |
| 9 | `preflight:progress` | `{step, state, detail}` | `{step, state, detail}` | 否 |
| 10 | `audit:failed` | `{error}` | `{message}` | **是** |
| 11 | `toolchain:download` | 内部标签枚举：`{"kind":"progress","downloaded":d,"total":n}` 等 | `{state, ...}`——同样的字段，标签改为 `state` | **是** |
| 12 | `qemu:download`（v0.9 沙箱 F1） | 内部标签枚举：`{"kind":"progress",…}` 等 | `{state, ...}`——同样的字段，标签改为 `state`，与工具链同形 | **是** |

十二个中有**四个**改形状，**八个**是恒等映射。
**三种改动均已在 v0.9 批次 3 落地**（第四种在 F1 批次），在发射点处：envelope 包裹它们，下表这些键就是客户端今天在 `payload` 里看到的。

### 3.1 旧 → 新迁移

`vm:state`。今日只在快照事件里给 payload 加上 `name`。v1 中 `name` 恒存在，非快照时为 `null`。原本用「有没有 `payload.name`」来判断快照的客户端，必须改为判断 `payload.state === "snapshot"`。`state` 仍取 `running` / `stopped` / `snapshot`。

`audit:failed`。键改名，以与 API 错误模型（该文 §4）一致——那里人类可读文本就叫 `message`：

| 旧 | 新 |
|---|---|
| `payload.error` | `payload.message` |

`toolchain:download`。今日 payload 是内部标签的 `DownloadEvent` 枚举，标签为 `kind`；v1 把标签改名为 `state`，变体字段留在旁边：

| 旧 | 新 |
|---|---|
| `{"kind":"started","total_bytes":n}` | `{"state":"started","total_bytes":n}` |
| `{"kind":"progress","downloaded":d,"total":n}` | `{"state":"progress","downloaded":d,"total":n}` |
| `{"kind":"verifying"}` | `{"state":"verifying"}` |
| `{"kind":"extracting"}` | `{"state":"extracting"}` |
| `{"kind":"done","install_path":p}` | `{"state":"done","install_path":p}` |
| `{"kind":"failed","reason":r}` | `{"state":"failed","reason":r}` |
| `{"kind":"cancelled"}` | `{"state":"cancelled"}` |

`qemu:download`（v0.9 沙箱 F1）迁移方式**完全相同**——同一个枚举、同一个标签——所以一张表就能描述两种装配的下载。区别只在帧出现的可能性：工具链那一族承载真实下载，而 QEMU 那一族今天会拒绝（没有 pin 任何发布版，`docs/qemu-distribution.md` §5），所以它承载的是拒绝而不是进度。

## 4. 过滤

客户端可订阅子集。这是 v0.9 的设计形状，机制在实现批次里建。

- **查询参数，可重复：**
  `GET /v0/events?event=agent:tool_call&event=vm:state&agent_id=dev-12345-1&task_id=task-12345-1`
  - `event` —— 十二个名字之一；重复即选多个。缺省为全部。
  - `agent_id` —— 选某个 agent 的事件。
  - `task_id` —— 选某个任务的事件。
- **服务端过滤。** 服务端在写入前丢弃不匹配的帧，因此过滤后的流不会让客户端为它不关心的事件耗带宽。
- **客户端仍须过滤。** 客户端必须容忍收到自己没要的事件（未来的服务端、缺陷、默认值变了）并忽略之。过滤是优化，从不是正确性保证。
- **`hello` 与 `gap` 永不参与过滤。** 它们描述流自身，故恒被投递。

## 5. 帧示例

逐事件一帧，均为线上原样（每帧以一个空行结束）。`id` 值仅作示意。

`agent:iteration` —— 一轮 LLM 迭代开始：

```text
id: 1758533001207-1
data: {"version":1,"kind":"event","event":"agent:iteration","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001207,"payload":{"model":"local-model","messages":[{"role":"user","content":"blink"}]}}

```

`agent:tool_call` —— 模型请求一个工具：

```text
id: 1758533001880-2
data: {"version":1,"kind":"event","event":"agent:tool_call","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001880,"payload":{"name":"write_source","arguments":{"path":"src/main.c","content":"int main(void){return 0;}"}}}

```

`agent:tool_result` —— 工具返回：

```text
id: 1758533001902-3
data: {"version":1,"kind":"event","event":"agent:tool_result","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001902,"payload":{"ok":true,"result":"wrote 24 bytes"}}

```

`agent:stream:delta` —— 增量助手文本：

```text
id: 1758533001950-4
data: {"version":1,"kind":"event","event":"agent:stream:delta","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001950,"payload":{"text":"Compiling"}}

```

`agent:stream:done` —— 流结束：

```text
id: 1758533001975-5
data: {"version":1,"kind":"event","event":"agent:stream:done","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533001975,"payload":{}}

```

`agent:final` —— 运行结束：

```text
id: 1758533002010-6
data: {"version":1,"kind":"event","event":"agent:final","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533002010,"payload":{"kind":"final","content":"Blink compiled and ran; the banner is on the serial console.","reason":null,"iterations":1}}

```

`serial:chunk` —— 新的串口输出：

```text
id: 1758533002033-7
data: {"version":1,"kind":"event","event":"serial:chunk","agent_id":"dev-12345-1","task_id":"task-12345-1","ts":1758533002033,"payload":{"chunk":"hello from riscv\n"}}

```

`vm:state` —— VM 启动：

```text
id: 1758533002050-8
data: {"version":1,"kind":"event","event":"vm:state","agent_id":"dev-12345-1","task_id":null,"ts":1758533002050,"payload":{"state":"running","running":true,"since_ms":1758533002050,"name":null}}

```

`vm:state` —— 保存了一次快照（注意 `name` 在此存在）：

```text
id: 1758533002099-9
data: {"version":1,"kind":"event","event":"vm:state","agent_id":"dev-12345-1","task_id":null,"ts":1758533002099,"payload":{"state":"snapshot","running":true,"since_ms":1758533002050,"name":"after-blink"}}

```

`preflight:progress` —— 一个环境检查步骤：

```text
id: 1758533002110-10
data: {"version":1,"kind":"event","event":"preflight:progress","agent_id":"dev-12345-1","task_id":null,"ts":1758533002110,"payload":{"step":"gcc_runs","state":"ok","detail":"riscv64-unknown-elf-gcc (GCC) 13.2.0"}}

```

`audit:failed` —— 一次审计写入在重试后仍失败：

```text
id: 1758533002140-11
data: {"version":1,"kind":"event","event":"audit:failed","agent_id":"dev-12345-1","task_id":null,"ts":1758533002140,"payload":{"message":"database is locked"}}

```

`toolchain:download` —— 下载进度（规范化后形态）：

```text
id: 1758533002160-12
data: {"version":1,"kind":"event","event":"toolchain:download","agent_id":"dev-12345-1","task_id":null,"ts":1758533002160,"payload":{"state":"progress","downloaded":10485760,"total":209715200}}

```

`gap` —— 请求的重放太旧：

```text
id: 1758533002200-13
data: {"version":1,"kind":"gap","event":null,"agent_id":"server","task_id":null,"ts":1758533002200,"payload":{"lost_after":"1758533000100-88"}}

```

用浏览器消费该流：

```js
const src = new EventSource("/v0/events"); // 已建立会话 cookie
src.onmessage = (e) => {
  const env = JSON.parse(e.data);
  if (env.kind === "event" && env.event === "vm:state") {
    renderVmBadge(env.payload.running, env.payload.since_ms);
  }
};
```
