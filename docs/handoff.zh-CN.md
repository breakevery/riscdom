[English](handoff.md) | 中文

# 交接 —— 把 RiscDom 带进下一个对话

**本文件是跨对话交接文档。** 第 1 节是易变快照，正式版发布时更新；第 2–12 节是稳定约束：它们在各批次
之间没有变过，也是新对话必须守住的东西。

仓库 `D:\codeagent\breakevery\riscdom`，远端 `https://github.com/breakevery/riscdom.git`，分支
`main`。每个批次的收尾流程一致：gate 全绿 → `scripts\commit.ps1 "<msg>"`（它自己会跑 gate）→ push ——
而这些面向远端的动作，只在当轮请求明确授权时才做（见 §2）。

## 1. 快照 —— `v0.6.0-preview.1` 已作为预发布版发出；Latest 仍是 `v0.5.0`（下次发布时更新本节）

- **v0.7 已开工，但尚未发布。** 两条线并行：**自建 i18n 设施**（v0.7 批次 1–2 —— `ui/src/i18n/`、
  gate 里的注册表检查、以及*设置 → 外观*里会写入 `settings.json` 并把 `<html>` 的 `lang` 跟着改的
  语言切换），和 **macOS/Linux 支持**（批次 3 只做侦察，批次 A 是第一批代码）。**用注册表铺开其余界面
  字符串这件事挂起了**：设施与那 4 条 diff 字符串保留，190 行的全量翻译不做 —— 价值在设施，而不在把
  一个内核形态的工具翻一遍。工作提交：`06fef0a`（可用的语言切换）← `6abcb44`（i18n 试点）← `fff3b7c`
  （快照指向）。
- **macOS 与 Linux：已决定，本地能做的部分已完成。** macOS **不签名** —— Developer ID 签名与公证属于
  商业化层，不属于 v0.7 —— 因此 Gatekeeper 会拦下首次运行，发布说明必须写清如何照常打开。目标形态是
  *能构建、能启动、golden path 手动跑一次*。批次 A（本地、Windows）新增 `icons/icon.icns`，让 QEMU 安装
  指引随平台变化（`winget` / Homebrew / 发行版包，见 `sandbox::qemu_discover::install_hint_for`），为
  Unix 的 `-qmp unix:` 参数加了跨平台单测，并把 QEMU 预检里的建议改成不再指向单一平台。仍未做：多 OS
  CI matrix（macOS 只在 release / 手动触发）、macOS 构建本身、以及真实 Unix socket 的 QEMU 运行 ——
  后两项需要一台 Mac 或 Linux 机器。
- **`v0.6.0-preview.1` 已作为预发布版发布**（v0.6 批次 1–2，批次 4 发布）：两次 run 逐字段对比 —— 数据层
  与 API（[../host/src/run_diff.rs](../host/src/run_diff.rs)、`AppState::compare_run_fingerprints`、
  `compare_run_fingerprints` 命令）以及审计页两 run 面板下方那个默认折叠的区块。预发布版**不持有 Latest
  标记**，因此 `v0.5.0` 仍是 Latest 正式版。附件：`RiscDom_0.6.0-preview.1_x64_en-US.msi` 与
  `RiscDom_0.6.0-preview.1_x64-setup.exe`，构建时带 `bundle.windows.wix.version = "0.6.0"`（WiX 不接受
  预发布版 `ProductVersion`），所以「应用和功能」里显示 `0.6.0`，而产物名保留包版本。它证明了什么、
  没证明什么写在 [RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) —— 缺口中的第一条：**没有任何人
  走过新界面**，这就是它是预览版的原因。
- **`v0.5.0` 已发布，它就是 Latest。**
  <https://github.com/breakevery/riscdom/releases/tag/v0.5.0> —— 附件为 `RiscDom_0.5.0_x64_en-US.msi`
  与 `RiscDom_0.5.0_x64-setup.exe`，构建时已去掉预览版的 MSI 版本覆盖（因此「应用和功能」里显示
  `0.5.0`）。它证明了什么、没证明什么，写在 [RELEASE_NOTES.zh-CN.md](RELEASE_NOTES.zh-CN.md)。
- **`v0.5.0-preview.1` 作为历史保留**（它是预发布版，所以直到本版发布前 Latest 一直由 `v0.4.0` 持有）。
  它的附件仍留在原处。
- **走查记录已有一份，且是本地那次**：[../walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md)
  —— 七步全过，真实模型 + 真实 key，用的是**安装版 MSI**；但**本机不是干净环境**（QEMU 与 RISC-V GCC
  早已装好）。它发现的问题已修复（S-1 / G-1 / E-1 / E-2 / E-3 在批次 11，G-4 在批次 12）；G-2
  （改模型未生效）与「在聊天框里输入中文」仍需真人用键盘复核，G-3（「已安装的应用」里的 `0.5.0.1`）
  已随覆盖本身一起消失。
