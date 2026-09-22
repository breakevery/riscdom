[English](RELEASE_NOTES.md) | 中文

# RiscDom v0.8.0

> **这是正式发布，不是预览版 —— 与 v0.7.0 相同的两条告诫。** **macOS 与 Linux 安装包由 CI 构建，从未在真机上启动
> 过**，并且它们**未签名**（macOS Gatekeeper 会拦下首次运行；Windows SmartScreen 会就安装包告警）。并且**黄金路径
> 验证过的平台仍然是 Windows**：由他人进行的干净机器走查仍未发生。

**v0.8 一句话：** 界面已完整双语（注册表里的 190 条字符串在两种语言下都存在），而在表层之下，项目长出了一台机器上
跑多个 agent 的地基 —— 一条多进程可共写的审计链、每条事件都带身份、快照不再相撞，以及一套把任务**派发**给执行者
（而不只是内联调用）的机制。

## 本版新增

- **双语界面完整覆盖**（v0.7 批次 1–2 铺开，v0.8 收口）。**190 条注册字符串**每一条在两种语言下都存在 —— 设置页、
  工具链与 QEMU 页、审计与 run 视图、快照与会话视图、diff 面板、preflight 各屏。*设置 → 外观*里的语言切换
  （**跟随系统 / 中文 / English**）对全部界面生效。这是 v0.8 里用户**能看见的**唯一变化。
- **审计链已为多进程准备好**（v0.8 批次 2）。`audit.db` 仍是每个 workspace 一条链；连接现在以 **WAL** 打开，带 5 秒
  busy timeout 与 `synchronous=NORMAL`，一次 append 在读取 head **之前**先拿写锁（`BEGIN IMMEDIATE`），被锁的
  append 会退避重试。仍失败的写入**绝不静默丢弃**：host 记日志、发 `audit:failed` 事件，并给出横幅与弹窗。
- **每条审计事件都写明是哪个 agent**（v0.8 批次 3）。身份形状为 `<device>-<pid>-<seq>`（单机上即
  `local-<pid>-<seq>`），每进程领一个、进程内每个 agent 一个，盖在 host、agent loop、工具层与 sandbox 的事件上。
  该字段位于链**旁边**：哈希公式、`prev_hash` 链接与历史行全部不动，因此旧链仍能通过校验。
- **快照按 agent 隔离**（v0.8 批次 3）。新快照写入 `<workspace>/.riscdom/snapshots/<agent_id>/`，于是共享一个
  workspace 的两个 agent 都能保存 `snap1` 而不互相覆盖；读取会回退到共享根目录，因此本版之前拍下的快照仍可列出、
  恢复与删除。
- **最小派发抽象**（v0.8 批次 4）。`Task` / `TaskId` / `AgentId` / `TaskOutcome`，两个 trait `AgentHandle` 与
  `Dispatcher`，以及一个按任务指名的执行者路由的本地实现。**远程一半有意缺席** —— 这个缺席正是留给后续版本的缝。
- **两进程雏形，可在终端演示**（v0.8 主体交付）。`worker` 是执行者进程：stdin 进一行 `Task`、stdout 出一行
  `TaskOutcome`、事件走 stderr，每个执行者拥有自己的 data 目录。`cargo run -p worker --example dispatch` 会起一小队
  执行者、让它们共享一个 workspace 并并发派发。这是**地基，不是已交付的功能**：没有 AI 监工、没有远程执行者、也还
  没有用户可见的入口。
- **两份文档**记录了项目所在的位置：`docs/architecture-evolution.md`（四层图景与背后的决策）与
  `docs/multi-agent-foundation.md`（v0.8 定下的四个形状，按代码写，并列出仍未结的部分）。

## 按平台安装

- **Windows 10/11** —— 从 Release 附件取 `RiscDom_0.8.0_x64_en-US.msi` 或 `RiscDom_0.8.0_x64-setup.exe`。安装包
  **未签名**，因此 SmartScreen 首次运行会告警（「更多信息 → 仍要运行」）。QEMU 与 RISC-V bare-metal GCC 不打
  包：*设置 → 工具链*会引导你执行 `winget install SoftwareFreedomConservancy.QEMU`（或官方页面），并可自行下载
  xPack GCC。
- **macOS** —— CI 的 `bundle` job 产出 `RiscDom_0.8.0_aarch64.dmg`（Apple Silicon）及其中的 `.app`。它们
  **未签名**，Gatekeeper 会拦下首次启动：右键 App → *打开*，或执行一次
  `xattr -dr com.apple.quarantine /Applications/RiscDom.app`。**QEMU 不打**：`brew install qemu`。
  **尚无人真机启动过这些包。**
