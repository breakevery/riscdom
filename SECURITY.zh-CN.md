[English](SECURITY.md) | 中文

# 安全策略

## 支持的版本

仅最新 release 与 `main` 分支接受安全更新。

## 报告漏洞

请通过 **GitHub Security Advisory** 私下报告，不要公开 issue。

报告时请勿附带真实 API Key；如需演示，请使用可撤销的临时 key。

## 密钥处理约定

- 本项目**不提供** API Key，所有模型访问均由用户自带（BYOK）。
- 审计日志**不含** key（`agent.llm.request` 只记 hash 与 token 数；keyring 事件只记 `provider_id`）。
- 前端**不持久化** key（`localStorage` 仅存"是否记住 key"这一布尔偏好）。
- 系统钥匙串（OS keyring）用于持久化（v0.2 起）；无法写入时静默降级为仅内存。
- `DEEPSEEK_API_KEY` 环境变量只在启动时被采纳到内存，**不会**被自动写入钥匙串。

## CI 范围说明

`.github/workflows/ci.yml` 只跑跨平台可执行的检查：

- secret scanning（gitleaks **全历史**；使用社区二进制，而非 `gitleaks-action`，
  因为组织私有仓库使用该 action 需要 `GITLEAKS_LICENSE`）
- Rust：`fmt --check`、`clippy -D warnings`、`check`、`audit --lib` 测试
  （**仅可移植 crate**：`audit` / `sandbox` / `agent`）
- 前端：`npm ci` + `npm run build`

`.gitleaksignore` 中按**指纹**精确豁免了一条历史命中：早期单测里用于验证打码逻辑的
**假占位 key**（已在 12.0 提交 `d5daf4f` 替换为非 key 字符串，仅存于历史）。
这是针对单个已知误报的豁免，不影响其余全历史扫描。

**完整测试**（`sandbox` / `agent` / `host-core` 的端到端）需要本机 QEMU
（`qemu-system-riscv64`）与 RISC-V 交叉编译器（`riscv64-unknown-elf-gcc`），
标准 runner 不具备，由开发者在本地执行 `cargo test`。

`host-tauri` 依赖 Tauri，在 Linux 需要系统库（webkit2gtk / gtk）。Linux 的 `gate` job 会把它们装上，
因此每个 workspace crate 在 CI 里也会被 lint 与 check——没有任何东西只在开发机上 lint。留在本地的是上面那类端到端工作。
