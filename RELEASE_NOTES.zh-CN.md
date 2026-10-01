[English](RELEASE_NOTES.md) | 中文

# RiscDom v1.0

> **这是 v1.0 —— API 已冻结。** **macOS 与 Linux 的包由 CI 构建，从未在真实机器上启动过**，而且它们
> **未签名**（macOS 的 Gatekeeper 会拦下首次运行；Windows 的 SmartScreen 会对安装包告警）。并且
> **Windows 仍是黄金路径被验证过的平台**：由别人在干净机器上走一遍这件事，仍未发生。

**v1.0 一句话：** RiscDom 在它承诺的地方停下不动 —— **内核 API 已冻结**，而[纲领](docs/roadmap-v1.0.zh-CN.md)
定下的三层都已在盘：**一台机器上多个沙箱**、**一个句句签名、并经由部署者自建的服务器互相到达的
workgroup**、以及**一个按名字把任务交给另一个节点的派发器** —— 其下是一条现在跨设备、却公式未变的审计链。

## 本版新增

- **冻结（M1、M8-1）。** [纲领 §6](docs/roadmap-v1.0.zh-CN.md) 要求「冻结前必须落盘」的六份文档都在盘 ——
  稳定性策略（`docs/api-compatibility.md`）、数据迁移（`api-compatibility.md` §6）、错误模型
  （`docs/error-model.md`）、凭据与密钥（`docs/security-model.md`）、升级路径（`docs/upgrade.md`）与披露
  政策（`SECURITY.md`）—— 而稳定性策略已**对照纲领 §1 的四条红线通过测试**（`api-compatibility.md`
  §9）。**`/v0/` 就是 v1.0 发布的路径**：冻结钉住的是路径的*含义*；前缀在下一个协议破坏性变更时才移到
  `/v1/`，不在这里。
- **第一层（M2）：一台机器上多个沙箱。** 一个定义可以派生出**实例**，连同实例表、它的五个端点，以及记录
  它们的审计行；每个执行者有**自己的模型配置**（`settings.json` 在打开时从 v1 迁到 v2，并留下 `.bak`）；
  参考 **M**（`examples/python`）经控制平面派发。
- **插件接口（M3）。** `docs/plugin-interface.md`：**进程外、stdio、JSON lines** —— 必含的机制层（启动 /
  停止 / 执行 / 输出）、可选的语义层（快照 / 指纹），以及一份 capability 声明。无进程内插件、无 ABI。
- **第二层（M4）：一个 workgroup，一个跨区域服务器。** 节点身份与签名（`node.key`，一个 JWK，Ed25519，
  六步验证，TCP 上一行 JSON）、发现（由内网服务器下发的同侪表，UDP 广播为补充）、**房间**
  （`rooms.json`）、**跨区域服务器**（部署者自己跑的一个部署，四种角色，无新凭据），以及用于 30 秒审计
  批次的**链 digest**。
- **audit v2（M5）。** **每设备一条链，外加临时段。** 站入节点开一个段、宣告它，中心回归后该段被送达、
  重建并**按转录合并**；冲突在**两侧**都记下，永不静默合并。**哈希公式未变。**
- **第三层（M6）：跨设备派发。** `POST /v0/tasks` 的 **`node`** 参数、运行时表（`GET /v0/online`）、中心
  写下的链 digest 行、端到端携带的 **`task_id`**、流上的 **`task_id` 过滤**、在另一个节点上留下的请求
  到达中心的队列、中心的**裁决沿提问者自己的会话回传**，以及**跨链验证**（锚点的第二半）。
- **生态（M7）。** **配置 schema**（`docs/config-schema.md`）、**可观测性**契约
  （`docs/observability.md`）、**性能预算**（`docs/performance-budget.md`）、多仓计划
  （`docs/multi-repo.md`）、**`riscdom-backup`**（整个节点 —— 两个根、审计库与凭据 —— 作为一个可搬移的
  单元），以及覆盖整个控制平面的 **Rust 与 TypeScript SDK**。

## 按平台安装

以下是 `v1.0.0` tag 的 CI 作业会构建并挂到 release 上的产物：

