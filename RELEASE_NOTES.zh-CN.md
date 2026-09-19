[English](RELEASE_NOTES.md) | 中文

# 智芯城 RiscDom v0.4.0

**v0.4.0 是 v0.3.1 之上的功能版**：主题切换、原生文件选择器、能在真实路径上编译并启动一个极小 guest 的
环境预检、点名「卡在哪一步」的端到端失败报告，以及引导式的 QEMU 安装。底层还有：每一次 run 成为审计日志
里的一等公民、relay 端口改由进程级租约分配、CI 跑的就是你本地跑的那套 gate、RiscDom 会清理自己的临时
目录。

**面向桌面的 AI 原生 RISC-V 沙箱**：在 QEMU RISC-V 裸机沙箱里，AI 拥有**虚拟内核级权限**——
它可以写 C / RISC-V 汇编、编译、运行、读串口并迭代。全过程**可审计、可回滚**，人类保留根权限。

*（面向开发者的逐条变更见 [CHANGELOG.md](CHANGELOG.md)；本文面向使用者。）*

## 本版亮点

- **浅色 / 深色 / 跟随系统**：在外观页切换主题；样式表里所有颜色都是令牌，串口终端跟随同一套令牌。
- **原生文件选择器**：手动指定编译器或 QEMU 现在是真正的文件对话框，不再是文本框。
- **预检会证明你的环境真的能跑**：工具链或 QEMU 路径变化后，RiscDom 在**真实路径**上编译一个极小的
  guest 并启动它，然后报告四步里哪一步失败、该怎么修 —— 仅告警、按配置缓存、可记录「仍要继续」。
- **失败的 run 会自己说清楚**：端到端失败现在会打印报告，点名第一个失败的步骤、该步骤的原始输出、
  串口状态（包含「guest 从未打印任何东西」）与审计链结论 ——
  [docs/e2e-debugging.md](docs/e2e-debugging.md)。
- **QEMU 改为引导式安装**：RiscDom 不捆绑 QEMU，也不代下载。它明确告诉你该跑什么 —— 有 `winget` 时
  `winget install SoftwareFreedomConservancy.QEMU`，没有时给官网下载页 —— 然后由它去找到、记住并验证
  这份安装。为什么不是另外两条路：[docs/qemu-distribution.md](docs/qemu-distribution.md) §5。
- **每次 run 都是一等记录**：run 拥有 id、配置指纹与链上的审计区间；快照恢复自成一次 run；崩溃遗留的
  未闭合 run 会在下次启动时被标记为弃置 —— [docs/run-provenance.md](docs/run-provenance.md)。
- **relay 端口来自同一条租约**：快照恢复或启动 VM 时交给 QEMU 的端口由进程级租约保留，而不是「拿号即
  弃」，因此应用的两个部分不可能再拿到同一个端口 —— [docs/qemu-stdio.md](docs/qemu-stdio.md)。
- **CI 跑的就是你本地那套 gate**：`.github/workflows/ci.yml` 只装 Rust + Node 然后调
  `sh scripts/gate.sh` —— 「全绿」只有一份清单。CI 上确实存在的差异（runner 没有 QEMU、Linux 上缺 Tauri
  系统库）会被**打印出来**，而不是静默跳过。
- **RiscDom 会自己收尾**：启动时清扫过期的 `riscdom-*` 目录，构建结束即清理临时目录，另可用
  `scripts/clean-temp.ps1` / `scripts/clean-temp.sh` 手动清理（默认 dry-run）。

## 验证

- `cargo test`：**261 passed / 0 failed / 7 ignored**（**75 个测试套件**）。7 个 ignored 是设计上需要真实
  API key 或真实 QEMU 引导的用例。
- gate —— `cargo fmt --check`、`cargo clippy -D warnings`（可移植 crate、`host`、`ui/src-tauri`）、
  `cargo check`、全量 `cargo test`、`npm run build`、六个 UI 探针、镜像常量守卫与双语文档检查 ——
  本地与 CI 均通过。
- CI（GitHub Actions，`ubuntu-latest`）在本版每个提交上都为绿，跑的正是 `scripts/gate.sh`。

## 前置条件

**必须安装 RISC-V 裸机 GCC 才能编译任何东西。** RiscDom 会自动探测
（`RISCDOM_RISCV_GCC` / `RISCV_GCC` → 常见安装路径 → `PATH`）；全部未命中时，它会明确告诉你
搜过哪些路径、怎么修（包括在 **设置 → 工具链** 手动指定路径）。请安装 xPack
[riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases)
或等价的 `riscv64-unknown-elf-gcc`；三平台步骤见
[docs/toolchain-setup.md](docs/toolchain-setup.md)。

**启动 guest 需要 QEMU（`qemu-system-riscv64`），RiscDom 既不随包分发、也不代为下载。** 可用

```text
winget install SoftwareFreedomConservancy.QEMU
```

安装，或从 <https://www.qemu.org/download/#windows> 下载安装包，或用 `RISCDOM_QEMU` 指向已有副本。
应用也会探测常见安装路径与 `PATH`，**设置 → 工具链** 支持手动指定路径（现在是原生文件对话框）。详见
[docs/qemu-setup.md](docs/qemu-setup.md)。

## 已知限制

- **以 Windows 为主平台**：macOS 与 Linux 尚未验证；QMP 与串口走 TCP（无 Unix socket）。
- **QEMU 不捆绑、不代下载**：需要你自己安装，RiscDom 会一路引导你。
- **无增量快照、无加密**：每次快照都是全量状态流；会话存储为本地明文 SQLite。
- **同一时间一台 VM**：界面驱动单台宿主持有的 VM，不支持多 VM 并行。
- **界面只有中文**：还没有语言开关（v0.5 候选）。

## 快速开始

1. **准备环境**——Windows 10/11、QEMU（`qemu-system-riscv64`）、RISC-V 裸机 GCC
   （`riscv64-unknown-elf-gcc`）、Node 20+ 与 Rust 工具链（详见 [ENVIRONMENT.md](ENVIRONMENT.md)）。
2. **跑起来**——`cd ui && npm install && npm run tauri dev`。
3. **配置模型**——在设置栏选服务商并填入 API key（或选 Ollama / LM Studio，完全本地、无需 key），
   然后输入例如：*"写一个 RISC-V 裸机 Hello World，编译、运行并把串口输出读回来"*。

## 安全声明

本项目**不提供、不代管、不内置**任何 API key：所有模型访问均为自带密钥（BYOK）。key 不离开你的机器；
审计日志位于 AI 之外且 append-only；任何内容都不会被上传。QEMU 与 RISC-V 工具链是按各自许可证发布的
独立程序，见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。完整声明与漏洞报告流程见
[SECURITY.md](SECURITY.md)。

## 许可证

[Apache License 2.0](LICENSE)。
