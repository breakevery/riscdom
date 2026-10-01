[English](capabilities.md) | 中文

# RiscDom —— 今天真能跑什么

> **本页的规则。** 只列在 `v1.0.0` 处**已落盘且被验证**的功能 —— 凡 gate 或一次真实运行会碰到的。
> 仅停留在纸面的设计，或 roadmap 中标为 `[default]` / `[open]` 的条目，**不在此列**。拿不准的一律
> 不收。每条都写明**如何调用**，好让评审者去核实，而不是相信。

命令假定控制平面已在运行，且 `riscdom` 已指向它，例如
`riscdom --remote 127.0.0.1:7821 <命令>`。HTTP 列是命令所用的端点；完整表在
[control-plane-api.md](../control-plane-api.md) §5。

## M2 —— 单机、多沙箱

| 功能 | 如何调用 | HTTP |
|---|---|---|
| **跑一条 agent 任务** —— 模型写代码、编译、启动、读回串口 | `riscdom run "<task>"` | `POST /v0/agent/run` |
| **沙箱注册表** —— 手写、扫描所得、内置兜底 | `riscdom sandboxes list` / `current` / `candidates` / `show <name>` | `GET /v0/sandboxes…` |
| **切换当前定义**（会停再启动 VM） | `riscdom sandboxes switch <name>` | `POST /v0/sandboxes/switch` |
| **执行器路由** —— 一条任务可去哪些执行器 | `riscdom executors list` | `GET /v0/executors` |
| **VM 生命周期** —— 手工启停访客 | `riscdom vm start` / `vm stop` | `POST /v0/vm/{start,stop}` |
| **快照** —— 保存 / 恢复 / 删除 VM 快照 | `riscdom snapshots list` / `save <name>` / `resume <name>` / `delete <name>` | `GET /v0/snapshots`、`POST /v0/snapshots/{save,resume,delete}` |
| **会话** —— 持久化对话，列出 / 打开 / 重命名 / 删除 | `riscdom sessions create <title>` / `open` / `rename` / `delete` / `clear` | `POST /v0/sessions/*` |
| **运行索引** —— 跑过什么，最新在前 | `riscdom runs list [--limit n]` / `runs get <id>` | `GET /v0/runs…` |
| **模型配置**（BYOK） | `riscdom llm set …` / `llm clear` / `llm load-key` | `POST /v0/llm/config…` |
| **工具链 / QEMU** —— 发现、指路径、下载 | `riscdom qemu path/clear/download/status`、`riscdom toolchain download/path/clear` | `POST /v0/qemu/*`、`POST /v0/toolchain/*` |
| **环境预检** —— 运行并确认步骤清单 | `riscdom preflight run` / `preflight ack` | `POST /v0/preflight/*` |
| **工作区导入 / 导出** —— 项目进出 | `riscdom workspace import` / `workspace export` | `POST /v0/workspace/{import,export}` |
| **状态** —— 连接、订阅者、agent 数、`agent_id` | `riscdom health` / `status` / `agents` | `GET /v0/health`、`GET /v0/status` |

## M4 —— 连接（工作组与 relay）

| 功能 | 如何调用 | HTTP |
|---|---|---|
| **节点身份** —— 配置后暴露的公钥身份 | `riscdom identity` | `GET /v0/identity` |
| **对端** —— 本节点认识谁 | `riscdom peers` | `GET /v0/peers` |
| **房间** —— `rooms.json` 定义的房间 | `riscdom rooms` | `GET /v0/rooms` |
| **连接状态** —— configured / connected / problem | `riscdom connection` | `GET /v0/connection` |
| **节点能力** —— 执行器 + 沙箱 + QEMU/工具链就绪 + 对端声明，一次答全 | `riscdom node capabilities` | 上述查询的合并 |
| **签名与传输** —— 每条消息一行已签名 JSON 走 TCP，直连优先再 relay | *（库，在 `net`；gate 自制自测）* | —— |
| **relay 服务端** —— 在节点间路由已签名帧 | `riscdom-relay` 二进制，或带 `settings.network.server_role` 的节点 | —— |
| **网内服务端** —— 节点服务其工作组，含注册 + 15 秒心跳与在线表 | `settings.network.server_role` | —— |
| **存活性** —— 探测工作组成员；服务端以一致意见裁断 | *（跑在节点自己的线程上）* | —— |

