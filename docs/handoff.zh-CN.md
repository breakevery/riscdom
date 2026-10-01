[English](handoff.md) | 中文

# 交接 —— 把 RiscDom 带进下一个对话

**本文件是跨对话交接文档。** 第 1 节是易变快照，正式版发布时更新；第 2–12 节是稳定约束：它们在各批次
之间没有变过，也是新对话必须守住的东西。

仓库 `D:\codeagent\breakevery\riscdom`，远端 `https://github.com/breakevery/riscdom.git`，分支
`main`。每个批次的收尾流程一致：gate 全绿 → `scripts\commit.ps1 "<msg>"`（它自己会跑 gate）→ push ——
而这些面向远端的动作，只在当轮请求明确授权时才做（见 §2）。

## 1. 快照 —— `v0.9.9` 已发布（正式发布时更新本节）

- **连接层的身份与签名已冻结，而 M4 是五块**（v1.0 M4a，2026-09-28）。[`docs/connection.md`](connection.zh-CN.md) 是那份规范：节点身份（`<data-dir>/node.key` 或 keyring 里的 Ed25519 密钥对，一个首成员为 `schema_version` 的 JWK，首次配置联网的启动时铸出）与 `@`（地址 **加**一个对 `{v, from, to, ts, body}` 规范 JSON 的签名、六个验证步骤，以及「签名认证、capability 授权」这条规矩）。发现（M4b）、房间（M4c）、跨区域服务器（M4d）与审计 digest（M4e，需单独授权）**deferred**；临时中心属 M5/M6。`node.key` 与 `peers.json` 是新的持久化格式。**决策 §88。** **它的 §3 开放项也已关闭**（v1.0 M4a，2026-09-28）：一条签名消息是**一行 JSON 走 TCP**，先直连、否则经 relay，**两条路径上逐字节相同**；重放由**每同侪、内存里的高水位**在 −5 min / +1 min 窗口上封顶。**决策 §89。**
- **发现也已冻结**（v1.0 M4b，2026-09-28）：[`docs/connection.md`](connection.zh-CN.md) §4 —— 内网服务器下发一张由 `peers.json` 形状条目构成的表（启动/重连/变更时，带 generation，作为一个 §3.1 帧），而 **UDP 广播**补充它：一个数据报、一个签名帧，且报名**能刷新地址、无法引入密钥**，所以 §9 仍然成立。房间隔离是一个默认拒绝的过滤器，它读的成员关系其形状属 M4c。**决策 §90。**
- **房间已冻结**（v1.0 M4c，2026-09-28）：[`docs/connection.md`](connection.zh-CN.md) §5 —— `rooms.json`（一个文件，`schema_version` 排第一）装着若干房间，其成员是 **`node_id`**、其规则是 `rate`（按成员）、`mention`（默认 `"nobody"`）与 `require_signature`（只有 `true`，因为 §3 已经垫底）。成员关系是配置 —— v1.0 没有加入协议 —— 而发现过滤器读的是「文件点名了那个房间**且**列出了本节点」。**决策 §91。**
- **连接层的实现已开始**（v1.0 M4a-impl-1，2026-09-28）：一个新的 **`net/`** crate，依赖 `audit`（从不依赖 `host-core`），把 [`docs/connection.md`](connection.zh-CN.md) §2 落到了盘上 —— `node.key` 是一个 JWK（`schema_version` 排第一，`OKP`/`Ed25519`，`x`/`d` 32 字节），在首次配置联网的启动时铸出，经 `create_new` 仅属主可读，且**读绝不生成** —— 外加一个可复用的版本化 JSON 加载器（`TooNew` 被拒）与一条 gate 现在会跑的 `--self-test`。**决策 §93。** **它的签名那一半也已落盘**（v1.0 M4a-impl-2，2026-09-28）：`net` 实现了 [connection.md](connection.zh-CN.md) §3 —— `SignedMessage` 与按冻结顺序的六个验证步骤，每次拒绝都携带错误模型的一个分类；以及 §3.2 的每同侪、内存里的重放记录。**决策 §94。**
- **传输也已落盘**（v1.0 M4a-impl-3，2026-09-28）：`net` 用 **`std::net`** 实现 [connection.md](connection.zh-CN.md) §3.1 —— 一消息一行 JSON、帧只序列化一次（所以直连与经 relay 的字节相同）、先直连后 `Relay` 这道缝，而 relay 的路由留给 M4d。**决策 §95。**
- **发现也已落盘**（v1.0 M4b，2026-09-28）：`net` 实现了 [connection.md](connection.zh-CN.md) §4 —— `peers.json`（对自身节点有权威，且拒绝私钥）、作为**来源**合并并报告冲突的下发 `NodeTable`、只能刷新地址的 **UDP 信标**，以及 `RoomFilter`（默认拒绝）。**决策 §96。**
- **房间也已落盘**（v1.0 M4c，2026-09-28）：`net` 实现了 [connection.md](connection.zh-CN.md) §5 —— `rooms.json`（`{name, members[], rules}`）、三条规则（`rate` 按成员、`mention` 默认 `nobody`、`require_signature` 只有 `true`）—— 而 `RoomFilter::from_rooms` 从那个文件读出成员关系，合上了 M4b 留下的环。**决策 §97。**
- **跨区域服务器的 relay 也已落盘**（v1.0 M4d，2026-09-28）：`net` 实现了 [connection.md](connection.zh-CN.md) §6 的 **relay** —— `RelayServer` 用自己那份 `peers.json` 认证一帧的发送者（§3 的模型，零新凭证）、只按签名内的 `to` 路由，并把那帧沿目的地的**会话**交下去（`SessionTable`），因为 §6.3 要服务器等着被拨、从不拨向节点。`RelayClient` / `RelaySession` 是节点那一半；`hello_body()` 开一条会话且**不**消耗发送者的 §3.2 记录；`src/bin/riscdom-relay.rs` 是部署者的程序。**决策 §98。**
- **而 §6.2 的另两个角色也已落盘**（v1.0 M4d，2026-09-28）：`net` 实现了 [connection.md](connection.zh-CN.md) §6.2 的 **signalling** —— `{"query": "<node_id>"}` 帧被答以 `{"addresses": [...]}`，即节点拨入所用的地址加上它的条目，且别无其他 —— 与 **management** —— 注册表请求被答以 §4.1 的表加房间定义，合并时本地 `peers.json` / `rooms.json` 赢、冲突被**报告**。服务器用自己的密钥签它的回答。于是 **M4 除 M4e 外走完了**（审计摘要，它等 M5 的授权）。**决策 §99。**
- **而 `net` 不再是一个没人用的 crate**（v1.0 批 W，2026-09-28）：`host-core` 依赖 `net`，并在启动时加载连接层的三个文件 —— `node.key`、`peers.json`、`rooms.json` —— **只在 `settings.network` 点名了接线时**；未配置的节点什么都不读、也不长密钥（connection.md §2）。`AppState` 答 `node_key()` / `peers()` / `rooms()` 与 `connection_problem()`；更新的文件被拒绝且从不被覆写；两个审计名记录发生了什么（`host.connection.key_minted`、`host.connection.data_too_new`）。**决策 §100。** V-2（跨区域指针 + 客户端）与 V-3（上层暴露）接在后。
- **而上报半边也已冻结**（v1.0 批 X，2026-09-28）：[`docs/connection.md`](connection.zh-CN.md) §6.6 —— 节点向上**注册**（一个普通 §3 帧；**帧里不走密钥**），随后每 **15 秒** **心跳**一次，而服务器保留一张**在线状态表**（**45 秒**内 `online`、之后 `offline`；一行永不因离线被删）。一种形状、**两个层级**（节点→内网服务器，内网服务器→跨区域服务器），内网服务器注册的是**它自己**而不是它背后的节点，加入靠配置且**没有自动审批**。**决策 §101。** 纯文档、无代码；V-proto-2（活性）与 V-2（指针 + 客户端）接在后。
- **而活性也已冻结**（v1.0 批 Y，2026-09-28）：[`docs/connection.md`](connection.zh-CN.md) §6.7 —— 节点**探测**它 workgroup 里的同侪（`{"probe": 1}` / `{"alive": 1}`，每 15 秒，连失三拍 = 它自己的*不可达*），向上**报告**视图，而服务器按**在剩下的见证者中全体一致**来**判定**（「活着」的见证者一票否决；独自的节点永不被判）。内网服务器自身的失联由它的**兄弟**确认、而不是由下级（下级与它共 LAN 与供电），跨区域服务器按同一条规则汇总判定。一次判定记下 `judged_at_ms` 并写 `host.connection.peer_offline` / `host.connection.peer_recovered`；**协议不定义移除** —— 踢是部署者的事。**决策 §102。** 纯文档；V-2（指针 + 客户端）与 V-3（踢的 API 会落在那里）接在后。
- **而接线也已落盘**（v1.0 批 Z / V-2，2026-09-28）：`NetworkSettings.cross_region_server` 点名服务器（一个在本节点 `peers.json` 里的 `node_id`），`host-core` **不拨号**地造出 `RelayClient`，并跑一条**注册 + 心跳线程**（`std::thread` + channel，§6.6 的 **15 秒**），而 `net` 的服务端侧保留一张 **`OnlineTable`**（**45 秒**内 `online`、之后 `offline`，行永不因离线被删）—— 注册被应答、心跳不被应答，而心跳单独不给人落位。悬挂指针被拒。不写任何链行。**决策 §103。** 接下来是 V-3（活性判定，以及上层暴露）。
- **而节点层的活性判定也已落盘**（v1.0 批 AB / V-3a，2026-09-28）：[`docs/connection.md`](connection.zh-CN.md) §6.7 的**节点层** —— 探测者**探测**它 workgroup 里的同侪（`{"probe": 1}` / `{"alive": 1}`；15 秒，连失三拍 = *不可达*），向上**报告**视图，而服务器按**在剩下的见证者中全体一致**来**判定**（「活着」的见证者一票否决；独自的节点永不被判）。那一行多出 **`judged_at_ms`**，而判定服务器把转换交给一个 **sink**，由它经 `emit_host` 写 **`host.connection.peer_offline`** / **`peer_recovered`** —— 于是两个事件落在 `host.connection.*` 本就住着的地方，而**不是** `control-plane-events.md`。`host-core` 在心跳线程旁跑一条**探测线程**，共用节点唯一一条会话；恢复是被听到；协议**不定义移除**。兄弟确认属 **V-3b**（它需要 V-4 的面）。**决策 §104。** 接下来是 V-4（上层暴露）。
- **而桌面能读连接层了**（v1.0 批 AD / AC-1，2026-09-28）：`host-tauri` 多出四个只读命令 —— `get_node_key`（`NodeKeyView`：`node_id`、`public_jwk`、`fingerprint`、`short_fingerprint` —— 是**视图、不是密钥**，因为 `net::NodeKey` derive 了 `Serialize` 且带着私钥 `d`）、`list_peers`（`Vec<net::PeerEntry>`）、`list_rooms`（`Vec<net::Room>`）与 `connection_status`（`configured` / `connected` / `problem`）—— 而 `ui/src-tauri` 的 `generate_handler!` 登记它们。每一个都包装既有的 `AppState` 方法、不写任何东西；设置仍作决定、接线仍照办。这是 V-4 四个面里的 **AC-1**；接在后的是 AC-2（server 路由）、AC-3（CLI）与 AC-4（服务端角色面，V-3b 需要的那个）。没有哈希公式、路由、capability 名、审计事件常量或持久化格式被改动。**决策 §105。**
- **而节点的连接层能经 HTTP 读了**（v1.0 批 AE / AC-2，2026-09-28）：`server` 提供四条只读查询 —— `GET /v0/identity`、`/v0/peers`、`/v0/rooms`、`/v0/connection` —— 包装的正是批 AD 那四个命令所包装的同一批 `AppState` 访问子，所以两个面不会漂开。四条都声明 **`status.read`**（本节点自身的外表面），因此没有新增 capability 名。**缺数据就是 `null`，绝不是 `404`**（§2）：层未配置时 `identity` 是 `null`，没有 `peers.json` 或 `rooms.json` 时 `peers` 与 `rooms` 是 `null`；`connection` 把 `configured` / `connected` / `problem` 分开。`docs/control-plane-api.md` 的 §5.1 两种语言都从 **33 挪到 37**，四条路由也进了 tool-schema 表。没有哈希公式、capability 名、审计事件常量或持久化格式被改动。这是 V-4 四个面里的 **AC-2**；接在后的是 AC-3（CLI）与 AC-4（服务端角色）。**决策 §106。**
- **而一个节点可以服务它的 workgroup 了**（v1.0 批 AF / AC-4，2026-09-28）：`NetworkSettings` 多出 **`server_role`**（`bind`，必填、无默认），`host-core` 用本节点自己的 `node.key` / `peers.json` / `rooms.json` 起一个 **`RelayServer`** —— §6.5 的**内网服务器**，与独立 `riscdom-relay` 跑的是同一套机制。它同步绑定（端口被占是一条已报告问题）、在一条线程上服务；`server_role_addr()` 报绑在哪，`server_role()` 交出句柄供 §6.7 的 sink 使用。由**部署者**配置，而**没配的节点什么都不会起**。§6.7 的兄弟确认是 V-3b。**决策 §107。**
- **而服务器会说出自己是服务器**（v1.0 批 AH / V-3b-proto，2026-09-29）：`docs/connection.md` 冻结了兄弟集怎么找 —— 内网服务器在它 §6.6 的注册里声明普通宣告 **`"server"`**，那一行保留注册带来的宣告，而跨区域服务器的兄弟就是宣告里含它的那些行。`"server"` 是**宣告、不是 capability**（不给词表加词、不授予任何东西）；虚假声明只会招来探测。纯协议正文 —— 实现是 V-3b-1/V-3b-2。**决策 §108。**
- **而节点会探它的兄弟，且任何探测都不再无限等待**（v1.0 批 AJ / V-3b-1，2026-09-29）：跑服务器角色的节点探 §6.7 的**第二层** —— 它自己 `peers.json` 里用 `"server"` 声明（`net::SERVER_CLAIM`）声明过的兄弟，密钥取自同一批条目 —— 跑在 V-3a 那条 `Probe` 线程旁的第二条线程上，经它的跨区域 client 上报。跨区域**汇总**半边是 V-3b-2。同一批还**给工具探测加上边界**：`exec_retrying` 现在等 60 秒就 kill，卡住的 `--version` 读作「不可用」、不再把宿主挂住 —— 那个四次拖住本地 gate 的 QEMU flake 关了。**决策 §109。**
- **而部署方接上判定 sink 了**（v1.0 批 AK / V-3b-2，2026-09-29）：`AppState::install_connection_sink(self: &Arc<Self>)` 把 §6.7 的 sink 装到**本节点自己的 server role** 上，而桌面的 setup、`riscdom-server` 的 `main` 与 CLI 的内嵌模式各自在拿得到 `Arc` 的地方调它（构造器不能：它交回 `Self`）。跨区域汇总无需改动 `net`，而独立 `riscdom-relay` 不装 sink —— 它没有链。**V-3b 至此完成**；§6.7 完整了。**决策 §110。**
- **而 CLI 会读连接层了**（v1.0 批 AL / AC-3，2026-09-29）：`riscdom` 多出 **`identity`**、**`peers`**、**`rooms`** 与 **`connection`** —— 四条覆盖 AC-2 路由的一个词命令，渲染键值行或小表，并把三个 `null` 用文字说出来。**V-4 收尾**（桌面 AC-1、路由 AC-2、CLI AC-3、服务端角色 AC-4），连同它一起**除 M4e 外 M4 收尾**。**决策 §111。**
- **而本地 QEMU flake 关了**（v1.0 批 AN，2026-09-29）：gate 的五次 QEMU/QMP 失败是一个**端口交接**竞争（CI 从未见过 —— CI 跳过 `--ignored` 测试）。`sandbox/src/vm.rs` 现在会点名死掉的 QEMU —— **「QEMU exited with code N during <QMP op>（原始错误：…）」** —— 而不是裸 `os error 10054`；`host-core/tests/snapshot_commands.rs` 的两条起客户机测试以文件内的 `SERIAL` 互斥锁一次只跑一条（gate 自身的并行未动）。批 AJ 的 60 秒探针界保留：另一个根因。**仍开放**：孤儿清理（父进程被 force-kill 时的 QEMU 子进程）需要 Job Object 或 `PR_SET_PDEATHSIG` —— 需要 owner 批准的依赖。**决策 §112。**
- **而配置 schema 落盘了**（v1.0 批 AP / M7f，2026-09-29）：`docs/config-schema.md` + zh 逐字段描述 **`settings.json`**、**`peers.json`** 与 **`rooms.json`** —— 类型、可选性、缺时是什么意思、嵌套段展开 —— 外加按格式的版本化（`version` = 2；`schema_version` = 1）与刻意不覆盖的东西（`node.key` 是身份、两个 SQLite 存储、token 文件、运行时状态）。这是 **M7 的第一批**；SDK、backup、observability 与预算接在后。`docs/README.md` 多一行；没有源文件被改动。**决策 §113。**
- **而可观测性契约写下来了**（v1.0 批 AQ / M7g，2026-09-29）：M7 的第三批是一份**规格** —— `docs/observability.md` + zh 写下 §17 的结构化日志字段、`/metrics` 路由（Prometheus 文本，声明**既有的 `status.read`**）及其第一批指标族，以及追踪 ID（`agent_id` + `task_id`，链自己的那一对）—— 外加 roadmap §12 的 `task_id` 缺口怎么用一个可选体字段合上。**仅规格**；写入器与路由以后落地，路由带着计数一起到。**决策 §114。**
- **而性能预算写下来了**（v1.0 批 AR / M7h，2026-09-29）：`docs/performance-budget.md` + zh 把 §18 的四个数字各固定为一个**区间** —— VM 启动（沙箱 `start` → guest 可用）、派发往返（对着工作可忽略地短的本地执行器的那一跳控制平面；跨网那一半等派发接口已为它留位的远程执行器）、内存（节点的十个 agent QEMU 子进程 RSS 之和；默认 `VM_MEMORY_MB = 128` → 十个 guest ≈ 1.25GB），以及日志增长（append-only、无轮转、无 `DELETE` —— 一个形状，不是一个速率）—— 外加每个怎么核对、以及不覆盖什么。**仅规格**；工装在后。**决策 §115。**
- **而两仓关系写下来了**（v1.0 批 AS / M7i，2026-09-29）：`docs/multi-repo.md` + zh 就是 roadmap §11 要的那份 CONTRIBUTING 增补 —— 管理程序（`riscdom-adminapp`）以 **git 依赖钉 tag** 消费内核（`{ git = …, tag = "vX.Y.Z" }`；内核的 `path` 依赖解析到同一份 checkout；`Cargo.lock` 是真正的钉），crates.io 推迟、vendored subtree 否决，外加第二仓继承什么、自己拥有什么。**CLA 跨仓刻意留开**，随拆仓（M7a）一并定。`CONTRIBUTING.md` + zh 多一个指向。**仅规格**；拆仓（M7a）与发布（M7b）在后。**决策 §116。**
- **而备份与可移植性写下来了**（v1.0 批 AT / M7e，2026-09-29）：`docs/backup.md` + zh 把 `riscdom-backup` 定成规格 —— 一个节点的持久状态（数据目录加 **OS keyring** 条目）导出为**一个在运维者口令下加密的文件**，**审计存储经 SQLite 的一致性路径取出**（WAL：字节拷贝会漏 `-wal` 帧）；导入拒绝静默覆盖与 `data_too_new` 的包、把凭据重新录入 keyring、并恢复节点的身份。可移植性的单位是整个节点。**仅规格**；工具在后。**决策 §117。**
- **而 SDK 写下来了**（v1.0 批 AU / M7c + M7d，2026-09-29）：`docs/sdk.md` + zh 把 **Rust 与 TypeScript** SDK（roadmap §14.12 的优先项）定成一个盖在冻结表面上的薄而带类型的层 —— API 的端点表、错误模型、事件 envelope 与配置类型 —— **不添加任何语义**、且**从服务器被断言对着的那张路由表生成**。Rust 不链接 workspace 运行时、不强加 async；TypeScript 用 `fetch` 而非 `EventSource` 读流。版本化跟随 API。**仅规格**；库在后。**决策 §118。**
- **而备份规格被修正了**（v1.0 批 AW / M7e，2026-09-29）：M7e-1 的第一次侦察发现冻结的 `docs/backup.md` 有两处错，本批在任何代码之前先把文档改对。**两个根**：审计存储在 `<workspace>/.riscdom/audit.db`、快照在 `<workspace>/.riscdom/snapshots/<device>/<id>/` 下 —— **不是**初稿所写的在数据目录下 —— 而 `settings.json`、`sessions.db`、`token`、`node.key`、`peers.json`、`rooms.json` 留在数据目录；`toolchain/`、`qemu/`、workspace 的项目文件与 `.bak` 文件不是状态、不入包。**凭据**：OS keyring **没有枚举 API**，所以工具**从 `settings.json` 反推账户名**（`llm-api-key:<executor_id>:<provider_id>`、legacy 的 `llm-api-key:<provider_id>`、`remote-token:<host>`），并**报告而非静默漏掉**它反推不出来的 —— 那是这个包唯一声明的包外依赖，一条像 `connection.md` §3.2 那样直说的诚实限度。**仅规格**；没有源文件被改动，也未追加新决策条目（修正在文档、`CHANGELOG` 与本节记录）。
- **而 `riscdom-backup` 存在了**（v1.0 批 AX / M7e-1，2026-09-29）：M7e 的第一个**实现**批。一个新的 workspace crate（`backup/`，bin `riscdom-backup`），其 `export` 读一个节点的**数据目录**（`settings.json`、`sessions.db`、`token`、`node.key`、`peers.json`、`rooms.json`），写一份**清单**（逐文件大小、SHA-256 与标记；`node_id`；导出的时刻），并把一个 gzip 过的 tar 封在 **AES-256-GCM** 之下、密钥由运维者口令经 **PBKDF2-HMAC-SHA256** 推得 —— 口令来自 `--passphrase-from-env` 或管道 stdin，绝不是命令行参数、绝不落盘、绝不打印。密码是 **`ring`**，已在 `Cargo.lock` 里，所以本批加的是**边、不是包**。审计存储、快照与 keyring 是 **AV-2**；清单的 `not_derived` 列表会大声说出这一点。**决策 §119。**
- **而 `riscdom-backup` 带上整个节点了**（v1.0 批 AY / M7e-2，2026-09-29）：包多出 `backup.md` §1 的另两个根。**审计存储**经 **SQLite 的一致性路径**进包（`rusqlite` 只读打开 `<workspace>/.riscdom/audit.db` 并跑 `VACUUM INTO`）—— 绝不逐字节拷贝，因为该文件是 WAL 且多进程；**快照**整棵遍历；**凭据**从 `settings.json` **反推**（`llm-api-key:<executor_id>:<provider_id>`、legacy 的 `llm-api-key:<provider_id>`、`remote-token:<host>`），因为 OS keyring 没有 list API。点不出名的都落进清单的 **`not_derived`** 列表 —— 每个缺席账户一条 `missing:` 行、settings 读不了时一条 `unreadable:` 行、以及一条长期的 `unnameable:` 声明。CLI 多出 `--workspace <dir>`。`rusqlite` 已在 `Cargo.lock` 里（**增边、不增包**）。**M7e 完成** —— 规格（AT/AW）与实现（AX/AY）都完成。**决策 §120。**
- **而 Rust SDK 存在了**（v1.0 批 BA / M7c-1，2026-09-29）：M7c 的前一半。一个新的 workspace 成员（`sdk/rust/`，包名 `riscdom-sdk`）把控制平面的 **37 条 `GET` 端点**做成带类型的方法，外加 bearer token、`{code, message, retryable, cause}` 错误作一个类型、以及带类型的请求参数 —— 经 `reqwest` 的 **blocking** 客户端，不链接本 workspace 的任何运行时件。**漂移守卫是一条测试**：crate 用 `include_str!` 读 `docs/tool-schema-control-plane.md`、解析它的 `queries` 标记块、断言 SDK 表与之相等；那个块已被断言对着服务器的 `ROUTES`，于是链条是 **SDK ⇄ tool schema ⇄ server**，无依赖、无第二份清单。`reqwest`/`serde`/`serde_json`/`thiserror` 都已在 `Cargo.lock` 里（**增边、不增包**）。控制类端点与事件流是 BB；TypeScript SDK 是 BC。**决策 §121。**
- **而 Rust SDK 完整了**（v1.0 批 BB / M7c-2，2026-09-29）：api §5.2 的 **36 条 `POST` 控制**作为 `CONTROL_ENDPOINTS` 加入，带类型的方法与参数；**事件流**作为 `Client::subscribe` → 一个阻塞式、逐帧的 `Subscription` 到来（在 `reqwest` 的阻塞响应上用 `std::io::Read` —— **无 async 运行时、无新包**）。envelope 是带类型的（`FrameKind::{Event, Hello, Gap, Unknown}`），而 **`gap` 保持为一条指令**：`lost_after()` 点名游标，看见它的客户端必须从查询重新同步。`workspace_export`/`workspace_import` 是字节进/字节出，因为 §5.2 说它们不是 JSON。第二条漂移守卫把控制表钉在 tool-schema 的 `controls` 块上。**M7c 完成** —— Rust SDK 整条线。TypeScript SDK 是 BC。**决策 §122。**
- **而 TypeScript SDK 存在了**（v1.0 批 BC / M7d，2026-09-29）：一个新包（`sdk/typescript/`，`@riscdom/sdk`，`private`），带**与 Rust SDK 同一个表面** —— 37 条查询、36 条控制、带类型的参数、`ClientError`、以及**事件流**作一个用 **`fetch` + `ReadableStream`、绝不用 `EventSource`** 读的异步 `Subscription`；**`gap` 保持为一条恢复指令**（`frameKind` + `lostAfter()`）。**没有 dependencies、也没有 devDependencies**：运行时是 `fetch`，测试跑在 Node 自带测试器 + 类型剥离上，所以 gate 只多一行、不需安装步骤。同样的漂移守卫把两张表钉在 tool-schema 文档上。**M7c/d —— SDK 整条线 —— 完成。** `M7b`（server 发布）与 `M7a`（拆仓）仍是各自的批次。**决策 §123。**
- **而两个服务器包可以构建了**（v1.0 批 BE / M7b-1，2026-09-29）：`scripts/pack.sh` 与 `scripts/pack.ps1` —— 孪生，与 `gate`、`commit` 同一种分工 —— 构建 release 二进制并组装 `riscdom-server-<version>-<platform>`（二进制、`web/`、README、`settings.example.json`）与 `riscdom-relay-<version>-<platform>`（二进制、README、空的 `examples/`）到 `target/dist/` —— Windows 用 `Compress-Archive` 产 `.zip`，其它用 `tar` 产 `.tar.gz`。版本取自 `[workspace.package] version`，平台取自宿主。**不带凭据、不带数据目录**（token 与 node key 首启铸造），**没有任何东西被签名**，也没有 CI job / tag / release：CI 打包是 M7b-2、发布动作是 M7b-3。`docs/server-distribution.md` + zh 是新文档。已在本机验证。**决策 §124。**
- **而 CI 构建它们**（v1.0 批 BF / M7b-2，2026-09-29）：第三个 job **`server-bundle`** 在 Linux 与 macOS runner 上跑打包器 —— 与 `bundle` 同一个 `if`（手动 dispatch 或 `refs/tags/v*` ref）、同一个 matrix —— 安装 Linux 系统库、Node 24 与 stable 工具链，在 `ui/` 跑 `npm ci` + `npm run build`，然后 `sh scripts/pack.sh --skip-ui-build --output-dir target/dist`，并把 `.tar.gz` 作为制品 `riscdom-servers-<runner.os>` 上传。**它只构建**（无测试、无 lint、无探针），且**不打 tag、不发布** —— 归档只是 run 制品，切 release 是另需授权的批 BG。gate 多出 `sh -n scripts/pack.sh`；roadmap §12 的 server zip 项移到 `[已定]`。**决策 §125。**
- **而项目现在记录三个仓库**（v1.0 批 BH，2026-09-29）：`docs/multi-repo.md` + zh 原是照两个仓写的，本批把它们对齐到现实 —— **本仓**（内核 + 控制平面）、**`riscdom-adminapp`**（`host-tauri` + `ui`）与 **`riscdom-server`**（`server` crate）—— 注明所钉的内核 tag 在 v1.0 才打、把 CLA 问题扩到三个、并新增 §7 记录两个程序的第一份 roadmap（adminapp：今天桌面，之后移动端与浏览器，连节点与 server 两端；server：今天命令行，之后 Windows / Linux 上的 web 状态页，只服务 RiscDom）与顺序 **v1.0 → server → app**。**M7a —— 拆仓 —— 明确推迟到 v1.0 之后**：每个新仓钉一个 v1.0 tag，而 `v0.9.9` 早于连接层、备份工具与 SDK。relay 二进制留本仓。**决策 §126。**
- **而跨设备设计已获授权**（v1.0 批 BI / M5，2026-09-29）：[decisions §33](decisions.zh-CN.md) 所要求、必须「单独」进行的那次批准 —— 把审计链扩展为「主链 + 临时段」、触及红线 5 —— 落盘为 **决策 §127**。它按八条覆盖整个跨设备设计（**M4e + M5 + M6**）：链语义而**哈希公式不变**；`provisional` / `fork` 且**绝不静默合并**；**三层**抑制缺一不可；M4e 的 **30 秒 digest**（关键事件即时推送）；M6 的**三级 M** 与跨链验证；以及本决策为 `PROJECT_CONSTITUTION.md` §8 的**明确例外**。roadmap §7 与 §15 从「未获授权」转为已授权，M5 行与文末句随之；§8 多出指向。**M4e、M5 与 M6 现在可以实现了** —— 那是下一批。**决策 §127。**
- **而第一块已经建成**（v1.0 批 BK / M4e-1，2026-09-29）：链的 digest 现在上路了。digest 是**对链上某一点的一份承诺** —— 链的头哈希与事件数，从 `audit` 已有的 `last_hash()` / `count()` 读出（不是第二个哈希，也不发整条链）；body 为 `{ "digest": 1, "chain": <头或 null>, "length": n }`，一条寻址到服务器的普通 §3 帧（新的 `Local::Digest`，不是新的帧*类型*），服务器**在内存里按节点持有最新一份**。`host-core` 按 **30 秒默认定时器**上报，作第三条连接线程、与心跳和探测并列。`connection.md` §7 写实，「deferred / 需单独授权」的注去掉。**`audit` 未被改动** —— 公式、`verify_chain` 与触发器均未变，一个刚被读过 digest 的储存仍然 `Intact`。关键事件的即时推送是 M4e-2。**决策 §128。**
- **而段 schema 落地了**（v1.0 批 BM / M5-1a，2026-09-30）：audit v2 的**形状**，仅此而已。`audit_events` 多出**可空**的 **`segment_id`** 列，`SCHEMA` 多出 **`segments`** 表（`segment_id`、`kind`、`head_hash`、`head_prev_chain`、`opened_at_ms`、`closed_at_ms`、`state`、`note`）。**`NULL` 意为主链** —— M5 之前的每一行本来就是它 —— 所以老日志零改写即读对，而 `AUDIT_SCHEMA_VERSION` **保持 1**。`audit` 还多出 `Segment` / `SegmentKind` / `SegmentState`；**不开任何段、不写任何行**（M5-1b）。`docs/audit-v2.md` + zh 是新文档（双语 **122 → 124**），`connection.md §7` 指向它们。**`compute_hash`、`verify_chain`、`append_once` 与两个 append-only 触发器未动。** 两件事留给 owner 拍板：临时段自己那条链的**物理形状**，与**事件名族**（推荐记为 `host.audit.*`）。**决策 §129。**
- **而一个段开与关**（v1.0 批 BN / M5-1b，2026-09-30）：`audit` 多出 `AuditStore::open_segment(kind)` / `close_segment(&segment_id)`。开启写下 `segments` 行 —— `state = open`，**`head_prev_chain` = 在任何追加之前读出的链头** —— 然后向**主链**（`segment_id IS NULL`）追加 **`host.audit.segment_opened`**（`{ segment_id, kind, head_prev_chain }`）；关闭更新该行并追加 **`host.audit.segment_closed`**（`{ segment_id, closed_at_ms }`）。追加失败时行被放回，所以段行总与它的生命周期事件一致。`SegmentKind` / `SegmentState` 的读取者改名 `parse`。**`compute_hash`、`verify_chain` 与两个 append-only 触发器未动**，且**尚无宿主接线** —— 临时中心是 M5-3、并入是 M5-2。**决策 §130。**
- **而物理形状已冻下：（b），一链一份文件**（v1.0 批 BO / M5-1c，2026-09-30）。主链仍是 `audit.db`；一个**临时段**是**同一个审计目录**里的 `audit-segments/<segment_id>.db`，由 `audit::segment_db_path_in` 拼出、由 `AuditStore::open_segment_store` 打开 —— 一份普通储存，同一套 schema、同一批触发器、以及**它自己的创世**。*主*储存里的 `segments` 行把两者系在一起（`head_prev_chain` = 开启时的主链头）。**并入即转录**（M5-2；名字 `host.audit.segment_merged` 已预留），且**验证仍是一个函数**：主链上 `verify_chain` **未改**，段自己的储存上用*同一个* `verify_chain`。跨链验证是 M6，以 `head_prev_chain` 为锚。`docs/audit-v2.md` + zh 修订 §2/§5/§6/§7 并新增 §8/§9。**`audit/src/hash.rs` 未动、`verify_chain` 的逻辑未动**；**未碰 host-core**（审计目录是调用者的）。**M5-1 整条线（a/b/c）完成。** **决策 §131。**
- **而一个段靠转录并入**（v1.0 批 BQ / M5-2a，2026-09-30）：`audit` 多出 `AuditStore::merge_segment(audit_dir, segment_id)`。它读临时段自己的储存，把每个事件作为新事件追加到主链 —— **去掉** `provisional` 成员，所以标是*靠写入清掉*、绝不靠更新 —— 然后追加 `host.audit.segment_merged`（`{ segment_id, kind, merged_at_ms, event_count }`）并把该行标为 `folded`。**段文件保留自己的行与标**，看得见冲突的并入会**拒绝**（完全相等 —— 不选择任何冲突规则；那是 M5-2b），部分并入记在段行的 `note` 上。`audit-v2.md` + zh 新增 §10。**`compute_hash`、`verify_chain` 与两个 append-only 触发器未动**；未碰 host-core（M5-3 调用它）。**决策 §132。**
- **而冲突被记在两边**（v1.0 批 BS / M5-2b，2026-09-30）：`merge_segment` 现答 `MergeOutcome::{Folded { merged } | Forked { reason }}` —— fork 是结果、不是错误。遇冲突（`actor` + `action` + **清标后** detail 完全相等，判据未改）时**一个字都不转录**：该行变为 `forked`、原因写进 `note`，并向主链写一条 `host.audit.segment_forked`（`{ segment_id, kind, forked_at_ms, reason, conflicting_event_id }`）—— 链侧的标；段保留它整个文件。**不加新 `SegmentState`**，而**部分并入**（`note` 以 `merge failed after …` 开头）被**当作自己的错误拒绝**、不被改贴。`audit-v2.md` + zh 新增 §11。**`compute_hash`、`verify_chain` 与两个 append-only 触发器未动**；未碰 host-core。**裁定仍属 M6。** **决策 §133。**
- **而三层抑制已建成**（v1.0 批 BU / M5-3a，2026-09-30）：`net` 多出抑制状态机 —— `SUPPRESSION_WAIT = 60 秒`、`SUPPRESSION_BACKOFF_MAX = 30 秒`、`SuppressionPhase`（`candidate` / `waiting` / `confirming` / `backing-off` / `standing-in`）、按 **`node_id` 序**的顺位、以及由 `(node_id, now)` 导出的确定性退避 —— 并以 **§6.7 的见证规则复用**为群体确认（不写第二条活性规则）。`host-core` 用定时器驱动它，输入是探测线程发布的中心可达性。**只是本地状态**：无链、无段、`audit` 一行未动。接管与广播属 M5-3b，回归与合并属 M5-3c。`connection.md` + zh 新增 §6.8。**决策 §134。**
- **而中心的报告在同侪之间走**（v1.0 批 BW / M5-3b-1，2026-09-30）：抑制状态机不再空转。一个节点把 §6.7 向上发的那对 `{"unreachable": …}` / `{"reachable": …}` 报告发给同 workgroup 的邻居 —— **横向**寻址（`report_to`）—— 而每个节点保留**自己的一份见证表**、**主语是中心**，于是 §6.7 的规则就地判。**接管广播**是一个新 body（`{ "takeover": 1, "centre": …, "by": …, "at_ms": … }`）；听到它就**退让**。**对跨区域服务器一无所求**；`audit` 未动。代行与*发出*广播属 M5-3b-2；回归与合并属 M5-3c。**决策 §135。**
- **而一个节点代行，并在链上说明**（v1.0 批 BX / M5-3b-2，2026-09-30）：§33 的第三层也不再空转。跨过退避、且**是第一顺位**时，一个节点把接管广播发给**每个**同侪、在自己的链上**开一个临时段**（`segments` 行与它的 `segment_opened` 事件 —— **不开文件**），并记下 **`host.connection.takeover_declared`**（`{segment_id, centre, by, at_ms}`）；在代行中听到广播则记下 **`host.connection.stood_down`**。**链照旧被写**，且 **`audit/src` 未动**。回归与合并属 M5-3c。**决策 §136。**
- **而回归会关掉段**（v1.0 批 BZ / M5-3c-1，2026-09-30）：一个看到中心回来的代行者会**关掉它开过的段**（`close_segment` —— 行变为 `closed` 并追加 `host.audit.segment_closed`），并记下 **`host.connection.centre_returned`**（`{segment_id, centre, at_ms}`），**然后** `observe(true)` 才把它带到 `candidate`。收窄在定时器里（`net` 保持纯净）；关段失败会被报出且不否决恢复。**`audit/src` 未动。** 送达、中心侧重建与 `merge_segment` 是 **M5-3c-2**；§137 记下尚未解决的「标记 vs 储存」张力。**决策 §137。**
- **而恢复不再无声失败**（v1.0 批 CB / CA-1，2026-09-30）：本地 QEMU/QMP `10054` flake 的根因是一个**自伤的时序竞态** —— 快照发送线程在我们自己的 `PortLease` 监听器仍绑着时就被启动，于是它可能连上**我们的** socket、被移交重置。它现在在移交与 QEMU 启动**之后**才起，且**它的结果被读取**；QEMU 的 stdout/stderr 被**捕获**（以前是 `Stdio::null()`）并并入错误；QEMU 仍存活而 QMP socket 被重置时**重连一次**。**不改公开 API、无新依赖、不新增审计事件。** CA-2/CA-3（生命周期监控 / gate 串行化）与 CA-4（满环回测试，BA-3）属后续。**决策 §138。**
- **而一个关闭的段被送达、重建并合并**（v1.0 批 CC / M5-3c-2，2026-09-30，**M5 的终点**）：代行者把刚关掉的段以一条流交给中心 —— 每事件一帧 §3 帧，外加一帧结束 —— 区间在开启行与关闭行之间。中心以段到达时的名字（`seg-<owner>-<ms>`）**收养**那一行、写下段的储存、并**原样**调用 `merge_segment`；两台节点分别记下 `host.connection.segment_delivered` / `segment_rebuilt`。**`audit-v2` §2/§8 现在说明形状 (b) 如何跨两台机器** —— 段在抵达中心之前是其主人链上的一段区间，到了中心才是它自己的储存。**M5 收尾**：§127 的授权、M5-1（schema）、M5-2（合并与 fork）与 M5-3（a → b → c）全部到位。之后只剩 **M6**。**决策 §139。**
- **而关键事件在发生的当下被推送**（v1.0 批 CE / M4e-2，2026-09-30）：roadmap §4 的另一半。节点把一条作为寻址到服务器的普通 §3 帧推送；服务器在内存里每节点保留最新 **256 条**，按 `(action, at_ms)` 去重，且什么也不答。接在两个事实被写下的地方 —— **fork** 与**接管** —— 而 30 秒批次未变。**`audit` 未动、无新依赖、无新路由或能力。** 第三个触发器（**逐出**）没有生产者，所以机制立着，事件在等它的定义。**决策 §140。**
- **而一个任务可以跨机器**（v1.0 批 CI / M6-1a，2026-09-30）：M6-1 的前一半。`TaskId` 现在是 `task-<device>-<pid>-<seq>`；`net` 把一个任务作为一帧 §3 帧送出去（`{"task": 1, …}`）、一帧收回来（`{"task_reply": 1, …}`），并带 typed 发送方法；`host-core` 多出 **`RemoteAgentHandle`** —— `AgentHandle` 那道缝的「远程那一个」—— 它发一帧、在共享回复槽上等待，并报告结果或四类可辨拒绝之一。**无新 capability、无新路由、无新依赖、不写调度。** 接收侧、`/v0/tasks` 的参数与两节点测试是 **M6-1b**。**决策 §142。**
- **而对端会真的跑这个任务**（v1.0 批 CJ / M6-1b，2026-09-30，**M6-1 的终点**）：`POST /v0/tasks` 多出可选成员 **`node`**（缺席 = 本地队列、不变；另一个节点 = 交出去并等待），接收方以**它自己 `peers.json` 里的 `dispatch` 声明**授权（**默认拒绝**）、在自己的队列上跑这个任务、记下 `host.dispatch.received`，并一帧作答。**路由表不动；无新 capability、无新依赖、不写调度；`audit` 未动。** 边界已记下：跨设备派发以「节点在某个 workgroup 里」为前提。`docs/cross-device-dispatch.md` + zh 是新的。**决策 §143。**
- **而锚点长出第二半**（v1.0 批 CL / M6-5-1，2026-09-30，**M6-5 的第一片**）：roadmap §7 的第二个 `[open]` 按**摘要**作答，于是跨段引用补上它从未有过的长度。`segments.head_prev_length`（紧挨 `head_prev_chain` 的 `INTEGER` 列，与头同一刻读出，早于它的行为 `NULL`；迁移走 `add_column_if_missing`，`AUDIT_SCHEMA_VERSION` **保持 1**）加上 `segment_done` 帧上的 `anchor_digest` + `anchor_length` —— `anchor_digest` ≡ `head_prev_chain`，一个值两个名字，读取时优先新名、回落到旧名，所以较早的发送方仍读得进来。`adopt_segment` 在中心那一行上记下两半、**一样都不验**。**`compute_hash`、`verify_chain`、append-only 触发器、路由表与能力词汇表都未动；尚无验证逻辑、无新事件名**（比较是 M6-5-2；`host.audit.chain_verified` / `chain_rejected` 是它的）。`docs/cross-chain-verification.md` + zh 是新的，`audit-v2` §3/§4/§5 记下该列、第二半与指针。**决策 §144。**
- **而交付的段会被验，裁决落到链上**（v1.0 批 CN / M6-5-2a，2026-09-30）：`receive_segment` 验**信封**（每个位置 `0..total` 恰好到一次、发给的是本节点）与**重建**（对它刚写出的存储调 `verify_chain`），记 `host.audit.chain_verified` 或 `host.audit.chain_rejected`，而拒绝时**不合并**：行既不是 `folded` 也不是 `forked`，段文件留下。没带任何事件的流现在会被**作答** —— `total: 0` 作为空段并入，`total: n` 被拒 —— 而锚点没有长度的段记为 **skipped**。信封检查是 `net::verify_delivery`（形状属 `net`；`audit` 是叶子，故未多出函数），且**事件的哈希仍然不上线**，所以通过是*已检查*、绝非*已证明*。**`compute_hash` / `verify_chain` / 触发器 / 线上帧都未动；锚点连续性是 M6-5-2b。** **决策 §145。**
- **而流会证明自己的链 —— 而它证不了的那一环要明说**（v1.0 批 CP / M6-5-2b，2026-09-30，**M6-5-2 整条线的终点**）：每个 `segment_event` 现在随身携带它被写下时的 `hash` 与 `prev_hash`，而 `net::verify_linkage` 用 `audit::compute_hash` **重算**每个事件、并检查它**承接**前一个。裁决以两个词走 —— **`checked`**（`"skipped"` / `"delivery"` / `"delivery+chained"`）与 **`linkage`**（`"ok"` / `"broken"` / `"skipped"`）—— 断链与其他失败一样被拒，而不带哈希的发送方记为 skipped（跨版本窗）而不是被拒。**锚点那一环以现有交付物不可能验**：锚点坐在 `segment_opened` 标记*之前*，所以第一条事件的 `prev_hash` 是**标记**的哈希 —— 批 CO 发现那个预定的等式按构造就是假的，owner 选了诚实的降级，记为 **M6-5 尚欠**，写在 `docs/cross-chain-verification.md` §4。**`compute_hash` / `verify_chain` 未动，无新帧，无新依赖。** **决策 §147。**
- **而冲突只差一个过滤器**（v1.0 批 CR / M6-5-3a，2026-09-30）：`riscdom audit events` 多出 **`--action-prefix`**（服务器本来就有的过滤），于是 `--action-prefix host.audit.segment_forked` 列出本节点的冲突、`host.audit.chain_rejected` 列出它被拒的交付（detail 用全局 `--json`）。**一个 CLI 开关加上把现状写下：无路由、无 capability、无事件名、不改段行，`cli` 与文档之外一律未动** —— 内核仍不选边。**技术债：fork 发给服务器的关键事件推送没有读者**（没有任何路由或命令读 `key_events_of`），所以这条推送除了服务器内存之外谁也到不了 —— 记在 `cross-chain-verification.md` §7。**决策 §148。**
- **而冲突可以被标为已解决**（v1.0 批 CT / M6-5-3b，2026-09-30，**M6-5-3 整条线的终点**）：`POST /v0/audit/conflicts/{segment_id}/resolve`（一条**带路径参数**的路由 —— 不是 `ROUTES` 的一行）收一个可选的 `note`，并追加**一条** `host.audit.conflict_resolved` 行（`{segment_id, resolved_by, resolved_at_ms, note}`），由 `riscdom audit resolve <segment_id> [--note <text>]` 请求。**不选边、不转录、不碰段行**（`state` 保持 `forked`），甚至不查段是否存在。它声明 `settings.write`，故**不加 capability 名、不动 §5.2 计数、不改 SDK** —— 一个 resolver 分支、一行 `patterns`（10 → 11）与一个 CLI 动词。**`compute_hash` / `verify_chain` / 触发器未动。** **决策 §149。**
- **而一个节点能跑什么，就一条命令**（v1.0 批 CV / M6-2a，2026-09-30，M6-2 的前半）：`riscdom node capabilities` 读**五个**端点 —— `/v0/identity`（取 `node_id`）、`/v0/executors`、`/v0/sandboxes`、`/v0/qemu`、`/v0/toolchain` —— 合并成 `{node_id, executors, sandboxes, qemu, toolchain}`。每一节都是那个端点的原样回答；**失败**的一节在原地报告（其余照常打印，退出码取最重那个），而 `found: false`（没有 QEMU、没有工具链）是一个回答，不是失败。合并是 **CLI 自己的拼装** —— 纯 HTTP 客户端从不调 `AppState` —— 所以**无新路由、无新 capability、无 SDK 改动、无 §5.1/§5.2 计数**。**技术债，与 §148 的关键事件推送同型：两条被携带而无人读的通道** —— §6.6 注册的 `capabilities` 被存在服务器行上、无人读；`RelayClient::request_registry` 无任何生产调用者 —— 记在 `docs/connection.md` §11。**`compute_hash` / `verify_chain` / 触发器未动。** **决策 §150。**
- **而节点现在会声明自己被配置成做什么了**（v1.0 批 CX / M6-2b-1，2026-09-30，M6-2b 的前半）：§6.6 注册携带**推导出的声明** —— 配置了内网服务器角色时加 `server`，至少认识一个同侪时加 `dispatch` —— 而在这之前，生产里唯一的注册发的是**空列表**（所以行的 `capabilities` 在构造上就是空的；批 CW 的发现）。**无新设置字段、无路由、无 capability、无 SDK，且 `net` 未动**（两者都没有的节点什么都不声明，所以线上字节兼容）。另外 `riscdom node capabilities` 多出第六节 **`PEER DECLARATIONS`**（读 `GET /v0/peers`，标题写着*一个声明，不是事实*；`--json` 多出 `peers` 键）。**并且 8 处 `ask_registry` 改为 `request_registry`** —— 批 CV 命名了一个并不存在的符号。**`compute_hash` / `verify_chain` / 触发器未动**；relay 仍无 HTTP 面。**决策 §151。**
- **而运行时表也有了读者**（v1.0 批 CZ / M6-2b-2，2026-10-01，M6-2b 的后半）：**`GET /v0/online`** —— 一条字面路由，capability `status.read` —— 用本节点的 **server role** 表（`RelayServer::online()`）作答，每个注册到它的节点一行，连着批 CX 给了写者的 `capabilities`。本节点没跑 server role 时回 `null`；在服务但无人注册时回 `[]`。`OnlineEntry`/`Online` 在 `net` 里获得 `Serialize`；`state`/`judged_at_ms` 是**这台服务器对某个同侪的意见**，§11.1 已写明。它**不是**下发的那份注册表（§11.2 的主动获取仍无调用者 —— 批 CV/CY 曾把那个缺口指向错误的子节），也**不是** `/v0/peers`。原子连锁：`ROUTES` + §5.1（37 → 38）+ tool-schema（37 → 38，表 + 定义）+ 两个 SDK（37 → 38）+ `smoke.rs`（38 → 39）。**不加 CLI 命令、不加 UI 面板、不加 capability 名、不加依赖；relay 仍无 HTTP 面。** **决策 §152。**
- **而一次 run 的事件命名引起它的那个任务**（v1.0 批 DB / M6-3a，2026-10-01，也是 roadmap §12 `task_id` 缺口的终点）：身份本来在每一跳上而无处安放。现在 **sink 带着它**，在构造时绑定（`HttpEventSink` / `TauriEventSink` / worker 的 `LineEventSink` 都经 `envelope(…, task_id, …)` 发布），而 `EventSink` 多了一个带默认实现的方法（`with_task`），好让长期存在的 emitter 能给单次 run 一份绑好的副本。`POST /v0/agent/run` 收一个可选的 `task_id`；`POST /v0/tasks` 在 handler 里铸它的 `id`，于是 sink 在调用前就被绑定；worker 绑定它从 stdin 读到的那份。**`emit` 签名不变**，没有 id 时信封仍然说 `null`。**无路由行、无 capability 名、无 SDK 改动、无依赖；`audit/src/hash.rs` 未动。** SSE 的 `task_id` 过滤器仍开放（M6-3b）。**决策 §153** —— 而 **DC（§154）就是那个后续：过滤器现在被强制执行了。**
- **而流上有一个过滤器被强制执行了**（v1.0 批 DC / M6-3b，2026-10-01）：`GET /v0/events?task_id=<id>` 把一个订阅者收窄到一个任务 —— 服务器丢弃其它任务的帧（实时**与** `Last-Event-ID` 重放都如此），而 `hello`/`gap`/注释永不被隐藏。`WireFrame` 多了 `FrameScope`，`SseHub::subscribe(task_id)` 多了一个 `Subscriber`，它的读取会跳过被排除的帧。**`event` 与 `agent_id` 未动**（仍被接受、仍被忽略，与本批之前一字不差）；`hello.filters` 回显 `task_id` 且只在一处写出。**无路由行、无 capability 名、无 SDK 改动、无依赖。** **决策 §154。**
- **而在另一个节点上做出的请求会到达本节点的队列**（v1.0 批 DG / M6-4a，2026-10-01，M6-4 的前半）：ask 是一条普通的 `m.request.ask` 链行，所以它本来就随站入节点的段旅行 —— 缺的那一半是**读者**。`receive_segment` 现在只把折叠合并刚写下的行经构造函数同样的 `restore` 折进活队列（碰撞按 id 报告；forked 合并什么都不折），于是远端 ask 无需重启就出现在 `GET /v0/sandboxes/requests`。**无新帧、无新路由、无新事件名；`merge_segment` 未动。** **决策 §155。**
- **而中心自己的链记下了它被告知的东西 —— 锚点那一环也收口了**（v1.0 批 DH / M6-5-4，2026-10-01，**M6-5 整条线（5-1…5-4）的终点**）：节点上报的 digest 由**跑服务器角色的那个节点**写成一条 `host.audit.digest_received` 行（`{node_id, chain, length}`），**按变化记**（重复什么都不写），经新增的 `net::DigestSink` —— 独立 `riscdom-relay` 无链、不装。**不造自己的汇总链。** 而 `segment_done` 带 **`anchor_hash`**，中心用 `events[0].prev_hash` 查它；判定是 `anchor: "ok" | "broken" | "skipped"`，与 `checked`、`linkage` 并列。**`merge_segment`、`audit/src/hash.rs` 与触发器未动。** **决策 §156。**
- **而参考 M 长出一个层级**（v1.0 批 DJ / M6-6，2026-10-01，M6-6 的第一块）：`examples/python/supervisor.py` 多出 **`--level node|lan`**（默认 `node`，即旧行为）。在 `lan` 下，快照读的是 **workgroup** —— `/v0/online`（服务器角色的运行时表）、`/v0/peers`，以及每个由 `--node <id>=<host:port>` 指名、token 放在文件里（`--node-token-file <id>=<path>`；**token 永不作参数**）的节点 —— 而没人监听的节点被**记下**（`{"unreachable": …}`），而不是致命。**`M_TOOLS` 不变（18 条）**，且**不碰任何 Rust 文件**：本批只是 Python 与文档，也就是红线 1 的调用方那一侧。跨区域级与 `--config` 是 DI-2 / DI-3。**决策 §157。**
- **而裁决到达提问的那个节点**（v1.0 批 DL / M6-4b，2026-10-01，**M6 的最后一块**）：在别处做出、在这里被裁决的请求，**沿提问者自己的会话**回答它（`send_to`；不新拨号、不经路由），而提问者把它写成 **`m.request.approve` / `m.request.reject`** —— 不新增事件名 —— `decided_by` 点名做决定的节点，于是它跨重启存活。读者是探测线程的 drain 循环，而 `CentreWatch` 现在携带队列。只告诉提问者；不删任何东西。**`merge_segment` 与 `audit/src/hash.rs` 未动。** **决策 §158。**
- **而 M6 收口**（v1.0 批 DM，2026-10-01）：多节点线 **M6-1…M6-6 在 `d7f2a76` 完成**。批 DL 的两条接缝 —— 中心 → 提问者这一段与「一个节点裁决自己的 ask 时不发送」—— 现已经过端到端测试（真 `RelayServer` 与真 `RelayClient`），**零实现改动**。**决策 §159。**
- **而 API 已冻结**（v1.0 批 DO / M8-1，2026-10-01 —— M8 的前一半）：冻结**由文档宣布，而不是 tag** —— `api-compatibility.md` 的状态行写作*自 v1.0 起冻结*，而 `roadmap-v1.0.md` §13 记录它、并不再自称草案。被冻结的表面对应 `api-compatibility.md` §1 列出的三样（控制面的 HTTP 协议、客户端可观测量、宿主 crate 的 `pub use` 面），而 **`/v0/` 就是 v1.0 发布的路径** —— 前缀在下一个协议破坏性变更时才移动，所以说过它*在 v1.0* 变为 `/v1/` 的四个文档（`api-compatibility` §2/§5、`control-plane-api` §7、`error-model` §2/§6/§9、`sdk`）已改正，而 `error-model` 的 `cause` 链改为挂在它真正的移动上。**仅文档：零代码、不 bump 版本、不打 tag。** **决策 §160。**
- **而 v1.0 已写成，版本号为 `1.0.0`**（v1.0 批 DP / M8-2，2026-10-01）：版本在发布流程自 v0.7 以来一直更新的**七处**更新（`Cargo.toml`、根 `Cargo.lock`、`ui/package.json`、`ui/package-lock.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/Cargo.lock`、`ui/src-tauri/tauri.conf.json`），`CHANGELOG` 的 `[Unreleased]` 定稿为 `## [1.0.0] - 2026-10-01`，而 `RELEASE_NOTES.md` + zh 为 v1.0 重写（被替掉的 v0.9.9 说明留在 git 历史与它自己的 Release 里）。不需要 `wix.version` 覆盖（`1.0.0` 是数字），而 `docs/README.md` 的 `api-compatibility.md` 行跟随文档的*自 v1.0 起冻结*状态。**tag 与 Release 是 M8-3**，需单独授权。**决策 §161。**
- **而 v1.0 已发布**（v1.0 批 DQ / M8-3，2026-10-01）：tag **`v1.0.0`**（轻量，打在 `891c237`）与 GitHub Release **「RiscDom v1.0.0」**（**Latest**，十二个资产：`.dmg`、`.deb`、`.rpm`、`.AppImage`、`.msi`、`.exe`，以及 Linux/macOS/Windows 的 `server` + `relay` 归档）都已上线。桌面/Linux/macOS 的包与非 Windows 归档来自 tag 自己的 CI（run `36821170320`）；Windows 安装包与 Windows 归档是**本机构建**（无 Windows runner）。**本批未改动任何文件。** **决策 §162。** **M8-4（拆仓）是最后一块，未获授权。**
- **而三个仓库写成了一致的文档**（v1.0 批 DS / M8-4-prep，2026-10-01）：M8-4 所需的五条裁决落在 `docs/multi-repo.md` —— **`serve` 归 `riscdom-server`**（本仓 `cli` 收缩为纯客户端）、**server 包的前端从 `riscdom-adminapp` 的 release 取**、**每个仓自带 CLA**、名字与顺序不变、**拆仓逐批授权** —— 而拆仓写成 **M8-4a（server）→ M8-4b（adminapp）→ M8-4c（本仓收尾）→ M8-4d（对账）**。§1 也不再称本仓为「内核与控制平面」。**仅文档：未搬任何文件、未写任何远端。** **决策 §163。** **M8-4a 待其各自的授权。**
- **而 M8-4a 已完成：控制平面已经是它自己的仓**（v1.0 批 DU/DV/DW，2026-10-01）：**`riscdom-server`** 已上线 —— `main` = `a41c505`（force 覆盖其空仓初始 commit；`git subtree split` 保留 42 个提交、平铺到根），已打 **`v1.0.0`**，并已从全新 clone 对 `?tag=v1.0.0#891c2375` `cargo check` 通过。CLI 先一步变成纯客户端（批 DT，`1a43c36`）。**`server/` 留本仓直到 M8-4c**：`ui/src-tauri` 以 path 依赖它，而 `ui/` 在 M8-4b 才离开 —— 在这里第一次删它时 gate 就红了，整段收尾（删目录、`pack.*` 只打 relay、`ci.yml` 的 `relay-bundle`、文档改指向）随之一起移入 M8-4c。**决策 §164。** **M8-4b（adminapp）、M8-4c（收尾）与 M8-4d 仍在。**
- **跨区域服务器已冻结**（v1.0 M4d，2026-09-28）：[`docs/connection.md`](connection.zh-CN.md) §6 —— 一个**部署者运行的**专用部署（从不由项目运行），四个角色的可知范围都有边界，路由只按签名内的 `to`，授权按 §3 模型（**无新凭证、无新 capability**），服务器从不拨向节点（所以不需打洞），以及 §6.5 对 roadmap §1 红线的明确回答。**决策 §92。**

