[English](CONTRIBUTING.md) | 中文

# 贡献指南

感谢你对智芯城（RiscDom）的关注。本项目按「分步骤」推进，每一步都必须可验证、可回滚——
贡献也遵循同一套纪律。

## 开发环境

- Windows 10/11（MVP 仅在 Windows 验证；QMP / 串口走 TCP）
- QEMU（`qemu-system-riscv64`，实测 11.1.0）
- RISC-V 裸机 GCC（`riscv64-unknown-elf-gcc`，实测 xPack 15.2.0）
- Rust / cargo（实测 1.98.1）+ MSVC 工具链（Tauri 需要）
- Node / npm（实测 24.11.1 / 11.16.0）

精确路径与平台限制见 [ENVIRONMENT.md](ENVIRONMENT.md)。

## 本地检查（gate）

每次提交前先跑 gate：

```powershell
scripts\gate.ps1      # Windows
```

```text
sh scripts/gate.sh    # Unix
```

gate 依次执行：`cargo fmt --all -- --check` → `cargo clippy -D warnings` → `cargo check` →
`cargo test` → `ui/src-tauri` 的 `cargo check` → `npm run build` → 双语文档链接检查
（`scripts/check-bilingual.ps1` / `.sh`）。

另有更轻量的预检：`scripts/preflight.ps1`（Windows）/ `scripts/preflight.sh`（Unix）。

## 受门禁保护的提交

**不要直接 `git commit`。** 请使用包装脚本，它先跑 gate，全绿才提交：

```text
scripts/commit.ps1 "feat(host): stage 20b persistent vm"     # Windows
./scripts/commit.sh "feat(host): stage 20b persistent vm"    # Unix
```

gate 非零退出时，包装脚本以 1 退出，**不会产生任何提交**。

## 提交信息约定

`type: subject`，`type` 取 `feat` / `fix` / `docs` / `test` / `chore` / `refactor` / `perf` /
`build` / `ci` 之一；subject 用祈使语气、约 70 字符以内；属于分步骤计划时带上阶段标记
（例如 `docs: stage 22c contributing, coc, bilingual check`）。**一个阶段一个提交。**

## PR 流程

1. Fork 仓库（有写权限则新建分支）。
2. 改动保持最小作用域；不要动无关文件，也绝不要让 AI 生成的代码路径修改宿主监控层。
3. 本地先跑 gate。**所有 PR 必须通过 CI 的 gate。**
4. 说明改了什么、如何验证（测试输出、截图）以及残留限制或后续项。
5. 安全问题走 [SECURITY.md](SECURITY.md)，不要开公开 issue。

## 绝不提交

- 任何 API key / token / 凭据（本项目为 BYOK，不自带 key；审计事件、日志、Debug 输出与
  前端都不得出现 key）
- 工作区与构建产物：`target/`、`node_modules/`、`*.db`、`*.jsonl`、`.env*`
- 审计数据库 / 会话数据库

`.gitignore` 已覆盖大部分；CI 另跑 secret scanning（gitleaks，全历史）。

## 文档规范

文档双语：英文为主文档（如 `README.md`），中文译本同目录 `*.zh-CN.md`，
首行加语言切换：

```markdown
[中文](README.zh-CN.md) | English
```

代码块、命令、路径、配置键名、API 名一律不译。gate 的双语检查会验证配对是否齐全、
切换行是否互相指向；它**只报告，不自动改文件**。
