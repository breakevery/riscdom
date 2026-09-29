[English](server-distribution.md) | 中文

# 分发两个服务器程序

**状态** v1.0 规范（M7b-1）｜ **日期** 2026-09-29 ｜ **面向读者** 构建、交付或解包 RiscDom 服务器包的人。

**本文是什么。** RiscDom 交付两个由**部署者**运行的程序：**单机控制平面**（`riscdom-server`）与
**连接层的服务器**（`riscdom-relay`）。本文说清每个**包**装什么、怎么造一个、以及两者都不带什么。它是
一份**规格**；打包脚本随它落地（M7b-1），而 CI 打包与发布动作是后续批次（M7b-2、M7b-3）。

## 1. 单机控制平面

`riscdom-server-<version>-<platform>` 装着：

| 条目 | 是什么 |
|---|---|
| `riscdom-server`（Windows 上 `.exe`） | 控制平面：HTTP + SSE，盖在宿主的 kernel facade 上（[server/README.md](../server/README.md)）。 |
| `web/` | 已构建的 Web UI，由 `--web-root web` 在 `/` 提供（不认证；API 不是）。 |
| `README.md` | [server/README.md](../server/README.md) —— 怎么构建、怎么跑、端点、认证。 |
| `settings.example.json` | 一份最小、合法的设置文档（`{"version": 2}`），供首次启动。 |

**跑它**：`riscdom-server --bind <addr> --workspace <dir> --data-dir <dir> --web-root web`。完整参数集就是
这个程序自己的 `--help`（`--bind`、`--workspace`、`--data-dir`、`--web-root`、`--heartbeat-ms`、
`--auth` / `--no-auth`、`--log-level`；退出码 `0`/`1`/`2`）。

## 2. 连接层的服务器

`riscdom-relay-<version>-<platform>` 装着：

| 条目 | 是什么 |
|---|---|
| `riscdom-relay`（Windows 上 `.exe`） | 跨区域服务器（[net/README.md](../net/README.md)、[connection.md](connection.md) §6）。 |
| `README.md` | [net/README.md](../net/README.md) —— 它所属的连接层。 |
| `examples/peers.example.json` | 一张空的同侪表（`{"schema_version": 1, "peers": []}`）。 |
| `examples/rooms.example.json` | 一个空的房间集（`{"schema_version": 1, "rooms": []}`）。 |

**跑它**：`riscdom-relay --data-dir <dir> --bind <addr> --node-id <name>`。**没有默认 bind**，这是刻意的：
命名一个默认地址就是项目在命名服务器在哪（[connection.md](connection.md) §6.1）。一个两文件都没有的数据
目录，是一个谁都不认识、什么都不发布的服务器 —— 诚实，而不是方便。

## 3. 两个包都不带什么

- **绝不带凭据。** 控制平面的 bearer token 首启生成到 `<data-dir>/token`，relay 的 Ed25519 密钥生成到
  `<data-dir>/node.key`；两者都是**本地铸造**，包两个都不带。打包器**绝不**把数据目录拷进去。
- **完全不带数据目录** —— 没有 `settings.json`、`peers.json`、`sessions.db` 或 `audit.db` 随软件同行。
  `settings.example.json` 与两个 `examples/` 文件是**示例**、不是状态。
- **不带 QEMU，也不带 RISC-V GCC。** 两个程序都是纯 Rust 二进制。（控制平面仍*需要* QEMU 与 RISC-V GCC 才
  能启动客户机 —— 但那是操作者自己安装的工具链，与桌面应用同一套，不是包该捆的东西：见
  [qemu-setup.md](qemu-setup.md)。）
- **relay 包不带 web root。** 只有控制平面提供页面。

## 4. 造一个包

`scripts/pack.sh`（unix）与 `scripts/pack.ps1`（Windows）是孪生 —— 与 `gate`、`commit` 保持同一种分工，
于是项目已验证的那个平台有原生实现，而不必依赖外部 `zip`。

```
scripts/pack.sh  [--output-dir <dir>] [--skip-ui-build]
scripts\pack.ps1 [-OutputDir <dir>]  [-SkipUiBuild]
```

脚本会：

1. 构建 release 二进制（`cargo build --release -p server --bin riscdom-server` 与
   `-p net --bin riscdom-relay`）—— 包**一律以 release 构建**；
2. 构建前端（在 `ui/` 跑 `npm run build`），除非 `--skip-ui-build` / `-SkipUiBuild` 复用已构建的
   `ui/dist/app`。它跑的是 `npm run build`、**不是** `npm ci`：gate 假定前端依赖已安装，而一个会去够网络的
   打包脚本会是另一个东西；
3. 组装两棵树，往输出目录各写**一个归档**；
4. 打印两条路径与它们的大小。

**版本**取自根 `Cargo.toml` 的 `[workspace.package] version` —— 发布时唯一 bump 的地方 —— 于是一个包不可能
与它里面的二进制不一致。**平台**取宿主自己的（`win-x64`、`linux-x86_64`、`macos-aarch64`……）。

默认输出目录是 **`target/dist/`**，它被忽略（`.gitignore` 里的 `**/target`），所以产物永不进仓。**没有任何
东西被签名**，也没有任何东西被发布：除构建本身外，脚本是离线的。

## 5. 平台

| 平台 | 格式 | 说明 |
|---|---|---|
| Windows | `.zip` | 已验证平台；v0.9.9 那次发布用的格式。 |
| Linux | `.tar.gz` | 由脚本在 Linux 宿主上构建；CI 打包是后续批次。 |
| macOS | `.tar.gz` | 同上。 |

**CI 尚未构建这些包**，且 **CI 没有 Windows runner**：v0.9.9 的 `riscdom-server-0.9.9-win-x64.zip` 是在
某台机器上手工构建、再挂到 Release 的。把它变成 CI job —— 先 macOS 与 Linux，等有 runner 再做 Windows ——
是 [M7b-2](roadmap-v1.0.zh-CN.md)，也是关掉 roadmap §12 那个 open 项（「Linux 与 macOS 的 server zip ——
v0.9.9 只发了 Windows 那份」）的批次。

## 6. 什么没有被签名

**这里什么都不签名。** Windows 二进制上没有 Authenticode 签名、macOS 上没有 notarization、任何地方都没有包
签名 —— 与桌面安装包的现状相同（RELEASE_NOTES 记着 macOS 与 Linux 包未签名、未启动过）。需要签名的操作者
从源码构建并自行签名。签名这件事会是它自己的决策、带自己的凭据，本文不作假设。

## 7. 不覆盖什么

- **CI 打包** —— [M7b-2](roadmap-v1.0.zh-CN.md)：一个 tag 门控的 job，在 macOS 与 Linux 上构建这些归档
  （Windows 需要一个项目尚没有的 runner）。
- **发布动作** —— [M7b-3](roadmap-v1.0.zh-CN.md)：打一个 `v*` tag 并把归档挂到 release。它需要自己的授权，
  也是让一个包变公开的那一步。
- **把 `riscdom-backup` 包塞进来** —— 可移植性工具有自己的规格（[backup.md](backup.zh-CN.md)）；它不与
  服务器捆绑。
- **桌面应用** —— 它以 CI `bundle` job 产出的 Tauri 包分发（`.dmg`、`.deb`、`.rpm`、`.AppImage`，以及手工
  构建的 Windows 安装包）。