- **插件接口已冻结**（v1.0 M3，2026-09-28）。[`docs/plugin-interface.md`](plugin-interface.zh-CN.md) 就是 [roadmap §8](roadmap-v1.0.zh-CN.md) 描述、decisions §3 要求在内核 API 之前冻结的那份规范：stdio / JSON lines 传输、四个必含的机制层操作（`start`/`stop`/`execute`/`output`）及其帧语法、两个可选的语义层操作（`snapshot`/`fingerprint`）、capability 声明的框架、manifest 的必备键、错误与版本规则、架构无关这条约束，以及信任模型 —— 每一节都说明它冻结了什么、留下了什么。有两样东西**刻意开放**：capability 声明的**格式**（draft；roadmap §8 说它最后冻结）与**架构抽象**（要求已冻结，trait 属 v1.x）。**决策 §87。**

- **等待队列能活过重启，而调用方自己收拾**（v1.0 缺口 3/N 批 D，2026-09-27）。申请队列是运行时状态，仍然不持久化 —— 但它里面 **pending** 的申请会回来：`derive_requests_from` 折叠链上的 `m.request.ask` / `m.request.approve` / `m.request.reject` 行，构造器用仍然 pending 的那些给活队列打底，于是重启之后决策照样能做。已决的申请是历史，只留在链上。`DELETE /v0/sandboxes/requests/{id}`（`sandbox.read`）把一条移出并答 `200` 带被移除的记录；它什么都不写 —— 于是「无 TTL」（§36）仍然成立，而调用方仍能收拾。链从未携带的那一个字段是 `reason`，它保持 `None`，而不是被加进一行受哈希保护的行里。**决策 §84。**

