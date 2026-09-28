[English](README.md) | 中文

# net

**连接层**（v1.0，[roadmap §4](../docs/roadmap-v1.0.zh-CN.md)），按 [`docs/connection.md`](../docs/connection.zh-CN.md)
冻结的样子：节点身份（§2）、签名（§3）、发现（§4）、房间（§5）与跨区域服务器（§6）。

**今天在这里的：§2、§3、§3.1、§4、§5 与 §6 的 relay。** 节点身份是 `<data-dir>/node.key` 里的一对 Ed25519 密钥 —— 一个首成员为
`schema_version` 的 JWK，以仅属主可读的方式写下，在**首次配置了联网的启动**时铸出，且**读绝不生成**。一条
消息是 `{v, from, to, ts, body}`、对它自己的规范 JSON 签名，`sig` 在旁边；而 `verify` 按
[§3](../docs/connection.zh-CN.md) 冻结的顺序跑那六项检查，并以错误模型的一个分类作答。重放防护按同侪、在
内存、窗口 −5 min / +1 min。传输把一帧作为**一行 JSON 走一条 TCP socket**（`std`，不引 async 运行时）发出，
**先直连**，失败则经过 `Relay` 这道缝。发现知道该往哪拨：`peers.json` 对它自己的节点有权威，下发的
`NodeTable` 作为**来源**被合并且冲突被报告，而 **UDP 信标**只能刷新地址 —— 绝不引入密钥。而 `rooms.json`
说谁在哪个房间里：成员关系，加上三条规则（`rate` 按成员、`mention` 默认 `nobody`、`require_signature`）。
跨区域服务器的 **relay** 以 `RelayServer` 立在代码里：它用自己那份 `peers.json` 认证一帧的发送者
（§3 的模型，零新凭证）、只按签名内的 `to` 路由，并把那帧**沿目的地的会话**交下去 —— 因为 §6.3 让
服务器等着被拨、**从不拨向节点**，这正是本项目不需要打洞的原因。
**还不在这里：§6 的 signalling 与 management 两个角色** —— 地址查询，以及发布注册表与房间定义 —— 它们紧接着
落在同一台服务器上；§7 的审计摘要在等 M5。每一块只在它实现的那一节冻结之后才落地 —— 这正是
[decisions §3](../docs/decisions.zh-CN.md) 要求的事，也是让跨设备工作不必做两遍的原因。

## 依赖方向

`net` 依赖 [`audit`](../audit/README.md)，不依赖本工作区里任何别的东西。链的规范 JSON 与哈希
（[`audit::canonical_json`]、[`audit::fingerprint`]）正是「签什么」与「指纹取什么」的定义，所以那些字节
只有一份描述而不是两份。将来是 `host-core` 依赖**本** crate，绝不反过来：方向是
`audit ← net ← host-core ← server`。

## 要紧的几个文件

