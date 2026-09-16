[English](PROJECT_CONSTITUTION.md) | 中文

# PROJECT_CONSTITUTION.md — 智芯城 RiscDom 完整项目宪法

> 本文件由 `AGENTS.md` 扩展而来，是项目的完整治理与架构说明。
> `AGENTS.md` 为每轮注入的核心宪法摘要；本文件为可检索的完整版本。
> 本文件内容不得与 `AGENTS.md` 冲突；如冲突，以 `AGENTS.md` 为准。

## 1. 项目身份

- 中文名：智芯城
- 英文名：RiscDom
- 定位：桌面应用。AI 在 RISC-V 虚拟沙箱中拥有虚拟内核级权限，可写 C/汇编、操控虚拟硬件。
- 全过程可审计、可回滚。人类保留根权限。边缘能力插件化。

## 2. 核心口号

自由在边界内，审计在 AI 外，根权限在人类。

## 3. 项目宪法（不可协商）

1. 宿主监控层不可被 AI 修改。
2. 审计日志在 AI 之外，append-only，不可关闭。
3. 能力默认拒绝，插件声明权限。
4. 人类永远有暂停、回滚、断网、终止权。
5. AI 民主是实验变量，不是 MVP 必做。
6. MVP 阶段 AI 在沙箱内只能生成 C 和 RISC-V 汇编。
7. AI 接入先用 API keys。

## 4. 架构分层

自外向内，权限逐层收紧：

1. **人类层（Human / Root）**
   根权限持有者。通过 UI 进行暂停、回滚、断网、终止。任何时刻可中断系统。
2. **宿主监控层（Host Supervisor，Rust + Tauri）**
   不可被 AI 修改。负责进程生命周期、QEMU 控制、能力仲裁、与 UI 通信。
3. **能力代理层（Capability Broker）**
   默认拒绝。所有对宿主资源的访问必须由插件显式声明并经人类批准。
4. **审计层（Audit，Rust）**
   位于 AI 之外，append-only，基于 SQLite + hash chain。不可关闭、不可被 AI 篡改。
5. **沙箱层（Sandbox，Rust + QEMU RISC-V）**
   QEMU `virt` 机器上运行裸机 ELF。AI 在其中拥有虚拟内核级权限，但仅限沙箱内。
6. **AI 代理层（Agent，Rust）**
   LLM 循环 + 工具调用。MVP 阶段只能生成 C11 与 RISC-V RV64GC 汇编。

## 5. 语言限制

MVP 沙箱内 AI 只能生成：

- C11：`-ffreestanding -nostdlib -march=rv64gc -mabi=lp64d`
- RISC-V RV64GC 汇编

禁止：C++、Rust、Zig、Python。

## 6. 审计事件类型（Audit Event Types）

所有事件写入 append-only 日志，字段至少包含：`id`、`timestamp`、`actor`、`kind`、`payload`、`prev_hash`、`hash`。

- `vm.start` / `vm.stop` — 虚拟机启动/停止
- `vm.snapshot.save` / `vm.snapshot.load` — 快照保存/回滚
- `vm.serial.write` / `vm.serial.read` — 串口读写
- `agent.prompt` / `agent.completion` — LLM 请求/响应
- `agent.tool_call` — AI 工具调用
- `capability.request` / `capability.grant` / `capability.deny` — 能力申请/授予/拒绝
- `sandbox.file.write` / `sandbox.file.read` — 沙箱内文件操作
- `human.pause` / `human.resume` / `human.rollback` / `human.terminate` — 人类干预
- `system.config.change` — 配置变更
- `audit.verify` — 审计链校验

事件分类（actor）：`human`、`host`、`agent`、`sandbox`、`system`。

实现说明（2026-09-14 更新）：上述事件由 `audit` crate 落地——append-only SQLite
（`BEFORE UPDATE` / `BEFORE DELETE` 触发器硬保证）+ SHA-256 hash chain，无任何
UPDATE / DELETE API、无关闭审计的开关。`sandbox` 通过 `audit::AuditSink` 写入，
依赖方向为 `sandbox → audit`。`audit::FileAuditSink` 仅作示例实现保留。

