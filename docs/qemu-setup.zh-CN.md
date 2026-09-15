[English](qemu-setup.md) | 中文

# QEMU 安装

智芯城（RiscDom）用 **`qemu-system-riscv64`** 启动 guest。它**不捆绑、也不代下载**（与 RISC-V GCC
不同）：由你安装，RiscDom 去找；或你在应用里手动指定。

## 1. 安装 QEMU

**Windows**

```text
winget install SoftwareFreedomConservancy.QEMU
```

或从 <https://www.qemu.org/download/#windows> 下载安装包。安装后可执行文件名为
`qemu-system-riscv64.exe`（通常在 `C:\Program Files\qemu\`）。

**macOS**

```text
brew install qemu
```

**Linux**

用发行版包即可（Debian/Ubuntu 的 `qemu-system-misc` 提供 RISC-V 系统模拟器；Arch/Fedora 用
`qemu`），或从 <https://www.qemu.org/download/> 自行构建。

## 2. 让 RiscDom 找到它

`sandbox::qemu_discover` 按以下顺序解析，命中即停：

1. `RISCDOM_QEMU` —— 可执行文件完整路径。
2. `QEMU_SYSTEM_RISCV64` —— 通用命名下的同一件事。
3. 常见安装位置：`C:\Program Files\qemu\`、`C:\Program Files (x86)\qemu\`、
   `C:\msys64\{mingw64,ucrt64}\bin\`、`C:\tools\**` / `D:\tools\**`、`/opt/homebrew/bin/`、
   `/usr/local/bin/`、`/usr/bin/`。
4. `PATH`。

**环境变量已设置但文件不存在 → 直接报错**，不会静默跳到下一级。

在桌面应用里：**设置 → 工具链 → QEMU** 显示解析到的路径与 source 徽标
（`EnvVar` / `KnownPath` / `Path` / `Manual`），提供“重新探测”与“手动指定”，并可展开完整探测
记录。手动路径写入应用数据目录的 `settings.json`，重启后仍生效。

```text
set RISCDOM_QEMU=C:\Program Files\qemu\qemu-system-riscv64.exe
```

## 3. 验证

```text
qemu-system-riscv64 --version
```

输出应包含 QEMU 与版本号。应用里 QEMU 一行应显示绿点。
启动 guest 还需要工具链——见 [toolchain-setup.md](toolchain-setup.md)。

## 故障排查

- **“QEMU (qemu-system-riscv64) not found.”** —— 全部未命中：按第 1 步安装，或设置
  `RISCDOM_QEMU` 后重启 RiscDom。报错会列出搜索过的每个位置。
- **“set, but that file does not exist”** —— 环境变量指向失效路径：改对，或清空它让自动探测生效。
- **“not runnable”** —— 文件存在但起不来（架构不对，或包装脚本缺 DLL）：重装 QEMU。
- **VM 起来了但什么都没跑** —— 同时检查 RISC-V 工具链；缺编译器会在 QEMU 介入之前就失败
  （`toolchain_missing`）。
- **重负载下偶发启动失败** —— 端口是“先探测、释放、再由 QEMU bind”（已知 TOCTOU）。
  RiscDom 会用新端口重试（审计事件 `sandbox.snapshot.resume.retry` 等可见）；若持续失败，
  说明另有原因。
