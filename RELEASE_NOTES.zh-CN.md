[English](RELEASE_NOTES.md) | 中文

# 智芯城 RiscDom v0.3.0

**v0.3.0 是「布局 + 环境」版本**：主视图改为聊天 + 串口双栏，设置移到独立页面；两个前置条件——
QEMU 与 RISC-V GCC——现在都在应用内解决，不必再回到终端。

**面向桌面的 AI 原生 RISC-V 沙箱**：在 QEMU RISC-V 裸机沙箱里，AI 拥有**虚拟内核级权限**——
它可以写 C / RISC-V 汇编、编译、运行、读串口并迭代。全过程**可审计、可回滚**，人类保留根权限。

*（面向开发者的逐条变更见 [CHANGELOG.md](CHANGELOG.md)；本文面向使用者。）*

## 本版亮点

- **双栏布局**：主视图改为聊天与串口并排，设置移至独立页面（Esc 返回聊天）。
- **一键下载 RISC-V GCC**：RiscDom 替你拉取 xPack 裸机 GCC，用 SHA-256 校验，解压时防 Zip Slip，
  下载中可随时取消。
- **QEMU 自动探测 + 手动指定**：RiscDom 自己找到 `qemu-system-riscv64`
  （`RISCDOM_QEMU` → 常见安装路径 → `PATH`），也可在 **设置 → 工具链** 手动指定路径——重启后
  仍然生效，并注入 AgentLoop 真正起作用。
- **VM 状态徽标**：顶栏始终显示 VM 是否在运行，跨 run 保持可见。
- **AI 不再主动停止 VM**：任务结束后 guest 继续运行——prompt、`stop_vm` 工具描述与 VM 状态徽标
  三重保证。
- **`read_serial` 完整读取输出**：返回前等待约 150ms 静默期，首字节不再被截断。
- **自动滚动跟随输出**：聊天与串口始终贴住最新一行，用户上翻不被打断，改为出现"回到最新"浮按钮。

## 前置条件

**必须安装 RISC-V 裸机 GCC 才能编译任何东西。** RiscDom 会自动探测
（`RISCDOM_RISCV_GCC` / `RISCV_GCC` → 常见安装路径 → `PATH`）；全部未命中时，它会明确告诉你
搜过哪些路径、怎么修（包括在 **设置 → 工具链** 手动指定路径）。请安装 xPack
[riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases)
或等价的 `riscv64-unknown-elf-gcc`；三平台步骤见
[docs/toolchain-setup.md](docs/toolchain-setup.md)。

**启动 guest 需要 QEMU（`qemu-system-riscv64`），它既不随包分发、也不代为下载。** 可用

```text
winget install SoftwareFreedomConservancy.QEMU
```

安装，或从 <https://www.qemu.org/download/#windows> 下载安装包，或用 `RISCDOM_QEMU` 指向已有副本。
应用也会探测常见安装路径与 `PATH`，**设置 → 工具链** 支持手动指定路径。详见
[docs/qemu-setup.md](docs/qemu-setup.md)。

## 已知限制

- **以 Windows 为主平台**：macOS 与 Linux 尚未验证；QMP 与串口走 TCP（无 Unix socket）。
- **QEMU 不捆绑、不代下载**：不同于 RISC-V GCC，QEMU 需要你自己安装。
- **无原生文件选择器**：Tauri dialog 插件尚未引入，因此手动指定编译器或 QEMU 时使用文本输入。
- **无增量快照、无加密**：每次快照都是全量状态流；会话存储为本地明文 SQLite。
- **同一时间一台 VM**：界面驱动单台宿主持有的 VM，不支持多 VM 并行。

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
