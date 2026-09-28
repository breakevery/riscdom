[English](README.md) | 中文

# net

**连接层**（v1.0，[roadmap §4](../docs/roadmap-v1.0.zh-CN.md)），按 [`docs/connection.md`](../docs/connection.zh-CN.md)
冻结的样子：节点身份（§2）、签名（§3）、发现（§4）、房间（§5）与跨区域服务器（§6）。

**今天在这里的：§2、§3 与 §3.1。** 节点身份是 `<data-dir>/node.key` 里的一对 Ed25519 密钥 —— 一个首成员为
`schema_version` 的 JWK，以仅属主可读的方式写下，在**首次配置了联网的启动**时铸出，且**读绝不生成**。一条
消息是 `{v, from, to, ts, body}`、对它自己的规范 JSON 签名，`sig` 在旁边；而 `verify` 按
[§3](../docs/connection.zh-CN.md) 冻结的顺序跑那六项检查，并以错误模型的一个分类作答。重放防护按同侪、在
内存、窗口 −5 min / +1 min。而传输把一帧作为**一行 JSON 走一条 TCP socket**（`std`，不引 async 运行时）
发出，**先直连**，失败则经过 `Relay` 这道缝。**还不在这里：发现（§4）、房间（§5）与跨区域服务器（§6）**
—— 包括 relay 的路由，这正是 `NoRelay` 存在的原因。每一块只在它实现的那一节冻结之后才落地 —— 这正是
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

## 怎么跑

```text
cargo run -p net --example identity -- --self-test
cargo run -p net --example sign -- --self-test
cargo run -p net --example transport -- --self-test
```

三条自证的形状与 gate 里其他 example 级证明一致（`worker/examples/remote_executor.rs`）：它们在 scratch
目录里、在内存中或在 loopback 上工作，检查协议冻结的东西 —— 密钥文件的形状、权限、`TooNew` 拒绝与「读绝不
生成」；全部六种验证拒绝，以及「密钥轮换不重置重放水位」这条规矩；而 `transport` 那条还检查消息能往返、
一条连接能承载多条帧、先试直连、两条路径携带**逐字节相同**的字节、以及失败映射到错误模型的分类。
`scripts/gate.sh` 三条都跑。

## 本 crate 还**不**做什么

发现（§4）、房间（§5）与跨区域服务器（§6）—— 以及随之而来的 relay 路由，这正是这里唯一的 `Relay` 实现是
[`NoRelay`](src/transport.rs) 的原因。`node.key` 的 keyring 路线 —— §2 允许密钥住在 OS keyring —— 也还没接：
先落的是文件路径，而 `NodeKey` 是一个调用方可以交给任何它偏好的存储的值。**授权也不在这里，且是刻意的**：
本 crate 回答的是*谁发的*，而那个节点可不可以做这件事是 capability 模型的问题
（[security-model.md §4](../docs/security-model.zh-CN.md)）。

[`audit::canonical_json`]: ../audit/src/run.rs
[`audit::fingerprint`]: ../audit/src/run.rs
