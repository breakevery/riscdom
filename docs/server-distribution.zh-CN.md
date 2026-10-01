[English](server-distribution.md) | 中文

# 分发 relay

**状态** v1.0 规格（M7b-1；自 v1.0 M8-4c 起只讲 relay）｜ **日期** 2026-09-29 ｜ **受众** 构建、发布或
解包 RiscDom relay 包的人。

> **控制平面不在这里。** `riscdom-server`（单节点控制平面）已在 **v1.0 M8-4a** 离开本仓，并在它自己的
> 仓里打包 —— <https://github.com/breakevery/riscdom-server>，其 `docs/server-distribution.md` 才是那个
> 包的权威。本文件现在只讲本仓仍在发布的那个程序：**连接层的服务器**。

**本文件是什么。** RiscDom 发布一个**部署者**要运行的程序：**连接层的服务器**（`riscdom-relay`）。本
文件说明它的**包**包含什么、如何构建、以及它不带什么。

## 1. 连接层的服务器

`riscdom-relay-<version>-<platform>` 包含：

| 条目 | 是什么 |
|---|---|
| `riscdom-relay`（Windows 上为 `.exe`） | 跨区域服务器（[net/README.md](../net/README.md)、[connection.md](connection.md) §6）。 |
| `README.md` | [net/README.md](../net/README.md) —— 它所属的连接层。 |
| `examples/peers.example.json` | 空的对端表（`{"schema_version": 1, "peers": []}`）。 |
| `examples/rooms.example.json` | 空的房间集（`{"schema_version": 1, "rooms": []}`）。 |

**运行它**：`riscdom-relay --data-dir <dir> --bind <addr> --node-id <name>`。**刻意没有默认 bind**：
指定一个默认值就等于项目替部署者命名服务器在哪（[connection.md](connection.md) §6.1）。一个既无
`peers.json` 也无 `rooms.json` 的 data 目录，是一个谁都不认识、也什么都不发布的服务器 —— 这是诚实，
不是方便。

## 2. 这个包不带什么

- **绝无凭据。** relay 的 Ed25519 密钥在首次启动时铸入 `<data-dir>/node.key`；包从不携带它，打包器也
  绝不能把 data 目录复制进去。
- **完全不带 data 目录** —— 没有 `settings.json`、`peers.json`、`sessions.db` 或 `audit.db` 随软件
  走。`examples/` 两个文件是示例，不是状态。
- **不带 QEMU，也不带 RISC-V GCC。** relay 是一个普通 Rust 二进制；它不启动访客。
- **不带 web 根。** relay 不提供任何页面。（控制平面会 —— 那个包属于 `riscdom-server`，见文首。）

## 3. 构建一个包

`scripts/pack.sh`（unix）与 `scripts/pack.ps1`（Windows）是一对孪生 —— 与 `gate`、`commit` 保持的同
一种拆分，让本项目验证的平台有原生实现，而不是依赖外部 `zip`。

```
scripts/pack.sh  [--output-dir <dir>]
scripts\pack.ps1 [-OutputDir <dir>]
```

脚本会：

1. 构建 release 二进制（`cargo build --release -p net --bin riscdom-relay`）—— 包永远以 **release**
   构建；
2. 组装目录树，往输出目录写出**一个归档**；
3. 打印归档路径与大小。

**版本**来自根 `Cargo.toml` 的 `[workspace.package] version` —— 一处发布时 bump 的地方 —— 所以包不可能
与里面的二进制不一致。**平台**是宿主自己的（`win-x64`、`linux-x86_64`、`macos-aarch64`、……）。

默认输出目录是 **`target/dist/`**，它被忽略（`.gitignore` 里的 `**/target`），因此制品永不进入仓库。
**没有任何签名**，也不发布任何东西：除构建本身外，脚本离线。

## 4. 平台

| 平台 | 格式 | 备注 |
|---|---|---|
| Windows | `.zip` | 已验证的平台。 |
| Linux | `.tar.gz` | 由脚本在 Linux 宿主上构建；tag 触发的 CI 作业也会构建它。 |
| macOS | `.tar.gz` | 同上。 |

**tag 触发的 CI 作业会构建这个归档** —— [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) 里
的 `relay-bundle`，在推送 `v*` tag 时跑打包器（批 BF；v1.0 M8-4c 中控制平面离开后由 `server-bundle`
改名而来）。**CI 没有 Windows runner**，所以 Windows 的 `.zip` 由人工在机器上构建 —— 那是 M7b-4。

## 5. 什么没有签名

**这里没有任何签名。** Windows 二进制没有 Authenticode 签名，macOS 没有公证，任何地方都没有包签名
—— 与桌面安装包处于同一状态（RELEASE_NOTES 记着 macOS 与 Linux 包未签名、未走查）。需要签名的运维
者可以自行从源码构建并签名。签名方案会是它自己的裁决、带它自己的凭据，这里不做假定。

## 6. 什么不在覆盖范围

- **控制平面的包**（`riscdom-server-<version>-<platform>`，含构建好的前端）—— 它属于
  <https://github.com/breakevery/riscdom-server>，并在那里有文档。
- **Windows CI 打包** —— [M7b-4](roadmap-v1.0.md)：还没有接上 Windows runner。
- **发布动作** —— 打 `v*` tag 并把归档挂到 release 上，需要它自己的授权，也正是它让一个包公开。
- **把 `riscdom-backup` 打进这个包** —— 可移植工具自有规格（[backup.md](backup.md)），不与 relay 一同
  打包。
- **桌面应用** —— 它以 [`riscdom-adminapp`](https://github.com/breakevery/riscdom-adminapp) 产出的
  Tauri 包（`.dmg`、`.deb`、`.rpm`、`.AppImage`，以及人工构建的 Windows 安装包）分发。
