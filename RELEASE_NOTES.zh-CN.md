[English](RELEASE_NOTES.md) | 中文

# RiscDom v0.5.0

**黄金路径，端到端：** 在 Windows 上装上 RiscDom，指定 QEMU 与 RISC-V 编译器，配好模型，跑一次 Agent
任务，存一个快照，导出这次 run 的审计记录，rollback 回快照，再改一个配置字段跑第二次 —— 七步，全部在
应用里手动完成。

## 本版新增

- **七步都已成为现实；自动比较还没有**（那是 v0.6）。设计、已拍板的决定与范围见
  [docs/golden-path.zh-CN.md](docs/golden-path.zh-CN.md)。
- **一次 run 的记录可自足导出**：*设置 → 审计* 把一次 run 写成 JSONL，从链的**第一条事件**写到**收束
  该 run 的那条事件**。文件锚定在 genesis，因此空库 + `audit-verify` 即可判定它，无需从产出它的机器上
  带任何东西。abandoned 的 run，其文件以 `host.run.abandoned` 标记结尾；进行中的 run 会被拒绝，而不是
  被截断。
- **run 会写明它来自哪个快照**（「恢复自 <快照名>」），并且**两个 run 可以并排对比** —— 短指纹、全量
  指纹、开始时间、状态、来源快照。
- **走查发现的问题已全部修复**：审计页的 run 行不再把内容藏在横向滚动条后面、快照弹窗不再每次建议同一个
  名字、工具调用行显示状态而不是字面量 `?`、预检句子不再冒充它自己的按钮、审计过滤器随输入实时生效、
  QEMU 也不再在应用上方弹出控制台窗口。

## 验证了什么 —— 以及没验证什么

**已验证**

- gate：`cargo fmt`、`cargo clippy -D warnings`、`cargo check`、完整 `cargo test`
  （**281 passed / 0 failed / 8 ignored**，79 个套件）、`npm run build`、七个 UI 探针、镜像常量守卫、
  wix 版本守卫与双语文档检查。本地与 CI 均绿。
- **第 3–7 步的端到端走查**（真实 QEMU guest，mock 模型驱动）：
  `cargo test -p host --test golden_path -- --ignored`，并对导出文件跑了 `audit-verify`。
- **七步走查做过一次** —— [walkthroughs/2026-09-19-preview1-local.md](walkthroughs/2026-09-19-preview1-local.md)：
  开发机、**真实模型**、**真实 API key**、**安装版 MSI**。每一步都通过。

**未验证**

- **干净机器走查。** 上面那次走查跑在一台早已装好 QEMU 与 RISC-V GCC 的机器上，因此**不能证明**第 1 步
  从零可用。由他人按 [docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md) 记录一次
  走查，本是本版的计划，但尚未发生；它作为 v0.5.x 的补强项保留。
- **macOS 与 Linux。** 仅 Windows；QMP 与串口走 TCP。
- **安装包未签名。** 首次运行时 Windows SmartScreen 会给出提示；「更多信息 → 仍要运行」是预期操作。
  本项目不下载任何东西，也不自带 key。
- 界面里的中文输入只被间接验证过（走查用的合成输入会把 CJK 弄乱，记录里已把这点标为测试局限）。

## 测试者需要什么

- **Windows 10/11**、**QEMU**（`qemu-system-riscv64`，由你安装 —— RiscDom 只引导你去 `winget` 或官网，
  既不自带也不下载）、**RISC-V 裸机 GCC**（xPack `riscv-none-elf-gcc` 或等价的
  `riscv64-unknown-elf-gcc`），以及**一个 API key**（你所选服务商的；也可用 Ollama / LM Studio 这类
  本地服务）。

## 如何回报

1. 打开 **[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)**，边走边填 —— 它要求
   机器、操作系统版本、QEMU 与 GCC 版本**及其来源**、服务商与模型、预检四步、两次 run 的短指纹、导出文件
   路径与其 `audit-verify` 结论，以及失败记录（哪一步、原始错误、是否可恢复）。
2. 在 <https://github.com/breakevery/riscdom/issues> 开一个 issue，**把填好的清单贴进去**；若你更愿意放在
   仓库里，就放进 `walkthroughs/`。
3. 没人写下来的行走，没人能复核 —— 填好的模板就是报告。

## 已知限制

- **仅 Windows**，且界面目前只有中文（尚无语言切换）。
- **没有增量或加密快照**；会话存储是本地明文 SQLite。
- **同一时刻只有一个 VM** —— GUI 驱动单个宿主持有的 VM。
- **审计日志还没有保留策略**：它随使用增长，且不会被裁剪。
- **走查的自动化缺口**：那次走查由脚本化输入驱动，因此记录里标为「需要真人确认」的两项应由人复核 ——
  修改模型后的保存、以及在聊天框里输入中文。

## 安全

本项目**不自带**任何 API key：所有模型访问都是 BYOK。key 不会离开你的机器；审计日志在 AI 之外且
append-only；不上传任何东西。QEMU 与 RISC-V 工具链是按各自许可证发布的独立程序，见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。完整声明与漏洞报告流程见 [SECURITY.md](SECURITY.md)。

## 许可证

[Apache License 2.0](LICENSE)。贡献需要签署 [CLA](CLA.md) —— 见
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。