| 文件 | 它是什么 |
|---|---|
| `src/versioned.rs` | 「一个首成员是 `schema_version` 的 JSON 文件」的唯一实现：读有四种结果（`Missing` / `Current` / `Migrated` / `TooNew`），更新的文件被**拒绝**而不是半读，另带机密用的仅属主写。`peers.json` 与 `rooms.json`（M4b、M4c）就是要复用它。 |
| `src/identity.rs` | `NodeKey`：铸、存、载，以及「一个文件算不算*可用*的 JWK」的检查（`kty`/`crv` 正确、两半都是 32 字节 base64url、且 `x` 由 `d` 重新导出并比对 —— 被拼接过的文件会被拒绝）。 |
| `src/message.rs` | `SignedMessage`：五成员序言、它所依据签名的规范字节（`audit::canonical_json`），以及一行式的线上形状。 |
| `src/sign.rs` | 按序的六个验证步骤、`PeerKeys`（每同侪一**集合**密钥，所以轮换的灰度期能工作），以及 `VerifyError` 到错误模型分类的映射。 |
| `src/replay.rs` | `ReplayGuard`：按同侪、在内存、一个高水位加上在该水位上见过的负载；推进水位即丢弃集合。 |
| `src/transport.rs` | `Connection` 与 `Listener`：一消息一行 JSON 走一条 **std** TCP socket，帧只序列化一次，先直连、后经 `Relay` 这道缝。端口与超时住在 `TransportConfig` 里，因为 §3.1 没有冻结它们。 |
| `src/error.rs` | 错误模型的五个分类，集中在一处 —— 验证器与传输都把各自的拒绝映射到它。 |
| `src/peers.rs` | `peers.json`：与下发一张表共用的条目形状、说了算的本地文件，以及「peer 表只装**公**钥」这条规矩。 |
| `src/discovery.rs` | `NodeTable`（下发的东西，带 generation，作为**来源**合并并报告冲突）、UDP 信标（`sign_announcement` / `receive_datagram`）与 `RoomFilter` —— 那个默认拒绝的过滤器，使信标无法引入密钥。 |
| `src/rooms.rs` | `rooms.json`：成员关系（成员是一个 **`node_id`**）、三条规则（`RateRule` 与 `RateCounters`、`Mention`、`require_signature`），以及一个房间要能加载必须通过的检查。 |
| `src/relay.rs` | 跨区域服务器的 relay：`RelayServer`（解析、认证、只按 `to` 路由、把那帧沿目的地的会话交下去）、`SessionTable`（谁正拨着 —— 正是服务器「从不拨出」所绕开的那张表）、`RelayClient` / `RelaySession`（节点那一半，以及开一条会话的 `hello`），与 `Forwarder` —— 一个单方法缝，使路由规则身边没有 socket 也能被测试。`src/bin/riscdom-relay.rs` 是**部署者**运行的程序。 |

## 怎么跑

```text
cargo run -p net --example identity -- --self-test
cargo run -p net --example sign -- --self-test
cargo run -p net --example transport -- --self-test
cargo run -p net --example discovery -- --self-test
cargo run -p net --example rooms -- --self-test
cargo run -p net --example relay -- --self-test
```

六条自证的形状与 gate 里其他 example 级证明一致（`worker/examples/remote_executor.rs`）：它们在 scratch
目录里、在内存中或在 loopback 上工作，检查协议冻结的东西 —— 密钥文件的形状与权限；全部六种验证拒绝，以及
「密钥轮换不重置重放水位」这条规矩；消息能往返、两条传输路径携带**逐字节相同**的字节、失败映射到错误模型的
分类；peer 文件能读回、冲突被报告、信标能在 UDP 上验证、过滤器默认拒绝；`rooms` 那条还检查文件能加载、
房间不得降低签名底线、预算是按成员的、以及过滤器从文件里读出成员关系；而 `relay` 那条检查一帧能**逐字节**
到达已拨入的目的地、发给服务器自己的帧绝不会被转交、未知发送者或未知目的地被拒绝、重放只到达一次，以及
服务器**从不拨向**一个没有拨入的目的地。`scripts/gate.sh` 六条都跑；部署者的程序是
`cargo run -p net --bin riscdom-relay -- --help`。

## 本 crate 还**不**做什么

§6.2 的 **signalling 与 management** 两个角色 —— 地址查询，以及发布注册表与房间定义 —— 它们紧接着落在同一台
服务器上；以及 §7 的审计聚合，它等 M5 的授权。在一个没有配置跨区域服务器的部署里，接的仍然是
[`NoRelay`](src/transport.rs)：§6.1 说这样的部署只是丢掉广域那条道。`node.key` 的 keyring 路线 —— §2 允许密钥
住在 OS keyring —— 也还没接：先落的是文件路径，而 `NodeKey` 是一个调用方可以交给任何它偏好的存储的值。**节点的对等端口是配置**（§4.3）
且属于宿主设置，本 crate 不碰它。**授权也不在这里，且是刻意的**：本 crate 回答的是*谁发的*、*房间允许什么*
与*谁可以被采纳*，而一个节点可不可以做某件事是 capability 模型的问题
（[security-model.md §4](../docs/security-model.zh-CN.md)）。

[`audit::canonical_json`]: ../audit/src/run.rs
[`audit::fingerprint`]: ../audit/src/run.rs
