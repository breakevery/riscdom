[English](connection.md) | 中文

# 连接层

**状态** v1.0 规范（里程碑 [M4](roadmap-v1.0.zh-CN.md)）｜ **日期** 2026-09-28 ｜ **基线**
v0.9.9（`3365970`）｜ **读者** 内核开发者与部署者 —— 跑不止一个节点的人。

**本文是什么。** 把第二层的规则（[roadmap §4](roadmap-v1.0.zh-CN.md)）写下来，好让它们**在被实现之前
先被冻结**。M4 是本项目第一个**从零建设**的里程碑：侦察发现全仓**没有任何网络代码**、没有任何密钥
材料、也没有 `node.key` / `peers.json` / `rooms.json` 的解析 —— 只有它们背后的决策
（[§7](decisions.zh-CN.md)、[§13](decisions.zh-CN.md)、[§33](decisions.zh-CN.md)）与将要承载它们的**缝**
（`Authn` 钩子、`NetworkSettings`、`audit` 的 digest、keyring 包装、`agent::identity` 的设备分区 id）。

**它是分部分写的。** M4 拆成五块，本文随之生长：**M4a 身份与签名**（下面已写）、M4b 发现、M4c 房间、
M4d 跨区域服务器、M4e 审计 digest。尚未写完的一节写 **deferred** 并点名它属于哪一块；任何 deferred 都
不是对它形状的承诺。

**它的同伴。** [decisions §13](decisions.zh-CN.md)（凭据与密钥管理）与 [§7](decisions.zh-CN.md)（跨设备）
是 §2 与 §3 背后的决策；[security-model.md](security-model.zh-CN.md) §1 说每份机密住在哪；
[api-compatibility.md](api-compatibility.zh-CN.md) §6 是新格式登记的地方；
[error-model.md](error-model.zh-CN.md) 是失败行走时用的词汇。

已写的每一节都以「**冻结**」与「**未冻结**」收尾。

## 1. 冻结了什么，没冻结什么

