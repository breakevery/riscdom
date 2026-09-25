[English](RELEASE_NOTES.md) | 中文

# RiscDom v0.9.9

> **这是正式版，与 v0.9.0 同样的两条 caveat。** **macOS 与 Linux 的包由 CI 构建，从未在真实机器上
> 启动过**，而且它们**未签名**（macOS 的 Gatekeeper 会拦下首次运行；Windows 的 SmartScreen 会对安装包
> 告警）。并且 **Windows 仍是黄金路径被验证过的平台**：由别人在干净机器上走一遍这件事，仍未发生。

**v0.9.9 一句话：** 桌面应用接入网络 —— 它既能**连出去**（连到内网一台 RiscDom 服务器上，看的是**那台**
节点的看板），也能**服务进来**（把自己这块看板开放给局域网上的手机与其它设备）—— 而远端凭据住在 OS
钥匙串里，不在设置文件里；第五个「本地绿、CI 红」机制也从根上修掉了。

## 本版新增

- **桌面应用可以「连出去」，连到另一个节点上。** *设置 → 网络* 收一个服务器地址和那台服务器要的令牌：
  地址是 `settings.json` 里的一个偏好，令牌是 **OS 钥匙串**里的凭据，按 `remote-token:<host>` 归档
  （`NetworkSettings` 里**根本没有** token 字段）。模式在**启动时定下一次** —— 这个窗口跟哪个 host
  说话是配置事实，不是按键 —— 所以「连接」（或「离开」）由重启应用来生效，页面上也这么写。此后桌面端就是
  那个远端节点的客户端：与浏览器同一道门、顶栏写明屏幕上是谁的节点、设置页按模式过滤（屏幕上放的是别人的
  节点时，配置**本**机的界面就不提供）。门上带着回来的路 —— **「断开并改用本机」** —— 而它后面那四个命令
  （钥匙串的三个 + 重启）**在任何模式下都作用于本机**：服务器不可达的窗口，仍必须能让自己不再做那个窗口。
- **桌面应用可以「服务进来」，把自己的看板开放到网络。** *设置 → 网络* 的另一半：一个开关、一个绑定地址
  （默认回环），以及一个「允许同网段的其它设备访问」开关 —— 勾上即给出警示。这块看板就是**这个窗口正在跑的
  那个节点**：内嵌控制平面起在应用自己的 `Arc<AppState>` 之上，**绝不是副本**（副本会有自己的 VM 槽，
  而一块能开出第二个 QEMU 的看板比没有看板更坏）。除非你另行允许，它**只绑回环**；首次启动时在
  `<data-dir>/token` 铸出自己的 token；在 `/` 提供构建好的前端、在 `/assets/*` 提供其哈希资产；设置一变就
  重绑；随窗口一起停（没有套接字会长过它）。构建好的前端作为 Tauri resource 随包分发，由同一个 helper
  解析（`tauri dev` 也走它）。
- **两个方向共用一个页面，而 token 可读、但不会被创建。** 网络 tab 显示看板的真实状态（是否在服务、绑到
  哪里、手机需要输入的地址）；「显示 token」只**读** `<data-dir>/token`、**从不创建** —— 打开一个设置页
  不该凭空造出一份凭据。浏览器一概拿不到这个页面：它能看一个节点，不能给节点重新接线。
- **工程：第五个「本地绿、CI 红」机制，以及一个仍然非机密的设置文件。** 声明 `bundle.resources` 把
  `ui/dist`（一个被 gitignore 的构建产物）变成了**编译期前置条件**，于是全新 checkout 再也跑不了
  `cargo clippy ui/src-tauri`。现在前端构建进 `ui/dist/app/`，父目录是稳定且被跟踪的，`emptyOutDir`
  保留默认 —— B-2 批记载的性质（构建产物不该是构建前置条件）恢复成立。这是五个机制里**第一个由我们自己
  的变更**而不是环境造成的，所以它被写成一条规矩。同一轮还在「明文远端 token」来得及写出任何凭据之前，
  把那个字段删掉了。

## 各平台安装

