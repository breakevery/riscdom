[English](README.md) | 中文

# 智芯城 RiscDom

> 让 AI 在 RISC-V 虚拟沙箱里当家作主，但人类永远握着根权限。

**许可证：** [Apache-2.0](LICENSE)。

RiscDom（中文名 **智芯城**）是一个桌面应用：AI 在 QEMU RISC-V 裸机沙箱中拥有
**虚拟内核级权限**，可以写 C / RISC-V 汇编、编译、运行、读串口并迭代。
全过程**可审计、可回滚**；人类保留根权限；边缘能力插件化。

## 核心口号

**自由在边界内，审计在 AI 外，根权限在人类。**

## 宪法摘要

1. 宿主监控层不可被 AI 修改。
2. 审计日志在 AI 之外，append-only，不可关闭。
3. 能力默认拒绝，插件声明权限。
4. 人类永远有暂停、回滚、断网、终止权。
5. AI 民主是实验变量，不是 MVP 必做。
6. MVP 阶段 AI 在沙箱内只能生成 C 和 RISC-V 汇编。
7. AI 接入先用 API keys。

完整版见 [PROJECT_CONSTITUTION.md](PROJECT_CONSTITUTION.md)。

## 架构

```text
                ┌────────────────────────────────────┐
                │  人类 / Human（根权限持有者）          │
                └──────────────────┬─────────────────┘
                                   │ Tauri commands / events
                ┌──────────────────▼─────────────────┐
                │  host（Rust + Tauri）前端唯一入口     │
                └───┬──────────────┬──────────────┬──┘
                    │              │              │
        ┌───────────▼──┐  ┌────────▼───────┐  ┌───▼──────────┐
        │ agent        │  │ sandbox        │  │ audit        │
        │ LLM 循环      │  │ QEMU RISC-V    │  │ append-only  │
        │ 工具 / 策略    │  │ 串口 / QMP     │  │ hash chain   │
        └──────────────┘  └────────────────┘  └──────────────┘
                    ▲              ▲
                    └──── ui (React) ────┘   前端不直接接触 Rust crate
```

依赖方向（单向，无环）：`ui/src-tauri → host → {agent, sandbox, audit}`，
且 `agent → sandbox → audit`。

## 环境要求

- Windows 10/11（MVP 仅在 Windows 验证；QMP/串口走 TCP）
- QEMU（`qemu-system-riscv64`，实测 11.1.0）—— RiscDom 会**自动找到它**（`RISCDOM_QEMU` /
  `QEMU_SYSTEM_RISCV64` → 常见安装路径 → `PATH`），也可在 **设置 → 工具链 → QEMU** 手动指定。
  Windows 安装：`winget install SoftwareFreedomConservancy.QEMU`，或
  <https://www.qemu.org/download/#windows>。详见 [docs/qemu-setup.md](docs/qemu-setup.md)。
- RISC-V 裸机 GCC（`riscv64-unknown-elf-gcc`，实测 xPack 15.2.0）
- Rust / cargo（实测 1.98.1）+ MSVC 工具链（Tauri 需要）
- Node / npm（实测 24.11.1 / 11.16.0）

