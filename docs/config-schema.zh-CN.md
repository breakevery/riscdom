[English](config-schema.md) | 中文

# 配置 schema

**状态** v1.0 规范（M7f）｜ **日期** 2026-09-29 ｜ **面向读者** 发行版与工具的开发者：写、校验或生成
节点配置文件的人。

**本文是什么。** [decisions §16](decisions.zh-CN.md) 已定：配置以 JSON Schema 描述，落在这个文件里。
下面就是那份描述，以**文档**形式给出：节点数据目录里的三个文件 —— `settings.json`、`peers.json`、
`rooms.json` —— 的每一个字段、类型、是否可缺、缺了是什么意思，以及每种格式怎么版本化。机器可读的
`*.schema.json` **尚未随包发布**；将来有的话，它是从这份描述**生成**的，而不是与它手工并置的。

**范围：写进磁盘的东西，不是运行时 API。** 控制平面的 HTTP 面是
[control-plane-api.md](control-plane-api.zh-CN.md)；线上协议是 [connection.md](connection.zh-CN.md)。
本文不描述任何请求、应答或帧。

## 1. settings.json

`<data-dir>/settings.json`。启动时读取，设置变化时由宿主写回。一个对象、没有外层包装：它的字段就是
`LocalSettings` 携带的那些（[`host-core/src/settings.rs`](../host-core/src/settings.rs)）。

| 字段 | 类型 | 可缺 | 缺时 | 含义 |
|---|---|---|---|---|
| `version` | number | 可（按**最老**格式读） | 最老格式，v1 | 格式版本。自 v1.0 M2b-1 起写成 `2`；`version` 不是数字的文件根本不是配置文档。 |
| `toolchain_path` | string \| null | 可 | `null` | 手动的 RISC-V GCC 路径；`null` 表示自动发现。空串或纯空白视同缺省。 |
| `zig_path` | string \| null | 可 | `null` | 手动的 Zig 可执行文件；`null` 表示自动发现。 |
| `rust_sysroot` | string \| null | 可 | `null` | 手动的 `rust-std-*` sysroot **目录**；`null` 表示取环境。 |
| `qemu_path` | string \| null | 可 | `null` | 手动的 QEMU 可执行文件；`null` 表示自动发现。 |
| `preflight` | object \| null（**PreflightCache**，§1.1） | 可 | `null` | 最近一次环境预检，与它被产出时对应的指纹绑定。 |
| `theme` | `"light"` \| `"dark"` \| `"system"` \| null | 可 | `null` = `system` | 界面主题。 |
| `language` | `"system"` \| `"en"` \| `"zh"` \| null | 可 | `null` = `system` | 界面语言。 |
| `alert_on_audit_failure` | boolean | 可 | **`true`** | 审计写入失败时界面是否叫出来。日志行与 `audit:failed` 事件无论如何都会发。 |
| `sandboxes` | **SandboxDef** 的数组（§1.2） | 可 | `[]` | 手写的定义。扫描结果从不落在这里：注册表在读取时合并。 |
| `default_sandbox` | string \| null | 可 | `null` | 调用方不点名时拿到的定义；`null` = 内建兜底。 |
| `executors` | **Executor** 的数组（§1.3） | 可 | `[]` | 本节点可派发到的执行器进程。空列表是可用配置：这时 `POST /v0/tasks` 对任何目标都答 `404`。 |
| `network` | object \| null（**NetworkSettings**，§1.4） | 可 | `null` | 本节点如何跟网络说话。`null` = 没有配置任何网络接线。 |
| `llm_configs` | **LlmConfigEntry** 的映射（§1.5） | 可 | `{}` | 每个执行器的模型配置，以**执行器 id** 为键。永不携带密钥：凭据住在 OS 钥匙串里。 |

**上表每个字段都是加法式的。** 一个字段出现之前写下的文件，会以该字段的默认值载入，且**加法不挪
`version`** —— 这也是本表大多数条目的写法是「可缺、且缺时是这个意思」的原因。