- **Windows 10/11** —— 从发行附件取 `RiscDom_0.9.9_x64_en-US.msi` 或 `RiscDom_0.9.9_x64-setup.exe`。
  安装包**未签名**，SmartScreen 会首次告警（「更多信息 → 仍要运行」）。QEMU 与 RISC-V 裸机 GCC 不随包
  分发：*设置 → 工具链* 会指给你 `winget install SoftwareFreedomConservancy.QEMU`（或官方页面），并且
  可以自己下载 xPack GCC。
- **macOS** —— CI 的 `bundle` job 产出 `RiscDom_0.9.9_aarch64.dmg`（Apple Silicon）和其中的 `.app`。
  它们**未签名**，Gatekeeper 会拦下首次启动：右键应用 → *打开*，或执行一次
  `xattr -dr com.apple.quarantine /Applications/RiscDom.app`。**QEMU 不随包分发**：`brew install qemu`。
  **还没有人在真实 Mac 上启动过这些包。**
- **Linux** —— 同一个 job 产出：`RiscDom_0.9.9_amd64.deb`、`RiscDom-0.9.9-1.x86_64.rpm` 或
  `RiscDom_0.9.9_amd64.AppImage`。**QEMU 不随包分发**：装你发行版的 `qemu-system-riscv64`（例如
  `sudo apt install qemu-system-misc`、`sudo dnf install qemu-system-riscv`）。**还没有人在真实 Linux
  机器上启动过这些包。**

## 验证了什么 —— 以及没验证什么

**已验证**

- 门禁共 **18 步**（其中 2 步可选）：`cargo fmt`、对每个 crate 在每个平台上跑
  `cargo clippy -D warnings`、`cargo check`、完整 `cargo test`、`npm run build`、**16 个** UI 探针、
  mirror guard、编码扫描、工具 schema 检查、两个示例自测、wix 版本护栏、UI 字符串注册表、双语文档检查。
  本地全绿；CI 在 `ubuntu-latest` 上跑同一个脚本。
- 测试：**688 个用例 / 118 个套件**。其中 3 个需要 API key 或会写 OS 钥匙串，被排除在门禁之外；其余在本机
  （有 QEMU 与 RISC-V GCC）全部跑过。
- **局域网看板**，在真机上：内嵌 server 起在应用自己的 state 之上、绑在设置说的地方（除非打开允许局域网，
  只绑回环）、在 `<data-dir>/token` 铸 token、在 `/` 提供构建好的前端、在 `/assets/*` 提供哈希文件、
  带 token 答 API 而不带则拒绝，并随应用一起停。
- **「出」模式，在 Windows 上人工走过一遍**，用同机的第二个节点 —— 它有自己的数据目录、因而有自己的
  token，所以这个检查是真的。把桌面端指向那个地址后，它停在**登录门**（桌面端跟自己的 host 说话时从不
  出现登录门）；点**「断开并改用本机」**会清空地址、删除钥匙串条目，并重启回本机看板。
- **兼容性**：字段删除之前写出的**真实** `settings.json` 仍能加载，并在下一次写入时丢掉那个字段。
- 黄金路径 **第 3–7 步**对真实 QEMU 客机的端到端走查，自 v0.5.0 起未变。

**未验证**

- **macOS 与 Linux 的包本身。** 它们能编译、能打包；没有人真在 Mac 或 Linux 机器上装过、启动过。这是下一
  次走查。