- **[已定]** **本文冻结**：节点身份（密钥对、它的文件、它的生成）见 §2，以及签名（`@`、被签的字节、验证、与 bearer token 的关系）见 §3，外加 §8 的两条常设约束、§9 的信任模型与 §10 的红线测试。
- **[待定]** **Deferred**：发现（M4b）、房间（M4c）、跨区域服务器（M4d）与审计 digest（M4e —— 它还要等 M5 的授权）。§4–§7 是标题，不是形状。
- **[待定]** **连 §2–§3 内部也有未冻结**：一条签名消息走什么**传输**（M4d）、重放窗口多大、已见消息的集合住在哪（[§3](#3-签名-)），以及消息所寻址的**房间**的线上形状（M4c）。

**冻结**：§2 与 §3 写明的身份与签名。**未冻结**：§4–§7 点名的任何东西，以及上面标 *待定* 的每一处。
沉默不是承诺。

## 2. 节点身份

- **[已定]** **一个节点的密码学身份是一对 Ed25519 密钥**（[decisions §13](decisions.zh-CN.md)）：一把它用来签的私钥、一把同侪用来验的公钥。没有一样是自造的 —— 曲线与编码都是标准，项目只在容器上**加一个字段**，而不是自造一个格式。
- **[已定]** **住在哪**：默认 `<data-dir>/node.key`，**可选**放 OS keyring（[decisions §13](decisions.zh-CN.md)、[security-model.md](security-model.zh-CN.md) §2）。文件权限 **`600`**（Windows 上是仅属主 ACL）；权限限制不了的文件会被**拒绝**而不是被使用 —— 与 bearer token 已经遵循的规矩一致（[security-model.md](security-model.zh-CN.md) §2）。
- **[已定]** **文件是一个 JWK**（[RFC 7517](https://www.rfc-editor.org/rfc/rfc7517) / [8037](https://www.rfc-editor.org/rfc/rfc8037) 的 `OKP`/`Ed25519`），JSON、UTF-8、**无 BOM**，另带一个 JWK 规范没有定义、而我们要求**排第一**的成员：

  ```json
  {
    "schema_version": 1,
    "kty": "OKP",
    "crv": "Ed25519",
    "x": "<base64url 公钥，32 字节>",
    "d": "<base64url 私钥，32 字节>"
  }
  ```

  **为什么是 JWK 而不是 PEM**：[decisions §13](decisions.zh-CN.md) 两者都允许，而 [decisions §11](decisions.zh-CN.md) 要求每个持久化格式把 `schema_version` 带在**首字段**。PEM 的首字段是它的 `BEGIN` 行，所以带版本的 PEM 需要一个容器 —— 那就是第二个格式。JWK 是标准的、本来就与项目写的其他文件一样是 JSON，而且规范允许增补成员：`schema_version` 就是那个成员，不认识它的读者**必须忽略**它。
- **[已定]** **首字段就是首字段**：`schema_version` 是写入的第一个成员、也是读取的第一个。**更新**的 `schema_version` 被拒（`data_too_new`，不读、不写）；更旧的按 [api-compatibility.md](api-compatibility.zh-CN.md) §6 在**打开时**迁移。
- **[已定]** **首次启动时生成，读绝不生成。** 没有密钥的节点在**配置了联网之后第一次启动**时铸一把 —— bearer token 已经遵循的规矩（[`server/src/token.rs`](../server/src/token.rs)、[security-model.md](security-model.zh-CN.md) §2：「读一份凭据绝不创建一份凭据」）。没配联网的节点什么都不铸。
- **[已定]** **密钥不是节点的名字，也不是进程 id。** `agent::identity` 给每个进程铸一个 `<device>-<pid>-<seq>` 的 `AgentId`，其中的 **device 部分就是节点的名字**（默认 `local`；由连接层设置，[roadmap §4](roadmap-v1.0.zh-CN.md)）。密钥对是一份**独立的、活得更久**的身份：它活过每一次重启、每一个 pid、每一次改名。同侪在 `peers.json` 的 `node_id` 里存的是**设备名**，用来验的也是那个名字的公钥。

**冻结**：Ed25519；文件、路径与权限；JWK 形状与排第一的 `schema_version` 成员；配了联网时首次启动
生成；以及「密钥对 / 设备名 / 进程 `AgentId`」三者的分离。**未冻结**：`node.key` 是否在任何部署里都能
住 keyring（[security-model.md](security-model.zh-CN.md) §8 已把它列为 open），以及设备名怎么选、怎么改。

## 3. 签名 `@`

- **[已定]** **`@` 意味着地址 *加* 签名**（[decisions §7](decisions.zh-CN.md)）：以 `@` 寻址的消息由它点名的密钥签名。本节定的是**签什么**，不是消息怎么走。
- **[已定]** **被签的是五成员序言的规范 JSON** —— `{"v", "from", "to", "ts", "body"}` —— 用与 `audit` 的链同一套规范纪律序列化（[`audit::canonical_json`](../audit/src/lib.rs)：对象键固定顺序、无无意义空白）。签名就是对这些字节的 Ed25519。

  | 成员 | 是什么 | 为什么它在签名里 |
  |---|---|---|
  | `v` | 协议大版本 | 签名无法被重放进入另一个协议版本 |
  | `from` | 发送方的 `node_id`（设备名） | 一个有效签名无法被改归他人 |
  | `to` | 接收方的 `node_id`，或所寻址的房间 | 一条签名消息无法被改投另一个节点 |
  | `ts` | epoch 毫秒，发送方的钟 | 接收方的窗口可以被执行 |
  | `body` | 负载 | 负载无法被改动 |

- **[已定]** **签名与消息同行**，放在名为 `sig` 的成员里，序言也随行（接收方需要 `from` 与 `ts` 才能验，也需要与所签**逐字节相同**的 `body`）。
- **[已定]** **验证按此顺序，遇首个失败即停：**
  1. 接收方**认识发送方** —— `from` 在它自己的 `peers.json` 里（或该消息所属的房间，M4c）；
  2. **签名验证通过** —— 用该条目的公钥，对上面那些规范字节；
  3. **`v` 是接收方会说的协议版本** —— 更新的 major 被拒，不猜（[api-compatibility.md](api-compatibility.zh-CN.md) §6）；
  4. **`to` 是本节点**（或它在的房间）；
  5. **`ts` 在接收方的窗口内** —— 窗口外**拒绝**，不排队；
  6. **不是重放** —— 该窗口内已见过的 `(from, ts, body-hash)` 被拒。
  任何一步失败都是一个**应答**，绝不是静默丢弃：理由映射到 [error-model.md](error-model.zh-CN.md) 的分类（发送方未知或重放 → `refused`；签名损坏或版本不符 → `invalid`；时间戳过旧或来自未来 → `network`）。
- **[已定]** **签名是 bearer token *之上*的一层，不是替代品。** bearer token 是**本地**控制平面的门 —— 一台机器、一份盘上的机密（[security-model.md](security-model.zh-CN.md) §3）—— 它**绝不**发给同侪：两个节点不共享机密，只共享公钥。所以节点到节点的消息带的是**签名**，本地调用带的是 **token**，两者刻意由不同的代码检查。同时跑控制平面的节点仍以 token 应答本地调用；签名在那里不加什么、也不改什么。
- **[已定]** **签名是认证；capability 是授权。** 验签说的是**谁**发的。那个节点能不能做这件事，是既有的 capability 问题（[security-model.md](security-model.zh-CN.md) §4），由与其他各处同一套模型回答 —— 一条签名正确、但声明能力里不含该动作的消息会被**拒绝**，正如今天一个有效但 capability 不够的 token 会被拒绝。

**冻结**：`@` 的含义；五成员序言与签名所依据的规范编码；`sig` 成员；六个验证步骤及其顺序；错误映射；
「签名与 token 并存」；「认证在这边、授权在那边」。**未冻结**：传输；重放窗口的大小与记住已见消息的
存储；`to` 的房间形态（M4c）；线上协议里的密钥轮换与撤销（[decisions §13](decisions.zh-CN.md) 定的是它们
的*形状* —— 并行轮换、广播的撤销列表 —— 而 M4d 是它们行走的地方）；以及一条消息能否被多把密钥签名。

## 4. 发现 —— deferred（M4b）

**Deferred 到 M4b。** 内网服务器下发的静态节点表，以及作为补充的、带房间隔离的 UDP 广播
（[roadmap §4](roadmap-v1.0.zh-CN.md)、[decisions §7](decisions.zh-CN.md)）。它的形状依赖 §2（一条节点表
条目是什么），在实现之前写下。

## 5. 房间 —— deferred（M4c）

**Deferred 到 M4c。** `rooms.json`、成员关系，以及房间携带的规则 —— 速率、谁能 `@` 谁、是否要求签名
（[roadmap §4](roadmap-v1.0.zh-CN.md)、[decisions §7](decisions.zh-CN.md)）。§3 已经定了**签名**是什么；
M4c 定的是房间对签名**要求**什么。

## 6. 跨区域服务器 —— deferred（M4d）

**Deferred 到 M4d。** 一台专用服务器，四个角色 —— signalling、relay、management 与 audit aggregation
—— relay 是**主路径**而不是例外，且直连之后数据路径离开 relay（[roadmap §4](roadmap-v1.0.zh-CN.md)）。
一条签名消息的**传输**在这里决定，因此 §3 的开放项（传输、重放存储）也在这里关闭。

## 7. 审计 digest —— deferred（M4e，且需单独授权）

**Deferred 到 M4e。** 以 **30 秒**定时器收集链的 digest，关键事件在发生的当下推送
（[roadmap §4](roadmap-v1.0.zh-CN.md)）。它**最后**写，且在 [decisions §33](decisions.zh-CN.md) 要求的、
针对任何触碰审计边界之事的授权到手之前**不开始**。临时中心、`provisional` 与 `fork` **不是 M4 的** ——
它们属 M5/M6。

## 8. 架构无关

- **[已定]** **身份层不对 guest 做任何假设。** §2 与 §3 不点名任何机器、指令集或模拟器：节点的密钥签的是*消息*，不是二进制，而消息体对承载它的那一层是不透明的。这里不得生长出任何只对一种架构才有意义的字段（[roadmap §4](roadmap-v1.0.zh-CN.md)、[decisions §2](decisions.zh-CN.md)）。
- **[已定]** **这与插件接口不冲突。** [decisions §3](decisions.zh-CN.md) 把**沙箱**放到进程外，[plugin-interface.md](plugin-interface.zh-CN.md) 冻结那道缝；本文讲的是**节点**。两者不在任何地方相遇：插件进程没有密钥、什么都不签、也从不是同侪。

**冻结**：这条约束，以及与插件缝的分离。**未冻结**：两层中任何一层在内部增补的东西。

## 9. 信任模型

- **[已定]** **身份是签名层，不是授权层。** 验签确立的是*谁*；它什么都不授予（[§3](#3-签名-)）。每一个权限决定仍归 capability 模型（[security-model.md](security-model.zh-CN.md) §4），所以一个节点无法靠持有一把密钥获得权力 —— 只能靠被授予 capabilities。
- **[已定]** **私钥绝不离开节点。** 它被读来签名；它从不被发送、从不被记日志、从不进消息、从不进命令行参数（[security-model.md](security-model.zh-CN.md) §2）。`peers.json` 只存**公**钥。
- **[已定]** **同侪在被认识之前不被信任。** §3 不接受来自接收方未被配置为认识的节点的消息：没有「首次使用即信任」，也没有按地址的信誉。未知的 `from` 是 `refused`。
- **[已定]** **节点替同侪做的事在链上。** 一条签名消息要求、节点照做的行为，是与其他行为一样的审计行（[security-model.md](security-model.zh-CN.md) §7）—— 这一层认证请求，绝不变成第二本账。

**冻结**：签名不是授权、私钥的收束、不做首次信任、以及「链仍是记录」。**未冻结**：部署自己的边界
（哪些节点能到哪些节点 —— [decisions §7](decisions.zh-CN.md) 把拓扑留给部署者）。

## 10. 四条红线

[roadmap §1](roadmap-v1.0.zh-CN.md) 写下四条约束，并说它们是每个里程碑的工作都得熬过的测试。此处只
引用，不复述。

- **非通用沙箱。** §8 已为这一层写下：身份与签名不点名任何架构，消息体是不透明的。
- **无内置 supervisor。** 签名决定的是**谁**，从不是**什么**：§3 止步于 capability 层，这里没有任何规则让一个节点凭它的密钥替另一个节点行动。
- **无官方运营服务。** 密钥对是部署者机器上的一个文件，签名由接收节点验证；本文不点名任何**提供**服务的一方（[§6](#6-跨区域服务器--deferred-m4d) 是履行该义务的地方，待它被写下时）。
- **审计不变量不动。** §9 让链仍是记录，而 §2 与 §3 没有定义任何审计事件、哈希输入或链操作 —— §7 的 digest 是对**已经存在**的链的**传输**。

**冻结**：本文的任何修正都不得让四条之一失败。**未冻结**：那四条是**测试**，不是改动其含义的许可。