**必须安装 RISC-V 裸机 GCC 才能编译任何东西**——而且 RiscDom **可以帮你下载**：
**设置 → 工具链 → 一键下载**会拉取对应平台的官方 xPack 构建（约 200 MB），校验 SHA-256 后
安装到应用数据目录。你也可以自行安装（xPack
[riscv-none-elf-gcc](https://github.com/xpack-dev-tools/riscv-none-elf-gcc-xpack/releases)，
实测 15.2.0，或等价的 `riscv64-unknown-elf-gcc`）：RiscDom 会自动探测
（`RISCDOM_RISCV_GCC` / `RISCV_GCC` → 常见安装路径 → `PATH`），也可在同一 tab 手动指定路径。
三平台步骤见 [docs/toolchain-setup.md](docs/toolchain-setup.md)。

细节与路径见 [ENVIRONMENT.md](ENVIRONMENT.md)。

## 安全声明

- 本项目**不提供、不代管、不内置**任何 API Key。所有模型访问均由用户自带（BYOK）。
- API Key 仅保存在本机（默认写入系统钥匙串），**不经过本项目任何服务器**。
- 本项目**不会**将用户代码、串口输出、审计日志上传到任何远端。
- 审计日志（SQLite + hash chain）仅存在于本地，用于审计与回滚。
- 如需完全离线运行，可使用 Ollama / LM Studio 等本地模型，**无需任何 key**。
- 发现安全问题请通过 GitHub Security Advisory 私下报告，不要在 issue 中贴 key 或漏洞细节。

> 提交前请先运行本地预检：`scripts/preflight.ps1`（Windows）或 `scripts/preflight.sh`（Unix）。

## 快速开始

```powershell
# 1. 前端依赖
cd ui
npm install

# 2. 启动桌面应用（会编译 Rust 后端）
npm run tauri dev
```

主界面为**聊天 + 串口双栏**（两栏始终同时可见）；设置通过右上角齿轮进入（`Esc` 返回）。

在设置栏填入 DeepSeek API Key（仅会话内存），然后在对话框输入例如：

> 写一个 RISC-V 裸机 Hello World，编译、运行并把串口输出读回来

## 目录结构

```text
riscdom/
├── AGENTS.md                 # 核心宪法（每轮注入）
├── PROJECT_CONSTITUTION.md   # 完整宪法 + 架构 + 审计事件类型
├── ENVIRONMENT.md            # 本机工具链与平台限制
├── CHANGELOG.md              # 版本记录
├── LICENSE                   # Apache-2.0
├── Cargo.toml                # Rust workspace（cli / host-core / host-tauri / sandbox /
│                             #   audit / agent / worker / server）
├── sandbox/                  # QEMU RISC-V 沙箱（进程/QMP/串口/快照）
├── audit/                    # append-only SQLite + hash chain（含 audit-verify）
├── agent/                    # LLM 循环、工具、能力策略、编译器封装
├── host-core/                # 宿主的可移植半（不碰 Tauri）
├── host-tauri/               # 桌面外壳：commands / events / state（Tauri）
├── server/                   # HTTP + SSE 上的控制平面
├── cli/                      # `riscdom` 命令行客户端
├── worker/                   # 执行者进程，以及监工半边
├── docs/                     # 设计记录、API 表格、指南（导航：docs/README.zh-CN.md）
├── examples/python/          # 参考监工
├── scripts/                  # gate、commit 包装脚本、各检查器
├── walkthroughs/             # 发布门禁的走查记录（刻意单语）
└── ui/                       # React 前端（Tauri shell + 三栏界面）
    ├── src/                  # 布局 / 面板 / API / 状态
    └── src-tauri/            # Tauri shell（注册 host-tauri 的命令）
```

## 测试

```powershell
# 全 workspace（Rust）
cargo test

# 单个 crate
cargo test -p sandbox     # QEMU 生命周期 + 串口捕获 + 快照降级
cargo test -p audit       # append-only + hash chain + 查询 + CLI
cargo test -p agent       # LLM 客户端 + 工具 + 策略 + 编译器 + Agent 循环
cargo test -p host-core   # 可移植宿主：状态、设置、派发、任务沙箱
cargo test -p server      # 控制平面：路由、capability、事件流
cargo test -p cli         # 命令行，对着一个控制平面
cargo test -p worker      # 执行者进程，以及监工半边

# 需要真实 QEMU/工具链的端到端（mock LLM）
cargo test -p host-core -- --ignored --nocapture

# 前端构建
cd ui && npm run build
```

## 设置 API Key

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."   # 仅当前终端会话
cargo test -p agent -- --ignored --nocapture   # 真实模型端到端
```

或在应用**设置栏**填写并"保存到本次会话"。

Key 只存在后端内存：**不写** localStorage / sessionStorage / 磁盘 / 审计 / 日志，
状态回显也不含 key。关闭应用即失效。

## 已知限制（MVP 降级项）

> 完整路线图见 [PROJECT_CONSTITUTION.md §10](PROJECT_CONSTITUTION.md)。

- **仅支持 DeepSeek**（v0.1）：LLM 客户端目前只对接 DeepSeek。v0.2 将重构为通用
  `OpenAiCompatClient`，内置 OpenAI / Ollama（本地）/ LM Studio（本地）预设，
  支持无 key 的本地模型。
- **API key 仅内存**（v0.1）：不落盘、不进审计，关闭应用即失效。v0.2 改用 OS keyring
  （Windows Credential Manager / macOS Keychain / Linux Secret Service），
  绝不使用 `localStorage` / 明文文件 / `.env`。
- **快照**：`save_snapshot` / `load_snapshot` 是"存参数 + 重启"，**不是**真实
  VM 状态（v0.2 换 QEMU `savevm`/`loadvm`）。
- **串口来源**：由 sandbox 串口读取线程**主动推送**（`subscribe_serial` → `serial:chunk`），
  不再是审计派生；订阅只收到订阅之后的数据（见 `host-tauri/README.md`）。
- **平台**：黄金路径在 Windows 上验证过。macOS/Linux 的安装包由 CI 产出（`.app`/`.dmg`、`.deb`/`.rpm`/`.AppImage`），**未签名、也尚未人工走查**；QMP over Unix socket 仍未实现（仅 TCP）。
- **无流式输出**：LLM 响应为整块返回。
- **无会话持久化**：每轮 `run_agent` 是独立上下文。
- **编译器注入 crt0**：AI 只需写 `int main(void)`（原因见 `agent/README.md`）。

## 贡献

欢迎提交问题、修复与文档改进。请先读 [CONTRIBUTING.md](CONTRIBUTING.md)：所有改动都必须通过本地
gate（`scripts/gate.ps1` / `scripts/gate.sh`），并经由受门禁保护的提交包装
（`scripts/commit.ps1` / `scripts/commit.sh`）提交——gate 非零时它拒绝提交。

## 行为准则

本项目采用 [Contributor Covenant v2.1](CODE_OF_CONDUCT.md)。如有不当行为，
请通过该文件中的联系方式报告。

## 许可证

[Apache License 2.0](LICENSE)。

- 本仓库代码采用 Apache-2.0 许可证。
- 若您向本仓库提交贡献，需签署 [CLA](CLA.md)。
- 使用本项目时请遵守你所用模型服务商的条款。
- QEMU 与下载来的 RISC-V 工具链是按各自许可证发布的独立程序；这对本项目意味着什么写在
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 更多

- **[docs/README.zh-CN.md](docs/README.zh-CN.md)** —— 文档导航：全仓每一份文档，按受众分组，并写明什么是历史
- [PROJECT_CONSTITUTION.md](PROJECT_CONSTITUTION.md) — 完整宪法、架构分层、审计事件类型
- [ENVIRONMENT.md](ENVIRONMENT.md) — 工具链与平台限制
- [CHANGELOG.md](CHANGELOG.md) — 版本历史
- 各 crate 的 README：[sandbox](sandbox/README.md) · [audit](audit/README.md) ·
  [agent](agent/README.md) · [host-core](host-core/README.md) · [host-tauri](host-tauri/README.md) ·
  [server](server/README.md) · [cli](cli/README.md) · [worker](worker/README.md) · [ui](ui/README.md)