- **外部走查仍未发生。** 它本是本版的计划，但没有发生，因此「干净机器走查」现在是 **v0.5.x 的补强项**
  （§8），而不是阻塞项：[golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md) 是测试者要填的
  表单，`walkthroughs/` 是它该去的地方。
- 近期提交（新→旧）：`b0efeb8`（v0.6.0-preview.1 发布：版本 bump、变更日志、发布说明）← `6c5ddb65`
  （变更日志与交接快照）← `b5aac541`（审计页里的字段级差异）← `4d407cea`（指纹 diff：数据层 + API）←
  `287ffdb`（v0.5.0-preview.1 发布 bump）← `2a5d296`（README/CLA 措辞）← `993e5d1`（CLA）←
  `f662ad7`（自足导出）。
- tag：`v0.6.0-preview.1` 即本次预览（给本文件定版的那个提交）；`v0.5.0` 是它之前的正式版，也是持有
  Latest 标记的那个；`v0.5.0-preview.1` =
  `cea44f7b9920a079422217f811afb49350e08477` → `287ffdb095e1659b89a8cafe040647ada64d0026`；
  `v0.4.0` = `25bd3da3c31c1d1ec7e163f3835b0c2bbb74546d` → `15fda1f6d76d53a4ff1b621c2d3d91f0b4b87311`；
  `v0.3.1` = `d8fdba66a366632ca569d8db2657ab5a566b991c` → `b9be9111c620faad686c7a9d095e0ebc04b31225`。
- 发布提交 `b0efeb8` 处的测试总况：**291 passed / 0 failed / 8 ignored / 80 suites**
  （v0.5.0 为 281 / 0 / 8 / 79）。gate 共 13 步（v0.7 批次 1 新增了 UI 字符串注册表；UI 探针无论跑多少个
  文件都算一步），本地与 CI 均全绿（`ubuntu-latest` 上跑 `scripts/gate.sh`，另加 gitleaks）。
- 未完成项：`%TEMP%` 下的临时目录仍未清理（删除确认始终未被放行；2026-09-21 统计到 144 个
  `riscdom-*` 条目）；CLA.md 待律师过目；macOS/Linux 已决定但只做了一半（见上面的批次 A）；**由他人进行的
  干净机器走查尚未发生** —— 仍是 v0.5.x 的补强项，而不是阻塞项；
  **预览版的界面尚未经人工走查** —— 已在 [RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) 披露，
  也是第 8 步走查要做的第一件事。

## 2. 远端操作按轮授权，且必须有明确文字

push、打 tag、创建或删除 Release、移动或删除远端 tag，以及任何其他远端写操作，**仅当当轮请求用文字
明确说出时**才执行。对话框里的选项、推断出的意图、上一批次的授权、或「这显然是下一步」都不构成授权。
固定的收尾（gate → commit → push）属于「请求里写了它」的批次，而不是默认动作。

## 3. 提交信息：ASCII，否则 `git commit -F`

在 Windows 上 `git commit -m "…"` 会把信息经控制台 ANSI 代码页传递，因此非 ASCII 文本**在 git 看到它
之前**就被替换成 `?`（`0x3F`）—— 本机 2026-09-18 实测：探针标题 `test: 中文正文测试` 被存成
`test: ?????????`。因此：

- `-m` 的信息一律用 ASCII（英文）书写；
- 必须含非 ASCII 时，把信息写成 **UTF-8（无 BOM）** 的文件，用 `git commit -F <file>`；
- `scripts/commit.ps1 "<msg>"` 以参数接收信息，同样受此约束。

同类事故还有第二道门：**PowerShell 重定向**。`>` 与 `Out-File` 默认写 **UTF-16LE**，于是
`git show … > file`（或任何重定向文本的命令）留下的文件首字节是 `FF FE` —— 按 UTF-8 读它的工具会
看到开头的乱字符（v0.5 批次 12 在比对旧版修订时就撞上过）。两个习惯能同时堵住两道门：文本一律经由
**显式指定编码的文件**写入，并且以**文件**而不是命令行参数传递 —— 被吃掉字符的正是 argv 那条路。
要普查整棵树，手动跑 `python3 scripts/scan-encoding.py` —— 它是诊断工具，**不是** gate 检查
（§4 解释了它为何不进门禁：它对「只含 `?` 的字面量」这条判据无法区分真损坏与合法代码）。

## 4. gate 是「绿」的唯一清单

`scripts/gate.sh` 按顺序持有每一项检查。`scripts/gate.ps1` 与 `scripts/commit.ps1` 只是薄包装：它们
定位 Git 的 `sh.exe` 并运行那个文件，绝不保留第二份命令清单。CI 跑的是同一个 `sh scripts/gate.sh`。
**新增检查写进 `gate.sh`**（并同步 `CONTRIBUTING.md` 里的那份简短清单），不要写进工作流、README 或
某个人的记忆里。

## 5. 审计不变量

以下冻结、永不更改：链结构（`audit_events` 及其 append-only 触发器）、哈希公式、历史行，以及
`audit-verify` 与 `audit-rebuild` 的语义。只读面可以扩展 —— 导出、过滤、带迁移的派生索引加列 —— 前提是
不碰链，且校验器不持有任何写路径。