- **链就是实例的记录，而审计读取接受一个窗口**（v1.0 缺口 3/N，2026-09-27）。`GET /v0/sandboxes/{name}/instances/history` 从链上的 `m.sandbox.spawn` / `m.sandbox.reap` 行**推导**一个定义曾经有过什么 —— 实例表无法持久化（`vm_slot` 是活句柄，§34），所以过去像 run 索引一样被推导。每项带 `spawned_at_ms`、`reaped_at_ms` 与 `running`，而 `running` 是「**现在**」，从活表读出（重启后：一切都是 `false`）。`history` 是这条路径上的保留字面量，所以对它发成员动作是 `405`、不是回收。`GET /v0/audit/events` 多出存储一直在 SQL 里应用的四个窗口参数（`from_ms` / `to_ms` / `from_id` / `to_id`；写反的一对是 `400`、点名下界）—— §5.1 的表多出一行（它是**模式**，所以标题 —— 数静态行的那个 —— 不动）。自批 E 起这个读取还能用**游标**翻页（`before_id` —— 比那个 id 严格更早的最新 `limit` 行 —— 背后是加法式的 `EventFilter.descending`），因为这个答案是 newest-first 而存储是升序扫描；`before_id` 与 `to_id` 同传是 `400`。而 `examples/python/supervisor.py` **给自己命名**：`--agent-id`（必填，或 `$RISCDOM_AGENT_ID`）随**每一个**请求以 `X-RiscDom-Agent` 发出，于是该调度员引起的行说得出是谁要的。**决策 §82。**

- **每个 AI 监督者做的动作都留下一行、并写下它自己的名字**（v1.0 缺口 2/N，2026-09-27）。客户端可以用可选的 **`X-RiscDom-Agent`** header 给自己命名；服务它的 `Actor` 便以那个名字与 `ActorKind::Supervisor` 出场，而**调度员能做的七个动作**写下指名它的链行 —— `m.sandbox.spawn`、`m.sandbox.reap`、`m.sandbox.switch`、`m.request.ask`、`m.request.approve`、`m.request.reject`、`m.task.dispatch` —— 而其中三个原本没有自己帧的动作各获得一帧，于是流里看得到动作、链上说得出是谁要的。身份走在 `AuditEvent.agent_id` —— 那个刻意不在哈希公式里的字段：归因到位的代价是零行历史被移动，而四十多个节点事件仍写节点。不带 header = 凭据自己的身份（`operator`），即本批之前每个版本的行为。事件词汇 **17 → 20**，而 `docs/control-plane-events.md` §3（此前代码 17 而文档只列 14）已拉回同步。**决策 §81。**

- **M 的决策层，以及 M2c 全线完成**（v1.0 M2c-2，2026-09-27）。`examples/python/supervisor.py` 现在会决策了：一个在节点状态上的**有上限工具调用循环**（`LLMDecider`，`--max-rounds` 默认 6），提供给模型 **十八个**工具 —— 启动时从 `docs/tool-schema-control-plane.md` 读出并过滤成「调度员应有」的那一批 —— 其中 `agent_run`（执行者的循环）与 `events`（客户指南 §8 明说它不是工具）被点名排除。M 的模型是 M 自己的（`--llm-base-url` / `--llm-model` / `--llm-api-key-file`，从不取自节点的 `llm_configs`），而不配模型就什么都不决定 —— 保守默认，不是坏掉的状态。**策略**刻意没给：system prompt 是骨架，而 self-test 用一个**假模型**（按脚本作答的 `http.server`）驱动循环，断言快照与工具真的出行、工具调用被执行并回喂、未提供的工具**不会**被执行、模型失败或不可达会以**零**控制请求结束该轮、以及轮次上限生效。**真机、两个进程**：一个全新的 `riscdom-server` 加带脚本模型的 M —— M 读到了状态，它的工具调用真的到达了节点：一次派发跑了一个**真实**的 worker 回合（写了一个裸机 guest 并把它在 QEMU 里启动）、`instance_create` 派生出一个真实实例（`m.sandbox.spawn`）、`instance_delete` 回收了它（`vm.stop`、`m.sandbox.reap`）、模型死掉时链**一字未变**（前后均 53 行）、而重启后的 M 从节点重建了上下文。**决策 §80。**

- **一个跑得起来的调度员，决策层仍然是空的**（v1.0 M2c-1，2026-09-27）。`examples/python/supervisor.py` 是 M 的骨架。它 **import** `dispatch.py` 的传输、token 规矩与错误分类，而不是抄一份；用**一次快照**读完节点的状态（`status`、`capabilities`、队伍、每个定义的实例、待批请求），决策，行动 —— 而 `decide()` 是**桩**，返回 `None`，所以空闲的一轮**一个控制请求都不发**。保守态是循环的**起点**而不是错误路径：读失败会在写下任何东西之前终止这一轮，而下一轮无需重启就会恢复。`--events` 用 `Last-Event-ID` 续订事件流，这是 E3 的读取器刻意不做的。文件的 docstring 与 `examples/python/README.md` 写下侦察找到的**五条已知边界**（M 在链上没有自己的身份；沙箱请求的决定不写进链；实例表与待批槽住在内存里；审计读取没有窗口也没有分页；五个能力名只是词汇）—— 五条都在 v1.0 缺口 3/N 各批里收口了：决策就是链上的行（§81）、审计读取接受窗口而实例表的过去由链推导（§82）、五个名字已删（§83）、队列里 pending 的申请也由链推导并有显式清理（§84）。两件值得复用的事：`import dispatch` 需要显式告诉 `sys.path` —— 本项目这个解释器跑在 `sys.flags.safe_path` 打开的状态下 —— 以及 gate 多了第二个 Python 步骤（15 → 16 步）。

- **执行者选择器，以及 M2b 全线完成**（v1.0 M2b-3b，2026-09-27）。模型表单与会话列表各有一个**执行者** `<select>`，都挂在同一个 `appStore` 字段（`executorSelection`）上。**空的那一项就是本节点自己** —— 这正是两种传输本来就读作「没有点名执行者」的拼法 —— 其余是舰队的 label；刻意**没有通配项**，因为「当前会话」只属于一个执行者。切换它会重读面板显示的东西（设置页的模型状态与就绪、聊天页的会话列表），而表单的保存/清除/钥匙串动作都跟着它，所以表单编辑的就是它点名的执行者。面板 wrapper 在**两种**传输里都带 `executor`，这一点由 `SharedApi` 强制（它的类型取自 HTTP 模块）。**未改任何 Rust。** 这条收尾了 M2：一个定义的多实例、每个执行者一份模型配置、每个执行者一份会话。

- **一个执行者的模型配置，以及拼成值的那个通配**（v1.0 M2b-3a，2026-09-27）。LLM 这条路从头到尾按执行者编键 —— 内存里的配置、`settings.json` 里的条目、钥匙串账号（`llm-api-key:<executor>:<provider>`）—— 而端点与 Tauri 命令都接受可选 **`executor`**，缺省即本节点自己。`/v0/sessions` 另认得 **`executor=*`**：**一张**列表给出所有执行者的会话，按新到旧，`limit` 数行数而不是「每执行者各几行」，每行自己带 `executor_id`。而那些答案只关于**一个**执行者的端点，以 `400 bad_request`、`cause: "executor"` 拒绝它 —— 模型配置属于某一个执行者，「当前会话」也一样。两个 id 现在是**保留**的（`local`、`*`）：带它们的 settings 文件仍然加载，而那一项被跳过并记一条 `host.executor.reserved`。**审计库拿到了兼容表上写明缺失的那个版本标记** —— `PRAGMA user_version = 1`，先于一切读出，打开时迁移，更新的文件以 `data_too_new` 被拒，且刻意**不放 `.bak`**：本文件是 WAL 且多进程打开，只拷它一个会漏掉还在 `-wal` 里的帧。**决策 §77 与 §78。**

- **会话归属某个执行者，而会话库也有了自己的版本号**（v1.0 M2b-2，2026-09-27）。`sessions` 新增
  **`executor_id`** 列，而会话库是第一个把版本放在 SQLite 自己的 **`PRAGMA user_version`** 里的格式 ——
  先于一切被读出，`0` 表示「本批之前写入的」，并在**打开时**迁移：列以**幂等方式**加上（先问
  `PRAGMA table_info`），迁移前的字节留作 `sessions.db.bak`（**只有真迁移时**），然后盖上版本。来自
  **更新**构建的文件以 `data_too_new` 被拒绝 —— 不读、不写。现在每次会话调用都要点名执行者：
  `/v0/sessions` 与它的六个兄弟接受**可选** `executor`，缺省为本节点自己（与 LLM 端点的形状一致），而
  `current_session_id` 变成**按执行者分表**。早于该列的行保持 **`NULL`**，并被读作**本节点自己的** ——
  这样日后给节点改名的不会丢掉自己的历史（把 `local` 写进老行会让它们直接消失）；新行总是写名字，而
  `rename` 绝不认领老行。`open_session` 也不再为了找一个会话而扫全表，`ensure_session` 会**修复陈旧指针**
  （current 指向的行已经不在）—— 另起一个新会话。**决策 §76。**

- **LLM 配置按执行者落盘，而 `settings.json` 第一次被迁移**（v1.0 M2b-1，2026-09-27）。一套模型配置里
  非机密的那一半 —— provider、endpoint、model —— 现在写到磁盘的 `llm_configs` 下，按**执行者 id** 归档：
  本机用它自己的**设备名**（`local`），worker 用它 `label` —— 与 `Task.target` 同一寻址空间，且**有意不是**
  节点的 `AgentId`，因为后者带 pid，会让这条为「重启」而存在的记录活不过它自己的那次重启。key 仍留在它
  自 v0.4 起就住的地方：OS 钥匙串，现在名字是 `llm-api-key:<executor>:<provider>`，而 v0.9.9 的条目
  （`llm-api-key:<provider>`）会被**向前读** —— 值写进新名，旧条目原地保留，于是回退到旧构建也仍找得到。
  `SETTINGS_VERSION` **1 → 2**，冻结级那套迁移规则的第一次真用：`LocalSettings::load_text` 读文档声明的版本
  （缺失 = 最老格式，不是数字 = 坏文件），`migrate` 把**更新**的文件以 `data_too_new` 拒绝、把更旧的迁到当前
  格式（v1 → v2 只加**空**的那张表 —— 不从钥匙串或环境猜任何东西），加载路径**只在迁移时**把迁移前的字节写进
  `settings.json.bak` 并把迁完的文档写回，而拒绝是**可见的** —— 一条审计事件（`host.settings.data_too_new`）
  加 `AppState::settings_problem` —— 不再是过去那句「读不懂的文件就当默认」的沉默。`LlmConfigStatus` 在
  `persisted` 旁新增 `config_persisted`：界面把 `persisted` 读作「你的 key 记住了」，这层含义不变；新字段说的是
  非机密的那一半已落盘。端点、会话与 UI 一字未动 —— M2b-2/3 才给端点加执行者维度、做 per-executor 会话切分
  与界面。**决策 §75。**

- **一次运行可以点名一个实例，而链会记下它**（v1.0 M2a-3，2026-09-27 —— M2a 的最后一块）。`Task` 新增
  `instance: Option<InstanceId>`（`#[serde(default)]`，与 `sandbox` 当初同形，所以更老的任务行仍可解析）；
  `HostAgentHandle` 与 `worker` 把它传给 `run_agent_for`（它现在是第四个参数），而 loop 拿到的**仍然是
  一个槽** —— `AgentLoop::with_vm` 一字未改。用哪个槽由 `task_instance` 决定：任务点名的那个实例，或节点
  当前的实例。被点名的实例在动任何东西**之前**就被严格校验：本节点不拥有的 id 是
  `404 cause "instance"`；而实例的定义、与任务同时点名的 sandbox 不一致是 `409 cause "instance"` ——
  那条检查存在的意义就是不让两个声明中的一个被静默忽略（决策 §74）。两条 `Task` 路径都带它
  （`/v0/agent/run` 与 `/v0/tasks`），Tauri 的 `run_agent` 命令多了一个可选参数（前端 wrapper 也接上；
  **调用点一处未改**）—— 而 agent 的 `start_vm` 工具现在把 `.mig` 写进**实例的**目录，不再自算 per-*agent*
  一个，M2a-1 记下的那个不一致就此闭环：`ToolContext` 新增 `snapshot_dir`、`AgentLoop` 新增
  `set_snapshot_dir`、`run_agent_for` 按实例设它。`run.start` 的 detail 终于写明一次运行解析到了哪个
  定义、跑在哪个实例上（`sandbox` / `instance`，此前都是 `null`）—— 新的 `run_start_detail_with` 负责写，
  `run_start_detail` 保留为测试用的短形式，而更老的 detail 仍可解析（解码器用 `get` 取每个键）。
  **对本批前提的一处更正**：`detail` **是**被每条事件自己的哈希覆盖的
  （`sha256(prev_hash|ts|actor|action|detail_json)`），所以这两个键改变的是**新**事件的哈希、旧的任何一行都不动
  —— 不动的是哈希**公式**。

- **实例模型有了它的五个端点**（v1.0 M2a-2，2026-09-27）。`POST /v0/sandboxes/{name}/instances`
  派生一个实例（`sandbox.instantiate`），以 `201` 回它的 id；同路径的 `GET` 列出该定义下的实例
  （`sandbox.read`）；`DELETE /v0/sandboxes/{name}/instances/{id}` 回收一个并回 `204`；
  `GET /v0/sandboxes/{name}/capabilities` 回答该定义的 `supports_multiplexing`；而
  `GET /v0/capabilities` 回答调用者凭证能做什么（`status.read`）—— 两个不同的问题，故意都存在。
  派生是在节点正在跑的东西**旁边**再起一台 VM：既不占用切换「一次一个」的槽，也不受飞行中 run 的检查。
  四条沙箱路由都是**路径参数路由**（路由表比对的是字面路径），因此由新的提取器解析
  （`sandbox_instance_path_from`、`sandbox_capabilities_from`），而 `Resolution` 的 `path_param`
  变成了 **`Vec`** —— 成员路由要同时带 `name` 与 `instance_id`，一个可选参数做不到。`instances` /
  `capabilities` **没有**加进保留名表：它们是第二段，所以定义可以叫这两个名字；`requests` 仍然是保留名。
  文档同步：§5.1 为 **33**，工具 schema 的表与定义多了五行（路径参数 4 → **8**，硬断言与
  `check-tool-schema.mjs` 的具名映射一并更新），浏览器侧新增 `SandboxInstanceView` /
  `NodeCapabilitiesView`（node-panel 探针现在也逐字段核对 `InstanceView`）。**决策 §73。**

