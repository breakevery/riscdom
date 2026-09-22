[English](THIRD_PARTY_NOTICES.md) | 中文

# 第三方声明

RiscDom 本身是 Apache-2.0（[LICENSE](LICENSE)）。它**不链接**下列任何软件：它们都以独立进程运行，由
RiscDom 启动，并通过管道或 TCP socket 与之对话。本文件只陈述这些程序的许可证**事实**，不是法律意见，
也不判定任何一种分发模式是否合规 —— 那个问题属于
[docs/qemu-distribution.md](docs/qemu-distribution.md) §4。

## QEMU

- **许可证：GPL-2.0。** 某个具体 QEMU 构建内部的组件可能采用其它许可证；以该构建随附的
  `COPYING` / `LICENSE` 为准。
- **RiscDom 如何使用**：把 `qemu-system-riscv64` 作为子进程启动，通过 QMP 与串口连接驱动它。RiscDom
  不链接任何 QEMU 代码，也不修改 QEMU 源码。
- **它如何到达用户**：由用户自行安装 QEMU（[docs/qemu-setup.md](docs/qemu-setup.md)）—— 应用只会把它引向
  `winget` 或官网下载页。RiscDom 不下载、不托管、不镜像、也不再分发 QEMU。

## RISC-V 工具链（xPack `riscv-none-elf-gcc`）

- **许可证：GPL-3.0-or-later，附 GCC Runtime Library Exception。** 该例外覆盖的是「**用** GCC 编译出来
  的程序」需要如何处理 GCC 自身的运行时库；它并不改变 GCC 本身的许可证。
- **RiscDom 如何使用**：作为独立程序调用，用于编译 freestanding guest。RiscDom 不链接它的任何部分。
- **它如何到达用户**：应用内下载从 xPack release 拉取归档并校验其公布的 SHA-256
  （`host-core/src/toolchain_download.rs`）。也可以自行安装任意 RISC-V GCC 并指向它。

## Rust 依赖

每个 crate 自带许可证；`Cargo.lock` 加上各 crate 自身的元数据才是权威来源。此处不逐一列出：这份
清单每次依赖升级都会变，写在这里只会悄悄过期而无人察觉。