### 1.1 `preflight`（PreflightCache）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `fingerprint` | string | 否 | — |
| `ok` | boolean | 否 | — |
| `failed_step` | string \| null | 可 | `null`：没有失败（仅在 `ok` 为假时出现） |
| `detail` | string \| null | 可 | `null`：没有细节 |
| `suggestion` | string \| null | 可 | `null` |
| `checked_at_ms` | number | 否 | — |
| `overridden` | boolean | 可 | `false`：用户没有选「继续」 |

### 1.2 `sandboxes[]`（SandboxDef）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `name` | string | 否 | — 请求点名的名字，也是合并时匹配的键 |
| `display_name` | string \| null | 可 | `null` |
| `memory_mb` | number \| null | 可 | `null` = 宿主的默认（128 MiB） |
| `qemu_exe` | string \| null | 可 | `null` = 自动发现 |
| `toolchain_path` | string \| null | 可 | `null` = 自动发现 |
| `kernel` | string \| null | 可 | `null`：由模型在 `start_vm` 里挑 ELF |
| `notes` | string \| null | 可 | `null` |
| `supports_multiplexing` | boolean | 可 | **`false`** —— 一条声明：该定义可以同时承载多个实例 |

### 1.3 `executors[]`（Executor）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `label` | string | 否 | — 任务寻址用的身份（`Task.target`） |
| `program` | string | 否 | — |
| `args` | string 的数组 | 可 | `[]` |

写残的执行器（`label` 或 `program` 为空）在载入时被**丢弃**，不会被修补。

### 1.4 `network`（NetworkSettings）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `remote_url` | string \| null | 可 | `null` = 内嵌宿主，即本字段出现之前每个版本的行为 |
| `lan_enabled` | boolean | 可 | `false` = 本节点谁也不服务 |
| `lan_bind` | string \| null | 可 | `null` = 回环（`127.0.0.1:7821`） |
| `lan_allow_lan` | boolean | 可 | `false` = 仅回环 |
| `cross_region_server` | string \| null | 可 | `null` = 没有广域那条道。一个必须出现在本节点 `peers.json` 里的 `node_id`。 |
| `server_role` | object \| null（**ServerRoleSettings**，§1.4.1） | 可 | `null` = 本节点只是客户端 |

**本节没有任何秘密。** 远端服务器的 bearer token 住在 OS 钥匙串里，密钥从不出现在配置文件里 —— 这是
`settings.json` 自 v0.4 起的规矩。

#### 1.4.1 `network.server_role`（ServerRoleSettings）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `bind` | string | **否** | — 地址由部署者写。刻意没有默认值：默认值就等于项目在指定服务器在哪。 |

### 1.5 `llm_configs`（值：LlmConfigEntry）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `provider_id` | string | 可 | `""` |
| `base_url` | string | 可 | `""` |
| `model` | string | 可 | `""` |

**密钥不在这里。** `llm_configs` 只放重启所需的非秘密半边；API key 住在 OS 钥匙串里。于是配置文件
可以被读、被复制、被备份，而不泄露对一个模型的访问权。

## 2. peers.json

`<data-dir>/peers.json`。这个节点认识谁。一个对象，版本标记**在最前**，随后是条目。

| 字段 | 类型 | 可缺 | 缺时 | 含义 |
|---|---|---|---|---|
| `schema_version` | number | 可（按 `1` 读） | `1` | 格式版本：自 v1.0 M4a 起为 **1**。 |
| `peers` | **PeerEntry** 的数组（§2.1） | 可 | `[]` | 条目。节点可以谁也不认识，那是可用配置。 |