- **实例表已落盘 —— v1.0 第一个代码批**（v1.0 M2a-1，2026-09-27）。`AppState` 的单一 VM 槽没了：
  节点现在拥有**实例表**（`instances: Mutex<HashMap<InstanceId, SandboxInstance>>`）加一个
  `current_instance` 指针，而原先属于节点的状态 —— VM 槽、串口发送端、串口缓冲、VM 起始时刻 ——
  变成**每实例一份**。节点在构造时创建一个实例，**它自己的那个**，「切换」与一次普通运行作用的就是它
  （这也是为什么上层什么都不必改含义）；派生一个（`spawn_instance`）会在它自己的槽里起第二台 VM，
  **不改**节点在跑的东西。`InstanceId` 是 `<device>-<pid>-<seq>`（`agent::identity`）：device 可设
  （默认 `local`；给节点命名是连接层的事，纲领 §4），且**一个计数器同时服务 agent 与实例**，于是两个
  空间不会铸出同一个字符串 —— 而三段不是路径语法，要从右往左读（`rsplitn(3, '-')`），因为 device
  名字里可能带 `-`。快照挪到 `snapshots/<device>/<instance_id>`，两种更旧的布局仍可读；`SandboxDef`
  新增 `supports_multiplexing`（默认 false，宿主自己造的两个构造器都给 false）；capability 词汇表升到
  **38**（+6：三个本地，三个 `.remote`），其端点属 M2a-2 —— v1.0 缺口 3/N 的清理随后删去其中没有任何
  路由要求的五个，落定 **33**（决策 §83）；而调度员的三个事件（`m:sandbox:spawn`、
  `m:sandbox:reap`、`m:request:approve`）加入事件流的 **17** 个名单 —— 前两个以 `m.sandbox.spawn` /
  `m.sandbox.reap` 写进链，实例身份走 `detail`（不加列、不改哈希公式）；v1.0 缺口 2/N 后该名单为 **21**。
  **`Capability::ALL.len() == 32` 不再被钉死**：守卫改为「已知集合必须在、且数量 ≥33」，因为一条
  为预期情况就得改的守卫，是会藏住非预期情况的守卫。**决策 §72。**

- **M1 的头三份规范已落盘**（v1.0 M1，2026-09-27）。[api-compatibility.zh-CN.md](api-compatibility.zh-CN.md)、
  [error-model.zh-CN.md](error-model.zh-CN.md) 与 [security-model.zh-CN.md](security-model.zh-CN.md) 把
  [v1.0 纲领](roadmap-v1.0.zh-CN.md) §6 的六个方向变成可以拿来验收的规矩：冻结覆盖什么、什么可改而不需
  冻结（含每种持久化格式的版本标记、打开即迁移、旧读新遇到新文件时 `Err(DataTooNew)`、写前先留 `.bak`）；
  六个 `DispatchError` 变体，各自的重试判定与线上映射；以及每份机密住在哪，加上带时限的披露政策。三份都
  双语、都带已定 / 默认 / 待定标签，且各自写下自己**不**定下的东西。`SECURITY.md` 补上了它缺的报告时限。
  无代码、无 CI、无门禁变化：M1 是一个「把已经成立的写下来」的里程碑。
  **已补齐**（v1.0 M1，2026-09-28）：第六份也在盘了 ——
  [`docs/upgrade.md`](upgrade.zh-CN.md)，decisions §14 一直在点名的那份流程 —— 而稳定性政策带着它的
  红线测试（`api-compatibility.md` §9），于是 roadmap §13 的 M1 满足。**决策 §86。**

- **v1.0 纲领已落盘，网络页「开关点不动」的修复也已推送**（v1.0 纲领落盘，2026-09-27）。
  [roadmap-v1.0.zh-CN.md](roadmap-v1.0.zh-CN.md) 把花了好几个对话才收敛的讨论记了下来，以免再讨论一次：
  **三层**（单设备 → 连接 → 跨设备派发）、连接层（workgroup 加一台**专用**的跨区域服务器，四个角色 ——
  信令、转发、管理、审计汇聚 —— 其中转发是主路径）、**冻结级需要的六件事**、沙箱插件接口、**M** 的形态、
  全走 HTTP 的设置统一，以及生态起步。**audit v2 须单独授权** —— 它触红线 5 —— 文中已标明。十四条待决
  被记成**可改的默认值**（§14），而每条决策都带三个标签之一（已定 / 默认 / 待定）。同一批推送
  **`dd599c0`** —— `fix(ui): the network page keeps its edits`：三个 effect 依赖了整个 store 对象、而不是
  store 里那些稳定的 `useCallback` 函数，于是表单在每次渲染时被重建、两个开关与两个输入框都在毫秒内被
  还原，页面在空闲时占掉约三分之一核。推送前已在本机修好并再验一遍，它随本批进入 `main`。

- **`v0.9.9` 已发布**（2026-09-25 本地准备，2026-09-26 切版）：版本 bump 到 `0.9.9`（7 个文件：`Cargo.toml`、两个 `Cargo.lock` —— 根 lock **8** 个工作区条目、外壳 lock **7** 个，而 `hashlink` / `memoffset` / `miniz_oxide` 留在 `0.9.1`，因为它们是别人的 crate —— `ui/package.json`、`ui/package-lock.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/tauri.conf.json`；wix 守卫要求纯数字正式版不带 `bundle.windows.wix.version`，当前确实没有），`CHANGELOG` 的 `[Unreleased]` 归入 `[0.9.9] - 2026-09-25`（**有意**记为变更集冻结日，比切版早一天），[RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) 按**功能版**重写 —— **GitHub Release 的正文就是该文件（英文版 `RELEASE_NOTES.md`）的逐字拷贝**（12,254 字符，已与文件核对）。**已在 2026-09-26 发布**：annotated tag `v0.9.9`（对象 `6357f43c60848541c3259eca351110b11b3e3f46` → `b1dc2fb`）、正文为该文件逐字拷贝的 GitHub Release、**7 个附件**（本机构建的两个 Windows 安装包 + 本地新打的 `riscdom-server-0.9.9-win-x64.zip`（server 可执行文件 + 作为 `dist/` 的前端）+ CI `bundle` job 的 macOS `.dmg` 与 Linux `.deb` / `.rpm` / `.AppImage`），**Latest 标记已移过来**，`v0.9.1` 降为前任。这一版带三件事：**连出去**（桌面端接入内网节点）、**服务进来**（桌面端自己的看板在局域网上）、以及从根上修掉的**第五个「本地绿、CI 红」机制**（`ui/dist/app`）。规模：**688 个用例 / 118 个套件**、**16** 个 UI 探针、**18 步**门禁。

- **有一条观察被记录下来、但未解释 —— 由层次 9 来结案。** 4/N 的真机走查里，写入远端地址后的第一次重启停在了**本机**看板，且地址从 `settings.json` 里消失了；清掉残留的 `tauri dev` 进程后，同样的步骤完全按设计复现（登录门 → 逃逸 → 回本机看板），也没有任何已知代码路径能解释第一次的结果。它被写下来而不是被解释，而 [manual-acceptance.md](manual-acceptance.md) 的**层次 9** 里专门写了一步来结案。

- **桌面端可以「连出去」，连到内网节点上。**（v0.9.9 内网接入 第 4 批）*设置 → 网络*的「连出去」组现在真的能用了：**地址**存 `settings.json`（它不是机密），**token** 存 **OS 钥匙串**，账户名 `remote-token:<host>`——`NetworkSettings` 里**根本没有 token 字段**（决策 §70）。`api/index.ts` 的实现不再是加载时常量，而是由模式定下的变量，每个数据面导出都是一行转发；而**八个给节点接线的名字**（`get_network`、`set_network`、`read_lan_token`、`lan_status`、三个钥匙串命令与 `restart_app`）在**任何模式下**都作用于**本机**（决策 §71）——这正是「回得来」唯一的原因。`App.tsx` 只读一次设置，把模式与 token 装好，然后才问 `isLocalHost()`：远端模式下的桌面端走的是与浏览器同一道门，而门上带一个**「改用本机」**按钮：删掉钥匙串条目、清空地址、重启应用。`restart_app` 用的是 Tauri 自己的 `AppHandle::restart`——不加插件、不加依赖。第 **16** 个 UI 探针（`probe-ui-remote.mjs`）钉住模式、凭据，以及那些现在必须问「这是谁的节点」的界面：顶栏会写明，设置页在远端窗口下保留 *Audit + Appearance + Network*（本批的过滤表）。

- **构建产物挪到了稳定的父目录下，`main` 上那个红的提交也修好了**（v0.9.9 3/N-fix2）。3/N 把 `bundle.resources` 声明为 `{"../dist": "dist"}`，而 `tauri-build` 用 `Path::exists()` 校验这些路径 —— 于是在**全新 checkout（`ui/dist` 是被忽略的构建产物）上 `cargo clippy ui/src-tauri` 直接失败**：`resource path '../dist' doesn't exist`。最直觉的修法——在 `ui/dist` 里放一个被跟踪的 `.gitkeep`——撞上了第二道墙：**Vite 的 `emptyOutDir`（默认开）每次 `npm run build` 都会删掉它**，工作树会永远脏着。于是改成结构性的答案：前端现在构建进 **`ui/dist/app/`**，父目录 `ui/dist/` 放一个被跟踪的 `.gitkeep` 让目录在全新 checkout 上也在，而 `emptyOutDir` 保留默认——它只清子目录。`frontendDist`、`resolve_web_root` 的两个分支与 `--web-root` 配方都跟着改。实测：`npm run build` 后 `.gitkeep` 仍在；`cargo check` 在**有与没有** `dist/app/index.html` 时都通过——B-2 当时记载的性质（`ui/dist` **不是** `cargo check` 的前置条件）恢复成立。这是本仓第五个「本地绿、CI 红」机制（决策 §69），也是**第一个由我们自己的变更**而不是环境造成的。

- **桌面端可以把白己的看板开放到网络**（v0.9.9 内网接入 第 3 批）。外壳在**应用自己的 state 之上**启动内嵌控制平面 —— 它现在管理 `Arc<AppState>`，而 `host-tauri` 的 **68** 个命令接受同一个句柄（克隆体会有一份自己的 VM 槽，而一块能开出第二个 QEMU 的看板比没有看板更坏）—— 除非打开 `lan_allow_lan`，只绑回环；token 在首次启动时由 `server::token` 生成，于是*网络* tab 的「显示 token」终于有真值可显示。构建好的前端作为 Tauri **resource** 随包分发（`"resources": {"../dist": "dist"}`），并由同一个 helper 解析（先试 resource 目录、再回落源码树），所以打包后的应用有 `web_root`、`tauri dev` 也走同一条路。任何网络设置的改动都会**重绑**（先停再起），而 `RunEvent::ExitRequested` 会 abort 掉 server，于是没有套接字会长过窗口本身。第 **15** 个 UI 探针（`probe-ui-lan.mjs`）钉住接线，[manual-acceptance.md](manual-acceptance.zh-CN.md) 新增**层次 8**，供只有人能走的那一步：同网段的手机。

- **网络接入有了自己的配置页，而 token 可读、但不会被创建**（v0.9.9 内网接入 第 2 批）。新的*设置 → 网络* tab 把两个方向放在一处：**连出去**（服务器地址 + 令牌，按钮禁用并写明原因——连接本身是下一批）与**服务进来**（开关、默认 `127.0.0.1:7821` 的绑定地址，以及一个勾上就立刻给出警示的「允许其它设备访问」开关）。`settings.json` **增量**新增 `network: Option<NetworkSettings>`——五个字段，`SETTINGS_VERSION` 不动。三个命令落在 shell crate（`ui/src-tauri`，不是 `host-tauri`：网络面是桌面端自己的），而 `read_lan_token` **只读文件、从不创建**：造 token 仍是服务端的事，于是打开一个设置页不可能凭空造出一份凭据（决策 §67）。token **只在被请求时**显示，只活在那一个组件的 state 里，不写到任何地方。浏览器一概拿不到——该 tab 与模型表单一样被过滤掉，而三个名字在 `http.ts` 里以一句话拒绝。第 **14** 个 UI 探针（`probe-ui-network-tab.mjs`）把这些全部钉住。

- **`v0.9.1` 是本次发布**（2026-09-25）：一个**修复版**，针对唯一让 v0.9.0 桌面版不可用的那件事 —— 打开即停在一个怎么输都过不去的登录页，因为 `App.tsx` 对所有运行时都索要 token，而桌面端（宿主就在自己进程里）既没有 token、也不提供 `/v0/health`。版本 bump 到 `0.9.1`（7 个文件：`Cargo.toml`、两个 `Cargo.lock`、`ui/package.json`、`ui/package-lock.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/tauri.conf.json` —— wix 守卫要求纯数字正式版不带 `bundle.windows.wix.version`，当前确实没有），`CHANGELOG` 的 `[Unreleased]` 归入 `[0.9.1] - 2026-09-25`，[RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) 按正式发布重写 —— **GitHub Release 的正文就是该文件（英文版 `RELEASE_NOTES.md`）的逐字拷贝**（v0.7.0 到 v0.9.0 都是这么做的）。发布当日即成：annotated tag `v0.9.1`（对象 `25957bcf1d11432986356617f2dfc27243a342d9` → `39ff71d`）、正文为该文件逐字拷贝的 GitHub Release，以及 **6 个附件**（本机构建的两个 Windows 安装包 + CI `bundle` job 产出的 macOS `.dmg` 与 Linux `.deb` / `.rpm` / `.AppImage`），Latest 标记也随之移过来。同一批还把 [manual-acceptance.md](manual-acceptance.md) 落盘：v0.9.0 的 P0 是有人打开应用才发现的，而本仓依赖的那次走查从来没有被写下来。**`v0.9.9` 是最新的 tag 与 Latest 持有者**（2026-09-26 发布）；`v0.9.1` 降为它的前任。

- **桌面端不再停在登录页**（v0.9.1 修复 1/N —— v0.9.0 唯一一个 P0）。`App.tsx` 对**所有**运行时都按 `api.currentToken() !== ""` 判定，于是发出去的桌面构建渲染出 `<Login>` 且永远进不去：桌面端没有 token（它从不与控制平面通话——宿主就在同一个进程里），而 `verifyToken` 会去打一个桌面端根本没有的 `/v0/health`。现在 `App` **先**回答桌面端（`if (api.isTauriRuntime()) return <AppShell />`），token 判定搬进桌面端永不进入的 `WebGate` 子组件——于是那里既不读 token、也不画登录页、更不会去问 `/v0/health`；`SharedApi` 与它的 `Omit` 名单一字未动。`probe-ui-login.mjs` 新增两条断言（桌面端在任何 token 之前就被放行；登录页只在 Web 侧），原先那条「shell 索引」断言按双面门重写。**本机真机验证**（`tauri dev`）：外壳直接起来、六个设置 tab 全可切、聊天框与串口面板俱在、审计页列出 **223 条事件（链完整）**、外观页的语言（English）与主题（Dark）都即时生效。

- **`v0.9.0` 是本次发布**（2026-09-25）：版本 bump 到 `0.9.0`（7 个文件：`Cargo.toml`、两个 `Cargo.lock`、
  `ui/package.json`、`ui/package-lock.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/tauri.conf.json`
  —— wix 守卫要求纯数字正式版不带 `bundle.windows.wix.version`，当前确实没有），`CHANGELOG` 的
  `[Unreleased]` 归入 `[0.9.0] - 2026-09-25`，[RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) 按正式发布
  重写 —— **GitHub Release 的正文就是该文件（英文版 `RELEASE_NOTES.md`）的逐字拷贝**（v0.7.0 与 v0.8.0 的
  发布都是这么做的）。发布当日即成：annotated tag `v0.9.0`（对象 `fe4e0bc4411792a064945d8a5ec2c2e9add8bbe7`
  → `8bc7719`）、正文为该文件逐字拷贝的 GitHub Release，以及 **6 个附件**（本机构建的两个 Windows 安装包 + CI
  `bundle` job 产出的 macOS `.dmg` 与 Linux `.deb` / `.rpm` / `.AppImage`），Latest 标记也随之移过来。
  v0.9 是什么：项目变得
  可驱动、可见（带真实鉴权与实时事件流的控制平面、能说它的 CLI、能读它的浏览器看板），沙箱在 C 之外多了
  两种语言（Zig 与 Rust）；多 Agent 部分交付的是**接口** —— 名册、派发端点、远程句柄 —— 还不是协作策略。

- **一个死字段没了，迁移 relay 现在持有它的端口，而 §1 里那些仍写着「仍开着」的注记也关闭了**（v0.9 发布前小修）。`DownloadSpec` 与 `QemuDownloadSpec` 不再携带 `install_subdir`：生产代码从不读它、只有一个测试断言过它的值，而 §49 那句「字段维持原样、继续是死的」已被取代（决策 §64）——**10 个构造点**移除了该字段（`toolchain_download.rs` 3 处、`host-core/tests/common/mod.rs` 5 处、crate 自己的测试 2 处），并连带去掉一条断言。`MigrationRelay::bind_local_with_timeout` 现在会**在本进程的登记表里预留它的回环端口**，并持有一个与 relay 同寿的 `PortLease`，于是 relay 活着时这个号不会被交给第二个持有者——这正是 `lease_local_ports` 立下的契约，如今也覆盖这次 bind（决策 §65）；公开构造函数未变。另外 §1 里四处仍写着「仍开着」的注记——gate 非 Windows 覆盖（由 B-2 / B-3a / B-3b 关闭）与 logging 批次的「这是否就是 CI 的根因」——现在都写明了是什么关闭了它们，两个语言同步。新增 1 个测试（683）。行为未变、接口未变。

- **Web 看板的节点页有三个子 tab 了**（v0.9 D2b-4b——D2b 线的最后一批）。`StatusPanel` 现在是一个容器（三个子 tab 存在本地 `useState` 里，用与设置页相同的 `.settings-tabs` / `.tab-btn` 一行渲染，因此**零新增 CSS**），下面是 `panels/node/NodeStatus.tsx`（原页面搬过去）、`NodeExecutors.tsx` 与 `NodeSandboxes.tsx`；**`AppShell` 的 view 联合未变**（`main | settings | status`）——决策 §63。两个新 tab 靠四个新读接口，而它们都是**共有名字**、不是 Web 独有：`listExecutors` / `listSandboxes` / `currentSandbox` / `sandboxCandidates` 既是 Tauri 命令（`list_executors`、`list_sandboxes`、`current_sandbox`、`sandbox_candidates`），也是 HTTP 端点（`GET /v0/executors`、`/v0/sandboxes`、`/v0/sandboxes/current`、`/v0/sandboxes/candidates`）——所以 `api/index.ts` 的 `Omit` 名单没动、探针的名字断言依然成立。`SandboxView` / `ExecutorView` / `CandidateView` 加入了 `api/types.ts`。**新增 19 个 i18n 键**（219 → **238**）与 **1 个探针**（`probe-ui-node-panel.mjs`，它还会读 `sandbox_def.rs`，核对浏览器端 `SandboxView` 真的带上 Rust 的每一个字段；ui 探针现为 13 个）。`refreshAll` **刻意不**包含节点清单：事件流丢帧不会改变本机装了什么。

- **浏览器端是只读看板**（v0.9 D2b-4a）。机制只有一个：新增 `ui/src/components/DesktopOnly.tsx`（`DesktopOnly` / `WebOnly`，二者都在渲染时读 `isTauriRuntime()`，因此**不需要**把 `readOnly` prop 传递到任何地方）。**六个文件、共 12 处包裹**藏起属于桌面的控件——`ToolchainTab` 5 处（盖住它的十个）、`AuditTab` 2 处（告警开关与单次 run 的导出）、`SnapshotTab` 2 处（保存；恢复+删除）、`SettingsTabs` 1 处（整个模型表单，Web 端根本不提供）、`ChatPanel` 1 处（输入框——对话记录保留）、`CanvasPanel` 1 处（串口导出；终端本身是视图，且它在挂载时**本来就会**调 `getSerialBuffer` 做初始填充）。三个设置 tab 都带 `web.readonly_note`。外观是唯一例外：**主题与语言实现了 HTTP 调用**（`POST /v0/settings/theme|language`），因为它们是显示偏好，而旧行为——本地已生效、随后收到宿主的错误——本就是坏的（决策 §62）。**新增 2 个 i18n 键**（217 → **219**）与 **1 个探针**（`probe-ui-web-readonly.mjs`，逐屏数包裹点，漏包即 gate 变红；ui 探针现为 12 个）。`probe-ui-api.mjs` 也学到了：那两个控制是 resolve，不是 refuse。

- **Web 客户端能实时刷新了**（v0.9 D2b-3）：浏览器用 `fetch` + `ReadableStream` 读 `GET /v0/events`——`EventSource` 带不了 `Authorization` 头——并用纯模块 `ui/src/lib/sse.ts` 解帧（`parseSseLine` + `SseReader`：保住未到齐的半帧、合并一帧内的多行 `data:`）。**一条流服务所有订阅者**，由第一个订阅打开、最后一个订阅关掉；断开后按倍延迟重连（1 秒 → 15 秒），并把最后看到的 `id:` 回填为 `Last-Event-ID`——服务端的重放缓冲正是为此而存在。envelope 按 `kind` 分流：`event` → 该 `event` 名字的订阅者，`gap` → 单独的 **`onGap`** 回调（`hello` 不分发：它描述的是流本身，而其 `id` 已经成为游标）。`onHostEvent` 保持名字、签名与「返回退订函数」的契约，因此商店的**10 处订阅一行未改**；`gap` 现在驱动一个新的 `refreshAll`（status/audit/runs/sessions/snapshots/vm/toolchain/preflight——丢帧会弄脏的那些；**流式聊天文本与串口字节不可恢复**，文档里明写）。**新增 1 个探针**（`probe-ui-sse.mjs`，ui 探针现为 11 个），21 项检查——15 项线上格式 + 6 项传输层（`fetch` 用替身 `ReadableStream` 替换）。`scan-encoding.py` 的 `print` 不再依赖控制台代码页。决策 §61。

- **源文件已清空 Windows 代码页事故的残留，而且 gate 现在会查它**（v0.9 编码清账）。Windows PowerShell 5.1 会把无 BOM 的 UTF-8 文件按 ANSI 代码页读入、再按 UTF-8 写回：`E2 80 xx`（破折号、省略号）变成 `U+9225` 加一个丢掉的字节，`C2 A7`（`§`）变成 `U+6402`，而 `Set-Content -Encoding utf8` 会添一个 BOM——这些**任何检查都看不到**，因为每一个受损字符都坐在注释里。它发生过**两次**：v0.7 的 `sandbox/`（4 处）与 v0.9 D2b-1 的 `ui/src/api/`（18 处加 3 个 BOM）。五个文件、**24 处**已全部修好，**零行为变更**；`scripts/scan-encoding.py` 新增 BOM 类与 `§` 残码、扫描范围加 `.py`/`.sh`/`.ps1`、并跳过自己的模式表；而 **`--check` 已进 gate**——只对**不可能误报**的两类（mojibake、BOM）失败，模糊类继续报告（决策 §59、§60）。其中 4 处是**不可见字符**（3 个 BOM、1 个私用区码点），需要字节级修复：`edit` 会在匹配前把这类字符规整掉，所以那条例外连带边界一起写进了账本（两侧 hex dump、`git diff` 作证）。

- **工具探针会重试「内核以『text file busy』拒绝的 `exec`」**（v0.9 ETXTBSY 修复——本账本记录的第**四**个「本地绿、CI 红」机制）。`state.rs` 的四个可运行性探针（`zig_runs`、`rustc_release`、`rust_runs`、`toolchain_runs`）现在都走 `exec_with_busy_retry`，它**只**重试 `ErrorKind::ExecutableFileBusy`——最多 5 次、每次相隔 10 毫秒——其它错误一律立即返回。这个拒绝与工具本身无关：`ETXTBSY` 的含义是「**某个**进程正以写方式打开这个文件」，而在 Unix 上这包括「已 **fork** 但尚未 **exec**」的进程（`CLOEXEC` 只在 exec 那一刻关闭继承来的描述符），因此兄弟线程的一次 spawn 可能在本进程已丢开自己的写句柄之后再持有几百微秒——这正是 D2b-2 那次在 `a_mock_zig_download_installs_and_adopts_the_compiler` 上变红、而同一份代码上一次却是绿的原因。匹配用 `kind()`，**绝不**用 `raw_os_error() == 26`：26 在 Unix 上是 `ETXTBSY`，在 Windows 上是毫不相干的错误码（决策 §58）。**新增 3 个测试**——一个跨平台（不存在的文件不重试）、两个仅 unix（errno 26 确实映射到该 kind；一次真正繁忙的 `exec` 会被重试到成功）。Windows：681 → **682**；在 unix 上另两个不会被 `cfg` 掉。

