[English](README.md) | 中文

# RiscDom 文档导航

> **适用于 v0.9（在 v1.0 之前不稳定）。** 全仓一页：每一份 Markdown 文件，按读者分组。[decisions.zh-CN.md](decisions.zh-CN.md) §21 定了五个受众（内核开发者、发行集成者、管理员、终端用户、贡献者），并要求每份文档有其一；本页让这条规定一眼可查。

怎么读一行：链接、这份文档是做什么的、它的**状态**——*活跃*（随代码同步，靠某个核对器或改它的那个批次）、*快照*（某一刻的记录，刻意不重写）、*历史*（只追加的日志）——以及它自己声明的适用版本。文档以对形式双语：`X.md` ↔ `X.zh-CN.md`，缺对偶会被 `scripts/check-bilingual.sh` 拦下。

## 1. 从这里开始

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [README.md](../README.md) — [中文](../README.zh-CN.md) | RiscDom 是什么、一屏架构、快速开始、怎么构建与测试。 | 活跃 |
| [ENVIRONMENT.md](../ENVIRONMENT.md) — [中文](../ENVIRONMENT.zh-CN.md) | 本项目在其上验证过的开发机：操作系统、工具链版本、路径。 | 快照（本机） |
| **docs/README.md**（本页） — [中文](README.zh-CN.md) | 导航：有什么、给谁看、什么是历史。 | 活跃 |

## 2. 内核开发者

内核是 `agent` + `sandbox` + `audit`，外面包着 `host-core`（可移植半）与 `host-tauri`（桌面外壳）。这些文档讲机器本身：它怎么搭起来、为什么这样、以及定下过什么。

### 2.1 设计记录

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [decisions.md](decisions.md) — [中文](decisions.zh-CN.md) | 决策账本：每个已定问题连同日期、决策、理由与影响。§21 就是本页服务的那条规矩。 | 活跃（只追加） |
| [architecture-evolution.md](architecture-evolution.md) — [中文](architecture-evolution.zh-CN.md) | **v0.7 快照**：架构怎么走到这里，以及随后的计划。 | 快照 —— **历史，不重写** |
| [roadmap-v1.0.md](roadmap-v1.0.md) — [中文](roadmap-v1.0.zh-CN.md) | 纲领：v1.0 冻结内核 API 并兑现三层，每条决策标明已定、默认或待定。 | 活跃（纲领 —— 随里程碑推进重写） |
| [api-compatibility.md](api-compatibility.md) — [中文](api-compatibility.zh-CN.md) | v1.0 规范（M1），**面向发行集成者**：冻结什么、什么可改而不需冻结、持久化数据怎么迁移、升级怎么跨大版本。 | 活跃（v1.0 规范） |
| [error-model.md](error-model.md) — [中文](error-model.zh-CN.md) | v1.0 规范（M1），面向内核开发者与集成者：错误分类、哪些可重试、cause 链，以及线上信封与 `DispatchError` 的映射。 | 活跃（v1.0 规范） |
| [security-model.md](security-model.md) — [中文](security-model.zh-CN.md) | v1.0 规范（M1），**面向管理员**：每份机密住在哪、一个 capability 值多少、威胁模型边界，以及披露政策。 | 活跃（v1.0 规范） |
| [upgrade.md](upgrade.md) — [中文](upgrade.zh-CN.md) | v1.0 规范（M1），**面向发行集成者**：成文的升级流程 —— 一次升级改了什么、每个版本标记住在哪、操作顺序，以及怎么跨大版本。 | 活跃（v1.0 规范） |
| [plugin-interface.md](plugin-interface.md) — [中文](plugin-interface.zh-CN.md) | v1.0 规范（M3），**面向插件作者**：被冻结的插件接口 —— 传输、机制层与语义层、capability 声明与 manifest。 | 活跃（v1.0 规范） |
| [connection.md](connection.md) — [中文](connection.zh-CN.md) | v1.0 规范（M4），面向内核开发者与部署者：连接层 —— 节点身份与签名已冻结，发现、房间、跨区域服务器与审计 digest 仍 deferred。 | 活跃（v1.0 规范） |
| [handoff.md](handoff.md) — [中文](handoff.zh-CN.md) | 跨对话交接：§1 是易变快照，§2–12 是新会话不得破坏的稳定约束。 | 活跃（§1）、稳定（§2–12） |
| [run-provenance.md](run-provenance.md) — [中文](run-provenance.zh-CN.md) | 设计：一次运行记录下关于自己的什么，以及指纹为什么长这样。 | 快照（v0.4 批次 1a） |
| [multi-agent-foundation.md](multi-agent-foundation.md) — [中文](multi-agent-foundation.zh-CN.md) | v0.8 定下的四个形状：一台机器上多个进程时的身份、按 agent 的快照、派发抽象、共享 workspace。 | 快照（v0.8） |

