[English](README.md) | 中文

# riscdom-sdk（Rust）

一个**给 RiscDom 控制平面的带类型 Rust 客户端**（v1.0 M7c），依 [docs/sdk.md](../../docs/sdk.md)
冻结的样子：一个薄而带类型的层，盖在其它文档已经定义好的表面上，**不添加自己的任何语义**。HTTP 表面是
[control-plane-api.md](../../docs/control-plane-api.md) §5，错误模型是它的 §4，认证是它的 §3，流是
[control-plane-events.md](../../docs/control-plane-events.md)。

**今天这里有什么（批 BA + BB）：整层表面。** §5.1 的 **37 条 `GET` 查询**与 §5.2 的 **36 条 `POST` 控制**
作带类型的方法、**bearer token**、`{code, message, retryable, cause}` **错误作一个类型**、带类型的请求参数，
以及**事件流**作一个阻塞式、逐帧的 [`Subscription`]。

**本 crate 不链接本 workspace 的任何运行时件。** 它经 `reqwest` 的 **blocking** 客户端说 HTTP、从不依赖
`host-core`、`server` 或 `net`，所以客户端在另一台机器上 —— SDK 的常态 —— 不需要别的。**不强加任何 async
运行时**（[sdk.md](../../docs/sdk.md) §3）：流是用 `std::io::Read` 在阻塞响应上读的，不需要 async、也不需要
额外 feature。每一项依赖（`reqwest`、`serde`、`serde_json`、`thiserror`）都已在 `Cargo.lock` 里，所以本
crate 只加**边、不加包**。

**端点表不可能与服务器漂开。** `QUERY_ENDPOINTS` 与 `CONTROL_ENDPOINTS` 是提交进仓的
`(tool, method, path, capability)` 表，两条测试用 `include_str!` 解析
[`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md) 的标记块、断言两者相等。
那些标记块**本就**被断言为恰好是服务器自己的 `ROUTES`，于是链条是 **SDK ⇄ tool schema ⇄ server** ——
无依赖、无第二份清单。

## 用法

```rust,ignore
use riscdom_sdk::{AgentRun, AuditEvents, Client, Filters, FrameKind, SnapshotName};

let client = Client::new("http://127.0.0.1:7821", "the-node-token")?;

// 查询：§5.1，每端点一个带类型的方法。
let status = client.audit_status()?;
let events = client.audit_events(&AuditEvents { limit: 50, ..Default::default() })?;

// 控制：§5.2，同一形状。
let outcome = client.agent_run(&AgentRun {
    user_input: "blink".into(),
    sandbox: None,
    instance: None,
})?;
client.snapshots_save(&SnapshotName { name: "after-blink".into() })?;

// 把客户端命名为一个 agent，于是它写的审计行会说清是谁做的（api §3）。
let client = client.with_agent_name("supervisor-1");

// 事件流：阻塞式、逐帧。
let mut subscription = client.subscribe(&Filters { events: vec!["agent:tool_call".into()], ..Default::default() }, None)?;
while let Some(frame) = subscription.next_frame()? {
    match frame.frame_kind() {
        Some(FrameKind::Event) => { /* frame.envelope().unwrap().event … */ }
        Some(FrameKind::Gap) => { /* 从查询重新同步 —— 见下 */ }
        _ => {}
    }
    let cursor = subscription.last_id(); // 重连时把它作为 Last-Event-ID 发回
}
```

- `Client::new(base_url, auth)` —— bearer 值是节点自己的 token（`<data-dir>/token`）；它住在 client 里，
  于是一次请求不可能忘记带上它。
- `Client::get` / `post` / `post_empty` / `post_raw` / `get_raw` —— 每个方法之下的传输层，供本 SDK 未包装的
  端点使用：**表面是 API 的，不是 SDK 的**。
- 两个控制不说 JSON，它们的方法也如此表明：`workspace_export()` 把归档作为 `Vec<u8>` 返回，
  `workspace_import(archive, force)` 把归档作为请求体（§5.2）。
- 答复是 `serde_json::Value`：API 文档点名每个响应类型（`AuditStatusView`……）但没有冻结其字段，而
  [sdk.md](../../docs/sdk.md) §1 禁止 SDK 自己发明它们。
- 失败是 `ClientError`：请求无法完成时是 `Transport`，服务器以 §4 的对象作答时是 `Api(ApiError)`。
  `ApiError` 携带 `status`、`code`、`message`、`retryable`、`cause`；`ClientError::api()` 把它作为类型交回。

## 事件流

`Client::subscribe(filters, last_event_id)` 返回一个 [`Subscription`]。`next_frame()` 一次给一帧
[`Frame`]（`id` 与 `data`），而 `frame_kind()` 读 envelope 的 `kind`：

- **`hello`** —— 流开了；payload 描述它的缓冲与过滤。
- **`event`** —— 二十个事件之一。
- **`gap`** —— **重放游标太旧；从查询重新同步。** 这是一条恢复**指令、不是错误**
  （[control-plane-events.md](../../docs/control-plane-events.md) §2），而 SDK 把 kind 如实暴露、不做粉饰：
  `Envelope::lost_after()` 点名服务器仍持有的最旧 id。忽略它的客户端会**静默漏事件**。
- **`Unknown`** —— 未来服务器新加的 kind。文档说新 kind 不升 `version`、客户端必须忽略不认识的帧，所以这是
  一个值、不是 panic。

**重连。** 留住最后一个 `id`（`Subscription::last_id`）并把它作为 `last_event_id` 传回；客户端把它作为
`Last-Event-ID` 发出，服务器重放其后的帧，或在游标已掉出缓冲时以 `gap` 作答。注释行（`: keep-alive`，每 15
秒）被跳过，一帧里的多行 `data:` 以换行拼接。

## 布局

- `src/lib.rs` —— 端点表、请求参数类型、错误模型、客户端及其查询与控制方法、流类型。漂移守卫与回环服务器
  测试在文件末尾。
- `README.md` / `README.zh-CN.md` —— 这一对。

用了 `with_agent_name` 时，每次调用都发送 `X-RiscDom-Agent` 头（api §3）；否则身份是凭据自己的。这里没有
任何东西决定一条路由是什么意思、重试何时安全、或一个 capability 授予什么 —— 那些是文档的事
（[docs/sdk.md](../../docs/sdk.md) §1）。TypeScript SDK 是下一批（BC）。
