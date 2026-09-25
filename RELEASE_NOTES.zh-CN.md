[English](RELEASE_NOTES.md) | 中文

# RiscDom v0.9.0

> **这是正式发布，不是预览版 —— 与 v0.8.0 同样的两条 caveat。** **macOS 与 Linux 的包由 CI 构建，
> 从未在真实机器上启动过**，而且它们**未签名**（macOS 的 Gatekeeper 会拦下首次运行；Windows 的
> SmartScreen 会对安装包告警）。并且 **Windows 仍是黄金路径被验证过的平台**：由别人在干净机器上
> 走一遍这件事，仍未发生。

**v0.9 一句话：** 项目变得**可驱动、可见** —— 一个带真实鉴权与实时事件流的控制平面、一个能说它的
命令行客户端、一个能读它的浏览器看板 —— 沙箱在 C 之外多了两种语言（Zig 与 Rust）；而多 Agent 部分
交付的是**接口**（执行者名册、派发端点、远程句柄），还不是协作策略。

## 本版新增

- **控制平面是一个真正的 API。** 32 个查询端点与 36 个控制端点，全部位于 bearer token 之后，每条路由
  都声明它需要的 capability —— 不持有就得到 `403`，而不是被猜。事件流（SSE）能从 `Last-Event-ID` 补发，
  游标早于缓冲时发出 `gap` 帧，于是读者知道自己漏了什么，而不是无声地继续。项目可以作为**一个归档**导出
  与导入，`POST /v0/tasks` 能把任务派给已配置的执行者。整个表面由两份文档冻结，测试直接读那些表格，因此
  端点不可能不带上它的工具定义就落地。
- **一个命令行客户端 `riscdom`。** 它要么对运行中的 `riscdom-server`（`--remote host:port`）说话，要么
  对**在自己进程内**启动的控制平面说话 —— 同一条代码路径，从不直接调用宿主。`--json` 原样透传控制平面的
  应答，退出码有文档（`0` 成功、`1` 本地失败、`2` 用法错误、`3` 被拒或 `5xx`、`4` 未认证或无权）。
  `--follow` / `--wait` 会在活儿还在干的时候把事件流出来。
- **沙箱里除 C 之外还能编 Zig 与 Rust。** `compile` 按源扩展名分派：`.zig` 走
  `zig build-exe -target riscv64-freestanding`，`.rs` 走
  `rustc --target riscv64gc-unknown-none-elf --sysroot …`。两者都在*设置 → 工具链*里一键可装——
  那正是上一个版本留下的下载缺口；而厂商发布的三种归档（`.zip`、`.tar.gz`、`.tar.xz`）都走同一个
  带 Zip-Slip 守卫的解包器。Rust 的 sysroot 是唯一**与产出它的版本硬绑定**的产物：版本与机器上
  `rustc` 不符时，宿主**在下载开始之前**就拒绝。沙箱成为可命名的对象（定义、扫描、合并、切换），
  而 AI 可以**申请**一次沙箱改动、由人来裁决。
- **管理程序。** 桌面外壳与浏览器共用**同一份构建产物**：`riscdom-server --web-root ui/dist` 提供的
  `dist/` 就是桌面端打包的那一份，于是节点可以在手机上看见，且与它的 API 同源。浏览器端是**只读看板**
  —— 它仅保留的两个控制是显示偏好（主题与语言）—— 带登录门、带三个子 tab 的节点页
  （Status / Executors / Sandboxes），以及走同一条事件流的实时刷新。看板不假装：属于桌面的控制会
  明说自己属于桌面，而不是被按下时才报错。
- **接口交付。** 执行者名册可读（`GET /v0/executors`），任务可派发（`POST /v0/tasks`），而 v0.8 留下的
  那道缝 `agent::AgentHandle` 也有了另一半：`HttpExecutorHandle` —— 一个把任务形状的 body POST 给另一
  个节点的远程执行者参考实现。两份 tool schema 文档分别描述「执行者的模型可以调什么」与「AI 监工可以调
  什么」，每份都对着它所描述的代码做校验。还有一个 Python 参考监工
  （`examples/python/dispatch.py`，只用标准库），端到端驱动一个节点。