### 2.2 操作这台机器

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [preflight.md](preflight.md) — [中文](preflight.zh-CN.md) | 环境预检：查什么、怎么缓存它的结论。 | 活跃 |
| [e2e-debugging.md](e2e-debugging.md) — [中文](e2e-debugging.zh-CN.md) | 一次端到端运行出了问题，怎么调试。 | 活跃 |
| [qemu-stdio.md](qemu-stdio.md) — [中文](qemu-stdio.zh-CN.md) | QEMU 的端口依赖怎么被消掉，以及取而代之的 relay 端口租约。 | 快照（v0.4 #1） |
| [qemu-distribution.md](qemu-distribution.md) — [中文](qemu-distribution.zh-CN.md) | 捆绑 QEMU 还是一次下载：这个决定，以及为什么不 pin 版本。 | 快照（v0.4 #4） |
| [golden-path.md](golden-path.md) — [中文](golden-path.zh-CN.md) | v0.5 人工发布走查的设计提案。 | 快照（v0.5） |
| [golden-path-checklist.md](golden-path-checklist.md) — [中文](golden-path-checklist.zh-CN.md) | 走查者逐步填写的清单。 | 活跃 |
| [manual-acceptance.md](manual-acceptance.md) — [中文](manual-acceptance.zh-CN.md) | 人工验收走查的工作顺序：装、起、驱动、核对，以及出问题时回传什么。 | 活跃 |
| [sandbox/docs/snapshot-experiment.md](../sandbox/docs/snapshot-experiment.md) — [中文](../sandbox/docs/snapshot-experiment.zh-CN.md) | 快照可行性实验，以及它测到了什么。 | 快照（阶段 18a） |

### 2.3 各 crate

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [agent/README.md](../agent/README.md) — [中文](../agent/README.zh-CN.md) | agent 运行时：模块、八个工具（schema 见 [tool-schema-executor.zh-CN.md](tool-schema-executor.zh-CN.md)）、它写下的审计事件、loop 与上下文。 | 活跃 |
| [sandbox/README.md](../sandbox/README.md) — [中文](../sandbox/README.zh-CN.md) | QEMU 生命周期、串口捕获、快照（含 MVP 降级）与 relay。 | 活跃 |
| [audit/README.md](../audit/README.md) — [中文](../audit/README.zh-CN.md) | 只追加存储、哈希链、事件词汇表，以及 `audit-verify`。 | 活跃 |
| [host-core/README.md](../host-core/README.md) — [中文](../host-core/README.zh-CN.md) | 宿主的可移植半：模块、与 `host-tauri` 的关系、它的约束（不碰 Tauri）。 | 活跃 |
| [host-tauri/README.md](../host-tauri/README.md) — [中文](../host-tauri/README.zh-CN.md) | 桌面外壳：命令、事件、钥匙串、快照、会话持久化、人工验证。 | 活跃 |
| [worker/README.md](../worker/README.md) — [中文](../worker/README.zh-CN.md) | 执行者进程与监工半边——含远程执行者句柄（v0.9 E4）。 | 活跃 |
| [net/README.md](../net/README.md) — [中文](../net/README.zh-CN.md) | 连接层（v1.0 M4）：今天在盘上的是节点身份；签名、发现、房间与跨区域服务器随各冻结节落地。 | 活跃 |