## 7. 开发纪律

- 每个动作写入审计事件。
- 只改指定目录，不越界。
- 输出测试和 diff。
- 宁可慢，确保每一步可验证、可回滚。

## 8. 红线

- 绝不窃取私人数据。
- 未经询问，不执行破坏性命令。
- 变更配置前先检查现有状态，默认保留/合并现有内容。
- 优先使用 trash 而非 rm。
- 如有疑问，先询问。

## 9. v0.1 完成情况

- [DONE] 宿主监控层（`host`）不可被 AI 修改；前端只能经 Tauri command 访问。
- [DONE] 审计日志在 AI 之外、append-only、不可关闭（`audit`：SQLite 触发器 + hash chain + `audit-verify`）。
- [DONE] 能力默认拒绝（`agent::WorkspacePolicy`，防穿越 + 扩展名白名单）。
- [DONE] 人类可暂停/回滚/终止（VMP 生命周期可控；VM 归 host 持有，快照支持真实保存/恢复）。
- [DONE] AI 接入使用 API keys（`DEEPSEEK_API_KEY`，仅内存，不落盘/不进审计）。
- [DONE] 沙箱内 AI 只生成 C11 与 RV64GC 汇编（`agent::tools` 白名单 + 编译器参数）。
- [DONE] 所有动作写入审计（sandbox / agent / host 均产生事件）。
- [DONE] 真实快照（TCP 迁移 + 本地文件中继）：保存/恢复可用；旧“重启式”降级保留兼容。
- [DONE] 串口事件由 sandbox 主动推送；串口订阅**跨 run 常驻**（20b）。

## 10. 路线图

### v0.2 新增 — 多模型接入与密钥安全

a. **LLM 客户端重构：`DeepSeekClient` → `OpenAiCompatClient`**
   - `base_url` / `api_key` / `model` 全部用户可配
   - 保持 OpenAI 兼容协议，DeepSeek 降级为默认预设之一
   - 理由：DeepSeek 兼容 OpenAI 协议，改造成本低，收益是解锁所有兼容服务商

b. **内置服务商预设**
   - DeepSeek（默认）：`https://api.deepseek.com`，`deepseek-chat`
   - OpenAI：`https://api.openai.com/v1`，`gpt-4o-mini`
   - Ollama（本地）：`http://localhost:11434/v1`，`qwen2.5-coder`，无需 key
   - LM Studio（本地）：`http://localhost:1234/v1`，用户指定
   - 自定义：用户填 `base_url` 与 `model`
   - UI：服务商下拉，选预设自动填 `base_url` / `model`

c. **本地离线模型支持**
   - Ollama / LM Studio 兼容 OpenAI 协议，复用同一客户端
   - 离线模式 = QEMU + RISC-V GCC + 审计 + 沙箱 + 本地 LLM，全程无网络
   - 对隐私敏感、教育、断网场景有价值

d. **无 key 降级体验**
   - 无 key 时不崩溃，UI 显示“请配置模型”引导
   - 自动探测 `localhost:11434`，发现 Ollama 提示“检测到本地模型，是否使用？”
   - 公开后新用户第一次启动不能直接报错

e. **API key 持久化：OS keyring**
   - Windows Credential Manager / macOS Keychain / Linux Secret Service
   - Rust 侧用 `keyring` crate
   - 绝不用 `localStorage` / 明文文件 / `.env`
   - 当前“仅内存”方案降级为 fallback（keyring 不可用时）

f. **公开前安全清单**
   - `.env.example` 只放占位符
   - `.gitignore` 覆盖 `.env` / `*.db` / `*.jsonl`
   - CI 加 secret scanning（gitleaks 或 GitHub 原生）
   - README 明确写“本项目不提供 API key，请自备”
   - 审计日志 LLM 请求只记 hash 和 token 数（已做）

