[English](architecture.md) | 中文

# RiscDom —— 三十分钟看懂架构

> 这是一份给评审者的短读本。长版在
> [architecture-evolution.md](../architecture-evolution.md)；本页不重写它，只指向它。以下一切
> 都描述 `v1.0.0` 这个 tag。

## 1. 它是什么

RiscDom 是一个**本地、单人操作的运行时，让 AI agent 在沙箱里干活**。人提一个要求；语言模型写
代码；代码被编译成 **RISC-V 裸机**程序并在 **QEMU** 里启动；串口输出被读回来；而每一个有意义的
动作都写入一份**本地、只追加、带哈希链的审计日志**。同一个节点可以从桌面应用、命令行，或局域网
里的手机浏览器驱动 —— 而 v1.0 新增：一个节点可以**把任务派发到另一个节点**。

两条不变量贯穿全部：

- **内核只提供机制，从不提供策略。** 它记录发生了什么，对「本应发生什么」不持判断。
- **审计日志只追加、且是本地的。** 它从不上传；它的链可以在持有它的那台机器上被验证。

## 2. 各部件

一个 Cargo workspace 里的十个 crate，加上前端：

| Crate / 目录 | 是什么 | 信任用于 |
|---|---|---|
| `host-core` | **可移植的 host**：应用状态、设置、会话、派发、任务/沙箱注册表、工作区导入导出、工具链与 QEMU 下载。不含 Tauri。 | —— |
| `host-tauri` | **桌面外壳**：Tauri 命令与事件，是 `host-core` 之上的薄门面。 | —— |
| `agent` | **agent 循环**：LLM 客户端、工具集、能力策略、编译器包装。 | 决策 |
| `sandbox` | **QEMU RISC-V 沙箱**：进程启停、QMP 控制、串口捕获、快照。 | 访客边界 |
| `audit` | **只追加存储**：SQLite 加哈希链，含 `audit-verify` 与 `audit-rebuild` 校验器。 | 证据 |
| `net` | **连接层**：节点身份（Ed25519）、签名、传输、发现、房间、relay 服务端与客户端。 | 网络身份 |
| `worker` | **执行器进程**及其监管半边（双进程原型）。 | —— |
| `cli` | `riscdom`，控制平面的**命令行客户端**。自 v1.0 起它是纯客户端：`--remote` 必需，且它不启动任何东西。 | —— |
| `server` | **控制平面**：`host-core` 之上的 HTTP + SSE。*（v1.0 M8-4a 已拆到 `riscdom-server` 仓；在本仓仍是 workspace 成员，直到 M8-4c）* | API 面 |
| `backup`（`riscdom-backup`） | **可移植性**：把一个节点的状态导出为一个加密包（M7e）。 | —— |
| `sdk/rust`（`riscdom-sdk`） | 控制平面的**类型化 Rust 客户端**（M7c）；TypeScript 客户端在 `sdk/typescript`。 | —— |
| `ui/` | **React 前端**与 `ui/src-tauri` Tauri 外壳。它从不直接碰 Rust crate；它说 HTTP。 | —— |

依赖方向单向且无环：`ui/src-tauri → host-tauri → host-core → {agent, sandbox, audit, net}`，
以及 `agent → sandbox → audit`、`net → audit`。

## 3. 一条 task 的路径

这是评审者唯一需要的那条流。它正是 [demo.md](demo.md) 端到端演练的东西。

```
人
  │  riscdom --remote 127.0.0.1:7821 run "写个 RISC-V hello，编译、运行、读串口"
  ▼
cli  ──HTTP POST /v0/agent/run──▶  server（控制平面）
                                      │  进程内调用
                                      ▼
                                  host-core（AppState::run_agent）
                                      │
                                      ▼
                                  agent 循环 ──HTTPS──▶ LLM 提供方（BYOK）
                                      │  工具调用
                    ┌─────────────────┼──────────────────┐
                    ▼                 ▼                  ▼
              compile（RISC-V     sandbox：QEMU      读串口
              裸机 GCC）          -machine virt        (TCP/文件)
                                 -bios none
                                 -kernel <elf>
                    │                 │                  │
                    └───────────────►─┴──────────────────┘
                                      │  每一个动作
                                      ▼
                                  audit：追加 + 哈希链（SQLite，本地）
                                      │  事件
                                      ▼
                                  server ──SSE /v0/events──▶ cli / 桌面 / 手机
```

用文字说：

1. **提出。** CLI（或桌面、或浏览器）通过 HTTP 把请求发给控制平面。控制平面是一个跑在 host 之上
   的普通 HTTP/1.1 服务：[control-plane-api.md](../control-plane-api.md)。
2. **决策。** `host-core` 在 `agent` 里跑 agent 循环。该循环通过 HTTPS 与**自带密钥（BYOK）**的
   模型提供方通信。API key 从不经过本项目任何服务器 —— 本项目没有这样的服务器；它只留在内存或
   OS keyring 里。
3. **构建。** 当模型要求编译，编译器包装会调用 RISC-V 裸机 GCC 产出 ELF。agent 只会写
   `int main(void)`；`crt0` 由工具注入。
4. **运行。** `sandbox` 启动 `qemu-system-riscv64`（`-machine virt -cpu rv64 -bios none
   -kernel <elf>`），用 QMP 控制它（Windows 上走 TCP），并捕获串口输出。
5. **记录。** 每个有意义的动作成为一条**审计事件**：追加进 SQLite 存储，并链入一条**哈希链**。
   该链只追加 —— 数据库触发器拒绝事件行上的 `UPDATE` 与 `DELETE`。
6. **观察。** 控制平面通过 **SSE** 把事件流推给客户端；桌面应用与只读局域网看板都消费这条流。

## 4. 跨设备

v1.0 加了第三层：一条 task 可以跑在**另一个节点**上。部件都在 `net`：

- **身份。** 每个节点有一对 Ed25519 密钥（`node.key`，或 OS keyring），导出一个 JWK。节点的
  `node_id` 由其公钥导出。
- **签名。** 一条消息对 `{v, from, to, ts, body}` 的规范 JSON 签名，并按固定的六步顺序验证。
  签名只在能力模型授权之处证明*来源*；它本身不授予权力。
- **传输。** 每条消息一行已签名 JSON，走 TCP —— **直连优先，否则经 relay**，两条路径字节一致。
  重放由每对端的最高水位记录界定。
- **房间与发现。** `peers.json` 与 `rooms.json` 描述一个节点认识谁、在哪些房间；发现是对成员
  资格的默认拒绝过滤。
- **服务端。** `riscdom-relay`（以及节点可自托管的网内服务端）只按已签名的 `to` 路由，从不主动
  拨号一个节点；服务端还接收注册、心跳与存活性报告，并以幸存证人的一致意见来裁断。

派发时，`POST /v0/tasks` 命名一个**目标节点**；task 在跨越时保留近端节点给它的 `task_id`，于是
两个节点的审计行可以对齐（见 [cross-device-dispatch.md](../cross-device-dispatch.md) 与
[cross-chain-verification.md](../cross-chain-verification.md)）。

## 5. 真值在哪里

- 架构长档：[architecture-evolution.md](../architecture-evolution.md)。
- 控制平面协议：[control-plane-api.md](../control-plane-api.md)。
- 连接层规格：[connection.md](../connection.md)。
- 审计链设计：[audit-v2.md](../audit-v2.md) 与
  [cross-chain-verification.md](../cross-chain-verification.md)。
- 安全模型：[security-model.md](../security-model.md)。

*本页是为审计包写的摘要。以上文档才是权威；若本页与其中某篇冲突，以它们为准。*