## 3. 发行集成者

写控制平面客户端的人，或把它装进别的东西里出货的人。规范表是 API 与事件两份文档；指南是可跑的走查。

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [control-plane-api.md](control-plane-api.md) — [中文](control-plane-api.zh-CN.md) | **规范表**：每个端点、它的方法、capability、请求与应答。 | 活跃 |
| [control-plane-events.md](control-plane-events.md) — [中文](control-plane-events.zh-CN.md) | 事件流：信封、事件词汇表、过滤、帧。 | 活跃 |
| [control-plane-client-guide.md](control-plane-client-guide.md) — [中文](control-plane-client-guide.zh-CN.md) | 怎么写客户端：第一次调用、错误、订阅事件流、CLI、用 AI 监工驱动它、写一个远程执行者句柄。 | 活跃 |
| [tool-schema-control-plane.md](tool-schema-control-plane.md) — [中文](tool-schema-control-plane.zh-CN.md) | 每个端点写成一条 OpenAI 风格工具定义——监工的 `tools[]`，可直接粘贴。 | 活跃（有核对） |
| [tool-schema-executor.md](tool-schema-executor.md) — [中文](tool-schema-executor.zh-CN.md) | 执行者的模型可用的八个工具，就是内核发送的那个数组。 | 活跃（有核对） |
| [config-schema.md](config-schema.md) — [中文](config-schema.zh-CN.md) | **配置 schema**（v1.0 M7f）：`settings.json`、`peers.json`、`rooms.json` 的每一个字段、类型、是否可缺，以及每种格式怎么版本化。 | 活跃（v1.0 规范） |
| [examples/python/README.md](../examples/python/README.md) — [中文](../examples/python/README.zh-CN.md) | 可跑的参考监工：三个端点、仅标准库、自带离线 `--self-test`。 | 活跃（有自证） |
| [server/README.md](../server/README.md) — [中文](../server/README.zh-CN.md) | 作为一个程序的控制平面：构建、运行、端点、事件流、鉴权——以及还没实现的东西。 | 活跃 |

## 4. 管理员

运行一个节点的人：它需要什么、它会拒绝什么、以及安全姿态写在哪儿。

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [SECURITY.md](../SECURITY.md) — [中文](../SECURITY.zh-CN.md) | 怎么报告漏洞、什么在范围内、以及关于密钥的承诺。 | 活跃 |
| [server/README.md](../server/README.md) — [中文](../server/README.zh-CN.md) | 怎么启动控制平面、它的绑定点、它的 token，以及 `--no-auth`。 | 活跃 |
| [qemu-setup.md](qemu-setup.md) — [中文](qemu-setup.zh-CN.md) | 安装节点需要的 QEMU（本项目从不捆绑它）。 | 活跃 |
| [observability.md](observability.md) — [中文](observability.zh-CN.md) | **可观测性契约**（v1.0 M7g）：结构化日志行、指标端点及其族、以及追踪 ID —— 外加 `task_id` 缺口与它怎么合上。 | 活跃（v1.0 规范） |
| [performance-budget.md](performance-budget.md) — [中文](performance-budget.zh-CN.md) | **性能预算**（v1.0 M7h）：§18 的四个数字（VM 启动、派发往返、内存、日志增长）写成规格 —— 每个从哪里量到哪里、在什么条件下、以及预算怎么核对。 | 活跃（v1.0 规范） |
| [multi-repo.md](multi-repo.md) — [中文](multi-repo.zh-CN.md) | **跨两个仓库工作**（v1.0 M7i）：`riscdom-adminapp` 如何与本仓并立、如何取得内核（git 依赖钉 tag）、继承什么、自己拥有什么，以及仍未定的 CLA 问题。 | 活跃（v1.0 规范） |
| [toolchain-setup.md](toolchain-setup.md) — [中文](toolchain-setup.zh-CN.md) | 安装并指向 RISC-V 裸机编译器。 | 活跃 |
| [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md) — [中文](../THIRD_PARTY_NOTICES.zh-CN.md) | QEMU、下载来的工具链及其余：它们各自是独立的程序、各自的许可证。 | 活跃 |