- **工程质量。** gate 在每个平台都是同一份清单 —— 每个 crate 在每个平台都被 lint 与 check，两个 Tauri
  crate 也在内 —— 双语规则由脚本强制，而新增的**编码护栏**会在两类「编译器看不见的 Windows 代码页
  损坏」上让构建失败。四个「本地绿、CI 红」的机制被找到并从根上修掉：缺一个系统包、本机恰好具备的
  一种能力、测试已停止读取的管道上的 SIGPIPE、以及兄弟进程「已 fork 但尚未 exec」造成的 `ETXTBSY`。
  反向的那一种也记在案：一个只在开发机上偶发失败的 guest 启动测试。

## 按平台安装

- **Windows 10/11** —— 从 release 附件取 `RiscDom_0.9.0_x64_en-US.msi` 或
  `RiscDom_0.9.0_x64-setup.exe`。安装包**未签名**，所以 SmartScreen 首次会告警
  （「更多信息 → 仍要运行」）。QEMU 与 RISC-V 裸机 GCC 不随包分发：*设置 → 工具链*会引导你
  `winget install SoftwareFreedomConservancy.QEMU`（或官方页面），也能自己下载 xPack GCC。
- **macOS** —— CI 的 `bundle` job 构建 `RiscDom_0.9.0_aarch64.dmg`（Apple Silicon）以及其中的 `.app`。
  它们**未签名**，因此 Gatekeeper 会拦下首次启动：右键 App → *打开*，或执行一次
  `xattr -dr com.apple.quarantine /Applications/RiscDom.app`。**QEMU 不随包分发**：`brew install qemu`。
  **还没有人在真实 Mac 上启动过这些包。**
- **Linux** —— 同一个 job 产出：`RiscDom_0.9.0_amd64.deb`、`RiscDom-0.9.0-1.x86_64.rpm` 或
  `RiscDom_0.9.0_amd64.AppImage`。**QEMU 不随包分发**：装你发行版的 `qemu-system-riscv64`
  （例如 `sudo apt install qemu-system-misc`、`sudo dnf install qemu-system-riscv`）。
  **还没有人在真实 Linux 机器上启动过这些包。**

## 验证了什么 —— 以及没验证什么

**已验证**

- gate 共 17 步（其中两步可选）：`cargo fmt`、对每个 crate 在每个平台跑 `cargo clippy -D warnings`、
  `cargo check`、完整的 `cargo test`、`npm run build`、13 个 UI 探针、镜像常量守卫、编码扫描、
  tool schema 校验、两个示例自测、wix 版本守卫、UI 字符串注册表，以及双语文档检查。本地全绿；CI 在
  `ubuntu-latest` 上跑同一个脚本。
- 测试套件：**688 个测试 / 118 个套件**。没有 QEMU 与 RISC-V GCC 时 —— 也就是 CI 看到的那种形态 ——
  gate 跑出 **625 passed / 0 failed / 63 ignored**，且每个被忽略的测试都写明了它缺什么前提；在具备这些
  工具的机器上，被忽略的那批会一起跑，只跳过三个需要 API key 或会写 OS 钥匙串的。
- **控制平面的表面**端到端：鉴权与 capability、带补发与 `gap` 的 SSE 流、项目导出/导入往返、
  派发端点、沙箱注册表、申请队列。被文档锁死的路由表由测试直接读取，而不是被抄一份。
- **两种新语言**端到端且离线：一个 Zig 与一个 Rust 源文件被编译到裸机目标、归档被解开、产物被采用、
  Rust 的版本拒绝生效。两者的 guest 启动测试都带标记，在有 guest 的机器上会跑。