- **Linux** —— 同一个 job 产出 `RiscDom_0.8.0_amd64.deb`、`RiscDom-0.8.0-1.x86_64.rpm` 或
  `RiscDom_0.8.0_amd64.AppImage`。**QEMU 不打**：安装发行版提供的 `qemu-system-riscv64`（例如
  `sudo apt install qemu-system-misc`、`sudo dnf install qemu-system-riscv`）。**尚无人真机启动过这些包。**

## 验证了什么 —— 以及没验证什么

**已验证**

- gate 13 步全绿：`cargo fmt`、`cargo clippy -D warnings`、`cargo check`、完整 `cargo test`
  （**330 passed / 0 failed / 8 ignored**，90 suites）、`npm run build`、八个 UI 探针、镜像常量守卫、wix 版本
  守卫、UI 字符串注册表检查与双语文档检查。
- **字符串注册表的完整性**：每个注册键在两种语言下都存在（该检查在 gate 内且自带自测），语言切换的规则由语言探针
  钉住。
- **两进程雏形**，端到端且离线：执行者协议（一行任务进、一行结果出；非法任务得到答复而非崩溃；到时不答的 worker 被
  杀掉）、监工的路由（指向不在机群里的执行者会被拒绝而非乱猜）、以及两个执行者确实是两个各自拥有 data 目录的进程。
- **审计链的多进程写入**（有并发测试）、**快照的 per-agent 隔离**、**每条事件都带身份**，各自都有测试钉住。
- 对真实 QEMU guest 的黄金路径**第 3–7 步**端到端走查，自 v0.5.0 起未变
  （`cargo test -p host --test golden_path -- --ignored`）。

**未验证**

- **macOS 与 Linux 安装包本身**。它们能编译、能出包；没有人在真机 Mac 或 Linux 上安装或启动过。这是下一步的走查。
- **干净机器走查**。仍是那条待补强的项：[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)
  是表单，`walkthroughs/` 是填好的表单该去的地方。
- **多 Agent 这部分还没有任何用户可见行为**。它是地基：零件存在且有测试，但还没有界面触达它们。
- **由他人进行的干净机器走查、以及第二个操作系统上的真实 key 走查**，与之前一样仍未发生。
- 安装包与包**未签名** —— Windows 上 SmartScreen、macOS 上 Gatekeeper。签名与公证仍是商业化层的事项。

## 测试者需要什么

- 自己的**模型服务商**与**API Key**（或本地服务商，如 Ollama / LM Studio）、**QEMU**
  （`qemu-system-riscv64`，由你安装 —— 见上文各平台说明）、以及 **RISC-V bare-metal GCC**（xPack
  `riscv-none-elf-gcc` 或等价的 `riscv64-unknown-elf-gcc`；应用可自行下载 xPack 版）。
- 做 run 对比需要：**两次指纹不同的已完成 run** —— 在两次之间改一个配置字段（比如模型）。

## 如何反馈

1. 对照 **[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)** 走查并随手填写。若在 macOS
   或 Linux 上，请注明用的哪个包、以及它是否能打开。
2. 在 <https://github.com/breakevery/riscdom/issues> 开 issue 并**粘贴填好的表单**；若你更希望留在仓库里，就放进
   `walkthroughs/`。
3. 没有写下来的走查等于没人能核对的走查 —— 填好的模板就是报告。

## 已知限制

- **Windows 是已验证平台。** macOS 与 Linux 能构建，但黄金路径未在那里走查；它们的包未签名、未被启动过。
- **多 Agent 地基是有意未完成的。** 有三处边角被点名留给下一版：派发得到的 outcome 尚不携带**执行者自己的
  身份**（只有任务被寻址到的身份）；执行者二进制仍会链接桌面工具箱（因为 `host` 无条件依赖它）；以及在共享同一
  workspace 的进程之间，环境 preflight 目录仍是共享的。
- **没有 AI 监工。** 本版里的监工是派发器 —— 一张路由表，不是一个 agent。
- **diff 区块只有一层深**：它按整值比较顶层指纹字段，不比较字段内部的键。
- **没有增量或加密快照**；会话存储是普通的本地 SQLite。
- **每个 agent 同时一个 VM。**
- **审计日志尚无保留策略**：它随使用增长，且从不清理。

## 安全

本项目**不附带**任何 API Key：所有模型访问都是自带密钥。密钥不离开你的机器，审计日志位于 AI workspace 之外且
append-only，任何东西都不上传。QEMU 与 RISC-V 工具链是各自许可证下的独立程序，见
[THIRD_PARTY_NOTICES.zh-CN.md](THIRD_PARTY_NOTICES.zh-CN.md)。完整声明与报告流程见
[SECURITY.zh-CN.md](SECURITY.zh-CN.md)。

## 许可证

[Apache License 2.0](LICENSE)。贡献需要 [CLA](CLA.md) —— 见 [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。
