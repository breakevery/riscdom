[English](dependencies.md) | 中文

# RiscDom —— 依赖清单，以及哪些依赖被信任用于什么

> 汇总自 workspace 的各 `Cargo.toml`、[ENVIRONMENT.md](../../ENVIRONMENT.md) 与
> [README.md](../../README.md)。版本是本项目在 `ENVIRONMENT.md` 那台机器上**验证**过的版本；
> 「(已验证)」意味着真有跑过，而不是随手挑的区间。对应 `v1.0.0`。

## 1. 运行时依赖（用户必须有的）

| 依赖 | 验证所用版本 | 为何需要 |
|---|---|---|
| **QEMU**（`qemu-system-riscv64`） | **11.1.0** | 沙箱。RiscDom **从不打包或下载它**（这是决策，不是疏漏）。RiscDom 依次经 `RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64`、常见安装位置、再 `PATH` 找到它；用户可在 *设置 → Toolchain → QEMU* 指一份自己的。Windows：`winget install SoftwareFreedomConservancy.QEMU`。 |
| **RISC-V 裸机 GCC** | **xPack 15.2.0**（`riscv-none-elf-gcc`），或等价的 `riscv64-unknown-elf-gcc` | 编译访客程序。RiscDom **可替用户取**：*设置 → Toolchain → 一键下载* 会拉官方 xPack 构建（约 200 MB）、**校验其 SHA-256**，再装到 app data 目录下。依次经 `RISCDOM_RISCV_GCC` / `RISCV_GCC`、常见路径、再 `PATH` 自动发现。 |
| **操作系统凭据库** | 平台自带 | 保存 API key（勾选「保存」时）与节点的 `node.key`：Windows 凭据管理器 / macOS 钥匙串 / Linux Secret Service，经 `keyring` crate。 |

**本地模型**（Ollama / LM Studio）可完全替代托管提供方 —— 完全离线时**无需 API key**。

## 2. 构建依赖（贡献者必须有的）

| 依赖 | 版本（已验证） | 备注 |
|---|---|---|
| **Rust / cargo** | **1.98.1** | 默认工具链 `stable-x86_64-pc-windows-msvc`。 |
| **MSVC 工具链** | BuildTools 2022（MSVC **14.44**.35207） | Tauri 需要。 |
| **Node / npm** | **24.11.1** / **11.16.0** | UI probe 与 TypeScript SDK 测试直接 import `.ts`（Node ≥ 22.6 需 `--experimental-strip-types`；23.6 起默认）。 |
| **Tauri** | **2** | 桌面外壳（`host-tauri` + `ui/src-tauri`）。 |
| **Python 3** *（可选）* | 任意 | 跑 `examples/python` 的两个自测与编码扫描；缺席时 gate **打印 skip** 而非失败。 |
| **仅 Linux 的开发包** | —— | 两个 Tauri crate 需要 `webkit2gtk` / `gtk` / `librsvg` / `libsoup`；`host-core` 经 `keyring` 需要 `libdbus-1-dev`。CI 会装。 |

## 3. 关键 Rust crate（直接依赖）

按用途分组。manifest 里的「no new package」指：给一个已在 `Cargo.lock` 里的 crate 加了一条边
—— lock 不增长。

**控制平面（`server`）**

| Crate | 版本 | 用途 |
|---|---|---|
| `hyper` | 1（`server`、`http1`） | HTTP/1.1 服务端 |
| `tokio` | 1（`net`、`rt`、`io-util`、`sync`） | IO 驱动与广播通道（无 timer feature；心跳跑在自己的线程上） |
| `http-body-util`、`futures-util` | 0.1 / 0.3 | SSE body 桥 |
| `getrandom`、`subtle` | 0.4 / 2 | bearer token 的随机源与常量时间比较 |

**审计与存储**

| Crate | 版本 | 用途 |
|---|---|---|
| `rusqlite` | 0.32（`bundled`） | 只追加存储（SQLite，由上游 amalgamation 编译 —— **不依赖系统 SQLite**） |
| `sha2` | 0.10 | 哈希链 |
| `hex` | 0.4 | 哈希的十六进制编码 |

**agent 与 HTTP 出网**

| Crate | 版本 | 用途 |
|---|---|---|
| `reqwest` | 0.12（`blocking`、`json`、`rustls-tls`） | LLM 客户端，以及 CLI/SDK 客户端（**rustls**，无需系统 OpenSSL） |
| `uuid` | 1（`v7`） | 可排序的 run id |

**连接层（`net`）**

| Crate | 版本 | 用途 |
|---|---|---|
| `ed25519-dalek` | 2 | 节点身份的 Ed25519 签名（纯 Rust） |
| `base64` | 0.22 | JWK 成员的 base64url（RFC 8037） |
| `getrandom` | 0.4 | OS 随机源，用于铸钥匙 |

**归档 / 可移植性**

| Crate | 版本 | 用途 |
|---|---|---|
| `zip`、`flate2`、`tar`、`xz2` | 2 / 1 / 0.4 / 0.1 | 工作区导入导出与备份包 |
| `ring` | 0.17 | 备份包的 AEAD（AES-256-GCM）与 PBKDF2 |

**桌面**

| Crate | 版本 | 用途 |
|---|---|---|
| `tauri`（+ `tauri-build`、`tauri-plugin-dialog`） | 2 | 桌面外壳 |

**各处支撑**：`serde`、`serde_json`（1）、`thiserror`（1）、`keyring`（3，显式选择各 OS 的原生
后端 —— 裸 `keyring = "3"` 会编译成静默 no-op）。

**计数**：**25** 个直接外部 crate，外加 workspace 内部的 path 依赖。

## 4. 被信任的那些 —— 以及为什么

以下依赖的安全性建立在它们的正确性之上。判据是：**标准的、成熟的、不是自造的。**

| 依赖 | 信任用于 | 为什么可信 |
|---|---|---|
| **QEMU** | 访客边界 —— AI 写的一切都在它里面跑 | 成熟、被广泛使用的模拟器，独立于本项目、自有许可；项目既不打包也不下载它，所以那道边界就是用户自己的 QEMU。 |
| **RISC-V 裸机 GCC（xPack）** | 把模型写的 C 变成访客的编译器 | 官方 xPack 构建；应用内下载在安装前**校验其 SHA-256**。 |
| **`ed25519-dalek`** | 节点身份与每一帧的签名 | Ed25519 的参考 Rust 实现（dalek-cryptography）；JWK 的裸 32 字节密钥被直接暴露，所以这里没有自造的曲线代码。 |
| **`rusqlite`（bundled SQLite）** | 审计存储 | SQLite 是被部署最广的嵌入式数据库；`bundled` 编译上游 amalgamation，于是被信任的是 SQLite 本身，而不是一层包装。 |
| **`ring`** | 备份包的加密 | 基于 BoringSSL 的、被广泛使用且有审计的密码学库；项目选择**标准原语**（AES-256-GCM、PBKDF2-HMAC-SHA256），而非自己设计。 |
| **`rustls`**（经 `reqwest`） | 模型与控制平面流量的 TLS | 刻意不用系统 OpenSSL，让 TLS 栈是项目可钉住的 Rust 实现。 |
| **OS 凭据库**（`keyring`） | 静态保存的 API key 与节点密钥 | 平台自带的存储（凭据管理器 / 钥匙串 / Secret Service）；密钥不发明自有的密钥文件。 |

**这里没有的东西**：没有自造密码学。`compute_hash`、签名方案、AEAD 与 KDF 全是标准原语或标准
crate —— 项目自己的代码关心的是*记录什么、以及如何把它们链起来*，而不是发明原语。