- **Web 客户端能打开、能登录、能看**（v0.9 D2b-2）。`App.tsx` 现在是一道**门，不是路由**：没有 token 时它只渲染 `<Login>`，而外壳（`useAppStore()` 所在处）只在拿到 token 后才挂载——这也正是「一屏未认证请求在任何人输入之前就飞出去」被挡掉的原因。token 在**装入之前**先用一次 `GET /v0/health` 验证，存放于 `sessionStorage`（勾选「记住此设备」时改存 `localStorage`；**绝不进 URL**），而 `verifyToken` 有**四种**答案——`ok` / `unauthorized` / `unreachable` / `other`——因为「令牌不对」与「服务没应答」对人是两句不同的待办。状态页（`panels/StatusPanel.tsx`）是外壳的**第三个视图**（`main | settings | status`），且**只在 Web 端**出现（`isTauriRuntime()` 判），因为桌面端没有可问的 `/v0/status`；它的措辞住在纯函数 `lib/statusView.ts` 里，`agents` 那个数旁边带着诚实的说明：尚未接入执行者名册的节点报的就是 1。**新增 24 个 i18n 键**（8 登录 + 16 状态；193 → **217**，两语言完全一致）与 **1 个探针**（`probe-ui-login.mjs`——ui 探针现为 10 个）。Web 独有的 API 名字（`clearToken` / `verifyToken` / `getHealth` / `getStatus`）在 `api/index.ts` 的 `Omit` 名单里声明，并由 `probe-ui-api.mjs` 断言（决策 §57）。产物 **630,388 → 637,030 字节（+6,642）**。SSE 与其余只读页属 D2b-3 / D2b-4。

- **一份构建产物现在同时服务桌面外壳与浏览器**（v0.9 D2b-1——管理程序这条线的适配层）。`ui/src/api/` 对同一个面有两个实现：`tauri.ts`（外壳的 `invoke` / `listen`）与 `http.ts`（控制平面的端点）；**形状**搬到 `api/types.ts`，使它们不属于任何一种传输；envelope 规则搬到 `api/envelope.ts`，因此只有一份。`api/index.ts` 在两份实现之间**只在运行时选一次**，依据是 Tauri 2 自己的全局量（`window.__TAURI_INTERNALS__`，即 `@tauri-apps/api/core.js` 调 `invoke` 时用的那个对象）——所以 `npm run build` 不用改，一份 `dist/` 既服务桌面外壳（`frontendDist`）也服务 `--web-root`（决策 §56）。四个消费者（`appStore`、`ChatPanel`、`CanvasPanel`、`ModelTab`——注意是**四个**，不是一个）现在都 import `../api`；类型标注 `impl: Omit<typeof http, …>` 让「只在某一份实现里存在的名字或签名」直接变成**编译错误**，新探针又在源码文本上重申同一件事。**26 个只读端点已实现**（8 个单字段包装在唯一一处解包）；**26 个控制一律以一句话拒绝**（「desktop control … controls arrive with D4」）而不假装成功；四个订阅返回空的退订函数而不是 reject，因为它们是从 React effect 里注册的。拒绝以**字符串**形式到达，与桌面的 `Result<_, String>` 完全一致，所以商店里的 `String(e)` 在两端都显示宿主自己的那句话。**新增 1 个探针**（`probe-ui-api.mjs`，ui 探针现为 9 个，已进 `gate.sh`）；产物大小 **623,347 → 630,388 字节（+7,041，+1.1%）**，因为这份单一产物现在也带上了 Web 客户端。登录、新页面与事件流属 D2b-2 / D2b-3 / D2b-4。

- **控制平面可以自己提供 Web UI 了**（v0.9 D2a——管理程序这条线的服务端半边）。`riscdom-server --web-root <dir>` 把构建好的前端的 `index.html` 挂在 `/`、把它的 hash 文件挂在 `/assets/*`，位置在路由表**之前**、且**不**做认证检查：文档、样式表与脚本不带任何秘密，而 `/v0/*` 下的一切仍要过 `Authn`。命名空间就是这两种形状、仅限 `GET`、**没有 SPA fallback**，并且**没有往 `ROUTES` 里加任何路由**——`docs/control-plane-api.md` 那两张被文档锁死的表一字未动，它们的两个测试仍过。目录是请求时读盘（不内嵌任何东西，所以前端重建不需要重编 Rust），`.js` / `.css` / 图片类型显式映射，hash 资源标 `immutable` 而 `index.html` 标 `no-cache`，路径穿越被拦两道——名字不得含 `..`/根/前缀组件（`workspace_io::safe_relative` 的规矩），且真正找到的文件必须解析在解析后的 root 之内——客户端给的名字**从不**做百分号解码。不给 `--web-root` 时，`/` 回一个 404，消息里点名该旗标。**新增 6 个测试**（`server/tests/web_ui.rs`：入口文档；hash 资源的类型与缓存头；三种穿越形状；不存在的资源；未配置的命名空间；以及「Web UI 免 token 而 API 仍要」）。决策 §55。另修：`docs/control-plane-events.md` 曾描述一个从不存在的 cookie 会话端点——浏览器读这条流用的是 `fetch` + `ReadableStream`，文档现在就是这么写的。页面本身属 D2b。

- **会话库会等锁，且一次 append 就是一个事务**（v0.9 会话批次——A3 那项遗留）。`SessionStore::init` 现在会在它**第一次写之前**设好 5 秒的 `busy_timeout`，与审计库同一个形状：SQLite 默认是 0，第二个写者当场就收到 `SQLITE_BUSY`，而因为这个失败落在 `SessionStore::open` 里，倒下的不是一条命令而是**整个实例**。两个进程确实可能撞上 `sessions.db`（两个走默认路径的 CLI / 服务器进程，或显式共用 `--data-dir`）；per-instance 仍是默认。**WAL 与 `synchronous` 故意不设**——审计库本来就是要跨进程共享的，会话库不是，所以这份不对称是决策而不是疏漏（决策 §54）。`append_message` 现在是**一个事务**（插入 + 会话 `updated_at_ms` 更新）：两条语句原本可能只落一半，留下一条「与其会话时间戳相矛盾」的消息。**新增 2 个测试**（`a_file_database_gets_a_busy_timeout`；以及 `a_failed_append_leaves_no_message_behind`——它用一个拒绝 `sessions` 表任何 `UPDATE` 的触发器注入失败，**修前会红**；同样的形状在 autocommit 下确实会把那条消息留下来）。

- **宪法与编译器在 Rust 上也不再矛盾——F 线到此收尾**（v0.9 多语言批 F3b-3）。`PROJECT_CONSTITUTION.md` 在 §3.6（在 `non-negotiable` 列表内）、§4.6、§5 三处禁 Rust——而 §47 当时只把这三处的 Zig 半边标了时效、把 Rust 留作「待 F3b」。现在这三条旁注各自补上 Rust 那句（`v0.9 F3b 解除 Rust 禁令——见 §53`），且 §5 的旁注把仍在禁的写明：**C++、Python 仍在禁令内**——Python 待 Linux 沙箱（v1.x），C++ 仍不在范围。原句与 `non-negotiable` 标题一字未动；§9（v0.1 完成情况）同样未动：那是历史，v0.1 当时确实只支持 C。沙箱能用的语言现为 C / Zig / Rust。决策 §53；本批**只动文档**，生产代码零改动。

- **Rust 的 sysroot 现在也一键可装——而且它是唯一「产物与版本硬绑定」的 pin**（v0.9 多语言批 F3b-2）。`Toolchain` 新增 `Rust`，于是 `LABELS` = `c` / `zig` / `rust`，Tauri 命令、HTTP body 与 CLI 都由同一个 `Toolchain::parse` 获得 `--toolchain rust`。这条件规格是第一个**没有 `(os, arch)` 分支**的：`rust-std` 组件是给**目标**而非宿主的，所以一个 11.9 MB 资产（`rust-std-1.98.1-riscv64gc-unknown-none-elf.tar.xz`，sha256 `32ff8091…`，取自旁边的 `.sha256` 文件）就服务所有平台——`rustc` 本身仍由机器提供（F3b 第一条裁决）。`find_rust_std` 是第三个定位器，也是唯一一个产物是**目录**的：归档把 sysroot 嵌下一层（`rust-std-<版本>-<目标>/rust-std-<目标>/`），定位器在深度 1 返回内层（手工解开的则在深度 0），既有的安装逻辑会把这段相对路径保留下来——所以**不需要「择层抠取」**，`product_locator` 的 `PathBuf` 契约也一字未改。采用走 `set_rust_sysroot`，它会校验 `lib/rustlib/<目标>/lib` 并一并报出 `rustc` 版本。**版本是硬门**：sysroot 只能由产出它的那个 release 使用，因此 `begin_toolchain_download`——每条边都必经的那个方法——在机器 `rustc -vV` 报出的 release 不等于钉住的 `1.98.1` 时**在下载之前**就拒绝。**新增 5 个测试**（单资产规格；`find_rust_std` 两种深度；走回环的 Rust 端到端；经 `download_toolchain_now` 的采用并断言 C 与 Zig 的固定值仍为 null；版本拒绝），另把标签测试扩到 `rust`，并加了一个会输出 GNU 长名条目的 fixture builder——真实归档正是那个形状。决策 §52。
- **logging 测试的 stdout 管道现在被读到 EOF，而 CI 那两次红就是它引起的**（v0.9 logging 修根因批）。`read_banner` 以前按值拿走子进程的 stdout，并在看到 banner 行的瞬间丢弃它；server 随后还要写**三行**（`server/src/main.rs:81-83`）到一个读端已关闭的管道——EPIPE，而在 Unix 上就是 SIGPIPE，它会**无声地**终止进程（无 panic 消息、stderr 到 EOF），从而来不及写出测试等的 `connection from … ended`。这正是前两批测到的形态：`0 line(s) unreadable`、`reader already stopped (EOF)`、子进程干脆消失。现在 `read_banner` **把读取器交还**，`start()` 用与 stderr 同一个 `drain_reader` 把 stdout 读到 EOF（两根管道从此对称），失败信息也在子进程退出状态旁多了 `server stdout: N line(s), reader …`。另有一个不依赖 server 的测试把形状钉住：写四行的子进程被读到结束，且必须报 `success()`（被 SIGPIPE 杀死不算成功）。Windows 一直是绿的，因为它没有 SIGPIPE。**生产代码零改动**——server 自己的 `println!` 没问题，问题在读取端。
- **logging 测试的失败信息现在会说子进程是怎么离开的**（v0.9 logging 诊断批次）。上一批把**读取器**排除了（`0 line(s) unreadable`、`reader already stopped (EOF)`），只剩一个事实：**服务器进程自己已退出**，且从未写出 `connection from … ended`。本批给那条信息补上另一半——`child: exited with code N` / `killed by signal N` / `still running`——并用一个单测把报告本身钉住（`cmd /c exit 1` / `sh -c 'exit 1'`）。**侦察排除的**：全仓**没有** `[profile.*]` 段（因此没有 `panic = "abort"`，否则任一 task panic 都会致命）、没有 `.cargo/config.toml`、没有 `RUSTFLAGS`，而 server 仅有的两处 `process::exit` 都是启动失败路径——两处都先 `eprintln!`。**剩下最强的线索在测试侧**：`read_banner` 按值拿走子进程的 stdout，并在看到 banner 行的瞬间丢弃它，随后 server 还要写三行 `println!`（`server/src/main.rs:81-83`）到一个读端已关闭的管道——这个竞态与每一条已观察到的事实相符（间歇、子进程消失、没有 `connection from`、子进程 stderr 上没有异常）且 Windows 比 Unix 更容易赢。至于到底是 `exited` 还是 `killed by signal`，下一次 CI 失败就会说出。**只改诊断：未碰生产代码、未改超时、未动 profile。**（v0.9 多语言批 F3b-1）。`compile` 第三次按扩展名分派：`.rs` 走 `rustc --target riscv64gc-unknown-none-elf --sysroot <dir>`，配上生成的 `link.ld`（与 C、Zig 共用）、把**配置里的** RISC-V GCC 当链接器（`-Clinker=…`——**故意不**用草案命令里写死的 `riscv64-unknown-elf-gcc`，因为 xPack 装出来的叫 `riscv-none-elf-gcc`），再加 `-Cpanic=abort`、`-Crelocation-model=static`、`-Ccode-model=medany` 与 `-Clink-arg=-{march,mabi,nostartfiles,T}`。`RustConfig` 是第三种语言的配置，也是第一个**两半都可能缺**的：`rustc` 靠探测（`RISCDOM_RUSTC` → `PATH`，同 Zig），sysroot 来自 `settings.rust_sysroot`（或 `RISCDOM_RUST_SYSROOT`）——它是一个**目录**而非可执行文件，因为 Rust 向我们要的是目标的 `core`。缺哪一半就**点名拒绝**（`RustConfig::require`），绝不静默失败。`RUSTUP_TOOLCHAIN` **只对该子进程**清空，所以 `rust-toolchain.toml` 无法在我们给的 sysroot 之下换掉编译器，并行构建也不受影响。宿主会拒绝不带 `lib/rustlib/<target>/lib` 的 sysroot（`set_rust_sysroot`），并把 `rustc` 版本一并报出——两者必须匹配。**新增 6 个测试**（4 个在 `compiler.rs`：精确的 `rustc` argv、扩展名臂、两半缺失各自的拒绝、Rust 搜索日志；`policy::allows_rust_inside_root`；以及宿主侧「sysroot 必须带目标库」）。真正的编译测试在这里打印 `skip: compiles_hello_rs_fixture -- no rustc + target sysroot`：**本机没有目标 `std`**（`rustup target list --installed` 只有宿主三元组）。`rust-std` 仍不可下载（F3b-2），§5 对 Rust 的禁令也仍未加时效旁注（F3b-3）。（v0.9 logging 批次）。`the_connection_line_appears_at_info` 在 Linux CI 上**连失败两次**（两次都是 5.09 s，报错相同：捕获到的 stderr 里只有 `--no-auth` 的 WARNING），而那个测试二进制的读取器正具备「丢行」的能力：`for line in reader.lines() { let Ok(line) = line else { break }; … }`——一行读不出来就结束线程，并把其后所有行丢掉，包括测试等的 `connection from … ended`。现在坏行**只计数、继续读**（`InvalidData` / 其它 I/O 错误计入坏行数，`Interrupted` 重试，EOF 才结束），且 `wait_for_line` 失败时打印**读取器状态**——已捕获行数、坏行数、仍在读还是已停止——下次变红就能区分「那行根本没来」与「读取器早就停了」。**已关闭**（见上方两个 logging 批次）：根因是 `read_banner` 丢掉了子进程的 stdout（EPIPE，在 Unix 上即 SIGPIPE），而不是这个读取循环；D2b-2 那次红另有原因（`ETXTBSY`，决策 §58）。我们的改动不可能造成该失败（`789f196` 对 `server` 的唯一改动是 `Action::ToolchainDownloadStart` 臂，`+11/-1`，健康检查路径没有动），可是同一测试在它之前那两次 Linux run 都是绿的。5 秒超时与 25 ms 轮询是**故意不动**的——本仓两个 flake 先例（relay 端口租约、`audit::concurrency`）都是靠消除竞态修好的，而不是等更久。决策 §50。
- **Zig 编译器现在只差一次点击，而下载终于会说自己在装哪种语言**（v0.9 多语言批 F3a-download-apply）。`DownloadSpec` 新增 `toolchain: Toolchain`（`C` / `Zig`，serde，缺省即 C），随之带来模块自己猜不出的**两件事**：**哪个定位器**找产物（`product_locator`：C 走 `find_compiler`，Zig 走新的 `find_zig`——深度 0/1，用 `agent::ZIG_NAMES`，因为 Zig 发行包是浅结构）以及宿主做**哪次采用**（`set_toolchain_path` vs `set_zig_path`）。`zig_spec_for_current_platform()` 是 C 规格的兄弟函数：Zig 的五个平台资产 + 自己那 5 条硬编码 0.16.0 校验和，取自 `ziglang.org/download/index.json` 的 `shasum` 字段（Zig 不像 xPack 那样发行逐资产 `.sha`）。语言以**标签**形式传递，只在一处解析（`Toolchain::parse`）：CLI 的 `--toolchain zig`、`POST /v0/toolchain/download` 的 body `{"toolchain":"zig"}`（该端点在本批之前**没有** body；不发 body 仍然等于 C）、Tauri 命令的 `Option<String>`。`ToolchainDownloadStatus` 新增 `toolchain: Option<Toolchain>`，UI 因此能说出在跑哪一个。**新增 5 个测试**（3 个单测：标签往返、Zig 五平台臂 + 校验和、`find_zig` 两种深度；2 个集成：Zig 走回环下载全链、Zig 经 `download_toolchain_now` 的采用并断言 C 的固定值仍为 `null`）；CLI 的 `--toolchain` 断言写进本就拥有命令表的两个测试里。`install_subdir` **仍是**死字段——本批需要的不是它，而把它当解压根相接法反而会搞坏 Zig 的安装（决策 §49）。
- **多了 `.tar.xz` 归档类型，其解包器就是 gzip 那个换个解码器**（v0.9 多语言批 F3a-download）。`ArchiveKind` 新增 `TarXz`；`extract_tar_xz` 逐行照抄 `extract_tar_gz`，只把 `flate2::read::GzDecoder` 换成 `xz2::read::XzDecoder`——同一个 Zip-Slip 守卫（`safe_relative`）、同一个 `set_overwrite(true)`、同一套逐条目取消——且分派臂**不带平台门**：`.tar.gz` 在这里只限非 Windows，因为它只是我们自制下载里的 unix 资产；而 `.tar.xz` 是**宿主自己** Zig 发行包与 Rust `rust-std-*.tar.xz` 的形态，所以 Windows 主机也得能读。`host-core` 新增直接依赖边 `xz2 = "0.1"`：它与 `lzma-sys` 本来就在 `Cargo.lock` 里（由 `zip` 带入），因此锁只**多一行**（那条边）、无版本变动；`lzma-sys` 在 MSVC 上编译其 vendored C，unix 主机没有 `liblzma` 时也回落到同一份 vendored 构建，所以**两个平台都不新增系统库前提**。**新增 5 个测试**，全部离线：4 个单测（解包 / 拒穿越条目 / 覆盖 / 取消）+ 1 个走回环下载全链的端到端。归档是**内存里现造**的——这就是本仓的归档测试惯例（`host-core/tests/common` 造 zip/gzip fixture 也一样；仓里没有 `tests/fixtures/`，也没有签入的二进制），所以**不需要改 `.gitattributes`**。不在这里的：目前还没有任何东西会去下载 Zig / Rust 归档——`spec_for_current_platform()` 仍只返回 xPack 那一条，Zig 定位器（`find_zig`）属 apply 批（决策 §48）。`PROJECT_CONSTITUTION.md` 在**三处**禁 Zig——§3.6（在 `non-negotiable` 列表内）、§4.6、§5——并在 §9 的 v0.1 清单里又记了一次。三处现在各带一条**时效旁注**（原句完整、`non-negotiable` 标题完整）：那些条款禁 Zig 的条件是「**MVP 阶段**」，而 MVP 已于 v0.8.0 结束。§5 的旁注写明只动了 Zig——C++、Rust、Python 仍在禁令内（Rust：F3b；Python：Linux 沙箱，v1.x）。§9 未动：v0.1 当时确实只支持 C。记入决策 §47。两项**只报不修**：宪法是一份停在 v0.5 roadmap 节之后的活文档；本批也没有补 v0.6–v0.9 节。
- **Zig 成了沙箱的第二种语言，语言按源扩展名选**（v0.9 多语言批 F3a）。`compile` 把 `.c` / `.h` / `.S` / `.s` 交给 GCC（未改），把 `.zig` 交给 `zig build-exe -target riscv64-freestanding -O ReleaseSmall -fno-stack-check -T <link.ld> --image-base 0x80000000 -femit-bin=<out>`。Zig 自带交叉链接器，裸机目标无需外部工具链、无需 sysroot，生成的 `link.ld` 原样复用。Zig 不注入任何东西：源自己写 `_start`，因为 `-bios none` 的客机跳到载入地址而不是 ELF 入口点，启动代码必须排最前（`.text.start` 正是让一份脚本同时服务两种语言的原因）。`compile_freestanding` 保持原签名，`CompilerConfig` 多带一个值（`zig: ZigConfig`），`settings.json` 新增 `zig_path`（`AppState::set_zig_path` / `clear_zig_path`；**故意不**让 preflight 缓存失效）。`Policy.allowed_extensions` 放行 `.zig`，工具 schema 文档、其人类表格与 `agent/README.md` 都点名两种语言。**这台机器没有 Zig**，所以编译测试打印 `skip: compiles_hello_zig_fixture -- no Zig found`；Zig→guest 启动测试既带标记**又**自我防护——gate 的 `--include-ignored` 会跑在有 QEMU 与 GCC、但不一定有 Zig 的机器上。下载 Zig 归档**不**在本批（其 macOS/Linux 构建是 `.tar.xz`，而下载器只认 `Zip` / `TarGz`），所以那是独立批次 F3a-download，与 Rust 共用（决策 §46）。具体体积要到那一批才有定论：本批没有下载任何 Zig 二进制。
- **最后四个环境依赖测试也点名了，两个 fixture 不再只在 Windows 上能跑**（v0.9 gate 一致性批 B-3b-fix）。B-3b 的仿真有个洞：它只把 `RISCDOM_*` 指向不存在的文件——那能打断 `discover()`，但打断不了 `CompilerConfig::from_env()`，它的回退是**裸可执行名、靠 `PATH` 解析**，而这台机器的 PATH 上正好有工具链。把 `PATH` 也滤掉后又找到 **4** 个（三个会编 C，一个 preflight 步骤），标记总数现在是 **54**（37 + 8 + 9）。七个红 target 里有两个**不是缺前提，而是只在 Windows 成立的 fixture**：`build_archive` 的 tar 分支在 `../` 条目上崩掉，因为 `tar` 自己的 `append_data` 拒绝 `..`（现在名称直接写进 header，于是拒它的变成*安装器*——而测试要考的正是安装器）；以及 `qemu_archive()` 的内容是个 `#!` 脚本，在 Unix 上 0755 的文件会真的*运行*，于是「不能运行的模拟器」竟被采纳了。Windows：**643 passed / 0 ignored**；没有工具的 gate：**584 passed / 62 ignored**。
- **需要 guest 的测试会自己声明，gate 改为按能力分叉**（v0.9 gate 一致性批 B-3b）。50 个需要 QEMU guest 或 RISC-V GCC 的测试现在都带一个 `#[ignore]`，其理由写明前提（37 个 `requires a QEMU guest and a RISC-V GCC`、8 个 `requires a discoverable QEMU`、5 个 `requires a discoverable RISC-V GCC`），因此没有这些工具的机器跑 `cargo test --workspace --no-fail-fast`——**588** 个测试，而本系列开始时是 10。有工具的机器跑 `cargo test --no-fail-fast -- --include-ignored`，再加三个 `--skip`（对应需要 `DEEPSEEK_API_KEY` 或会真写 OS 钥匙串的测试）——**643 passed / 0 ignored**。`--skip` 匹配的是测试的**函数名**——最初那版旗标里的文件名（`real_api`、`stream_real`、`keyring_os`）一个都匹配不上，这个错误就是这样被抓住的。`scripts/gate.sh` 已无按平台分叉的测试分支，账本的 §44 记下了这个惯例。
- **两个真的会编 C 的单元测试，在没有工具链时会明说**（v0.9 gate 一致性批 B-3a-fix）。B-3a 在 Linux 上当场就红：`agent/src/compiler.rs` 的 `compiles_hello_fixture` 与 `reports_compile_failure_without_panicking` 调 `compile_freestanding(...).expect("run gcc")`，而 CI 没有 RISC-V GCC。现在它们探测 `CompilerConfig::discover()`，缺失时打印 `skip: ... -- no RISC-V GCC found` 而不是失败；有工具链的机器仍然真跑。两处 `cargo test` 也加上了 `--no-fail-fast`，一个测试二进制失败不会遮掉其余。**值得记住的教训**：Windows 机器**有**工具链，所以在那里跑一遍套件**不可能**暴露「需工具链」这类前提——本地检查一路都是绿的。
- **没有 guest 的 gate 现在跑**每个** crate 的单元测试**（v0.9 gate 一致性批 B-3a）。该分支原本跑 `cargo test -p audit -p sandbox --lib`——638 个测试里的 **10** 个——于是 `agent`、`host-core`、`cli`、`server`、`worker` 以及 audit/sandbox 的 `tests/` 目录在非 Windows 上完全无覆盖。现在是 `cargo test --workspace --lib`：**204** 个单元测试，零 QEMU 风险（全是纯逻辑；唯一带平台门的是 `sandbox/src/platform.rs`（unix）与 `server/src/token.rs`（unix + windows））。**已关闭**（见上方 B-3b 条目）：需要 guest 的测试都带上了写明前提的 `#[ignore]` 标记，因此没有工具的机器也会跑可移植的**集成**测试——`cargo test --workspace --no-fail-fast`，**588** 个，而本系列开始时是 10。
- **gate 现在在每个平台都 lint 并 check 每个 workspace crate**（v0.9 gate 一致性批 B-2）。最后两处跳过没了——`host-tauri` 与 `ui/src-tauri` 在 Linux 也被 lint——而 `worker`（此前两个平台的任何 clippy 列表都没提到它）也加了进来。`scripts/gate.sh` 里的 OS 分支随跳过一起删掉，`ci.yml` 的 `gate` job 现在在原有的 `libdbus-1-dev` 之外再装 Tauri 的系统库（webkit2gtk / gtk / librsvg / libsoup）。`ui/dist` **不是** `cargo check` / `clippy` 的前置条件：只要 `custom-protocol` 特性没开（`tauri-macros/src/context.rs`），`tauri::generate_context!()` 就走 dev 分支，而裸 `cargo check` 正是这种情形；已用「把 `ui/dist` 挪开再 check」实测确认。**已关闭**（见上方 B-3a / B-3b 条目）：无工具时 gate 跑 `cargo test --workspace --lib`（**204** 个单元测试），有工具时跑 `cargo test --no-fail-fast -- --include-ignored`（**643 passed / 0 ignored**）——不再是 `-p audit -p sandbox --lib` 那 10 个。
- **B-1 的后续：它暴露的那个 lint 已修**（v0.9 gate 一致性批 B-1-fix）。在 Linux 上解开 `cli` 后当场就红：`cli/tests/control.rs` 的 `write_executor_settings` 只被一个 `#[cfg(windows)]` 派发测试调用，自己却没有 cfg 门，于是在 Linux 上是死代码，`-D warnings` 把它变成了编译失败——而只有它和假执行者 helper 用到的 `std::path::Path` 导入也因为同一原因没有门。现在两者都是 `#[cfg(windows)]`。**生产代码零改动。CI 已回绿。**
- **gate 现在在 Linux 上也 lint `cli` / `server` / `host-core`**（v0.9 gate 一致性批 B-1）。`scripts/gate.sh` 的非 Windows 分支原本把这四个 crate 合在一起跳过；现在它在**每个**平台都跑 `cargo clippy -p cli -p server -p host-core --all-targets --no-deps -- -D warnings`，只跳过两个 Tauri crate（`host-tauri`、`ui/src-tauri`）。**没有新增系统包**：CI 已装的 `libdbus-1-dev` + `pkg-config` 正是 `host-core` 的 `keyring` 后端在 Linux 上所需。这个缺口是**付了代价才发现的**——E4 的 `worker` example 步骤是 Linux 上第一个编译 `host-core` 的 gate 步骤，它连红了四个提交。`ci.yml` 未改。**两条都已关闭**：B-2 让两个 Tauri crate 在每个平台都被 lint，按平台分叉的测试分支随 B-3a / B-3b 一起消失（见上方条目）。
- **crate 示例已记载，贡献者模板已就位**（v0.9 小改批次）。`agent/README` 与 `audit/README`（两个语言）各加了一节 `## 示例`，分别对应 `examples/audit_demo.rs` 与 `examples/chain_demo.rs`——它们是唯一两个没在本 crate README 里点名的示例（`sandbox` 的 `run_hello`、`worker` 的 `dispatch` 与 `remote_executor` 早就有了）。`.github/` 新增两个 issue 表单（`ISSUE_TEMPLATE/bug_report.yml`、`ISSUE_TEMPLATE/feature_request.yml`）与一份双语 PR 模板（`PULL_REQUEST_TEMPLATE.md` + `.zh-CN.md`）。表单刻意用 `.yml`：`scripts/check-bilingual.sh` 扫每一个 `*.md`，所以 Markdown 模板会需要一个 `.zh-CN.md` 兄弟文件——而 GitHub 又会把它当成第二个模板列出来。**决策 §43** 撤销了 §20 的 DCO 条款：两件工具干的事不同，CLA（权利授予）更强、也是 open-core 必需的，而 DCO 只是来源声明。
- **Linux 上的 CI 红已在配置层修好**（v0.9 CI 修复）。`gate` job 自 `00fca17` 起在 Linux 上连续红了**四个提交**：它的 `remote executor example` 步骤是 Linux 上第一个编译 `host-core` 的 gate 步骤，而 `host-core` 在 Linux 上的 `keyring` 后端会编译 `libdbus-sys`，后者经 pkg-config 需要系统 `dbus-1` 库。`gate` job 现在安装 `libdbus-1-dev`，Linux 的 `bundle` job 的依赖列表也加上了它。本地（Windows）gate 走 `keyring` 的 `windows-native` 后端，从不需要它——于是四个提交在 CI 全红的情况下被推了出去。**流程教训**：本地 gate 绿不等于 CI 绿；每次 push 后跑 `gh run list --limit 3`。
- **文档有了入口**（v0.9 文档批次）。[`docs/README.zh-CN.md`](README.zh-CN.md) 是那张导航图：全仓每一份 Markdown，按 [decisions](decisions.zh-CN.md) §21 的五个受众分组（从这里开始 · 内核开发者 · 发行集成者 · 管理员 · 终端用户 · 贡献者），每行写明这份文档做什么、它的**状态**（*活跃* / *快照* / *历史*）与版本——历史被标为历史（`architecture-evolution.md` 是 v0.7 快照；较早的 `CHANGELOG` 段与已发布的 `RELEASE_NOTES` 不重写）。根 `README` 的目录树落后了好几个批次（它只认识 `host/` 与四个 crate；工作区有八个），测试清单还把 `host-core` 写成「Tauri 后端命令」——自 A1 拆分后那是 `host-tauri` 的活；两个语言都已改正，而「更多」节现在以导航开头并列全九个 crate README。规范文档里两处陈旧计数（`control-plane-api` 说 35 个控制端点、客户端指南说 31 个查询端点）也已改。**只报不修**：四个 crate README（`audit`、`cli`、`host-core`、`ui`）没有任何指向邻居的链接——它们唯一的链接是语言切换行。（那一批报的另外两项——crate 示例未在本 crate README 记载、以及 §20 的模板与 DCO——已在 2026-09-24 的小改批次里结清；§20 的 DCO 条款事后看是判断错了，§43 已撤销它。）
- **行尾现在归仓库管**（v0.9 行尾批次，机械性）。一份 `.gitattributes`（`* text=auto eol=lf`；`*.sh` 明确写出；六个被跟踪的二进制标 `binary`；**没有 `*.ps1` 例外**——五个 PowerShell 脚本今天就是 LF，而且每批的 gate 与提交都是经它们跑的）。工作树里躺着 38 个 CRLF 或混用行尾的文件（最重 `sandbox/src/relay.rs`，413 行里 401 行），而 git 看不见：**索引里一直就是 LF**，所以 `git add --renormalize .` 一无所获，**本提交不含任何行尾改动**——它是一次工作树修理，并已被证明与内容无关（356 个被跟踪文件逐个与其已提交 blob 对比，逐字节相同）。七处相对链接是错的（从 `docs/` 指向根级文件或反之漏了前缀：CHANGELOG 的 `multi-agent-foundation`、`handoff(.zh-CN).md` → `RELEASE_NOTES`、`qemu-distribution(.zh-CN).md` → `THIRD_PARTY_NOTICES`、`toolchain-setup(.zh-CN).md` → `ENVIRONMENT`）；全仓 365 条相对链接现在全部有效。
- **小债还清**（v0.9 清账）。四件事，没有新表面。（一）**测试里不再有需要手改的计数**：`every_control_endpoint_answers` 通过一个新的只读访问子（`server::routes::control_paths()`）从路由表算出预期，于是少一条用例的控制端会点名缺失的路径而失败，而测试不管的七个是被**点名**而不是被数出来的；`the_table_has_the_documented_endpoints` 从 API 文档的 §5 标题读计数，两个语言都读。（二）**忽略 `__pycache__/` 与 `*.pyc`**。（三）数端点的注释（「26 查询 / 27 控制」、「两条带路径参数的路由」）已改正或去掉数字。（四）`agent/README.zh-CN.md` 的工具表变成与它英文兄弟一样的索引。扫描带出两个发现，按本批规矩**只报不修**：**七处相对链接失效**（从 `docs/` 指向根级文件或反之——`CHANGELOG.zh-CN.md` → `docs/multi-agent-foundation.zh-CN.md`、`handoff(.zh-CN).md` → `../RELEASE_NOTES(.zh-CN).md`、`qemu-distribution(.zh-CN).md` → `../THIRD_PARTY_NOTICES.md`、`toolchain-setup(.zh-CN).md` → `../ENVIRONMENT.md`），以及**11 个文件在工作树里行尾 CRLF/LF 混用**（`sandbox/src/relay.rs` 及邻居）——编辑工具与 `core.autocrlf=true` 的产物，git 看不见。本批要找的两个缺陷**并不存在**：85 份 Markdown 里没有 lone `\r`，也没有不配对的围栏。
- **那条缝有了另一半：远程执行者**（v0.9 接口交付 E4）。`agent/src/dispatch.rs` 自 v0.8 起就写着「远程实现……正好实现这个 trait。**尚未写**」；`worker/examples/remote_executor.rs` 就是它。`HttpExecutorHandle` 持一个本地 label、节点的 base URL、可选的 bearer token 与**远端 target**，它的 `run` 把一个任务形状的 body POST 到 `POST /v0/tasks`——那个把任务路由给**远端**节点拥有的执行者、并回 `TaskOutcome` 的端点，这正是它是真正执行者而非形状演示的原因。应答里的身份是节点的，绝不是句柄的 label；一条指向别的任务的应答是协议破裂；`404` 是 `NoSuchAgent`（缺的是对面的队伍），其它失败都是 `Failed`。**没有传输层 crate**：`worker` 只依赖 `host-core`、`agent` 与 `serde_json`，请求是用 `std::net::TcpStream` 手写的——`server/tests/smoke.rs` 与 `host-core/tests/common/mod.rs` 已在用的手法，于是读者看得见究竟过了什么。**没有生产 crate 被改动**：`LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn AgentHandle>])` 就是全部集成，这正是那条缝承诺的。示例零配置即可跑（`127.0.0.1:0` 上的进程内替身节点），并用 `--self-test` 自证（七条断言）；`scripts/gate.sh` 会跑这一步。客户端指南多了 §9（怎么写一个句柄），README 写明它不是：回环 HTTP，不是跨设备方案（决策 §42）。