- **干净机器走查。** 仍是那条待加强项：[docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  是表格，填好的放进 `walkthroughs/`。
- **多 Agent 协作。** v0.9 交付的是**接口** —— 名册、派发端点、远端句柄、请求队列。由哪个 AI 拆解复合
  任务、各部件如何分工，留给用户与更晚的版本。
- **安装包未签名** —— Windows 上的 SmartScreen、macOS 上的 Gatekeeper。签名与公证仍是商业化层的事。

## 测试者需要什么

- 你自己的**模型提供方**与 **API key**（或本地提供方，如 Ollama / LM Studio）、**QEMU**
  （`qemu-system-riscv64`，由你安装 —— 见上面的平台说明）、以及一个 **RISC-V 裸机 GCC**（xPack
  `riscv-none-elf-gcc` 或等价的 `riscv64-unknown-elf-gcc`；应用可以下载 xPack 那个）。Zig 与 Rust 可选：
  *设置 → 工具链* 两个都能装。
- 网络面还需要**第二个节点**：「连出去」用同机的第二个 `riscdom-server`（**它自己的数据目录**）就够，
  「服务进来」用任何第二台带浏览器的设备就够；真有第二台机器更好。`riscdom-server --help` 列全部参数。

## 如何反馈

1. 对着 **[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)** 逐步走查，边走边填。
   macOS 或 Linux 上请写明你用的是哪个包、它到底能不能打开。
2. 在 <https://github.com/breakevery/riscdom/issues> 开 issue 并**粘贴填好的表格**；更愿意的话把表格放进
   `walkthroughs/`。
3. 没人写下来的走查，就是没人能核对的走查 —— 填好的模板就是报告。

## 已知限制

- **`task_id` 没有带进 run。** `POST /v0/agent/run` 不收 `task_id`，而 `--follow` 假设只有一个客户端：
  多个客户端跟同一个节点时，一个事件帧无法归属到产生它的任务。此事在 v1.0 之前解决。
- **内嵌的 `--follow` / `--wait` 有一条毛边**：进程退出时流可能还开着。这是已知毛边，如实报告，没有偷偷
  绕开。
- **v0.9 交付的是多 Agent 的*接口*，不是策略。** 名册、派发端点、远端句柄与沙箱请求队列都已就位并有测试；
  由谁拆解复合任务、各部件如何分配，这里没有决定。管理程序在 v0.9 也**随本仓**分发 —— v1.0 时它会独立成
  自己的仓库。
- **发布走查尚未发生**：由别人带真实 API key 在干净机器上走一遍，仍待办 —— 与 v0.8.0 同一条 caveat。
- **Windows 是已验证的平台。** macOS 与 Linux 能构建，但黄金路径没在那里走过；它们的包未签名、未被启动。
- **每个 agent 同时只有一个 VM**，且审计日志还没有保留策略：它随使用增长，从不清理。
- **网络面有一部分没走**（新增）。在登录门上输入远端服务器的 token、看板随后显示**那台**节点的数据、以及
  远端节点被停掉时窗口的行为，这里**没有**走：它们需要一份凭据，而为了让走查变成可能，没有任何凭据被放进
  过命令行。它们连同局域网步骤一起写在
  [docs/manual-acceptance.zh-CN.md](docs/manual-acceptance.zh-CN.md) 的**层次 9**。
- **一条未解释的观察**（新增）。在上面那次走查里，第一次重启后应用停在**本机**看板，且设置文件里的地址
  没了；清掉残留的开发进程后它不再复现，也没有任何代码路径能解释它。这一条被**记录**而不是被解释 ——
  写在 [docs/handoff.zh-CN.md](docs/handoff.zh-CN.md) §1 与层次 9 里 —— 由层次 9 来结案。

## 安全

本项目**不分发**任何 API key：所有模型访问都是自带 key。key 从不离开你的机器，审计日志住在 AI 工作区
之外且只可追加，任何东西都不会被上传。控制平面默认只绑回环，且除非以 `--no-auth` 启动，都要求 token；
浏览器看板从数据目录读同一个 token。**内网服务器的 token 是凭据，不写进 `settings.json`**：它住在 OS
钥匙串里、按 host 归档，落盘它的那条命令从不记录它。看板对外服务在你打开开关之前是关的，在你允许其它设备
之前只绑回环，而那个开关上的警示写明了它的含义。QEMU 与 RISC-V 工具链是各自许可证下的独立程序；见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。完整声明与上报流程在 [SECURITY.md](SECURITY.md)。

## 许可证

[Apache License 2.0](LICENSE)。贡献需要 [CLA](CLA.md) —— 见 [CONTRIBUTING.md](CONTRIBUTING.md)。