## 6. CLA 处于待命状态，不是活跃流程

- `CLA.md`（以英文为准）与 `CLA.zh-CN.md`；`CONTRIBUTING.md` 里的条件式 CLA 章节；
  `.github/workflows/cla.yml` 运行**自托管**的 CLA Assistant action（无需安装 GitHub App），触发器为
  `pull_request_target` 且**不检出 PR 代码**；`signatures/version1/cla.json` 已预创建。
- 第 3 条（双许可与再许可）是商业化的关键授权，在任何人依赖它之前**需要律师过目**。文件本身写明：
  只有项目所有者确认后才生效。
- 签署句**不翻译** —— 机器人按 `I have read the CLA Document and I hereby sign the CLA` 精确匹配。
- 未来贡献可能被接收到本仓库以外，所以 CONTRIBUTING 的措辞是条件式。

## 7. QEMU：引导安装，绝不下载或捆绑

v0.4 定案：应用只告诉用户装什么（`winget install SoftwareFreedomConservancy.QEMU`，或官网页面），然后
找到并记住、验证它。理由 —— 上游没有可钉的 Windows 二进制、第三方打包者会变成没被点名的供应链环节、
以及不让自己成为 GPL-2.0 二进制的分发者 —— 见 [qemu-distribution.zh-CN.md](qemu-distribution.zh-CN.md) §5。

## 8. 发布门槛是「走查」，不是「代码写完」

v0.5 的发布条件是：有人在干净机器上用真实 API key 走完第 1–2 步，并对照
[golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md) 记录 —— 机器、操作系统版本、QEMU/GCC
版本**及其来源**、服务商与模型、预检四步、两次 run 的短指纹、导出文件与其 `audit-verify` 结论，以及
任何失败的原样错误。那次走查在 `v0.5.0` 发布前**没有发生**，因此它是 **v0.5.x 的补强项**而不是阻塞项
（§1），由 [../walkthroughs/2026-09-19-preview1-local.md](../walkthroughs/2026-09-19-preview1-local.md)
那份本地走查代位。第 3–7 步由 `cargo test -p host --test golden_path -- --ignored` 覆盖；第 8 步的
比较目前**没有**这样的自动走查（§9）。「实现完成」不是门槛。

## 9. v0.6 从黄金路径第 8 步开始 —— 该步已交付

v0.6 是两次 run 的自动比较 —— 它们的指纹差在哪些字段 —— 这正是 v0.5 刻意不做完的那一步。**已交付**
（批次 1–2：`host/src/run_diff.rs`、`AppState::compare_run_fingerprints`、`compare_run_fingerprints`
命令，以及审计页两 run 面板下方默认折叠的字段级区块）；等待走查与发布。
`PROJECT_CONSTITUTION.md` §10 的 v0.5 路线图里那些并行项（QEMU stdio、macOS/Linux、多 VM、增量快照、
会话加密、多 AI、双语界面）**不是** v0.6 的内容：它们是待选项，可挑可弃。

## 10. 文档双语，且有门禁

仓库里每个 `*.md` 都要有成对的另一语言版本与首行精确的语言切换行（`X.md` ↔ `X.zh-CN.md`）；
`scripts/check-bilingual.sh` 只报告、绝不改文件。新增文档即新增一对，且在同一个提交里。

## 11. 在本机构建安装包需要真实 Node

本工作区 `PATH` 上的 `node` 是 LobsterAI 的 Electron-as-node 垫片
（`…\LobsterAI\cowork\bin\node.cmd` → `ELECTRON_RUN_AS_NODE=1 "<electron>" %*`）。在它之下，Tauri CLI
的原生插件会读错 `argv`，所有打包命令都以
`error: unrecognized subcommand '<…>\LobsterAI.exe'` 失败。绕过方式：用真实 Node 运行 CLI —— 例如
`C:\Users\cloud_user\AppData\Local\Programs\Tuanjie Cowork\cli\bin\win32-x64\node.exe`（v24.16.0）——
在 `ui/` 下执行 `node node_modules\@tauri-apps\cli\tauri.js build`。WiX 3.14 与 NSIS 已缓存在
`%LOCALAPPDATA%\tauri`，打包不需要网络。

## 12. `wix.version` 是预览版专用覆盖，且有守卫

带预发布后缀的包版本**不是**合法的 MSI `ProductVersion`（WiX 只接受 `major.minor.patch.build`，纯数字），
因此 `ui/src-tauri/tauri.conf.json` 里带着 `bundle.windows.wix.version = "0.5.0.1"`，而包版本 —— 也就是
产物名 —— 仍是 `0.5.0-preview.1`。**包版本重新变成纯数字后立即删除该字段**；残留的值会让 MSI 的
ProductVersion 与其他所有产物、tag、文档悄悄不一致，而且不会报任何构建错误。
`scripts/check-wix-version.mjs`（在 gate 里，带自测）正是在这种情况下让构建失败。