- **一个可以跑起来的监工**（v0.9 接口交付 E3）。`examples/python/dispatch.py` 是最小的完整外部监工：一个在内核之外、自己不持有模型、通过控制平面驱动一个节点的进程——用 `GET /v0/executors` 拿队伍，每条任务一次 `POST /v0/tasks`（同步、一条一条），`--follow` 时用 `GET /v0/events` 在干活过程中看事件流。**只用标准库**（`urllib.request`、`json`、`argparse`，加上手写的 SSE 拆帧），因为参考实现不该教人一个它并不需要的依赖；token 来自 `--token-file` 或 `$RISCDOM_TOKEN`，绝不来自参数。它的退出码沿用 CLI 的约定（`0` 全部成功、`1` 有没成功、`2` 用法错误、`3` 不可达或被拒），而它的 `--self-test` 用一个 stdlib `http.server` 假控制平面离线跑通整条路径——`scripts/gate.sh` 现在会在 `PATH` 上有 Python 解释器时跑这一步（没有就打印 skip；这是 gate 的第一处可选工具链）。随行两份文档：`examples/python/README.zh-CN.md` 与客户端指南 §8，后者现在指向它。Rust 那位兄弟保持原样（`worker/examples/dispatch.rs`：执行者是子进程、不走 HTTP、并行），README 特意把差别列成表：它们是同一幅画的两半（决策 §41）。

- **两份工具 schema 都是文档，且都被校对**（v0.9 接口交付 E2）。这条接口的另一半是词汇：执行者的模型可以叫什么，AI 监工可以叫什么。`docs/tool-schema-executor.zh-CN.md` 是 `tools_json()` 构建的八个工具——逐字，就是 `AgentLoop` 放进请求 `tools` 字段的那个数组——而 `docs/tool-schema-control-plane.zh-CN.md` 把每个端点写成一条 OpenAI 风格函数定义（32 查询 + 36 控制 + 3 本机 + 4 带路径参数 = 75 条工具），名字从路径机械推导（去掉 `/v0/`、折叠分隔符；同时服务两种方法的路径给它的 `POST` 加 `_post`；四条带 id 的路由用一个动词）。区分正是要点，两份文档都说了：执行者的工具在**执行者内部**跑，监工的工具是**驱动**一个节点的方式。手写的 schema 会漂，所以两半各有一个归属：`agent/tests/tool_schema_doc.rs` 把执行者文档与 `tools_json()` 对校；`server/src/routes.rs` 自己的测试把控制平面那三张带标记的路由表与 `ROUTES` + `LOCAL_ROUTES` + `resolve` 对校（于是端点不可能没带工具就落地）；而新增的 `scripts/check-tool-schema.mjs`——`scripts/gate.sh` 里的一步——owns 两者都看不见的东西：译文必须携带相同的带标记块，表里每个名字必须是文档 §2 的推导结果，且每个名字既有定义又有表行。`agent/README.md` 的工具表现在只是指向 schema 文档的索引，不再是清单的第二份（决策 §40）。

- **节点可以被派任务，而它的执行者队伍是配置**（v0.9 接口交付 E0）。本批把 `Dispatcher` 变得可以从 HTTP 抵达。`settings.json` 里的 `executors`——一个 label、一个 program 与它的参数，**没有 `env`**，因为设置文件不是密钥库——在 `load_settings` 之后一次性变成 `StdioExecutorHandle`，而 `AppState::dispatch_task(target, input, sandbox, id)` 把一条任务送进 worker 监工所建的那个同一个 `LocalDispatcher`。登记**不起任何子进程**：在任务到来之前 handle 只是数据，所以它不需要懒初始化（也因此，不存在的 program 是第一个任务的失败，而不是启动错误）。`POST /v0/tasks` 接 `Task` 的四个标量字段，调用方没给 `TaskId` 就补一个，并以该执行者的 `TaskOutcome` 作答——**同步**，和 `/v0/agent/run` 一样：没有任务表，也没有 `GET /v0/tasks/{id}`，所以没有可轮询的东西。阶梯是分开的两件事：无人拥有的目标是调用方的 `404 cause "target"`，断掉的派发是宿主的 `500 cause "task"`，而一次只是*跑失败*的运行仍是 `200`，它的 `outcome` 会说 `failed`。`GET /v0/executors` 列出可抵达的都有谁，按配置顺序——空列表也一样。**节点故意不是它自己的执行者之一**——目标写它是 `404`，因为在这里跑是 `POST /v0/agent/run`——所以两个端点是兄弟而非同义词，而这正是让 E0 值得单独一批的那个区分。两条路由都声明 `agent.run`（E0 裁决三：不加任何 capability——能跑 agent 的人就是能问「能问谁」的人），各有一条 Tauri 命令（已注册、未接到界面：D 线）与两个 CLI 子命令（`executors list`、`tasks dispatch --target <agent_id> --input <text> [--sandbox <name>]`）。随行八份文档，含决策 §39。

- **一个任务声明它在哪个沙箱下跑，而节点一点不动**（v0.9 沙箱 F2d，沙箱线的最后一块）。`Task` 多出 `sandbox: Option<String>`（`#[serde(default)]`，旧 supervisor 的行仍可读）与 `Task::with_sandbox` builder；三个能携带声明的表面都带上它：`POST /v0/agent/run`（请求体字段 `sandbox`）、Tauri 的 `run_agent` 命令（新的可选参数）与 worker（`Task.sandbox`，本就在它读的那一行上——协议改动恰好是一个可选字段，因为 `Task` 自 v0.8 起就可序列化）。它落在 `AppState::run_agent_for(emitter, input, sandbox)`——`run_agent` 保留旧签名并改以 `None` 委派，所以直接调用点一个都没动。优先顺序是**任务 > `current_sandbox` > 配置默认 > 内置兕底**，未定义名字是 `404`（`cause: "name"`）而不是静默回落，解析出的定义被注入为编译器、QEMU 路径与客户机内存（新增 `agent.set_memory_mb`，因为 `VM_MEMORY_MB` 是硬编码的 128）。**它不到内核**：`start_vm` 的 `elf_path` 仍旧选 ELF，`def.kernel` 仍是*切换*时启的（F2d 裁决二）。两条拒绝守着声明：未定义的名字，以及不是**正在跑的** VM 所来自的名字——`409` `cause: "sandbox"`，因为任务只声明、只有切换才改变，而报文指名两条出路（停掉它，或 `POST /v0/sandboxes/switch`）。这个检查需要一个宿主原本没有的事实：`current_sandbox` 只由成功的切换写入，所以由 `start_vm` **工具**启起的 VM 没有记录下来的来源。现在 `active_sandbox` 记录它——在运行的 VM 出现时、由 `switch_sandbox`、以及（作为节点自己的沙箱）快照恢复时写入，由 `stop_current_vm` 清除。随之而来的是：运行端点的整条拒绝阶梯（先是声明，再是就绪）移进了 `run_agent_for`——坏的*参数*先于环境被回答，所以 `POST /v0/agent/run` 带一个未定义沙箱时是没有配置模型也拿到的调用方 `404`；就绪拒绝变成类型化的 `HostError::NotConfigured`，路由因此不再重复检查就绪。随行七份文档，含决策 §38。

- **项目以一个文件的形式行走**（v0.9 项目进出）。`POST /v0/workspace/export` 把 workspace 以 `tar.gz` 作答——**字节，不是 JSON**，是那个表面上除事件流之外的第一个此类 body——而 `POST /v0/workspace/import` 把归档当请求体收（zip、tar.gz 或 tar，由 `Content-Type` 选、不认知时看字节）。宿主自己的 `.riscdom/` 状态从不打包、从不解包；逃出 workspace、以链接到访、或读不出来的 entry → `400`、`cause: "archive"`；已有同名文件 → `409`、`cause: "exists"`，除非 `?force=true`；而 import 带**自己的 64 MiB 上限**（`413`），而不是抬高每个 JSON body 共用的那个 64 KiB。capability 上两者分开：import 需要新的 **`workspace.write`**（第 32 个），export 需要 `workspace.read`。AI 的写入也变可见了：`write_source` 现在往审计链记一条 **`agent.file.write`** `{path, bytes}`（不是 SSE 事件——事件计数仍为 14），于是「模型写过哪些文件」是一行记录，而不是去重剖一个被截断的工具参数。两个打包器（`zip`、`flate2`+`tar`）本来就在锁文件里，现在对每个平台都声明：Windows 宿主能读 `.tar.gz`，unix 宿主能读 `.zip`。另有两条 Tauri 命令（已注册、未接线——D 线）与两个 CLI 子命令（`workspace import <archive> [--force]`、`workspace export [--out <file>]`；导出把归档写到 `--out` 或 stdout，计数走 stderr）。随行八份文档，含决策 §37。

- **AI 可以申请改动沙箱，而由别人来决定**（v0.9 沙箱 F2c）。控制平面长出一条申请队列：`POST /v0/sandboxes/requests`（声明 `agent.run`——能跑 agent 的 actor 就是可以表达它所想的 actor）落一条申请并答 `201` 带它的 id；`GET /v0/sandboxes/requests?status=`（`sandbox.read`）读回来，新的在前。`approve` / `reject` 只改记录，**别的什么都不做**：切换仍是另一次带授权的 `POST /v0/sandboxes/switch` 调用（F2c 裁决 4）。决策需要该请求自己的 `action` 所隐含的 capability——`switch` 要 `sandbox.switch`，`define` / `assemble` 要新的 **`sandbox.assemble`**（第 31 个）——而静态路由表表达不了这个，所以这两条路由以 `sandbox.read` 为门，由处理器针对 `dispatch` 现在收到的 actor 做精确检查（F2c 裁决 1）；id 未知是 `404`，再次决策是 `409`。**没有 TTL**（F2c 裁决 2）：`expired` 为预留，没有任何路径产生它。队列是队列、不是单槽，id 是 `req-<pid>-<seq>`——自己的命名空间，不与 `task-` 共用。AI 拿到两个让这个表面可达的工具：`request_sandbox` 与 `sandbox_status`，经新增的 `agent::SandboxRequester` trait 接线（宿主用克隆的子句柄实现它：`agent` 不能命名 `AppState`，而 loop 就住在它里面——F2c 裁决 4），另有四条 Tauri 命令（已注册、未接到界面：那是 D 线）与三个 CLI 子命令（`sandboxes requests`、`requests approve|reject`；两条决策和 `sandboxes switch` 一样先问）。`sandbox:request` 是第 14 个 SSE 事件——`{id, status, requester, action}`，每次变化一条——`all_events_are_named` 守卫重新列全十四个。随行的八份文档：API 表格（§5.1 32、§5.2 33，且词汇表那句「每个 capability 都至少有一条路由」改成「都在某处被强制」——因为 `sandbox.assemble` 在那个处理器里被强制，直到它自己的端点落地）、事件表、客户端指南、两份 README、本节、CHANGELOG 与决策 §36。

- **relay 的端口租约契约写下来了，而它那个偶发失败的测试现在断言的正是它**（v0.9 relay-fix 2/N）。`concurrent_leases_never_repeat_a_port` 失败过两次，两次 `sandbox` crate 都未被改动。侦察结论：分配器是对的——检查与登记在同一锁作用域（`relay::reserve`），且 `PortLease` 只有一个构造点——而断言过强：它记录了本次运行中*曾*取到的每个端口，于是一个已结束线程释放、又被 OS 重新发出的号码，看起来就像两个持有者同时持有。现在测试把每个线程的租约停放到八个线程全部取完再比对，即代码真正保持的不变量；新增的 `a_released_port_is_free_to_come_back` 钉住另一半（被销毁的租约让端口对任何人可 bind）。库侧卫生，行为不变：`HELD_PORTS` 改为 `LazyLock` 包着的 `HashSet`（`HashSet::new` 无法初始化 `static`；`relay::reserve` 现在就是一次 `insert`），`Drop` 只删**自己的**号码而不是所有相同项。API 与调用点均未变（`agent/src/tools.rs`、`host-core/src/state.rs`、`sandbox/src/vm.rs` 未动），`sandbox/README.md` 双语写下契约，`port_race.rs`（默认忽略的真 QEMU 压力测试）仍在走调用方用重试绕开的跨进程窗口。更强的「进程存活期间绝不复用」不变量已被考虑并否决（账本 §35）。

- **切换有了对外表面：一个事件、一个端点、一个 capability 与一个 CLI 命令**（v0.9 沙箱 F2b-2）。`sandbox:switch` 是第 13 个 SSE 事件——每次出口发一条 `{from, to, ok, reason}`，成功与失败都发——而 `POST /v0/sandboxes/switch`（capability `sandbox.switch`，第 30 个）为每种失败各答一个状态：`200` 带 `{from, to}`；没有这个名字的定义时 `404`、`cause: "name"`；运行中 `409`、`cause: "run"`，另一次切换进行中 `409`、`cause: "sandbox"`；定义不能跑时 `503`，`cause` 就是原因码（`sandbox_qemu_missing` / `sandbox_toolchain_missing` / `sandbox_kernel_missing`）；而每项校验都过、VM 起不来时 `500`、`cause: "sandbox_start_failed"`——即「已停而非半切换」那种情形，它变成了自己的 `HostError`（`SandboxStart`）以便应答带上原因码。路由答的是两端而不是空的 `204`，所以只读应答的客户端也能知道发生了什么。`switch_sandbox` 现在接收调用方自己的 `EventSink`（路由注入 `HttpEventSink`，Tauri 命令注入 `TauriEventSink`；界面仍未接线——那是 D 线），`sandbox_switch_in_progress()` 是决定 `409` 的探针。CLI 有 `sandboxes switch <name>`——属破坏性家族：先问，`--yes` 提前回答，非终端 stdin 直接拒绝（退出 2），成功时打印 `switched from <old> to <new>`。**`events.rs` 的守卫恢复完整**：`all_events_are_named` 列全 13 个名字并断言互不重复，F1 的 `qemu:download` 不再遗漏；`docs/control-plane-events.md` §3 现有 13 行。自 F2b-1 起报告、仍未关闭：*成功*路径无 hermetic 测试（需要真 QEMU 与内核 ELF，即黄金路径的 `--ignored` 票）；端点的 `409 cause: "run"` 分支同样无法在测试里走到（运行中需要 LLM 与工具链），因此只在状态层守住。

