[English](RELEASE_NOTES.md) | 中文

# 智芯城 RiscDom v0.2.0

**面向桌面的 AI 原生 RISC-V 沙箱**：在 QEMU RISC-V 裸机沙箱里，AI 拥有**虚拟内核级权限**——
它可以写 C / RISC-V 汇编、编译、运行、读串口并迭代。全过程**可审计、可回滚**，人类保留根权限。

*（面向开发者的逐条变更见 [CHANGELOG.md](CHANGELOG.md)；本文面向使用者。）*

## 本版亮点

- **自带密钥，任意兼容模型**：LLM 客户端重构为通用的 OpenAI 兼容客户端，内置 DeepSeek（默认）/
  OpenAI / Ollama / LM Studio / 自定义预设。在设置栏选一个服务商，`base_url` 与 `model`
  会自动填好。
- **离线可用、无需 key**：指向本地模型（Ollama / LM Studio）后，整条链路——QEMU + RISC-V GCC +
  审计 + 沙箱 + 本地 LLM——**全程无网络、无需任何 API key**。
- **系统钥匙串持久化**：可选择把 key 存入 Windows Credential Manager / macOS Keychain /
  Linux Secret Service。key 本身绝不写入磁盘、审计日志或前端；钥匙串不可用时静默降级为仅内存。
- **流式响应**：回答逐字出现，不再是整块吐出。
- **会话持久化**：对话自动保存在本地，重启后可恢复；恢复只注入历史消息，**不重放**工具调用。
- **真实快照**：通过 QMP 迁移 + 本地 TCP 中继，把 VM 的**真实状态**存为 `.mig`，并可从界面
  恢复（带二次确认）。
- **串口跨 run 连续**：串口终端在多次运行之间持续累积，可以看到比单轮更长寿的 guest；
  VM 归宿主持有并复用同一台。
- **双语文档**：所有文档都有英文（正本）与中文（`*.zh-CN.md`）两版，并由本地门禁的自动检查
  保持同步。

## 已知限制

- **以 Windows 为主平台**：QMP 与串口走 TCP；Unix socket、macOS 与 Linux 尚未实现。
- **真实快照走 TCP 中继**：Windows + QEMU 11.1.0 下 `migrate` → `file:` 不可用，因此由本地中继
  落盘。同名快照**拒绝覆盖**；恢复时的 `-kernel` 取工作区内最新的 `*.elf`（迁移流会覆盖内存，
  内核仅用于让 QEMU 起机）。
- **无增量快照、无快照/会话加密**：每次快照都是全量状态流；会话存储为本地明文 SQLite。
- **同一时间一台 VM**：界面驱动单台宿主持有的 VM。

## 快速开始

1. **准备环境**——Windows 10/11、QEMU（`qemu-system-riscv64`）、RISC-V 裸机 GCC
   （`riscv64-unknown-elf-gcc`）、Node 20+ 与 Rust 工具链（详见 [ENVIRONMENT.md](ENVIRONMENT.md)）。
2. **跑起来**——`cd ui && npm install && npm run tauri dev`。
3. **配置模型**——在设置栏选服务商并填入 API key（或选 Ollama / LM Studio，完全本地、无需 key），
   然后输入例如：*"写一个 RISC-V 裸机 Hello World，编译、运行并把串口输出读回来"*。

## 安全声明

本项目**不提供、不代管、不内置**任何 API key：所有模型访问均为自带密钥（BYOK）。key 不离开你的机器；
审计日志位于 AI 之外且 append-only；任何内容都不会被上传。完整声明与漏洞报告流程见
[SECURITY.md](SECURITY.md)。

## 许可证

[Apache License 2.0](LICENSE)。