房间、成员资格与加入都是**配置**；v1.0 没有加入协议。

## M5 —— 审计 v2

| 功能 | 如何调用 | HTTP |
|---|---|---|
| **链状态** —— 事件数与链的裁决 | `riscdom audit status` | `GET /v0/audit/status` |
| **事件列表** —— 最新在前，可按 action 前缀过滤 | `riscdom audit events [--limit n] [--action-prefix <p>]` | `GET /v0/audit/events` |
| **导出** —— 整条链或单次运行的切片，输出 JSONL | `riscdom export audit-jsonl --out <f>` / `export run-audit <run_id> --out <f>` | `POST /v0/audit/export`、`POST /v0/runs/export` |
| **独立验证** —— 在运行中的应用之外校验链 | `audit-verify <db> --runs` → `Intact { … }` / `Broken { … }` | *（本地二进制）* |
| **告警阈值** —— 设置审计告警 | `riscdom audit alert set …` | `POST /v0/audit/alert` |
| **冲突记录** —— 记录某次分叉已被查看（不指认任何一方） | `riscdom audit resolve <segment_id> [--note <t>]` | `POST /v0/audit/conflicts/{id}/resolve` |

事件行只追加：触发器拒绝 `UPDATE`/`DELETE`。这正是 [demo.md](demo.md) 第 5 步让评审者**在一份
拷贝上**打破、并眼看校验器判为 `Broken` 的性质。

## M6 —— 跨设备派发

| 功能 | 如何调用 | HTTP |
|---|---|---|
| **把任务派发到另一节点** | `riscdom tasks dispatch --target <node_id> --input "<task>"` | `POST /v0/tasks` |
| **任务身份连续性** —— `task_id` 随任务跨过节点边界 | *（派发信封的一部分）* | —— |
| **跨链验证** —— 送达的分段按其自己的末帧与锚点链接校验 | *（审计流的一部分）* | —— |

**M 的跨区域层**（一组 LAN 级 M、一个 `--config` 文件）**不在 v1.0** —— 见
[known-issues.md](known-issues.md)。

## M7 —— 生态

| 功能 | 如何调用 |
|---|---|
| **Rust SDK** —— 控制平面的类型化客户端 | `sdk/rust`（`riscdom-sdk`） |
| **TypeScript SDK** —— 端点表、客户端与流 | `sdk/typescript` |
| **参考监管器与派发器**（Python） | `examples/python/supervisor.py`、`examples/python/dispatch.py` |
| **备份工具** —— 把节点状态导出为一个加密包 | `riscdom-backup` |
| **预算** —— 文档化的性能上限 | [performance-budget.md](../performance-budget.md) |

## M8 —— 冻结与发布

| 功能 | 位置 |
|---|---|
| **API 冻结声明** —— 三个冻结面 | [api-compatibility.md](../api-compatibility.md) §1 |
| **`/v0/` 是 v1.0 的路径前缀** —— 它不在 v1.0 处移动 | [api-compatibility.md](../api-compatibility.md) §5 |
| **v1.0.0 已发布** —— 版本、tag、发布说明 | tag `891c237`；[RELEASE_NOTES.md](../../RELEASE_NOTES.md) |
| **控制平面已自成一座仓** | `riscdom-server`（M8-4a） |

## 故意不列的

- [roadmap-v1.0.md](../roadmap-v1.0.md) 中标 `[default]` 或 `[open]` 的任何东西 —— 例如把 Python
  作为访客语言、会话库的 WAL 模式、GUI 开关的点击区。
- **M8-4b / M8-4c / M8-4d** 的拆分工作（桌面仓、本仓收尾、对账）—— 已设计，未完成。
- 仍停留在规格而非代码的可观测性条目 —— 见 [known-issues.md](known-issues.md)。
