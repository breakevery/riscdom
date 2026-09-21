[English](RELEASE_NOTES.md) | 中文

# RiscDom v0.7.0

> **这是正式版，不是预览版 —— 但带两条限制。** **macOS 与 Linux 的安装包由 CI 构建，从未在真机上启动过**，
> 而且**未签名**（macOS 的 Gatekeeper 会拦下首次启动；Windows 的 SmartScreen 会对安装包告警）。另外，
> **黄金路径验证过的平台仍是 Windows**：由他人进行的干净机器走查尚未发生。两条限制的细节见下文。

**v0.7 一句话说：** 界面现在能说两种语言（自建字符串注册表 + *设置 → 外观*里的语言切换），同一份源码现在
能在 Windows、macOS 与 Linux 上构建 —— 而 QEMU 指引随平台变化，并告诉你应用不打包的那些东西该怎么装。

## 本版新增

- **自建 i18n 设施与语言切换**（v0.7 批次 1–2）：`ui/src/i18n/` 是一套双语字符串注册表，**不引任何第三方
  库**；`scripts/check-ui-strings.mjs`（在 gate 里，带自测）在任一语言缺键时让构建失败；*设置 → 外观*提供
  **跟随系统 / 中文 / English**，写入 `settings.json`、改写 `<html>` 的 `lang`，并即时重渲染。v0.6 那四条
  diff 字符串是试点，而**把其余约 190 条界面字符串一并翻译这件事有意不做**：价值在设施，而不在把一个内核
  形态的工具翻一遍。
- **macOS 与 Linux 构建**（v0.7 批次 A–B）：同一份源码能在 macOS（aarch64）与 Linux（amd64）上构建。
  CI 的 `bundle` job 在两个 runner 上执行 `npm run tauri build`，并把结果作为 workflow artifact 上传 ——
  `.app` + `.dmg`，以及 `.deb` + `.rpm` + `.AppImage`。QEMU 安装指引随平台变化（`winget` / Homebrew /
  发行版包，见 `sandbox::qemu_discover::install_hint_for`），macOS 打包所需的 `icons/icon.icns` 已补齐，
  Unix 的 `-qmp unix:` 参数由一条跨平台单测钉住。
- **`host` 在非 Windows 上重新能编译**（v0.7 批次 8）：一个 Windows 专属的 `extract_zip` 被一段与平台无关的
  `match` 调用了，于是所有 macOS/Linux 构建都死在 `error[E0425]`。现在非 Windows 平台得到一个同名存根，
  返回 “zip archives are not supported on this platform”。这个缺陷是新 `bundle` job 发现的 —— 那是 `host`
  第一次在非 Windows 上被编译。

## 按平台安装

- **Windows 10/11** —— 从 Release 附件取 `RiscDom_0.7.0_x64_en-US.msi` 或
  `RiscDom_0.7.0_x64-setup.exe`。安装包**未签名**，因此 SmartScreen 首次运行会告警（「更多信息 → 仍要
  运行」）。QEMU 与 RISC-V 裸机 GCC 都不打包：*设置 → 工具链*会引导你执行
  `winget install SoftwareFreedomConservancy.QEMU`（或官网），并可自行下载 xPack GCC。
- **macOS** —— CI 的 `bundle` job 产出 `RiscDom_0.7.0_aarch64.dmg`（Apple Silicon）及其中的 `.app`；
  在对应 workflow run 的 *Artifacts* 里下载。它们**未签名**，因此 Gatekeeper 会拦下首次启动：右键应用 →
  *打开*，或执行一次 `xattr -dr com.apple.quarantine /Applications/RiscDom.app`。**QEMU 不打包**：
  `brew install qemu`。**这些安装包尚未在真 Mac 上启动过。**
- **Linux** —— 同一个 job 产出 `RiscDom_0.7.0_amd64.deb`、`RiscDom-0.7.0-1.x86_64.rpm` 或
  `RiscDom_0.7.0_amd64.AppImage`。**QEMU 不打包**：安装发行版提供的 `qemu-system-riscv64`（例如
  `sudo apt install qemu-system-misc`、`sudo dnf install qemu-system-riscv`）。**这些安装包尚未在真
  Linux 机器上启动过。**

## 验证了什么 —— 以及没验证什么

**已验证**