- **Windows 10/11** —— `RiscDom_1.0.0_x64_en-US.msi` 或 `RiscDom_1.0.0_x64-setup.exe`。安装包**未签名**，
  所以 SmartScreen 会首次告警（「更多信息 → 仍要运行」）。QEMU 与 RISC-V 裸机 GCC 不随包附送：
  *设置 → 工具链*会引导你 `winget install SoftwareFreedomConservancy.QEMU`（或官方页面），并可自行下载
  xPack GCC。
- **macOS** —— `RiscDom_1.0.0_aarch64.dmg`（Apple Silicon）及其内的 `.app`。**未签名**：右键应用 → *打开*，
  或运行一次 `xattr -dr com.apple.quarantine /Applications/RiscDom.app`。**不附送 QEMU**：`brew install qemu`。
  **还没有人在真实 Mac 上启动过这些包。**
- **Linux** —— `RiscDom_1.0.0_amd64.deb`、`RiscDom-1.0.0-1.x86_64.rpm` 或 `RiscDom_1.0.0_amd64.AppImage`。
  **不附送 QEMU**：装你发行版的 `qemu-system-riscv64`（例如 `sudo apt install qemu-system-misc`、
  `sudo dnf install qemu-system-riscv`）。**还没有人在真实 Linux 机器上启动过这些包。**
- **服务器** —— tag 的 **`server-bundle`** 作业产出的 `riscdom-server-1.0.0-linux-x64.tar.gz` 与
  `riscdom-server-1.0.0-macos-*.tar.gz`，以及 Windows 的 `riscdom-server-1.0.0-win-x64.zip`。每份含二进制、
  它的 `web/`、一个 README 与 `settings.example.json`；不带任何凭据、不带数据目录。专用转发器
  （`riscdom-relay-*`）就在旁边，而且 —— 按设计 —— **它根本没有 HTTP 面**。

## 验证过的 —— 与没验证过的

**验证过的**

- gate，**二十六步**（v0.9.9 的十八步，加上 `net` 层的六个示例自测、远端执行者示例，以及打包脚本语法
  检查）：`cargo fmt`、逐 crate 逐平台的 `cargo clippy -D warnings`、`cargo check`、完整 `cargo test`、
  `npm run build`、**十六个** UI 探针、镜像守卫、编码扫描、工具 schema 检查、各示例自测、wix 版本守卫、
  UI 字符串登记表、打包语法，以及双语文档检查。本地全绿；CI 在 `ubuntu-latest` 上跑同一脚本。
- 测试套件：workspace 运行下 **958 个测试、146 个套件**（v0.9.9 报的是 688 / 118；增长来自三层各自的
  套件）。有少数几个需要 API key 或写入 OS 钥匙串，被排除在 gate 的运行之外；其余都在这里跑 —— 本机有
  QEMU 与 RISC-V GCC。
- 针对真实 QEMU 客机的黄金路径端到端走查，自 v0.5.0 起未变。
- **连接层，手工走过**：同一台机器上两个节点，各有自己的数据目录、因而各有自己的 key 与 token，在内网
  服务器处注册、心跳，并读到彼此的注册；以及三层新增的流与派发面。

**没验证过的**

- **macOS 与 Linux 的包本身。** 它们能编译、能打包；没人真在 Mac 或 Linux 机器上安装或启动过。那是下一个
  走查。
