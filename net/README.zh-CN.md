[English](README.md) | 中文

# net

**连接层**（v1.0，[roadmap §4](../docs/roadmap-v1.0.zh-CN.md)），按 [`docs/connection.md`](../docs/connection.zh-CN.md)
冻结的样子：节点身份（§2）、签名（§3）、发现（§4）、房间（§5）与跨区域服务器（§6）。

**今天在这里的：crate 骨架，以及 §2。** 节点身份是 `<data-dir>/node.key` 里的一对 Ed25519 密钥 —— 一个
首成员为 `schema_version` 的 JWK，以仅属主可读的方式写下，在**首次配置了联网的启动**时铸出，且**读绝不
生成**。别的都还没落：**没有网络代码、没有传输、没有签名。** 每一块只在它实现的那一节冻结之后才落地 ——
这正是 [decisions §3](../docs/decisions.zh-CN.md) 要求的事，也是让跨设备工作不必做两遍的原因。

## 依赖方向

`net` 依赖 [`audit`](../audit/README.md)，不依赖本工作区里任何别的东西。链的规范 JSON 与哈希
（[`audit::canonical_json`]、[`audit::fingerprint`]）正是「签什么」与「指纹取什么」的定义，所以那些字节
只有一份描述而不是两份。将来是 `host-core` 依赖**本** crate，绝不反过来：方向是
`audit ← net ← host-core ← server`。

## 两个要紧的文件

| 文件 | 它是什么 |
|---|---|
| `src/versioned.rs` | 「一个首成员是 `schema_version` 的 JSON 文件」的唯一实现：读有四种结果（`Missing` / `Current` / `Migrated` / `TooNew`），更新的文件被**拒绝**而不是半读，另带机密用的仅属主写。`peers.json` 与 `rooms.json`（M4b、M4c）就是要复用它。 |
| `src/identity.rs` | `NodeKey`：铸、存、载，以及「一个文件算不算*可用*的 JWK」的检查（`kty`/`crv` 正确、两半都是 32 字节 base64url、且 `x` 由 `d` 重新导出并比对 —— 被拼接过的文件会被拒绝）。 |

## 怎么跑

```text
cargo run -p net --example identity -- --self-test
```

自证的形状与 gate 里那几条 example 级证明一致（`worker/examples/remote_executor.rs`）：它在系统临时目录
下的一个 scratch 目录里工作，铸一把密钥、读回来，并检查文件的形状、权限、`TooNew` 拒绝、以及「读绝不
生成」这条规矩。`scripts/gate.sh` 把它与其他几条并排跑。

## 本 crate 还**不**做什么

签名（§3）、传输（§3.1）、重放防护（§3.2）、发现（§4）、房间（§5）与跨区域服务器（§6）。`node.key`
的 keyring 路线 —— §2 允许密钥住在 OS keyring —— 也还没接：先落的是文件路径，而 `NodeKey` 是一个
调用方可以交给任何它偏好的存储的值。

[`audit::canonical_json`]: ../audit/src/run.rs
[`audit::fingerprint`]: ../audit/src/run.rs