### v0.2 其他

g. 给 `AgentLoop` 暴露最小串口访问接口（已完成，见 15b）
h. **[DONE: A′]** QEMU 真实快照：以 **TCP 迁移 + 本地文件中继**实现（`migrate` → file 在
   Windows + QEMU 11.1.0 不可用，见 `sandbox/docs/snapshot-experiment.md`）；
   **20b/20c/20d 完成**：VM 归 `AppState::vm_slot`、串口订阅跨 run 常驻，UI 可保存/恢复。
   残留限制：恢复用的 `-kernel` 取工作区内最新的 `*.elf`；不做多 VM 并行（v0.3 评估）。
i. 流式 LLM 响应
j. 会话持久化
k. **[DONE]** `real_api` 测试补 `verify_chain` 断言（阶段 21：文件 SQLite + 独立句柄验证链完整）
l. host 串口轮询改为 sandbox 主动回调

- gdbstub 接入（调试）
- Unix socket（macOS / Linux）与 virtio 设备
- 审计日志分片与远程备份

m. **[DONE]** 公开前完成中英双语文档
   - README / CHANGELOG / PROJECT_CONSTITUTION / AGENTS / Release notes 双语
   - 英文为主文档（GitHub 默认展示），中文为 `*.zh-CN.md`
   - 顶部加语言切换链接
   - 内容不逐字对应：英文版更简练，中文版保留原文风格
   - LICENSE 无需翻译，保持英文法律原文

### v0.3 状态

- [DONE] **布局重构**：主视图改为聊天 + 串口双栏，设置移至独立的 tab 页
  （模型 / 工具链 / 快照 / 审计 / 插件），Esc 返回聊天。
- [DONE] **一键下载 RISC-V GCC**（xPack）：SHA-256 校验、Zip Slip 防护、可取消，并写入审计。
- [DONE] **QEMU 自动探测与手动路径**：`RISCDOM_QEMU` → 常见路径 → `PATH`，并可在
  「设置 → 工具链」手动指定路径；持久化到 `settings.json`，注入 AgentLoop 真正生效。
- [DONE] **VM 状态徽标**：顶栏显示，跨 run 保持可见。
- [DONE] **prompt 改进**：system prompt 全英文，AI 不再主动停止 VM
  （prompt + `stop_vm` 工具描述 + VM 徽标三重保证）。
- [DONE] **自动滚动**：聊天与串口跟随最新输出；用户上翻不被打断（出现“回到最新”浮按钮）。
- [DONE] **`read_serial` 静默期**：返回前等待约 150ms 静默，首字节不再被截断。
- [DONE] **快照中继端口重试**：恢复时在中继端口失败（QMP 10054 / 绑定失败）重试，最多 3 次。
- [DONE] **门禁稳定性**：`start_vm` 的端口 TOCTOU 重试。

### v0.4 路线图

1. **QEMU stdio（方案 3）与统一的 relay 端口租约**：彻底消除 QMP/串口的 TCP 端口依赖，
   relay 端口由单一租约分配。
2. **阶段 5c-3：端到端失败路径诊断日志**（可选）：e2e 运行失败时提供更完整的诊断。
3. **引入 `tauri-plugin-dialog`**：用原生文件选择器选择工具链 / QEMU 路径，替代
   `window.prompt` 文本输入。
4. **QEMU 一键下载或捆绑评估**（含 GPL 合规审查）。
5. **QEMU × RISC-V GCC 版本兼容性校验**：不兼容组合时拒绝或告警。
6. **macOS / Linux 支持与多 OS CI 矩阵**。
7. **多 VM 并行**：同一时间驱动多台宿主持有的 guest。
8. **增量快照 + 加密**。
9. **会话加密 / 导出 / 搜索**。
10. **多 AI 社会与 `Governance` trait**（宪法中的实验变量）。
11. **主题切换；代码注释双语**。