## 5. 终端用户

只想把它跑起来的人。

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [cli/README.md](../cli/README.md) — [中文](../cli/README.zh-CN.md) | `riscdom` 命令行：每条命令、两种模式、token、输出、退出码。 | 活跃 |
| [ui/README.md](../ui/README.md) — [中文](../ui/README.zh-CN.md) | 桌面应用：布局、自动滚动、怎么运行、快照面板与审计页。 | 活跃 |
| [CHANGELOG.md](../CHANGELOG.md) — [中文](../CHANGELOG.zh-CN.md) | 逐版本改了什么，以及未发布的那一行。 | 历史（只追加） |
| [RELEASE_NOTES.md](../RELEASE_NOTES.md) — [中文](../RELEASE_NOTES.zh-CN.md) | 最新发行版（v0.9.9）的正文，含它的已知限制。 | 历史（逐发行版） |

## 6. 贡献者

| 文档 | 它是什么 | 状态 |
|---|---|---|
| [CONTRIBUTING.md](../CONTRIBUTING.md) — [中文](../CONTRIBUTING.zh-CN.md) | 怎么构建、怎么测（含 `--ignored` 的端到端测试），以及本仓提交必经的 commit 包装脚本。 | 活跃 |
| [PROJECT_CONSTITUTION.md](../PROJECT_CONSTITUTION.md) — [中文](../PROJECT_CONSTITUTION.zh-CN.md) | 完整宪法：原则、架构分层、审计事件类型、红线。 | 活跃 |
| [AGENTS.md](../AGENTS.md) — [中文](../AGENTS.zh-CN.md) | 本仓内 AI 会话的工作约定（核心宪法，每轮注入）。 | 活跃 |
| [CODE_OF_CONDUCT.md](../CODE_OF_CONDUCT.md) — [中文](../CODE_OF_CONDUCT.zh-CN.md) | 贡献者公约，以及怎么报告不可接受的行为。 | 活跃 |
| [CLA.md](../CLA.md) — [中文](../CLA.zh-CN.md) | 每份贡献都需要的贡献者许可协议。签署记录在 `signatures/version1/cla.json`。 | 活跃 |
| [walkthroughs/README.md](../walkthroughs/README.md) | 走查记录是什么、为什么不在 `docs/`、以及双语门禁为什么跳过这个目录。 | 活跃 |
| [walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md) | 一个人在一台机器上走黄金路径的记录，照当时发生的事情记下。 | 快照（刻意单语） |

## 7. 不在导航内，以及原因

- **`IDENTITY.md`、`SOUL.md`、`USER.md`**（仓根）是 agent 工作区的身份文件：它们由 AI 读，不给人类读者，且刻意单语——`scripts/check-bilingual.sh` 按文件名把它们排除。它们不是文档，本页不导航它们。
- **`LICENSE`** 是 Apache-2.0 正文，按设计保持英文，且被双语门禁排除。
- **`signatures/version1/cla.json`** 是 CLA 签署存储，属数据而非文档。

## 本页如何保持为真

已有两项检查覆盖了它的大部分，所以本页不会悄悄烂掉：

- 某份文档（含本页）缺对偶时，`scripts/check-bilingual.sh` 失败。
- 「相对链接全解析」的扫描——[行尾批次跑过的同一种]——是坏链接被抓住的方式；上面每条链接都有效。
- 本页引用的端点与工具计数住在 [control-plane-api.zh-CN.md](control-plane-api.zh-CN.md) §5，而 `server/src/routes.rs` 的测试会拿它与路由表比对，所以这里的一个数字必须被有意地改。

**没有**被检查的：新增文档是否被加进本页。若你加了一份，就把它的行加上——本页就是那份清单，而一份不在清单里的文档，是没人找得到的文档。
