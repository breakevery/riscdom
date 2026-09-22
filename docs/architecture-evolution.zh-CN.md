[English](architecture-evolution.md) | 中文

> **本文档为 v0.7.0 快照。** 此后的决策见 [decisions.zh-CN.md](decisions.zh-CN.md)。

# RiscDom 架构演进：从单机沙箱到 AI 协同运行时

版本：1.0 ｜ 日期：2026-09-21 ｜ 性质：架构演进说明，基于 v0.7.0 事实侦察

## 1. 背景与目标

v0.7.0 已发布。RiscDom 当前是“可信的 AI 代码执行沙箱”——让一个 AI 安全地干活。

目标形态是“AI 协同工作运行时”——让一群 AI 分工干活，人只在关键点介入。

本文档回答：从当前架构到目标形态，需要什么分层、哪些设计支持未来、哪些堵死了路、怎么分步走。

## 2. 当前架构事实

依赖图（无环 DAG）：

```text
ui/src-tauri → host → { agent, sandbox, audit }
                agent → { sandbox, audit }
                sandbox → audit
                audit → （叶子）
```

三个关键事实：

- Tauri 耦合极小：AppState（state.rs:366）代码里不引用 Tauri。耦合集中在两文件：commands.rs（52 命令，66 处引用）、events.rs（8 处）。其余 10 个模块不碰 Tauri。
- AppState 已是 per-instance：new() / in_memory() 可多次调用，字段全 per-instance，测试已多实例。
- 两处全局与两处单例挡住多 Agent：
  - paths.rs:9 APP_DATA_DIR: OnceLock<PathBuf>——只生效一次
  - state.rs:373 vm_slot: Arc<Mutex<Option<RiscVVirtualMachine>>>——一 AppState 一 VM
  - audit 单链、单 Arc<Mutex<AuditStore>>、single writer（sink.rs:59）
  - sandbox/relay.rs:34 HELD_PORTS 进程级端口注册表

## 3. 目标形态

从“被调用”到“持续运行”。

| 维度 | 当前 | 目标 |
|---|---|---|
| AI 角色 | 单一执行者，被动 | 监工 + 多执行者，主动 |
| 工作模式 | 一次任务 | 持续运行 |
| 人的角色 | 操作者 | 监督者 |
| 沙箱数量 | 单个 | 多个 |
| 设备范围 | 单机 | 多设备（远期） |

分阶段：单机多 Agent → 联机 → 远程管理。第一阶段不引入分布式复杂度。

## 4. 分层设计

```text
Layer 3  宿主      ui/src-tauri / 未来 server / CLI / 管理端
   ↓ 只依赖 Layer 2 稳定 API
Layer 2  内核门面   host 公共 API（Tauri 依赖隔离到 Layer 3）
   ↓
Layer 1  内核能力   agent（LLM 循环）、sandbox（VM）
   ↓
Layer 0  内核底座   audit（叶子）
```

这个分层已存在于依赖图里，只是未被命名。工作是把 Layer 2 的 Tauri 依赖挤到 Layer 3。

## 5. syscall 层：内核提供什么

原则：内核提供机制，发行版提供策略。

| 内核提供（机制） | 发行版决定（策略） |
|---|---|
| 启动/停止 VM、跑代码、取结果 | 何时启动、跑什么 |
| 审计链的追加与验证 | 审计策略（谁授权、保留多久） |
| 快照的创建/回滚/列表 | 快照调度 |
| 指纹的产出与对比 | 指纹的用途 |
| 权限检查 check(capability) | 权限策略 |
| 资源计量 | 资源分配 |
| 事件发射（EventSink） | 事件的消费 |

EventSink（events.rs:29）是现存设计里最接近正确形态的部分——内核发射，宿主消费。

## 6. 管理 API：统一控制平面

核心洞察：管理端（人管 AI）与协同（AI 管 AI）是同一套接口。

```text
        控制平面（HTTP + WebSocket）
         /          |          \
    人（手机/Web）  监工 AI    执行者 AI
    监督          派任务      收任务
```

对执行者而言，“命令来自监工 AI”和“命令来自人”没有区别——都是控制平面来的授权指令。审计链通过 agent_id 区分。

这是架构的核心简化。否则会做出两套控制通道，各自演化，最终冲突。

协议：HTTP（查询）+ WebSocket（推送）

暴露能力：状态查询、控制（暂停/恢复/终止）、审批、审计查询、资源视图

