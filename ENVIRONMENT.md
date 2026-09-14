# ENVIRONMENT.md — 智芯城 RiscDom 开发环境

> 本文件记录本机（Windows）已验证可用的工具链与平台限制。每一步实测通过后再落盘。

## 平台

- OS：Windows（Windows_NT 10.0.22631 x64）
- Shell：PowerShell
- 主机名：Z0624145651262
- 本地仓库路径：`D:\codeagent\breakevery\riscdom`

## 工具链版本（实测）

- QEMU：11.1.0
  - 路径：`C:\Program Files\qemu\`
  - 可执行：`qemu-system-riscv64.exe`
  - 已加入系统 PATH
- RISC-V GCC：xPack GNU RISC-V Embedded GCC 15.2.0
  - 真实前缀：`riscv-none-elf-`
  - 真实路径：`D:\tools\xpack-riscv-none-elf-gcc-15.2.0-1\bin`
  - 别名：`riscv64-unknown-elf-*` → `D:\tools\riscv64-unknown-elf\bin`（SymbolicLink）
  - 两条路径均已加入系统 PATH
- Rust：1.98.1（rustc / cargo），默认工具链 `stable-x86_64-pc-windows-msvc`
  - 路径：`C:\Users\cloud_user\.cargo\bin`（已加入用户 PATH）
- Node：v24.11.1
- npm：11.16.0
- MSVC：BuildTools 2022，MSVC 14.44.35207
  - `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools`
- git：2.55.0.3（`C:\Program Files\Git\cmd`）

## Windows 平台限制（重要）

- QMP（QEMU Monitor Protocol）：使用 **TCP**，不用 Unix domain socket。
  例：`-qmp tcp:127.0.0.1:<port>,server=nowait`
- 串口：使用 **TCP socket** 或 **file** 重定向，不用 `mon:stdio`。
  例：`-serial file:<path>` 或 `-serial tcp:127.0.0.1:<port>,server=nowait`
- 不使用 `-nographic` + `mon:stdio` 组合做自动化捕获（交互式，无法脚本化）。
- GDB/调试端口同理走 TCP。

## 已验证的裸机启动范式（smoke test）

```text
qemu-system-riscv64 \
  -machine virt -cpu rv64 -bios none \
  -kernel <elf> -display none \
  -serial file:<serial.log>
```

关键约束：

1. 交叉编译加 `-mcmodel=medany`（目标地址 0x80000000 超出 medlow 范围）。
2. `_start` 必须显式初始化 `sp`，否则首次压栈即 store fault。
3. 入口 `_start` 必须链接在镜像最前（`.text.start` 段置首，`ENTRY(_start)`），
   因为 `-bios none` 下 QEMU 从 `0x80000000` 起跳，不读取 ELF 的 e_entry 偏移。
4. 让 guest 主动退出可用 SiFive test finisher：向 `0x100000` 写 `0x5555`。