- **节点可被切到另一个沙箱，先校验后停止**（v0.9 沙箱 F2b-1）。`AppState::current_sandbox()` 变成**运行时状态**（F2b 决策 1，账本 §34）：切换成功前为 `None`，从不写进 `settings.json`；`sandbox_default_name()` 仍回答存储的 `default_sandbox`——切换改的是*正在跑的*，不是*配置*。`AppState::switch_sandbox(name)` 取定义（列表服务的同一份合并，现已收为一份实现 `merged_sandbox_defs`）、校验它（`sandbox_check` 答四个新 `HostError`：`SandboxNotFound` / `SandboxQemuMissing` / `SandboxToolchainMissing` / `SandboxKernelMissing`；`sandbox_runnable` 是它的 bool 面，行为不变）、定内核（`def.kernel`，否则工作区最新 ELF），**之后才**停当前 VM、按定义启一台新的（三次尝试、每次新端口，与 `tool_start_vm` 同形）并把名字记为当前。切换进行中拒绝第二次（`begin_sandbox_switch` / `cancel_sandbox_switch` / `finish_sandbox_switch`，与下载槽同形，`already in progress`），运行中拒绝（`run_in_flight()`，读自 `begin_run` 置、`finish_run` 清的运行记账——loop 共享这个 VM 槽并每次工具调用取一次锁，运行中切换会把它交给另一台 guest）。切换失败 = 节点**已停**而非半切换（`Drop` 杀掉失败句柄 spawn 的东西）；校验失败的定义完全不碰正在跑的 VM。本批尚无事件、端点、Tauri 命令、CLI 与 `sandbox:switch` 审计行——那是 F2b-2，连同 capability（29 → 30）。已报告、已在 F2b-2 关闭：`events.rs` 的守卫只列十一个名字而文档已算十二个（列表里漏了 `qemu:download`）。仍未关闭：成功路径无 hermetic 测试（需要真 QEMU 与内核 ELF，即黄金路径的 `--ignored` 票）。

- **无版本的扫描资源按资源本身命名，而不是按一个缺失的版本**（v0.9 沙箱 F2a-3）。F2a-1 的 `format!("{kind}-{version}")` 把机器自带的 QEMU——扫描不记录其版本——变成了一个叫 `qemu--` 的定义，F2a-2 又把这个名字经四条端点与 CLI 服务出去。它现在在每个平台都叫 `qemu-system-riscv64`：即 `sandbox::qemu_discover` 搜索的那个主干名，裁掉 Windows 构建的 `.exe` 后缀，因此文件里不留第二份名字。扫描**确实**知道版本的资源仍为 `<kind>-<version>`（`toolchain-15.2.0-1`、`qemu-11.1.0`）；`"-"` 哨兵变成一个导出的常量 `NO_VERSION`，不再是两个必须彼此一致的字面量。扫描与合并的其余部分未变——这是一次命名修复。

- **沙箱注册表有了控制平面表面：四条只读路由、四个 Tauri 命令、四个 CLI 子命令**（v0.9 沙箱 F2a-2）。`GET /v0/sandboxes` 答合并后的注册表加上 `current` 与 `default`；`/v0/sandboxes/current` 答这两个名字；`/v0/sandboxes/candidates` 答**原始扫描**（不是注册表，也不写回）；`/v0/sandboxes/{name}` 答一个定义，没有这个名字的定义时 `404` 并在 `cause` 指出参数。四条都声明第 29 个 capability `sandbox.read`。名字路由是继 `/v0/runs/{run_id}` 之后的第二条路径参数路由；其字面子路径（`current`、`candidates`，以及 F2 线后面才落的三个：`requests`、`switch`、`assemble`）永不被当作名字读——在 F2b/F2c 服务它们之前一律 `404`。四个 Tauri 命令（`list_sandboxes`、`current_sandbox`、`sandbox_candidates`、`get_sandbox`）已在桌面外壳注册，但**未与界面接线**——那是 D 线。四个 CLI 子命令（`sandboxes list` / `current` / `candidates` / `show <name>`）人类模式打表格，`--json` 原样透传。**F2a-1 报告的那处缺口已闭合**：§5.1 为 31 个查询，词汇表为 29 个名字，且每个名字现在都至少有一条路由。切换已在上方落地（F2b），审批属 F2c，`Task.sandbox` 属 F2d。

- **沙箱注册表已存在：定义、扫描、合并**（v0.9 沙箱 F2a-1）。「沙箱」现在是可命名的：`host-core/src/sandbox_def.rs` 持有 `SandboxDef`（**存储**字段：`name` / `display_name` / `memory_mb` / `qemu_exe` / `toolchain_path` / `kernel` / `notes`，除名字外全部可选），以及 API **服务**的两种形状：`SandboxView`（定义加上只有宿主当场能回答的三项：`source`、`runnable`、`shadowed`）与 `CandidateView` / `CandidatesView`（一个已安装资源；两个互相独立的列表）。`LocalSettings` 新增 `sandboxes` 与 `default_sandbox`，均 `#[serde(default)]`，因此不做任何迁移，写于这两个字段存在之前的文件照样读得进。`AppState` 回答 `sandboxes()`（合并后的注册表）、`sandbox(name)`、`current_sandbox()`、`sandbox_candidates()` 与 `sandbox_default_name()`。合并优先级是手写 → 扫描 → 内置 `default`；同名时手写者胜，被遮的扫描项**留在列表里并标 `shadowed`**，所以合并是可见的而不是悄无声息的。扫描限于 `<data-dir>/toolchain/*` 与 `<data-dir>/qemu/*`（走下载器自己的 `find_compiler` / `find_qemu`，现已 `pub(crate)`，不再长第二份扫描）加上本机 QEMU，且**从不写回 `settings.json`**。`runnable` 是算出来的、从不存储：QEMU 存在且 `--version` 能跑、工具链存在、内核存在或可编译；因此卸掉一个资源只会让定义不可运行，不会让它消失。`Capability` 增至第 29 个名字 `sandbox.read`；四个端点、Tauri 命令与 CLI 属 F2a-2。已报告未修：API 文档 §5 表格仍只列出已有的 28 条路由的 capability，因为 `sandbox.read` 的端点在 F2a-2 才落——计数故意走在表格前面一格。

- **QEMU 与工具链同样接线，而拒绝本身是已定决策**（v0.9 沙箱 F1）。F 侦察发现一处不对称：`toolchain_download` 端到端跑通（宿主方法、端点、事件、CLI），而 `qemu_download` 是**有代码没 caller**。现在 caller 齐了，形状与工具链完全一致：`AppState::begin_qemu_download` / `qemu_download_status` / `cancel_qemu_download` / `download_qemu_now`（另加 `record_qemu_download_event` / `finish_qemu_download` 与 `qemu_dir()`）、审计事件 `host.qemu.download.start|done|failed|cancelled`、SSE 事件族 `qemu:download`、三个 Tauri 命令、三个端点（`GET`/`POST /v0/qemu/download`、`POST /v0/qemu/download/cancel`，capability `qemu.read` / `qemu.configure`）与三个 CLI 子命令（`qemu download [--wait]` / `cancel` / `status`）。**没接线的是「下载」本身**，这是决策：`spec_for_current_platform()` 在每个平台都拒绝（`docs/qemu-distribution.md` §5——RiscDom 引导用户自己安装 QEMU、不 pin 发布版，因为上游不提供 Windows 二进制，而猜一个摘要等于静默的完整性漏洞），所以 `POST /v0/qemu/download` 答 `503 unavailable`、`cause: "qemu"` 并附指引，也不占槽。拒绝之后的东西全部由回环夹具测试覆盖，因此**将来 pin 一个版本只是数据变更**：答案变成 `202`，其余已可用。两个形状合成了一个：`QemuDownloadEvent` 的 payload 标签现在是 `state`（曾是 `kind`），与 `toolchain:download` 同一套词汇；CLI 的 `--wait` 终止判定也变成一个函数（`download_terminal`）两族共用。`docs/control-plane-api.md` §5.1/§5.2 现为 27 查询与 29 控制（原 26/27），`docs/control-plane-events.md` 现为十二个事件（原十一个，§3 表第 12 行），`docs/decisions.md` §29 记录共用的装配形状。两处不对称保留未修（已报告）：`host-tauri` 在工具链下载失败时仍无条件 `eprintln!`（F1 的 QEMU 命令故意不写——C6 的日志开关只覆盖 `server`）；`paths.rs` 只加了 `qemu_dir_in`，没有进程级 `qemu_dir()`，因为工具链那个进程级对应物同样没有任何调用者。
- **接口说实话，库的日志变成可选项**（v0.9 CLI 批次 6/N）。C3/C4 留下的四件小事，合并修完。两条审计导出答的字段叫 `bytes_written`，返回的却是**事件数**（`write_events_jsonl` 返回 `events.len()`）：现在答 `events_exported`；而确实按字节写的 `/v0/serial/export` 保留 `bytes_written`——所以上面批次 5/N 那条读起来就是历史。workspace 策略拒绝的路径此前是 `403 forbidden`，读起来像授权判定，实际却是调用方参数问题：现在是 `400 bad_request` 加 `cause: "path"`，`403` 只留给认证与授权（`http.rs` 的 capability 检查与 `Authn` 钩子——钉这两者的测试未动）。库的运行日志行变成可选：`http.rs` 的 `connection … ended` 与 `accept failed`、`routes.rs` 的 `toolchain download failed` / `preflight failed` 都过 `ServerConfig::with_log_level`——`--log-level <off|error|info>`，**默认 off**——这正是把内嵌服务端挡在 CLI 的 stderr 之外的关键：批次 4/N 报告的粗糙点，现已闭环。`main.rs` 的启动横幅、用法文本与致命错误仍无条件输出：它们是二进制自己的控制台输出，内嵌场景根本不会跑到那个 `main`。另外本节两处失效的 `../host/src/run_diff.rs` 链接也已修正。
- **CLI 线收尾：五批，且每一个控制端点都有了子命令**（v0.9 CLI 批次 5/N）。`docs/control-plane-api.md` §5.2 剩下的十七个端点全部落地——三个导出（`export audit-jsonl`、`export run-audit <run_id>`、`export serial-log`）与十四个按批次 4 定的分组方式归类的管理配置命令（`llm set|clear|load-key`、`qemu path|clear`、`toolchain download|cancel|path|clear`、`preflight run|ack`、`audit alert set <on|off>`、`theme set`、`language set`）。加上批次 4 的十二个，27 个控制端点已全部可从 shell 触及，另加表外的两个（`vm start` → 预留的 `501`、`runs abandon-stale`）。查询半边未动：仍是批次 2/N 的六个只读命令。
  随之而来两件事。**`--api-key` / `--api-key-file` / `--remember`**：模型的 key 可以直接写在命令行（会警告，与 `--token` 完全一样）或从文件读，只有加 `--remember` 才会存进操作系统凭据存储。**`--wait`**：`toolchain download --wait` 与 `preflight run --wait` 在发请求**之前**先订阅 `/v0/events`，打印该工作所属事件族的帧（`toolchain:download` / `preflight:progress`），并以**工作本身**的判定退出——下载失败或预检某一步失败即 `3`。
  途中记下三个事实，每一个都塑造了实现：**`--out` 是服务端的路径**，相对于 workspace 根解析（逃出 workspace 即策略的 `403`），CLI 从头到尾拿不到文件；**两条审计导出的 `bytes_written` 是事件数、不是字节数**（序列日志导出才是字节数），所以人类模式那行会说明是哪种；**`preflight:progress` 没有“结束”事件**，所以 `--wait` 结束于 fail-fast 的第一个 `failed`，或最后一步的 `ok`（步骤表取自宿主自己的 `preflight::STEPS`）。已知粗糙处，只报告未修：内嵌 `--follow`/`--wait` 若在流仍打开时退出，可能把服务端那行 `connection … ended` 留在 CLI 的 stderr 上。
- **CLI 现在能驱动控制平面了**（v0.9 CLI 批次 4/N）。八个只读子命令之外又添十二个控制类子命令：`run <task>`（一轮 agent 的结果）、`vm stop`、`vm start`（预留的 `501`）、`snapshots save|resume|delete`、`sessions create|open|rename|delete|clear-all` 与 `runs abandon-stale`——全部是对控制平面的 HTTP `POST`，没有一个直接摸 `AppState`。随之而来两件事。**确认**：五条销毁状态的命令（`vm stop`、`snapshots resume`、`snapshots delete`、`sessions delete`、`sessions clear-all`）动手前先问——`--yes` 提前回答，终端上弹提示，stdin 不是终端即拒绝（退出码 `2`），因为沉默不是同意。**`--follow`**：`run --follow` 在发起运行**之前**先订阅 `/v0/events`，事件一到就打印（事件名加一小段 payload；带 `--json` 时是原样的 envelope），最后再打结果，因此运行的流从不需要事后补读。`cli/src/sse.rs` 为新增，负责读帧；`client::confirm` 拥有提问；`cli/tests/control.rs` 用真实二进制对着一个未配置模型的控制平面跑，因此不联网、不碰 QEMU。已知粗糙处，只报告未修：内嵌 `--follow` 若在流仍打开时退出，可能把服务端那行 `connection … ended` 留在 CLI 的 stderr 上，排在 CLI 自己的错误体之前。
- **`server` 已纳入门禁 clippy，且已 clippy-clean**（v0.9 CLI 批次 3/N）。该 crate 此前从未被 lint 过——门禁只选可移植 crate 与 `host-core`/`host-tauri`——把 `-p cli` 加进该步骤才暴露了它。6 处 `clippy::result_large_err`（`http.rs:379`、`routes.rs:489/495/503/513/522`）按 lint 的建议修：错误类型改为 `Box<Response<RespBody>>`，所有调用方返回 `*response`——同一个 Response 值，只是装箱了。随之一并修掉的还有 `routes.rs` 测试里 3 处 `bool_assert_comparison` 与 `tests/smoke.rs` 里 1 处 `filter_next`。现在 `cargo clippy -p server --all-targets --no-deps -- -D warnings` 静默通过，门禁步骤选 `-p cli -p server -p host-core -p host-tauri` 并保留 `--no-deps`。行为未变：同样的 `400` 对象、同样的构造、同样的传播路径。
- **CLI 现在是控制平面客户端**（v0.9 CLI 批次 2/N）。新增 `cli` crate（bin `riscdom`），只对控制平面说 HTTP：加 `--remote host:port` 就连已在运行的 `riscdom-server`；不加则在**自己进程内**把控制平面起在 `127.0.0.1:0`（由内核挑端口）——两种模式同一条代码路径，因此 `riscdom` 从不直接调 `AppState`。八个只读子命令（`health`、`status`、`agents`、`runs list|get`、`audit status|events`、`snapshots list`）；`--json` 原样透传控制平面的 JSON，人类模式打表格；退出码 `0`/1/2/3/4（成功 / 本地 / 用法或 400 / 拒绝或 5xx / 401-403）。token 本地取自 `<data-dir>/token`（首次由 `riscdom-server` 同一套代码生成），远程按 `--token-file` > `RISCDOM_TOKEN` > `--token`；从不被打印。锁文件未新增任何 crate，且门禁的 clippy 步骤现在也覆盖 `-p cli`。
- **更名遗留的失效引用已清空**（v0.9 A1 第 5 波）。所有**仍然生效**的旧 crate 提及都已改指真正拥有该物的 crate：`cargo test -p host` → `-p host-core`（测试住在那里）、`host/tests/…` → `host-core/tests/…`、`host/src/state.rs` 等 → `host-core/src/…`、`host/src/commands.rs` → `host-tauri/src/commands.rs`、`host/README.md` → `host-tauri/README.md`、`host::` 前缀按所指对象改为 `host_core::` 或 `host_tauri::`。CONTRIBUTING、SECURITY、PROJECT_CONSTITUTION 与 ci.yml 注释里的 crate 清单现在都写两个 crate，`README.md` 的 crate 索引也列出 `host-core` 与 `host-tauri`。历史未动：CHANGELOG、RELEASE_NOTES、决策账本、本文件 §1 与 architecture-evolution 快照仍按当时的事实使用 `host`。非 Windows 的 clippy 缺口本波**刻意不补**（后来在 v0.9 的 gate 一致性批 B-1 里**部分**补上：`cli`、`server` 与 `host-core` 在 Linux 也 lint；两个 Tauri crate 留 B-2）。
- **宿主拆分完成：`host-core` + `host-tauri`**（v0.9 A1 4 波中的第 4 波）。`host` 更名为 `host-tauri`（目录、`[package] name`、workspace member），桌面壳依赖它：`ui/src-tauri/src/lib.rs` 里 57 处 `host::` 全部改为 `host_tauri::`，且每一处都经门面解析，因此外壳只依赖一个 crate，无需直接依赖 `host-core`。清单里的 `tokio` 死依赖已删除（本 crate 与其测试从未用过它）。`cargo tree`：`-p host-core` 0 行 Tauri、`-p host-tauri` 15 行、`-p worker` 与 `-p server` 仍 0 行。`worker` 补上了它一直缺的 README 双语对；`host-tauri/README.md` 说明两 crate 边界；账本新增一条（`docs/decisions.md` §27）。
- **`worker` 与 `server` 不再链接 Tauri**（v0.9 A1 4 波中的第 3 波）。两者都从 `host` 切到 `host-core`：30 处 `host::` 路径改为 `host_core::`（worker 4 文件 7 处，server 7 文件 23 处——按出现次数全量改，不只 `use` 行），各自的 `Cargo.toml` 依赖也改成可移植半边。`cargo tree -p worker` 与 `-p server` 现在**一个 Tauri crate 都没有**；此前各列出 15 行 `tauri`。逻辑未变——只是 import 路径与各一行依赖。两个 crate 的端到端测试（worker 协议、HTTP + SSE 控制平面）原样通过，工作区测试仍为 418 条。还剩一个消费者：第 4 波把 `host` 更名 `host-tauri` 并搬桌面壳。
- **宿主的测试随拆分搬迁，且被拆弱的两个守卫恢复完整**（v0.9 A1 4 波中的第 2 波）。39 个集成测试文件全部由 `host/tests` 搬到 `host-core/tests`（`git mv`，保留历史），其 133 处 `host::` 路径改为 `host_core::`——67 条 `use host::`、65 处函数体内路径、1 处文档链接。`host` 现在没有测试、也没有 `[dev-dependencies]`；测试用的是 `host-core` 自己的依赖（`agent`、`audit`、`sandbox`、`serde_json`、`sha2`、`rusqlite`、`zip` / `flate2` + `tar`），因此没有任何依赖被声明两次。第 1 波里悄悄失效的两个守卫在此修好：`scripts/check-mirrored-constants.mjs` 现在同时扫描 `host-core/src` **与** `host/src`（17 个文件，此前只有 3 个），`scripts/gate.sh` 在同一个 clippy 步骤里跑 `-p host-core -p host`（此前 host-core 完全逃过 clippy）。消费者仍未动——那是第 3、4 波。
- **宿主已拆：`host-core` + 门面**（v0.9 A1 4 波中的第 1 波）。内核门面的可移植半边——审计接线、VM 槽、快照、会话、两条下载路径、预检、事件 envelope 与 `EventSink` trait——现在在 `host-core` crate，且**其依赖树里没有任何 Tauri crate**（`cargo tree -p host-core` 一个都没有；`-p host` 仍有 15 行）。`host` 保留 53 个 Tauri 命令、`TauriEventSink` 传输与 Tauri 依赖，并再导出可移植面（`pub use host_core::*;` 加 `host::events::*`），因此**本波没有任何消费者或测试改动**：`worker` / `server` / `ui/src-tauri` 仍对着 `host` 编译、仍链接 Tauri。后续三波依次：把测试搬到 `host-core/tests`；把 `worker` + `server` 搬去 `host-core`（两者从此不链 Tauri）；把 `host` 更名 `host-tauri` 并搬桌面壳。见 [host-core/README.zh-CN.md](../host-core/README.zh-CN.md)。
- **权限已强制，两种传输对身份一致**（v0.9 批次 5/N）。`Capability` 现在是路由表（`server/src/routes.rs`）的类型化列：写不出一条不声明它的路由，也就没有跳过检查的路径。`Authn` 钩子返回 `Actor` 之后，请求路径会问它是否 `allows` 该 capability，不持有即 `403 forbidden`、`cause` 为 `"capability"`（`server/src/http.rs`）——默认拒绝、28 个名字、`Actor` 携带该集合。v0.9 只有两种 actor 形状且都持有全部（token 持有者记为 `operator`，以及 `--no-auth` 的钩子），故 `403` 只来自返回更窄 actor 的钩子。身份复审：每个 sink 打的都是**来源**的身份——`AppState::agent_id()`——且 `Server::sink()` 不再接受身份参数，故同进程内的两种传输不可能不一致；新测试把同一事件经两个 sink 发出，在线上断言 `agent_id` 一致。文档：API 文档 §3 现为「认证与权限」，客户端指南新增 capability 一节与安全部署示例（nginx 前置、流用 `proxy_buffering off`），[server/README.zh-CN.md](../server/README.zh-CN.md) 说明 token 管什么、不管什么。
- **控制端点、token 与事件补发已落**（v0.9 批次 4）。`docs/control-plane-api.zh-CN.md` §5.2 的 27 个 `POST` 端点全部可调，另加预留的 `POST /v0/vm/start`（501）与已实装的 `POST /v0/runs/abandon-stale`（G4）；服务端现在默认装入 `TokenAuth`——在 `<data-dir>/token` 生成 32 字节随机 token、仅属主可读、常量时间比对，可用 `--no-auth` 关闭——事件流的 id 改为服务端全局帧序号，带 `Last-Event-ID` 时从 1024 帧的有界缓冲补发，游标过旧则发 `gap` 帧。每条路由都声明所需权限并交给钩子（强制已于批次 5/N 落地，见上）。
- **审计存储的打开路径已并发安全**（v0.9 批次 3 之后的修复）。两个进程同时打开同一个全新的 `audit.db`，过去必有一个失败：`PRAGMA journal_mode = WAL` 需要独占访问，且绕过 busy_timeout 直接回 `SQLITE_BUSY`，因此覆盖写路径的 5 秒超时从未覆盖这次切换——这正是 gate 曾在 `audit::concurrency` 上 flake 一次的原因。现在 `AuditStore::open` 会对整条序列重试（每次新连接，`OPEN_MAX_ATTEMPTS` / `OPEN_BACKOFF_BASE`，定义为写路径常量的别名），且只对锁错误重试。写路径的 `BEGIN IMMEDIATE` + 重试、哈希公式、链上历史行与触发器均未动。由两条新的竞态测试钉住（8 线程 × 25 轮，一个新文件、一个已在 WAL）。
- **查询 API 与统一事件 envelope 已落**（v0.9 批次 3/N）。`docs/control-plane-api.zh-CN.md` §5.1 的 26 个查询端点全部可经 HTTP 调用——另加宿主本地端点 `/v0/health`、`/v0/status`、`/v0/events`、预留的 `/v0/resources`（501）、以及新增错误码 `405 method_not_allowed`——并且每个传输现在都把要发的东西包进同一个 envelope，构建于 `host/src/events.rs`。事件文档标为「有变」的三种 payload 形状均已落地（`vm:state` 恒带 `name`、`audit:failed` 用 `message`、`toolchain:download` 标签为 `state`）；其余八种未动。webview 在唯一边界处解包（`ui/src/api/tauri.ts`）；worker 的行协议与 SSE 流按原样承载 envelope。客户端走查见 [docs/control-plane-client-guide.zh-CN.md](control-plane-client-guide.zh-CN.md)。此条列出的三项缺口——控制类、`gap` / `Last-Event-ID`、权限强制——已分别由批次 4 与批次 5/N 闭环。
- **控制平面骨架已落**（v0.9 批次 2/N）。新增 `server` crate——可执行文件 `riscdom-server`——绑定了已定稿的接口面：`GET /v0/health`、`GET /v0/status`、`GET /v0/events`（SSE，`hello` 与 `event` 两种帧）、B1 错误模型、以及 `Authn` 钩子（当时默认 `NoAuth`；批次 4 起默认 `TokenAuth`）。它是架在 `host` 之上的 Layer 3，不引用任何 Tauri 类型（仍会链接 `tauri`——已知代价）。它不改 `host` 任何源码：`HttpEventSink` 从 `run_agent` 的 `emitter` 参数位注入。`gap` 帧与 `Last-Event-ID` 补放**尚未实现**（下一批）；设计上给它们留的位置已在 `docs/control-plane-events.zh-CN.md` 标注。见 [server/README.zh-CN.md](../server/README.zh-CN.md)。
- **控制平面协议已定稿**（v0.9 批次 1/N，仅设计，无代码）。两份双语成对的文档固定了管理侧要照着实现的接口：HTTP 命令/查询面见
  [control-plane-api.zh-CN.md](control-plane-api.zh-CN.md)——53 个命令变成 53 个端点（26 个 `GET` / 27 个 `POST`），四项内核能力缺口逐项明确处置；推送侧见
  [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)——SSE 分帧，加一个十一个事件共用的 envelope（`version` / `kind` / `event` / `agent_id` / `task_id` / `ts` / `payload`）。十一个 payload 里有三个改了形状（`vm:state`、`audit:failed`、`toolchain:download`），其余八个是恒等映射。**传输选 HTTP + SSE，不用 WebSocket**：推送是单向（服务端到客户端），SSE 是纯 HTTP（无升级握手、无额外 crate），且自带 `Last-Event-ID` 重连。设计定稿，尚无任何实现。
