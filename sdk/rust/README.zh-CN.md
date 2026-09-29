[English](README.md) | 中文

# riscdom-sdk（Rust）

一个**给 RiscDom 控制平面的带类型 Rust 客户端**（v1.0 M7c），依 [docs/sdk.md](../../docs/sdk.md)
冻结的样子：一个薄而带类型的层，盖在其它文档已经定义好的表面上，**不添加自己的任何语义**。HTTP 表面是
[control-plane-api.md](../../docs/control-plane-api.md) §5，错误模型是它的 §4，认证是它的 §3。

**今天这里有什么（批 BA）：查询那一半。** §5.1 的 **37 条 `GET` 端点**作带类型的方法、**bearer token**、
`{code, message, retryable, cause}` **错误作一个类型**、以及那些带参数的端点的请求参数。**还没有：**控制类
端点（`POST`，§5.2）与事件流，那是下一批（BB）；TypeScript SDK 是 BC。

**本 crate 不链接本 workspace 的任何运行时件。** 它经 `reqwest` 的 **blocking** 客户端说 HTTP、从不依赖
`host-core`、`server` 或 `net`，所以客户端在另一台机器上 —— SDK 的常态 —— 不需要别的，且**不强加任何 async
运行时**（[sdk.md](../../docs/sdk.md) §3）。`reqwest` 已在 `Cargo.lock` 里，所以本 crate 只加一条边、
**不新增包**。

**端点表不可能与服务器漂开。** `QUERY_ENDPOINTS` 是一张提交进仓的 `(tool, method, path, capability)` 表，
而一条测试用 `include_str!` 解析
[`docs/tool-schema-control-plane.md`](../../docs/tool-schema-control-plane.md) 的标记块、断言两者相等。那些
标记块**本就**被断言为恰好是服务器自己的 `ROUTES`，于是链条是 **SDK ⇄ tool schema ⇄ server** —— 它们之间
没有依赖，也没有第二份要维护的清单。守卫是一条测试，所以它不需要构建步骤、也不需要代码生成。

## 用法

```rust,ignore
use riscdom_sdk::{AuditEvents, Client};

let client = Client::new("http://127.0.0.1:7821", "the-node-token")?;

// 每个端点一个带类型的方法，名字就用 tool-schema 文档给它的名字。
let status = client.audit_status()?;
let events = client.audit_events(&AuditEvents { limit: 50, ..Default::default() })?;

// 把客户端命名为一个 agent，于是它写的审计行会说清是谁做的（api §3）。
let client = client.with_agent_name("supervisor-1");
```

- `Client::new(base_url, auth)` —— bearer 值是节点自己的 token（`<data-dir>/token`）；它住在 client 里，
  于是一次请求不可能忘记带上它。
- `Client::get(path, query)` —— 每个方法之下的传输层，供本批未包装的端点使用：**表面是 API 的，不是 SDK
  的**。
- 答复是 `serde_json::Value`：API 文档点名每个响应类型（`AuditStatusView`……）但没有冻结其字段，而
  [sdk.md](../../docs/sdk.md) §1 禁止 SDK 自己发明它们，所以本批不发明。等某份文档把形状定下来，带类型的
  响应结构再跟上。
- 失败是 `ClientError`：请求无法完成时是 `Transport`，服务器以 §4 的对象作答时是 `Api(ApiError)`。
  `ApiError` 携带 `status`、`code`、`message`、`retryable`、`cause`；`ClientError::api()` 把它作为类型交回。

## 布局

- `src/lib.rs` —— 端点表、请求参数类型、错误模型、客户端及其 37 个查询方法。漂移守卫与回环服务器测试在文件
  末尾。
- `README.md` / `README.zh-CN.md` —— 这一对。

`X-RiscDom-Agent` 头（api §3）在用了 `with_agent_name` 时发送；否则身份是凭据自己的。这里没有任何东西决定
一条路由是什么意思、重试何时安全、或一个 capability 授予什么 —— 那些是文档的事
（[docs/sdk.md](../../docs/sdk.md) §1）。
