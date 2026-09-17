[English](RELEASE_NOTES.md) | 中文

# 智芯城 RiscDom v0.3.1

**v0.3.1 是 v0.3.0 之上的缺陷修复版**：修掉了验收 v0.3.0 时发现的 5 个缺陷——快照恢复忽略手动配置的
QEMU、QEMU 已退出但 VM 徽标仍显示运行中、串口读取在缓冲区已满时报"无输出"、阅读历史时聊天被拉回底部、
窄窗下串口列被挤出屏幕。

**面向桌面的 AI 原生 RISC-V 沙箱**：在 QEMU RISC-V 裸机沙箱里，AI 拥有**虚拟内核级权限**——
它可以写 C / RISC-V 汇编、编译、运行、读串口并迭代。全过程**可审计、可回滚**，人类保留根权限。

*（面向开发者的逐条变更见 [CHANGELOG.md](CHANGELOG.md)；本文面向使用者。）*

## 本版亮点

- **快照恢复会使用你配置的 QEMU**：恢复路径此前自建 VM 配置并静默回落到自动探测，可能用与
  「设置 → 工具链」所配不同的二进制引导。现在运行与恢复共用同一路径注入。
- **VM 徽标不再说谎**：QEMU 进程已退出（guest 关机，或 QEMU 被杀/崩溃）后残留的句柄会让顶栏一直显示
  "VM 运行中"。现在同时检查子进程，并丢弃死句柄。
- **`read_serial` 会交出 guest 打印的内容**：持续输出的 guest（心跳行、繁忙日志）永远等不到静默期，
  此前工具会在缓冲区已满时回答"无串口输出"。现在只有缓冲区真的为空才报空。
- **阅读历史不再被打断**：任务结束后此前会强制把聊天拉到底，即使用户已上翻。现在保持你的位置并提供
  "回到最新"按钮，与串口侧一致。
- **布局适配任意窗口**：聊天列此前按固定宽度夹紧，窄窗下会把串口列挤出屏幕。现在拖动上限跟随窗口，
  串口列最小宽度自适应。

## 验证

- `cargo test`：**181 passed / 0 failed / 7 ignored**（58 个测试套件）。含 3 个新增回归测试——快照恢复
  必须走所配置的 QEMU 路径、自行关机的 guest 不得报为运行中、持续打印的 guest 必须返回其输出。
  7 个 ignored 是设计上需要真实 API key 或真实 QEMU 引导的用例。
- `cargo fmt --check`、`cargo clippy -D warnings`、`cargo check`（可移植 crate 与 `ui/src-tauri`）、
  `npm run build`（tsc + vite）与双语文档检查全部通过。
- 守护两个 UI 修复的探针已进入本地门禁（`scripts/gate.ps1` / `scripts/gate.sh`）。

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
