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
