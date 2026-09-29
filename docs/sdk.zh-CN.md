[English](sdk.md) | 中文

# SDK

**状态** v1.0 规范（M7c / M7d）｜ **日期** 2026-09-29 ｜ **面向读者** 写 RiscDom 客户端的人，以及构建他们所
用库的人。

**本文是什么。** [roadmap §11](roadmap-v1.0.zh-CN.md) 把一套 **SDK** 放进 v1.0 的生态工作 —— *「[默认]*
**Rust 与 TypeScript 优先**，因为那正是本仓已经在说的两门语言」（[§14.12](roadmap-v1.0.zh-CN.md)）——
而 SDK 就是 M7c 与 M7d。本文说这些库**是什么**：一个薄而带类型的层，盖在其它文档已经冻结的表面上，
好让客户端作者不必手搓 wire。它是一份**规格**；库是后续批次。

**两个 SDK、一个表面。** Rust 与 TypeScript，依 §14.12。它们是**同一份契约**的两种视图 ——
[control-plane-api.md](control-plane-api.zh-CN.md)、[control-plane-events.md](control-plane-events.zh-CN.md)
以及 [config-schema.md](config-schema.zh-CN.md) 的配置类型 —— 不是两套设计。一个 SDK 能说的，另一个也
以自己的习惯说法说。

## 1. SDK 是什么、不是什么

**它是一个带类型的客户端。** 它携带：
- 每个端点的**请求与响应类型**，于是一次调用是一个函数、不是一个字符串键的字典；
- **错误模型**（[control-plane-api.md](control-plane-api.zh-CN.md) §4）作为带类型的错误，而不是一个里面
  塞着字符串的异常；
- **认证**（bearer token）与调用所需的 **capability**，在它被检查之处暴露出来；
- **事件流**（[control-plane-events.md](control-plane-events.zh-CN.md)）作为一种带类型的订阅，并把 envelope
  的 `kind` —— `hello`、`event`、`gap` —— 讲清楚。

**它不是一次重新实现。** SDK **不添加任何语义**：它不决定一条路由是什么意思、重试何时安全、或一个
capability 授予什么。那些都由上面那些文档定下，而 SDK 是它们忠实的镜像。凡规则住在 API 文档里的地方，
SDK 的文档**指向那份文档**而不复述，好让改动只有一处。

**它不是产品功能。** SDK 是给**第三方**的。本仓自己的客户端 —— CLI（`cli/src/client.rs`）与管理程序的
前端（`ui/src/api/`）—— 先于它存在，且不会被改写到它上面：它们在项目之内，SDK 是给项目之外的。

## 2. 它包住的表面

SDK 精确覆盖 API 所服务的东西，且只覆盖那些：

| 部件 | 它的文档 | SDK 把它变成什么 |
|---|---|---|
| HTTP 表面 | [control-plane-api.md](control-plane-api.zh-CN.md) §5 —— 查询与控制端点表 | 每个端点一个带类型的方法，按端点的 `(method, path)` 命名 |
| 认证与 capabilities | [control-plane-api.md](control-plane-api.zh-CN.md) §3 | 配置里的 bearer token；一次调用所需的 capability，写在该方法的文档里 |
| 错误模型 | [control-plane-api.md](control-plane-api.zh-CN.md) §4 —— `{code, message, retryable, cause}` | 一个携带全部四个字段的带类型错误，绝不是裸字符串 |
| 事件流 | [control-plane-events.md](control-plane-events.zh-CN.md) —— envelope 及其 `kind` | 一种带类型的订阅；`gap` 作为一种**恢复指令**暴露，不是错误 |
| 持久化类型 | [config-schema.md](config-schema.zh-CN.md) | 客户端读或写的配置类型，出自该文档冻结的 schema |

**路由清单是单一真源。** [control-plane-api.md](control-plane-api.zh-CN.md) §5 的表由一条测试对着服务器
自己的路由定义断言（`the_table_has_the_documented_endpoints`），所以据它生成的 SDK 不可能与服务器漂开。
这正是把 SDK 的表面**生成**而不是手工拷贝的意义：**一条新端点到达 SDK，是因为它先到达了那张表。**

## 3. Rust SDK（M7c）

- **一个不依赖本 workspace 任何运行时件的 crate。** 它说 HTTP；它从不链接 `host-core`，也从不*在进程内*
  需要控制平面。客户端在另一台机器上，是 SDK 的常态。
- **不强加运行时。** 这个库不会把一个 async executor 硬塞给只想要一次阻塞调用的消费者；它是同步、异步
  还是两者兼备，是**实现批**的事，且会以「单线程工具能用它」为准则来决定。
