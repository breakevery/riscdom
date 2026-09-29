[English](README.md) | 中文

# @riscdom/sdk（TypeScript）

一个**给 RiscDom 控制平面的带类型 TypeScript 客户端**（v1.0 M7d），依
[docs/sdk.md](../../../docs/sdk.md) 冻结的样子：一个薄而带类型的层，盖在其它文档已经定义好的表面上，
**不添加自己的任何语义**。它是 Rust SDK（[`sdk/rust`](../rust/README.zh-CN.md)）所承载的那个**唯一表面的
TypeScript 一半**；HTTP 表面是 [control-plane-api.md](../../../docs/control-plane-api.md) §5，错误模型是
它的 §4，认证是它的 §3，流是 [control-plane-events.md](../../../docs/control-plane-events.md)。

**今天这里有什么：整层表面。** §5.1 的 **37 条 `GET` 查询**与 §5.2 的 **36 条 `POST` 控制**作带类型的方法、
bearer token、`{code, message, retryable, cause}` 错误作类型化的 `ClientError`、带类型的请求参数，以及
**事件流**作一个异步、逐帧的 `Subscription`。

**一个包、浏览器与 Node 通用、运行时零依赖。** 运行时是 `fetch` —— 每个当代浏览器与 Node ≥18 的全局；
本包**没有任何 `dependencies`**（也没有 `devDependencies` —— 测试跑在 Node 自带的测试器上、用类型剥离，
所以什么都不必安装）。流用 `fetch` 加 `ReadableStream` reader 读 —— **绝不用 `EventSource`**，它无法设置
API 需要的 `Authorization` 头（[control-plane-events.md](../../../docs/control-plane-events.md) §1）。本包
需要 **Node ≥23.6**，那里直接导入 `.ts` 文件是默认行为。

**端点表不可能与服务器漂开。** `QUERY_ENDPOINTS` 与 `CONTROL_ENDPOINTS` 是提交进仓的
`(tool, method, path, capability)` 表，两条测试读 `docs/tool-schema-control-plane.md` 并断言两者相等；
那些标记块**本就**被断言为恰好是服务器自己的路由，于是链条是 **SDK ⇄ tool schema ⇄ server** —— 与 Rust
SDK 同一条守卫，两者之间没有依赖。

## 用法

```ts
import { Client, frameEnvelope, frameKind, lostAfter, type Envelope } from "@riscdom/sdk";

const client = new Client("http://127.0.0.1:7821", "the-node-token", { agentName: "supervisor-1" });

// 查询：§5.1，每端点一个带类型的方法，名字就用 tool-schema 文档给它的名字。
const status = await client.audit_status();
const events = await client.audit_events({ limit: 50, actor: "local-1-1" });

// 控制：§5.2，同一形状。
const outcome = await client.agent_run({ user_input: "blink" });
await client.snapshots_save({ name: "after-blink" });

// 事件流。
const subscription = await client.subscribe({ events: ["agent:tool_call"] });
for await (const frame of subscription) {
  const envelope = frameEnvelope(frame) as Envelope;
  switch (frameKind(envelope)) {
    case "event":
      // envelope.event 点名它；envelope.task_id 把它系到某个被派发的任务。
      break;
    case "gap":
      // 重放游标太旧：从查询重新同步。`lostAfter(envelope)` 点名那个游标。
      break;
    default:
      break;
  }
  const cursor = subscription.lastId; // 重连时把它作为 `lastEventId` 传回
}
```

- `new Client(baseUrl, token, options?)` —— token 是节点自己的（`<data-dir>/token`）；它住在 client 里，
  于是一次请求不可能忘记带上它。`options.fetch` 可换掉实现（包装、测试替身）。
- `get` / `get_raw` / `post` / `post_empty` —— 每个方法之下的传输层，供本 SDK 未包装的端点使用：**表面是
  API 的，不是 SDK 的**。
- 两个控制不说 JSON，它们的方法也如此表明：`workspace_export()` 以一个 `Uint8Array` 作答，
  `workspace_import(archive, force)` 把归档作为请求体（§5.2）。
- 答复是 `unknown`：API 文档点名每个响应类型但不冻结其字段，而 [sdk.md](../../../docs/sdk.md) §1 禁止 SDK
  自己发明它们。需要什么就自己收窄。
- 失败是 `ClientError`，带 `kind: "transport" | "api"`；当 `"api"` 时，`error.api` 携带 `status`、`code`、
  `message`、`retryable`、`cause`。

## 事件流

`client.subscribe(filters, lastEventId?)` 返回一个 `Subscription`：`next_frame()` 一次给一帧，而该对象是
一个**异步可迭代**。`frameKind(envelope)` 读 envelope 的 `kind`：

- **`hello`** —— 流开了；payload 描述它的缓冲与过滤。
- **`event`** —— 二十个事件之一。
- **`gap`** —— **重放游标太旧；从查询重新同步。** 一条恢复**指令、不是错误**
  （[control-plane-events.md](../../../docs/control-plane-events.md) §2），作为 kind 如实暴露、不做粉饰：
  `lostAfter(envelope)` 点名服务器仍持有的最旧 id。忽略它的客户端会**静默漏事件**。
- **`unknown`** —— 未来服务器新加的 kind。文档说客户端必须忽略不认识的帧，所以这是一个值、不是抛出。

**重连。** 留住 `subscription.lastId` 并把它作为 `lastEventId` 传回；客户端把它作为 `Last-Event-ID` 发出，
服务器重放其后的帧，或在游标已掉出缓冲时以 `gap` 作答。注释行（`: keep-alive`，每 15 秒）被跳过，一帧里的
多行 `data:` 以换行拼接。

## 布局与测试

- `src/index.ts` —— 端点表、参数类型、错误模型、客户端及其查询与控制方法、流类型。
- `test/endpoints.test.ts` —— 漂移守卫（两张表对照文档）。
- `test/client.test.ts` —— 客户端与流，跑在一个回环 `node:http` 服务器上。
- `npm test` —— `node --test`，在 Node ≥23.6 上，**无需安装任何东西**。
- `npm run typecheck` —— `tsc --noEmit`，给手上有 TypeScript 的人；gate 不跑它，因为本仓的 gate 不安装
  任何 Node 依赖。

设置了 `agentName` 时，每次调用都发送 `X-RiscDom-Agent` 头（api §3）。这里没有任何东西决定一条路由是什么
意思、重试何时安全、或一个 capability 授予什么 —— 那些是文档的事（[docs/sdk.md](../../../docs/sdk.md) §1）。
本包是 `private`，不发布到任何 registry。