- **干净机器走查。** 仍是当年那个「待加强」项：[docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  是表格，`walkthroughs/` 是填好的表该放的地方。
- **管理程序还没有拆出去。** v0.9.9 的说明说它「在 v1.0 成为自己的仓库」。那次拆分（**M7a**）
  **推迟到 v1.0 之后** —— 每个新仓都要钉一个 v1.0 tag，而那个 tag 随本版发布 —— 所以桌面端与它的前端
  仍住**在本仓内**。计划写在 [docs/multi-repo.zh-CN.md](docs/multi-repo.zh-CN.md)。
- **多智能体协作仍是一个接口。** 花名册、派发端点、请求队列、以及回传的裁决都在位、都有测试；哪个 AI
  拆分复合任务、各部分怎么分派，仍由使用者决定。
- **安装包与包都未签名** —— Windows 上 SmartScreen、macOS 上 Gatekeeper。签名与公证仍属商业化层的事。

## 测试者需要什么

- 一个**模型提供方**和**你自己的 API key**（或本地提供方，如 Ollama / LM Studio）、**QEMU**
  （`qemu-system-riscv64`，由你安装 —— 见上面各平台说明），以及**一个 RISC-V 裸机 GCC**（xPack
  `riscv-none-elf-gcc` 或等价的 `riscv64-unknown-elf-gcc`；应用可下载 xPack 那个）。Zig 与 Rust 可选：
  *设置 → 工具链*两者都能装。
- 网络那面：**第二个节点**。同一台机器上再起一个**有自己的数据目录**的 `riscdom-server`，就够走「连出去」；
  任何第二个带浏览器的设备，就够走「服务进来」；真实的第二台机器更好。`riscdom-server --help` 列出所有
  开关，[docs/control-plane-client-guide.zh-CN.md](docs/control-plane-client-guide.zh-CN.md) 走过 API。

## 怎么反馈

1. 边走边填 **[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)**。在 macOS 或
   Linux 上，请说明你用的是哪个包、它到底打不打得开。
2. 在 <https://github.com/breakevery/riscdom/issues> 开 issue 并**贴上填好的表**；若你更愿意放进仓库，放
   进 `walkthroughs/`。
3. 没人写下的走查，就是没人能核对的走查 —— 填好的表就是报告。

## 已知限制

- **M 的跨区域级属 v1.x。** 参考 M（`examples/python`）读一个 **workgroup**（`--level lan`：`/v0/online`、
  `/v0/peers`，以及每个以文件给出 token 的节点）。它上面那一级 —— 一列 LAN M，以及一个 `--config` 文件
  —— **不在 v1.0 范围内**（决策 §157、纲领 §13）。
- **`/v1/` 不是路径。** v1.0 发布的是 `/v0/`；前缀在下一个协议破坏性变更时才动，不在这里。有四个文档曾
  说反了，已在冻结声明同一批里改正（决策 §160）。
- **内嵌的 `--follow` / `--wait` 有粗糙边**：进程退出时流可能仍开着。这是已记录的粗糙边，被报告、没有
  被悄悄绕过。
- **Python 不是 guest 语言。** 有 C、Zig、Rust；Python 等 Linux 沙箱（v1.x）。
- **session 数据库的 WAL 模式是有意不设的**（决策 §54）。
- **每节点同时一台 VM**，且审计日志**还没有保留策略**：它随使用增长，从不清理。
- **Windows 是被验证过的平台。** macOS 与 Linux 能构建，但黄金路径没在那里走过；它们的包未签名、未启动。
- **发布走查还没发生**：由别人持真实 API key 做的干净机器走查，仍悬着。
- **Windows 服务器归档是手工构建的。** CI 在 `v*` tag 上构建 Linux 与 macOS 归档；没有 Windows runner，
  所以 Windows 的 `riscdom-server-*.zip` 是在一台机器上手工组装的（v0.9.9 时也一样）。

## 安全

本项目**不附带**任何 API key：所有模型访问都是自带密钥。密钥从不离开你的机器，审计日志住在 AI 工作区之外
且只可追加，任何东西都不上传。控制平面默认绑定 loopback，除非以 `--no-auth` 启动，否则需要 token；浏览器
看板从数据目录读同一个 token。节点身份是 `<data-dir>/node.key` 或 OS 钥匙串里的一把 Ed25519 密钥，节点
之间的每一帧都签名。**内网服务器的 token 是凭据，不写进 `settings.json`**：它住在 OS 钥匙串里，按 host
分条，写它的代理从不记录它。把看板开放给网络，在你自己打开之前是关着的；在允许其它设备之前它是 loopback；
那个开关上的警告说明了它的含义。QEMU 与 RISC-V 工具链是各自许可下的独立程序；见
[THIRD_PARTY_NOTICES.zh-CN.md](THIRD_PARTY_NOTICES.zh-CN.md)。完整声明与报告流程见
[SECURITY.zh-CN.md](SECURITY.zh-CN.md)。

## 许可

[Apache License 2.0](LICENSE)。贡献需要 [CLA](CLA.zh-CN.md) —— 见
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。