### 2.1 `peers[]`（PeerEntry）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `node_id` | string | 否 | — 签名消息 `from` 携带的设备名 |
| `addresses` | string 的数组 | 可 | `[]` —— 只能经 relay 抵达的节点一个都不报 |
| `public_key` | object：**公开** JWK（`OKP`/`Ed25519`） | 否 | — 带 `d` 成员的条目被**拒绝**：公钥位置出现私钥是错误，不是值 |
| `capabilities` | string 的数组 | 可 | `[]` —— 节点*声称*自己能做什么；是声明，不是事实 |
| `rooms` | string 的数组 | 可 | `[]` —— 节点*声称*的房间；成员关系以本地 `rooms.json` 为准 |

## 3. rooms.json

`<data-dir>/rooms.json`。成员关系与房间携带的规则。

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `schema_version` | number | 可（按 `1` 读） | `1` —— 自 v1.0 M4c 起的格式版本 |
| `rooms` | **Room** 的数组（§3.1） | 可 | `[]` |

### 3.1 `rooms[]`（Room）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `name` | string | 否 | — |
| `members` | string 的数组（**`node_id`**） | 可 | `[]` |
| `rules` | object（**RoomRules**，§3.2） | 否 | — |

### 3.2 `rooms[].rules`（RoomRules）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `rate` | object（**RateRule**，§3.3） | 否 | — |
| `mention` | `"members"` \| `"nobody"` | 可 | **`"nobody"`** —— 默认拒绝是这个项目的习惯 |
| `require_signature` | boolean | 否（且必须为 **`true`**） | — `false` 在载入时被**拒绝**，因为 §3 让签名在对等路径上普遍成立，房间设置不得拉低那条底线 |

### 3.3 `rooms[].rules.rate`（RateRule）

| 字段 | 类型 | 可缺 | 缺时 |
|---|---|---|---|
| `messages` | number | 否 | — 一个成员在一个窗口内可发的条数 |
| `window_seconds` | number | 否 | — 窗口，以秒计 |

预算是**按成员**的、从不按房间：房间级的预算会让一个成员把别人饿死。

## 4. 版本化

规矩在 [api-compatibility.md §6](api-compatibility.zh-CN.md)：**每种持久化格式把版本标记放在它的第一个
字段**，且标记**按格式各自独立** —— 它们不是全仓一个号：

| 格式 | 标记 | 今天 |
|---|---|---|
| `settings.json` | `version` | **2**（v1.0 M2b-1；第一次真正的迁移，1 → 2） |
| `peers.json` | `schema_version` | **1**（v1.0 M4a） |
| `rooms.json` | `schema_version` | **1**（v1.0 M4c） |

- **新的能读旧的。** 读取方在打开时迁移；配置文件在被重写前会先在旁边留一份 `settings.json.bak`，
  于是迁移出问题时有一条已知的退路。
- **旧的不读新的。** 来自更新版本的文件答 `data_too_new`，什么都不读、不应用、不写入 —— 绝不半读、
  绝不静默降级。
- **加法不挪标记。** 新增的可选字段是加法式的：旧文件以该字段的默认值载入。只有**结构性**变化才挪
  那个数字，而且挪它的同一批里要写下迁移步骤。

## 5. 不覆盖什么

- **`node.key` 是身份，不是配置。** 它是一个 Ed25519 JWK，带自己的 `schema_version` = 1
  （[connection.md](connection.zh-CN.md) §2），和每种持久化格式一样被列在
  [api-compatibility.md §6](api-compatibility.zh-CN.md) 的标记表里 —— 且**刻意不属**本 schema：配置的
  校验器永远不该被迫处理密钥材料。
- **两个 SQLite 存储**（`audit.db`、`sessions.db`）把标记放在 `PRAGMA user_version` 里；链的 schema 与
  会话存储的 schema 是它们自己的事，不是配置。
- **token 文件**（`<data-dir>/token`）根本没有版本：一行十六进制，做形状检查。
- **运行时状态从不是配置。** 在线表、见证者表、会话、运行索引与一台 VM 的活状态，是节点*观察*到的
  东西，不是部署者*写*下的东西。它们都不在上面三个文件里，也都不属本文能冻结的任何 schema。