- gate（13 步）：`cargo fmt`、`cargo clippy -D warnings`、`cargo check`、完整 `cargo test`
  （**295 passed / 0 failed / 8 ignored**，80 个套件）、`npm run build`、八个 UI 探针、镜像常量守卫、
  wix 版本守卫、UI 字符串注册表检查与双语文档检查。本地与 CI 均全绿。
- **macOS/Linux 构建路径**，端到端：`bundle` job 在 run `35572294916` 里产出并上传了
  `.app`/`.dmg` 与 `.deb`/`.rpm`/`.AppImage` —— 也正是它证明了 `host` 现在能在非 Windows 上编译。
- **i18n 设施**：注册表的完整性检查（每个键两种语言都有）在 gate 里、带自测；语言切换的规则 —— 三态选择、
  `lang` 属性、即时重渲染、持久化 —— 由语言探针钉住。
- **第 8 步的界面已人工走查**（由项目所有者走查；结论通过）。v0.6 那次比较的数据层与 API 仍保留 6 个单元
  测试 + 4 个集成测试。
- **第 3–7 步**对真实 QEMU guest 的端到端走查，自 v0.5.0 起未变
  （`cargo test -p host --test golden_path -- --ignored`）。

**未验证**

- **macOS 与 Linux 的安装包本身。** 它们能编译、能打包；但没有人真 Mac / 真 Linux 机器上安装、启动或走查过。
  那是下一轮走查。
- **干净机器走查。** 仍是原来的 v0.5.x 补强项：[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)
  是表单，填好的放 `walkthroughs/`。
- **界面大部分仍是中文。** 注册表与切换器已经有了；其余约 190 条字符串在本版中有意未翻译。
- **Unix socket 的 QMP 仍未实现**（比较等一切都走 TCP）。
- **安装包与各平台包均未签名** —— Windows 上是 SmartScreen，macOS 上是 Gatekeeper。签名与公证属于
  商业化层的工作。
- 界面里的中文输入仍只是间接验证过。

## 测试者需要什么

- 自备**模型服务商**与 **API key**（或用 Ollama / LM Studio 之类的本地服务）、**QEMU**
  （`qemu-system-riscv64`，由你自行安装 —— 见上面的平台说明），以及**一份 RISC-V 裸机 GCC**
  （xPack `riscv-none-elf-gcc` 或等价的 `riscv64-unknown-elf-gcc`；应用可以帮你下载 xPack 那份）。
- 对比较功能：**两次已结束、且指纹互不相同的 run** —— 在两次之间改一个配置字段（比如模型）。尚未结束的
  run 同样有指纹、同样能比较；只有导出会拒绝它们。

## 如何回报

1. 对照 **[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)** 走，边走边填 ——
   并补上差异视图的表现：区块是否展开、是否列出每一个字段、被标出的不同行是否正是你改过的那些、标题统计出
   的数字是多少。在 macOS / Linux 上，请说明你用的是哪个包、以及它能否打开。
2. 在 <https://github.com/breakevery/riscdom/issues> 开 issue 并**粘贴填好的清单**；若你偏好放进仓库，
   就放到 `walkthroughs/`。
3. 没人写下来的走查，就是没人能核对的走查 —— 填好的模板就是报告。

## 已知限制

- **验证过的平台是 Windows。** macOS/Linux 能构建，但黄金路径尚未在那里走过；它们的安装包未签名、也未启动过。
- **界面大部分仍是中文**：双语注册表与切换器已就位，但只有少数几条字符串注册了两种语言。
- **差异区只做一层**：它把指纹的顶层字段作为整体值比较，不进入字段内部。
- **没有增量快照或加密快照**；会话存储是本地明文 SQLite。
- **同时只支持一台 VM。**
- **审计日志暂无保留策略**：它随使用增长，永不清理。

## 安全

本项目**不携带** API key：所有模型访问都是自带密钥。密钥不离开你的机器，审计日志位于 AI 工作区之外且
只可追加，任何内容都不上传。QEMU 与 RISC-V 工具链是各自许可下的独立程序；见
[THIRD_PARTY_NOTICES.zh-CN.md](THIRD_PARTY_NOTICES.zh-CN.md)。完整声明与报告流程见
[SECURITY.zh-CN.md](SECURITY.zh-CN.md)。

## 许可证

[Apache License 2.0](LICENSE)。贡献需要签署 [CLA](CLA.zh-CN.md) —— 见
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。