认证：现在只预留钩子，机制随 v0.8 权限中介定

约束：每个内核能力必须有对应的管理 API。官方管理程序不用它，它就是摆设。

## 7. 跨沙箱与跨设备协同

协同模型：

| 角色 | 职责 |
|---|---|
| 监工 | 分解目标、派任务、收结果、处理失败 |
| 执行者 | 在各自沙箱里干活 |
| 控制平面 | 任务流转、状态同步、指令下发 |
| 审计链 | 记录“谁让谁做了什么” |

两个层次，不同时做：

- 跨沙箱（同机，v0.9）：多进程（B2 模型），共享 workspace；控制平面走本地 IPC；审计链单链 + agent_id
- 跨设备（v1.0 后）：多机，网络连接；控制平面走网络；审计链必须演进 → audit v2

四个“现在要留的缝”：

1. Agent 身份全局唯一（agent_id 含设备标识 + 进程标识 + 序号）
2. 任务派发抽象（派发到“执行者句柄”，本地/远程对监工透明）
3. 审计链为多设备预留（现单链 + agent_id；跨设备时每设备一链 + 汇总链）
4. 控制平面协议设备无关（本地 IPC 与网络 HTTP 语义相同）

## 8. 已定决策

**决策 1：Tauri 解耦 —— A3 起步，A1 终态**

- 现在（A3）：host 单 crate，Tauri 依赖留在 commands.rs / events.rs，不再扩大
- 终态（A1）：拆 host-core + host-tauri，等做 server 宿主时执行

**决策 2：多 Agent 进程模型 —— B2（同机多进程）**

- 每 Agent 一进程，各一 AppState；共享 workspace，每进程设自己 data dir

**决策 3：审计链 —— 单链 + agent_id**

- 链结构、哈希公式不动；每 event 增加 agent_id；多进程写同一 SQLite，靠 WAL + 重试

**决策 4：三个堵死点列为 v0.8 前技术债**

1. APP_DATA_DIR: OnceLock → 可传入
2. vm_slot 单槽 → 每 AppState 一 VM
3. 审计链无 agent_id → 加字段

## 9. schema 演进：为多编程语言预留

当前：toolchain 单对象。目标：toolchain 变 map {c: {...}, rust: {...}}。

路径（现在设计，实现后置）：

- fingerprint_schema v1 → v2
- 旧 run 的 v1 指纹不动；新建 run 一律 v2
- diff_fingerprints 支持 v1↔v2：按共同字段比，新增字段标 null
- FINGERPRINT_FIELDS 顺序常量不变

不变量守住：链结构、哈希公式、历史行全不动。

实现时点：v1.1（Rust → Zig → Python 逐个加）。

## 10. 参考实现完整性

决定变更：i18n 铺开从“有意不做”改为“要做”。

理由：参考实现是第三方的样板。它自己不完整，第三方会以此为借口不做。内核可以简陋，参考实现不行。

范围：190 行硬编码 UI 字符串铺开为完整双语。独立批次，不与架构重估混做。

## 11. 演进路径与里程碑

| 阶段 | 交付物 | 可演示 |
|---|---|---|
| v0.7.0（已完成） | 跨平台可信沙箱 | 三平台包、黄金路径、审计链 |
| v0.8 | 技术债清理 + 单机多 Agent 雏形 | 一个监工 + 多个执行者进程 |
| v0.9 | 多 Agent 协同 + 管理 API + 官方管理程序仓库 | AI 分工完成复合任务，手机可看 |
| v1.0 | 内核 API 冻结 | 稳定 syscall 层 |
| 之后 | 联机 / 远程 | 多设备 |

每个阶段可独立交付、可独立演示。

## 12. 官方管理程序仓库

- 定位：内核级官方工具，与内核功能同步推进
- 性质：独立仓库，同源维护（类 Linux 的 coreutils / iproute2）
- 约束：每个内核能力必须有对应管理 API + 管理 UI
- 时点：先设计、v0.9 建仓

## 13. 风险与不确定

1. 没有参照物——“AI 持续自主工作”行业里没人真正解决
2. 分布式复杂度高——跨机是量级跃升，故留到后期
3. 时间尺度长——基础设施以年计
4. 资源需求——目标形态需团队规模

## 14. 明确不做

- 不为想象中的未来过度设计：留缝，不预实现
- 不改审计不变量
- 不引入分布式复杂度
- 不让内核膨胀
