[English](toolchain-setup.md) | 中文

# RISC-V 工具链安装

智芯城（RiscDom）用一个 **RISC-V 裸机 GCC** 来编译 AI 写的 C / 汇编。没有它，沙箱什么都启动不了——
应用会明确提示，并指向本文。

你需要 `riscv64-unknown-elf-gcc`（或 xPack 的等价物 `riscv-none-elf-gcc`）。

## 1. 安装工具链

**Windows（推荐 xPack）**

1. 从 <https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases> 下载最新的
   `xpack-riscv-none-elf-gcc-*-win32-x64.zip`。
2. 解压到 `C:\Program Files\`（或任意位置，例如 `D:\tools\`）。
3. 可执行文件是 `<解压目录>\bin\riscv-none-elf-gcc.exe`。把该 `bin` 目录加入 `PATH`，
   或在 RiscDom 里手动指定（见第 2 步）。

Windows 备选：MSYS2 包 `mingw-w64-x86_64-riscv64-unknown-elf-gcc`（安装到
`C:\msys64\mingw64\bin` 或 `C:\msys64\ucrt64\bin`），或 SiFive / GNU 官方的 Windows 构建。

**macOS**

```text
brew tap riscv-software-src/riscv
brew install riscv-tools
# 或：brew install riscv64-elf-gcc
```

**Linux**

用发行版包即可（Debian/Ubuntu：`gcc-riscv64-unknown-elf`；Arch/Fedora：
`riscv64-unknown-elf-gcc`），或从
<https://github.com/riscv-collab/riscv-gnu-toolchain> 自行构建。

## 2. 让 RiscDom 找到它

RiscDom 按以下顺序解析，命中即停止：

1. `RISCDOM_RISCV_GCC` —— 编译器的完整路径（例如
   `C:\Program Files\xpack-riscv-none-elf-gcc-15.2.0-1\bin\riscv-none-elf-gcc.exe`）。
2. `RISCV_GCC` —— 通用命名下的同一件事。
3. 常见安装位置：`C:\Program Files\xpack-riscv-none-elf-gcc-*\bin\`、
   `C:\Program Files (x86)\...`、`C:\msys64\{mingw64,ucrt64}\bin\`、`C:\tools\**\bin\`、
   `D:\tools\**\bin\`、`~/.local/bin/`、`/opt/riscv/bin/`、`/usr/local/bin/`、`/usr/bin/`。
4. `PATH` —— 先找 `riscv64-unknown-elf-gcc`，再找 `riscv-none-elf-gcc`。

如果**环境变量已设置但指向的文件不存在**，RiscDom 会**直接报错说明**，而不是悄悄跳到下一级——
因为那是你显式指定的编译器。

在桌面应用里也可手动指定：**设置 → 工具链 → “手动指定”**，粘贴编译器的完整路径，
RiscDom 会用 `--version` 校验通过后才接受；点“清除手动路径”回到自动探测。
**“探测详情”** 会列出所有尝试过的位置。

## 3. 验证

```text
riscv64-unknown-elf-gcc --version
# 或 xPack 构建：
riscv-none-elf-gcc --version
```

输出应显示目标三元组 `riscv64-unknown-elf`（或 `riscv64-none-elf`）与版本号。
在应用里，工具链一行应显示绿点 + source 徽标（`EnvVar` / `KnownPath` / `Path` / `Manual`）。

## 故障排查

- **“RISC-V GCC not found.”** —— 全部未命中：按第 1 步安装，或设置 `RISCDOM_RISCV_GCC` 后重启
  RiscDom。报错信息会列出搜索过的每一个路径。
- **“set, but that file does not exist”** —— 环境变量指向了失效路径：改对，或清空它让自动探测生效。
- **“`--version` exited with …”** —— 文件存在但跑不起来（架构不对，或包装脚本缺少配套文件）：
  重新安装一份干净的工具链。
- **编译报 relocation / `medlow` 相关错误** —— RiscDom 已经传了 `-mcmodel=medany`；
  请检查你是否通过异常的包装脚本覆盖了编译参数。

另见：[ENVIRONMENT.md](ENVIRONMENT.md)（本项目开发机上实测的版本）。