- **类型就是 schema 的。** 内核持久化的东西（[config-schema.md](config-schema.zh-CN.md)）与 API 回答的
  东西（[control-plane-api.md](control-plane-api.zh-CN.md) §5）就是 SDK 的类型，不是一套并行的模型。

## 4. TypeScript SDK（M7d）

- **一个包，浏览器与 Node 通用。** 它就是管理程序的适配器已经经 HTTP 做的那类调用
  （`ui/src/api/http.ts`），提升成一个带类型的库。
- **流用 `fetch` 读，不用 `EventSource`。** 这是一条有记录的坑、不是偏好：`EventSource` 不能设 header，
  而流需要 `Authorization: Bearer`，帧是 `id:` / `data:` 行
  （[control-plane-client-guide.md](control-plane-client-guide.zh-CN.md) §11）。SDK 把这条规则藏在一次订阅
  之后，但它不把其下的 `gap` 语义抽象掉。
- **类型随包发布。** 消费者绑好 `Bearer` 与 base URL，每个响应都带类型，于是客户端作者不必重新声明 API
  的形状。

## 5. 版本化，以及为何 SDK 才谈得上承诺

一个 SDK 的版本，只有和 API 的稳定性一样有意义，而 **API 尚未稳定**：
[control-plane-api.md](control-plane-api.zh-CN.md) §7 说整个 v0.x 线发布破坏性变更而**不**升前缀，而
**v1.0 才是冻结**（自此路径变为 `/v1/`，加法式字段不升版本、语义变更才升）。

因此 SDK 遵循 API 自己的规则，并把它说出口：
- **冻结之前**，一个 SDK 对不稳定性是诚实的：它钉一个 **RiscDom 版本范围**、不是 API 版本，恰如 §7 对
  任何 v0.x 客户端的规定。
- **在冻结之时**，SDK 继承 §7 给出的保证 —— 加法式字段不升版本、语义变更才升 —— 而那才是 SDK 第一次
  可以称之为稳定的版本。
- **持久化类型跟随它们的标记。** 凡 SDK 暴露一个持久化形状之处，其兼容规则是
  [api-compatibility.md §6](api-compatibility.zh-CN.md) 的标记，不是 SDK 自己的编号。

## 6. 一个表面、没有第四份拷贝

本仓已经在一处以上有这层表面，而 SDK 不得再添一个**会分岔的**：

- **API 文档是契约**（[control-plane-api.md](control-plane-api.zh-CN.md)）；它的表被对着服务器断言，所以
  它们就是真相。
- **CLI 与前端是消费者、不是来源。** `cli/src/client.rs` 说 HTTP、从不直接调状态；`ui/src/api/` 是一个
  表面、两种传输（Tauri IPC 与 HTTP）。两者都不是契约，也都不被改写到 SDK 上 —— 它们先于它存在。
- **SDK 在能生成处生成、必须手写处手写。** 端点表面与类型来自文档及其断言；易用性部分 —— 一个 builder、
  一个错误类型、一个订阅 —— 是写出来的。**规则**是一个事实只住一处：加一条路由，就把它加进 API 表，
  而两个 SDK 从那**张表**而不是第二份清单跟随。

**与第二仓的关系。** SDK 是一个*第三方*库，与内核自己的 crate 不同；它不是管理程序（现在的
`riscdom-adminapp`，[multi-repo.md](multi-repo.zh-CN.md)）所消费的东西 —— 该程序在项目之内、用自己的
适配器。SDK 是否发布到 registry（crates.io、npm）是实现批的事，系于
[multi-repo.md §2](multi-repo.zh-CN.md) 为内核推迟的同一个问题。

## 7. 不覆盖什么

- **暂没有其它语言的 SDK。** §14.12 把 Rust 与 TypeScript 定为**优先**、不是上限 —— 后来的语言遵循同一
  份契约，而它所包的表面本就语言中立（HTTP + JSON + 一个 SSE 形状的流）。
- **没有托管服务、没有服务端组件。** SDK 是一个客户端链接的库；它只拨客户端点名的节点，别的不拨。项目
  不运营服务（[roadmap §1](roadmap-v1.0.zh-CN.md)）。
- **没有按 capability 的凭据。** 今天的单一 token 持有整套词表
  （[control-plane-client-guide.md](control-plane-client-guide.zh-CN.md) §10）；按 capability 的 token 是
  v1.0 的工作，SDK 会在它们存在时把它们暴露出来。
- **没有 API 没有的行为。** 四个内核能力缺口（[control-plane-api.md](control-plane-api.zh-CN.md) §6）是
  API 的缺口；SDK 按 API 的方式报它们，不做粉饰。
- **库本身** —— 它们的打包、确切模块布局与发布 —— 是实现批的事，对着本文写。