- **v0.8.0 发布之后：preflight 目录也改为 per-agent**（遗留项 A2 —— 共享 workspace 里最后一条写入路径）。
  新产物写入 `<workspace>/.riscdom/preflight/<agent_id>/`；要启动的 guest 先在 agent 自己的目录里找、再回退共享
  根目录，因此 A2 之前的 guest 仍然可用、不会被孤立。`settings.json` 里的缓存本就随实例隔离。v0.8.0 发布说明中
  「preflight 目录仍共享」这一条据此闭环；那边仅剩 `tauri` 仍未 optional。
- **v0.8.0 发布之后：派发得到的 outcome 写明的是真正跑了任务的执行者**（主体交付 3/3 —— v0.8.0 发布说明里
  三条「已知限制」的第一条已闭环）。`AgentHandle::run` 返回 `TaskOutcome`，并填入只有它知道的身份：本地 loop
  自己的 id、host 实例自己的 id、或子进程声明的 id（从它的 `worker:ready` 事件读出，缺失则上报而不是猜）。
  `LocalDispatcher` 改为透传而不再盖 `Task.target`，因此 `TaskOutcome.agent_id` 现在回答的是「谁跑了它」而不是
  「它被发给谁」。v0.8.0 还有一个边角未结：`tauri` 仍未 optional（共享 preflight 那条已由遗留项 A2 闭环）。
  `RELEASE_NOTES.md` 是已发布的正文、**不改**：它的「已知限制」里仍带着已被本行与上一行取代的边角。
- **`v0.8.0` 已发布**（2026-09-22）：版本 bump 到 `0.8.0`（7 个文件：`Cargo.toml`、两个 `Cargo.lock`、
  `ui/package.json`、`ui/package-lock.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/tauri.conf.json`
  —— wix 守卫要求纯数字正式版不带 `bundle.windows.wix.version`，当前确实没有），`CHANGELOG` 的
  `[Unreleased]` 归入 `[0.8.0] - 2026-09-22`，[RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) 按正式发布
  重写 —— **GitHub Release 的正文就是该文件（英文版 `RELEASE_NOTES.md`）的逐字拷贝**（v0.7.0 的发布就是
  这么做的）。附件：本机构建的 `RiscDom_0.8.0_x64_en-US.msi` 与 `RiscDom_0.8.0_x64-setup.exe`，以及从 CI
  `bundle` job 下载的 macOS/Linux 包。
  v0.8 是什么：完整双语的界面，以及
  多 Agent 地基（多进程审计写入、每条事件带身份、per-agent 快照、派发抽象、两进程雏形）。
- **`v0.7.0` 已发布**（2026-09-21，本次发布之前的那一次）。 版本已 bump 到 `0.7.0`（7 文件 / 15 处 —— 与 v0.6.0-preview.1 相同
  的落点），预览版专用的 `bundle.windows.wix.version` 覆盖再次**删除**（包版本已是纯数字，wix 守卫要求
  如此），`CHANGELOG` 与 `RELEASE_NOTES` 已按正式发布重写，Windows 安装包已构建：
  `RiscDom_0.7.0_x64_en-US.msi` 与 `RiscDom_0.7.0_x64-setup.exe`。次日发布了：annotated tag `v0.7.0`
  （→ `2bddae6b0897bb5fe262af2b7e4bf4b3733ec7eb`）、正文为 `RELEASE_NOTES.md` 逐字拷贝的 GitHub Release，以及
  6 个附件（两个 Windows 安装包 + 一次重新 dispatch 的 `bundle` 产出的、名字为 `0.7.0` 的 macOS/Linux 包）。
  v0.7 落地了三块：**自建 i18n
  设施**（批次 1–2 —— `ui/src/i18n/`、gate 里的 `scripts/check-ui-strings.mjs`、以及*设置 → 外观*里写入
  `settings.json` 并把 `<html>` 的 `lang` 跟着改的语言切换）、**macOS/Linux 构建**（批次 A 的平台工作 +
  批次 B 的 `bundle` job，见下一条），以及让 `host` 在非 Windows 上能编译的修复（批次 8）。**用注册表铺开
  其余约 190 条界面字符串这件事有意不做**：设施与那 4 条 diff 字符串保留，全量翻译不做。
- **macOS 与 Linux：CI 能出包，但尚未人工走查。** `ci.yml` 有 `bundle` job（dispatch 或 `v*` tag；
  macOS + Linux 两个 runner），执行 `npm run tauri build` 并把安装包作为 artifact 上传 —— macOS aarch64
  的 `.app` + `.dmg`，Linux amd64 的 `.deb` + `.rpm` + `.AppImage` —— 在 run `35572294916` 全绿。批次 A
  还补齐了 `icons/icon.icns`、让 QEMU 安装指引随平台变化（`winget` / Homebrew / 发行版包，见
  `sandbox::qemu_discover::install_hint_for`），并用一条跨平台单测钉住 Unix 的 `-qmp unix:` 参数。
  **尚未做**：没有人启动过这些安装包；它们**未签名**（macOS Gatekeeper 会拦下首次运行；Developer ID
  签名与公证属于商业化层）；真实 Unix socket 的 QEMU 运行仍需一台 Mac 或 Linux 机器。Windows 仍是黄金
  路径验证过的平台。当时的 CI 包是在 `0.6.0-preview.1` 名字下构建的，所以 v0.7.0 发布时重新 dispatch 了
  `bundle` 以取得 `0.7.0` 名字的包 —— 它发布的就是那些。
- **`v0.6.0-preview.1` 已作为预发布版发布**（v0.6 批次 1–2，批次 4 发布）：两次 run 逐字段对比 —— 数据层
  与 API（[../host-core/src/run_diff.rs](../host-core/src/run_diff.rs)、`AppState::compare_run_fingerprints`、
  `compare_run_fingerprints` 命令）以及审计页两 run 面板下方那个默认折叠的区块。预发布版**不持有 Latest
  标记**，因此当时 Latest 仍在 `v0.5.0`（此后已先后移到 `v0.7.0`、`v0.8.0`）。附件：`RiscDom_0.6.0-preview.1_x64_en-US.msi` 与
  `RiscDom_0.6.0-preview.1_x64-setup.exe`，构建时带 `bundle.windows.wix.version = "0.6.0"`（WiX 不接受
  预发布版 `ProductVersion`），所以「应用和功能」里显示 `0.6.0`，而产物名保留包版本。它证明了什么、
  没证明什么写在 [RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) —— 它所点的第一条缺口如今已闭环：
  **第 8 步的界面已经人工走查、结论通过**（由项目所有者走查；没有单独归档记录文件，因此
  `walkthroughs/` 里仍只有 v0.5 那次本地走查）。
- **`v0.5.0` 已发布，当时它持有 Latest 标记。**（Latest 此后已先后移到 `v0.7.0`、`v0.8.0`。）
  <https://github.com/breakevery/riscdom/releases/tag/v0.5.0> —— 附件为 `RiscDom_0.5.0_x64_en-US.msi`
  与 `RiscDom_0.5.0_x64-setup.exe`，构建时已去掉预览版的 MSI 版本覆盖（因此「应用和功能」里显示
  `0.5.0`）。它证明了什么、没证明什么，写在 [RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md)。
- **`v0.5.0-preview.1` 作为历史保留**（它是预发布版，所以直到本版发布前 Latest 一直由 `v0.4.0` 持有）。
  它的附件仍留在原处。
- **走查记录已有一份，且是本地那次**：[../walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md)
  —— 七步全过，真实模型 + 真实 key，用的是**安装版 MSI**；但**本机不是干净环境**（QEMU 与 RISC-V GCC
  早已装好）。它发现的问题已修复（S-1 / G-1 / E-1 / E-2 / E-3 在批次 11，G-4 在批次 12）；G-2
  （改模型未生效）与「在聊天框里输入中文」仍需真人用键盘复核，G-3（「已安装的应用」里的 `0.5.0.1`）
  已随覆盖本身一起消失。
- **外部走查仍未发生。** 它本是本版的计划，但没有发生，因此「干净机器走查」现在是 **v0.5.x 的补强项**
  （§8），而不是阻塞项：[golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md) 是测试者要填的
  表单，`walkthroughs/` 是它该去的地方。
- 近期提交（新→旧）：`20f2052`（v0.7.0 发布准备：版本 bump、变更日志、发布说明）← `57dd25e`（v0.7 文档
  快照）← `202dd75`（非 Windows 的 `extract_zip` 存根）← `344fd2b`（Linux 包需要的 rpm）← `0633bdc`
  （macOS/Linux 的 bundle CI job）← `833f9c3`（随平台变化的 QEMU 指引、icon.icns、Unix QMP 单测）←
  `06fef0a`（语言切换）← `6abcb44`（i18n 试点）← `b0efeb8`（v0.6.0-preview.1 发布）。
- tag：`v0.9.1` 是最新的 tag，也是**持有 Latest 标记**的那次发布（已用 `gh release list` 确认：
  2026-09-25T08:01:34Z，annotated tag 对象 `25957bcf1d11432986356617f2dfc27243a342d9` →
  `39ff71dc2fcee0316f3c146014d867e55f633367`）；`v0.9.0` = annotated tag 对象
  `fe4e0bc4411792a064945d8a5ec2c2e9add8bbe7` → `8bc77196bd4ac3cd03da7581214aea193a839b51`（Latest
  标记原在它身上，本次发布接管）；`v0.8.0` = annotated tag 对象
  `0b018081de1a4e89e04e7bc1570d595d38ab4b4b` → `8a5381436b62fa84b4f4a972a630061ce2203373`；
  `v0.7.0` = annotated tag 对象
  `f267f13dc6f8df9a3ff196b05d3bb9b7724f2d60` → `2bddae6b0897bb5fe262af2b7e4bf4b3733ec7eb`；
  `v0.6.0-preview.1` 与 `v0.5.0-preview.1` 为预发布版；
  `v0.5.0` =
  `cea44f7b9920a079422217f811afb49350e08477` → `287ffdb095e1659b89a8cafe040647ada64d0026`；
  `v0.4.0` = `25bd3da3c31c1d1ec7e163f3835b0c2bbb74546d` → `15fda1f6d76d53a4ff1b621c2d3d91f0b4b87311`；
  `v0.3.1` = `d8fdba66a366632ca569d8db2657ab5a566b991c` → `b9be9111c620faad686c7a9d095e0ebc04b31225`。
- `main`（本次发布）处的测试总况：**688 个测试 / 118 个套件** —— 没有 guest 工具时 gate 跑出
  **625 passed / 0 failed / 63 ignored**（每个被忽略的测试都写明了它缺什么前提），有 QEMU 与 RISC-V GCC
  的机器会连被忽略的那批一起跑，只跳过三个需要 API key 或会写 OS 钥匙串的（**685**）。gate 共 17 步
  （其中两步可选：Python 监工自测需要有 Python 在 `PATH` 上），本地与 CI 均全绿（`ubuntu-latest` 上跑
  `scripts/gate.sh`，另加 gitleaks）。
- **架构演进文档已定稿并落盘**：[architecture-evolution.md](architecture-evolution.md)（双语，与
  [architecture-evolution.zh-CN.md](architecture-evolution.zh-CN.md) 成对）记录了 v0.7.0 之后做的架构重估 ——
  四层分层与 syscall 层的「机制/策略」划分、已定决策（Tauri 解耦 A3 → A1、B2 多进程模型、审计单链 +
  agent_id）、为多设备预留的缝，以及通往 v1.0 内核 API 冻结的里程碑路径。只写文档：未改代码。
- **v0.8 主体交付完成 —— 监工派发器 + 多执行者，可演示**（v0.8 主体交付 2/2）：端到端可跑。`worker`
  增加了库半边（`worker::supervisor`）与可运行演示（`cargo run -p worker --example dispatch`）：它起多个
  执行者进程，**共享一个 workspace**、各自拥有一个 **data dir**；任务按 `Task.target` 路由到同名执行者；
  每个任务打印一行，并给出计数。任务**并发**派发（`std::thread::scope` 每任务一线程 —— 句柄是
  `Send + Sync`，因此不需要线程池依赖）；指向不在机群里的执行者会被**拒绝**，不会发给「猜一个」的执行者。
  监工里没有任何模型：这一阶段的监工是派发器，不是 agent。同批收尾：`worker` 的 `audit` 依赖声明了却从未
  使用（执行者经 `host::AppState` 触链）已删除；监工逻辑放进 `worker` 的库，使演示与测试共用一份实现；
  四项已定决策写进了 [多 Agent 地基文档](multi-agent-foundation.md)。
- **两进程雏形已落地**（v0.8 主体交付 1/2）：监工与执行者是两个进程，经 **stdio + JSON lines** 对话。
  `worker`（新 crate，执行者二进制）在 stdin 上读一行 `Task` JSON，用注入的 data 目录跑 host 自己那条
  `run_agent` 路径，在 stdout 上写一行 `TaskOutcome`；它的事件以 JSON 行写到 stderr。监工侧
  `host::StdioExecutorHandle`（一个 `AgentHandle`）起那个二进制、发任务、读结果、吸干事件，到时不答就杀掉
  —— 句柄之上没有改动，这正是 v0.8 批次 4 留开的缝。传输零新依赖；但 worker **确实会链接 Tauri**
  （host 无条件依赖它 —— 改为 optional 是 v0.9 的清理）。两个值得知道的边角：子进程自己的 `agent_id` 是经
  它的事件回来的，而分发器的 `TaskOutcome.agent_id` 保持批次 4 的语义（被寻址到的那个执行者）；以及环境里
  有 key 的执行者**会真的跑**（探针测试移除了该变量，因此这里不会调模型）。
- **最小派发抽象已落地**（v0.8 批次 4）：任务现在可以**派发**，而不再只能内联调用。
  `agent::dispatch` 持有词汇 —— `Task`、`TaskId`（`task-<pid>-<seq>`）、`AgentId`（批次 3 的身份形状）、
  `TaskOutcome`、`DispatchError` —— 以及构成缝的两个 trait：`AgentHandle`（执行者：跑这个任务、返回结果）
  与 `Dispatcher`。**本地**一半已实现：`agent::LocalAgent` 包一个 loop，`agent::LocalDispatcher` 按 target
  路由，host 在其既有 `run_agent` 路径上新增 `HostAgentHandle` + `host::local_dispatcher`。**远程**一半
  有意缺席 —— 这个缺席本身就是那道缝。它放在 `agent` 而不是 `host`，正是为了让该抽象不归 Tauri 外壳所有、
  **不**强迫做 host-core / host-tauri 拆分（[架构演进](architecture-evolution.zh-CN.md) §7 缝 2）。Tauri
  命令仍照旧直接调 `run_agent`：派发路径是新增的内部通路。
- **v0.8 技术债批次已落地**（v0.8 批次 1）：[架构演进文档](architecture-evolution.md) §8 列为债务的三个
  堵死点已清除。app data 目录改为**注入** —— `AppState::with_data_dir(workspace, data_dir)` 取代了
  `host::paths` 里进程级的 `OnceLock`，于是同一进程内的两个实例各自拥有 `settings.json`、会话 DB 与工具链
  目录。审计链新增 **`agent_id`** 列，位于链**旁边**：哈希公式、`prev_hash` 链接与每一行既有的 `hash`
  全不动，因此 v0.8 之前的链仍能通过校验。以及**每个 `AppState` 一个 VM 槽**，以测试钉住而非假定。改的是
  代码不是文档：审计侧两条新测试、host 侧一条新测试（`multi_instance.rs`），黄金路径无变化。
- **每条审计事件都写明是哪个 agent，快照改为 per-agent**（v0.8 批次 3）：身份为
  `local-<pid>-<seq>`（`agent::next_agent_id`），每个 `AppState` 领一个并传给它构建的 `AgentLoop`，
  于是 host、sandbox 与 agent 的事件都带上它；`AgentLoop` 与 `audit_hook` 助手改为接收该参数。
  `agent_id` 仍是链**旁边**字段 —— 哈希公式、`prev_hash` 链接、历史行均不动。快照改到
  `<workspace>/.riscdom/snapshots/<agent_id>/`，读取回退到共享根目录，因此共享同一 workspace 的两个
  agent 不会互相覆盖，v0.8 之前的快照仍可列出、恢复与删除。
- **审计写入现已支持多进程，且失败会响**（v0.8 批次 2）：`audit.db` 是有意共享的（每个 workspace 一条
  链），因此连接以 **WAL** 打开，带 5 秒 `busy_timeout` 与 `synchronous=NORMAL`；一次 append 在读取 head
  **之前**先拿写锁（`BEGIN IMMEDIATE` —— 没有它两个写者会链到同一行并把链分叉，新增的并发测试抓住了这一
  点）；被锁的 append 先以 20/40/80/160 ms 退避重试 5 次，再报错。`AuditSink::record` 改为返回
  `Result<(), AuditError>`，不再丢弃事件；sink 通知 host，host 记日志、发 `audit:failed`，并（默认）在
  *设置 → 审计*给出横幅与弹窗。**产品决策：告警默认开、用户可关；事件与日志行不可关。** 链结构、哈希
  公式、历史行与触发器全部未动。
- 未完成项：`%TEMP%` 下的临时目录仍未清理（删除确认始终未被放行；2026-09-21 统计到 144 个
  `riscdom-*` 条目）；CLA.md 待律师过目；**由他人进行的干净机器走查尚未发生** —— 仍是 v0.5.x 的补强项，
  而不是阻塞项；**macOS/Linux 的安装包从未被启动过**且未签名（签名属商业化层）；v0.5 走查留给真人的两项
  （G-2、真实键盘输入中文）仍未做。**v0.7.0 的发布本身就是下一批**：push、tag、Release，以及一次重新
  dispatch 的 `bundle`（产出 `0.7.0` 名字的 macOS/Linux 包）。

## 2. 远端操作按轮授权，且必须有明确文字

push、打 tag、创建或删除 Release、移动或删除远端 tag，以及任何其他远端写操作，**仅当当轮请求用文字
明确说出时**才执行。对话框里的选项、推断出的意图、上一批次的授权、或「这显然是下一步」都不构成授权。
固定的收尾（gate → commit → push）属于「请求里写了它」的批次，而不是默认动作。

## 3. 提交信息：ASCII，否则 `git commit -F`

在 Windows 上 `git commit -m "…"` 会把信息经控制台 ANSI 代码页传递，因此非 ASCII 文本**在 git 看到它
之前**就被替换成 `?`（`0x3F`）—— 本机 2026-09-18 实测：探针标题 `test: 中文正文测试` 被存成
`test: ?????????`。因此：

- `-m` 的信息一律用 ASCII（英文）书写；
- 必须含非 ASCII 时，把信息写成 **UTF-8（无 BOM）** 的文件，用 `git commit -F <file>`；
- `scripts/commit.ps1 "<msg>"` 以参数接收信息，同样受此约束。

同类事故还有第二道门：**PowerShell 重定向**。`>` 与 `Out-File` 默认写 **UTF-16LE**，于是
`git show … > file`（或任何重定向文本的命令）留下的文件首字节是 `FF FE` —— 按 UTF-8 读它的工具会
看到开头的乱字符（v0.5 批次 12 在比对旧版修订时就撞上过）。两个习惯能同时堵住两道门：文本一律经由
**显式指定编码的文件**写入，并且以**文件**而不是命令行参数传递 —— 被吃掉字符的正是 argv 那条路。
要普查整棵树，手动跑 `python3 scripts/scan-encoding.py` —— 它是诊断工具，**不是** gate 检查
（§4 解释了它为何不进门禁：它对「只含 `?` 的字面量」这条判据无法区分真损坏与合法代码）。

## 4. gate 是「绿」的唯一清单

`scripts/gate.sh` 按顺序持有每一项检查。`scripts/gate.ps1` 与 `scripts/commit.ps1` 只是薄包装：它们
定位 Git 的 `sh.exe` 并运行那个文件，绝不保留第二份命令清单。CI 跑的是同一个 `sh scripts/gate.sh`。
**新增检查写进 `gate.sh`**（并同步 `CONTRIBUTING.md` 里的那份简短清单），不要写进工作流、README 或
某个人的记忆里。

## 5. 审计不变量

以下冻结、永不更改：链结构（`audit_events` 及其 append-only 触发器）、哈希公式、历史行，以及
`audit-verify` 与 `audit-rebuild` 的语义。只读面可以扩展 —— 导出、过滤、带迁移的派生索引加列 —— 前提是
不碰链，且校验器不持有任何写路径。

## 6. CLA 处于待命状态，不是活跃流程

- `CLA.md`（以英文为准）与 `CLA.zh-CN.md`；`CONTRIBUTING.md` 里的条件式 CLA 章节；
  `.github/workflows/cla.yml` 运行**自托管**的 CLA Assistant action（无需安装 GitHub App），触发器为
  `pull_request_target` 且**不检出 PR 代码**；`signatures/version1/cla.json` 已预创建。
- 第 3 条（双许可与再许可）是商业化的关键授权，在任何人依赖它之前**需要律师过目**。文件本身写明：
  只有项目所有者确认后才生效。
- 签署句**不翻译** —— 机器人按 `I have read the CLA Document and I hereby sign the CLA` 精确匹配。
- 未来贡献可能被接收到本仓库以外，所以 CONTRIBUTING 的措辞是条件式。

## 7. QEMU：引导安装，绝不下载或捆绑

v0.4 定案：应用只告诉用户装什么（`winget install SoftwareFreedomConservancy.QEMU`，或官网页面），然后
找到并记住、验证它。理由 —— 上游没有可钉的 Windows 二进制、第三方打包者会变成没被点名的供应链环节、
以及不让自己成为 GPL-2.0 二进制的分发者 —— 见 [qemu-distribution.zh-CN.md](qemu-distribution.zh-CN.md) §5。

## 8. 发布门槛是「走查」，不是「代码写完」

v0.5 的发布条件是：有人在干净机器上用真实 API key 走完第 1–2 步，并对照
[golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md) 记录 —— 机器、操作系统版本、QEMU/GCC
版本**及其来源**、服务商与模型、预检四步、两次 run 的短指纹、导出文件与其 `audit-verify` 结论，以及
任何失败的原样错误。那次走查在 `v0.5.0` 发布前**没有发生**，因此它是 **v0.5.x 的补强项**而不是阻塞项
（§1），由 [../walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md)
那份本地走查代位。第 3–7 步由 `cargo test -p host-core --test golden_path -- --ignored` 覆盖；第 8 步的
比较目前**没有**这样的自动走查（§9）。「实现完成」不是门槛。

## 9. v0.6 从黄金路径第 8 步开始 —— 该步已交付

v0.6 是两次 run 的自动比较 —— 它们的指纹差在哪些字段 —— 这正是 v0.5 刻意不做完的那一步。**已交付**
（批次 1–2：`host-core/src/run_diff.rs`、`AppState::compare_run_fingerprints`、`compare_run_fingerprints`
命令，以及审计页两 run 面板下方默认折叠的字段级区块）；等待走查与发布。
`PROJECT_CONSTITUTION.md` §10 的 v0.5 路线图里那些并行项（QEMU stdio、macOS/Linux、多 VM、增量快照、
会话加密、多 AI、双语界面）**不是** v0.6 的内容：它们是待选项，可挑可弃。

## 10. 文档双语，且有门禁

仓库里每个 `*.md` 都要有成对的另一语言版本与首行精确的语言切换行（`X.md` ↔ `X.zh-CN.md`）；
`scripts/check-bilingual.sh` 只报告、绝不改文件。新增文档即新增一对，且在同一个提交里。

## 11. 在本机构建安装包需要真实 Node

本工作区 `PATH` 上的 `node` 是 LobsterAI 的 Electron-as-node 垫片
（`…\LobsterAI\cowork\bin\node.cmd` → `ELECTRON_RUN_AS_NODE=1 "<electron>" %*`）。在它之下，Tauri CLI
的原生插件会读错 `argv`，所有打包命令都以
`error: unrecognized subcommand '<…>\LobsterAI.exe'` 失败。绕过方式：用真实 Node 运行 CLI —— 例如
`C:\Users\cloud_user\AppData\Local\Programs\Tuanjie Cowork\cli\bin\win32-x64\node.exe`（v24.16.0）——
在 `ui/` 下执行 `node node_modules\@tauri-apps\cli\tauri.js build`。WiX 3.14 与 NSIS 已缓存在
`%LOCALAPPDATA%\tauri`，打包不需要网络。

## 12. `wix.version` 是预览版专用覆盖，且有守卫

带预发布后缀的包版本**不是**合法的 MSI `ProductVersion`（WiX 只接受 `major.minor.patch.build`，纯数字），
因此 `ui/src-tauri/tauri.conf.json` 里带着 `bundle.windows.wix.version = "0.5.0.1"`，而包版本 —— 也就是
产物名 —— 仍是 `0.5.0-preview.1`。**包版本重新变成纯数字后立即删除该字段**；残留的值会让 MSI 的
ProductVersion 与其他所有产物、tag、文档悄悄不一致，而且不会报任何构建错误。
`scripts/check-wix-version.mjs`（在 gate 里，带自测）正是在这种情况下让构建失败。
