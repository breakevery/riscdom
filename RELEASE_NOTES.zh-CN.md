[English](RELEASE_NOTES.md) | 中文

# RiscDom v0.5.0-preview.1

> **这是预览版，尚未在干净环境验证过。** 它是本仓库自己的 gate 与端到端测试覆盖到的那个构建，而不是
> 陌生人走过的那个。那次走查正是本预览版的目的，也是版本号写成 `preview.1` 的原因。

**一句话说黄金路径：** 在 Windows 上装上 RiscDom，指定 QEMU 与 RISC-V 编译器，配好模型，跑一次 Agent
任务，存一个快照，导出这次 run 的审计记录，rollback 回快照，再改一个配置字段跑第二次 —— 七步，全部
在应用里手动完成。v0.5 让这七步都成为现实；两次 run 的自动比较属于 v0.6。

## 本预览版新增

- **得审计记录**（*设置 → 审计*）：把一次 run 导出成 JSONL。该文件**自足** —— 从链的第一条事件写到
  收束该 run 的那条事件 —— 所以你可以把它放进一个空数据库，让 `audit-verify` 在不需要这台机器的
  情况下判定它。
- **rollback 之后看得见来路**：从快照恢复的 run 会写明「恢复自 <快照名>」，并且可以选中两个 run，
  并排查看它们的指纹、开始时间与状态。
- **一份人工清单**（覆盖前两步），以及一个对真实 QEMU guest 走完第 3–7 步的 `--ignored` 端到端测试。
- **贡献者许可协议**，以及许可证说明重新变回两行的 README。

## 测试者需要什么

- **Windows 10/11**（唯一验证过的平台）。
- **QEMU** —— `qemu-system-riscv64`，由你自己安装。RiscDom 会引导你跑
  `winget install SoftwareFreedomConservancy.QEMU` 或去官网下载页；它既不自带也不下载 QEMU。
- **RISC-V 裸机 GCC** —— xPack `riscv-none-elf-gcc` 或等价的 `riscv64-unknown-elf-gcc`。RiscDom
  会自动探测，也可由你手动指定。
- **一个 API key**（你所选服务商的；也可用 Ollama / LM Studio 这类本地服务）。RiscDom 不自带 key，
  也不会有任何 key 离开你的机器。

## 如何回报

1. 打开 **[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)**，边走边**填** ——
   它要求机器、操作系统版本、QEMU 与 GCC 版本**及其来源**、服务商与模型、预检四步、两次 run 的短指纹、
   导出文件路径与其 `audit-verify` 结论，以及失败记录（哪一步、原始错误、是否可恢复）。
2. 在 <https://github.com/breakevery/riscdom/issues> 开一个 issue，**把填好的清单贴进去**。
3. 没人写下来的行走，没人能复核 —— 填好的模板就是报告。

## 验证情况

- `cargo test`：**281 passed / 0 failed / 8 ignored**，共 **79 个测试套件**。ignored 的是那些按设计
  需要真实 API key 或真实 QEMU 启动的测试；第 3–7 步走查就是其中之一，在本机通过
  （`cargo test -p host --test golden_path -- --ignored`）。
- gate —— `cargo fmt --check`、`cargo clippy -D warnings`、`cargo check`、完整 `cargo test`、
  `npm run build`、六个 UI 探针、镜像常量守卫、双语文档检查 —— 本地与 CI 均通过
  （`scripts/gate.sh` 是「绿」的唯一清单）。
- **未验证：** 干净机器 + 真实 API key 的首次真实走查。这是本预览版唯一的开放项，而清单就是关闭它的
  方式。

## 已知限制

- **仅 Windows。** macOS 与 Linux 尚未支持也未验证；QMP 与串口走 TCP。
- **预览版不承诺可升级。** 配置格式与审计表结构是最终 v0.5.0 预期会保持的，但预览版存在的意义就是
  去发现例外。
- **界面目前只有中文**，还没有语言切换。
- **没有增量或加密快照**；会话存储是本地明文 SQLite。
- **同一时刻只有一个 VM** —— GUI 驱动单个宿主持有的 VM。
- MSI 的安装器版本单独钉为 `0.5.0.1`，因为 `preview.1` 不是合法的 MSI `ProductVersion`；产物名仍然
  带 `0.5.0-preview.1`。

## 安全

本项目**不自带**任何 API key：所有模型访问都是 BYOK。key 不会离开你的机器；审计日志在 AI 之外且
append-only；不上传任何东西。QEMU 与 RISC-V 工具链是按各自许可证发布的独立程序，见
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。完整声明与漏洞报告流程见
[SECURITY.md](SECURITY.md)。

## 许可证

[Apache License 2.0](LICENSE)。贡献需要签署 [CLA](CLA.md) —— 见
[CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md)。