- **Web 看板**对真实服务端：登录门、状态页与它的三个子 tab、SSE 刷新，以及只读包裹（探针逐屏数包裹点，
  因此一个新控制忘了藏起来会让 gate 变红）。
- 对真实 QEMU guest 的黄金路径 **3–7 步**端到端走查，自 v0.5.0 以来未变。

**未验证**

- **macOS 与 Linux 包本身。** 它们能编译、能打包；还没有人在真实 Mac 或 Linux 机器上安装或启动过它们。
  那是下一次走查。
- **干净机器走查。** 仍是它原本的那项补强项：[docs/golden-path-checklist.md](docs/golden-path-checklist.md)
  是那份表，`walkthroughs/` 是填好的表该放的地方。
- **多 Agent 协作。** v0.9 交付的是接口 —— 名册、派发端点、远程句柄、申请队列。由哪个 AI 分解一个
  复合任务、各分片怎么分，留给了用户与后续版本。
- **安装包与包都未签名** —— Windows 上 SmartScreen、macOS 上 Gatekeeper。签名与公证属于商业化层的项。

## 测试者需要什么

- 一个**模型提供方**与**你自己的 API key**（或本地提供方，如 Ollama / LM Studio）、**QEMU**
  （`qemu-system-riscv64`，由你自己安装 —— 见上面的平台说明），以及**一个 RISC-V 裸机 GCC**
  （xPack `riscv-none-elf-gcc` 或等价的 `riscv64-unknown-elf-gcc`；App 可以下载 xPack 那个）。
  Zig 与 Rust 是可选的：*设置 → 工具链*两者都能装。

## 如何反馈

1. 打开 **[docs/golden-path-checklist.md](docs/golden-path-checklist.md)** 边走边填。在 macOS 或 Linux 上，
   说明你用的是哪个包、它到底有没有打开。
2. 在 <https://github.com/breakevery/riscdom/issues> 开一个 issue 并**贴上填好的表**；如果你更愿意走仓库，
   就把它放进 `walkthroughs/`。
3. 没有人写下来的走查，就是没有人能核对的走查 —— 填好的模板就是报告。

## 已知限制

- **`task_id` 没有带进这次运行。** `POST /v0/agent/run` 不接受 `task_id`，而 `--follow` 以单客户端为前提：
  多个客户端同时跟一个节点时，一个事件帧无法归给产生它的那个任务。这要在 v1.0 之前解决。
- **内嵌的 `--follow` / `--wait` 有粗糙边**：进程退出时流可能还没结束。这是已上报的粗糙边，没有被悄悄绕开。
- **v0.9 交付的是多 Agent 的*接口*，不是策略。** 名册、派发端点、远程句柄与沙箱申请队列都已就位并有测试；
  由谁分解一个复合任务、分片如何指派，本版不做这个决定。管理程序在 v0.9 里也**仍在本仓库内** ——
  到 v1.0 才独立成仓。
- **发布走查未做**：由别人在干净机器上、用真实 API key 走一遍，仍未发生 —— 与 v0.8.0 同一 caveat。
- **Windows 是已验证平台。** macOS 与 Linux 能构建，但黄金路径未在那里走过；它们的包未签名、未启动过。
- **每次一个 VM**（per agent），并且审计日志还没有保留策略：它随使用增长，从不被裁剪。

## 安全

本项目**不分发**任何 API key：所有模型访问都是自带 key。key 从不离开你的机器，审计日志住在 AI 工作区
之外且只可追加，任何东西都不会被上传。控制平面默认只绑回环，且除非以 `--no-auth` 启动，都要求 token；
浏览器看板从数据目录读同一个 token。QEMU 与 RISC-V 工具链是各自许可证下的独立程序；见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。完整声明与上报流程在 [SECURITY.md](SECURITY.md)。

## 许可证

[Apache License 2.0](LICENSE)。贡献需要 [CLA](CLA.md) —— 见 [CONTRIBUTING.md](CONTRIBUTING.md)。
