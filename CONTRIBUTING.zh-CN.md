[English](CONTRIBUTING.md) | 中文

# 贡献指南

感谢你对智芯城（RiscDom）的关注。本项目按「分步骤」推进，每一步都必须可验证、可回滚——
贡献也遵循同一套纪律。

## 开发环境

- Windows 10/11（MVP 仅在 Windows 验证；QMP / 串口走 TCP）
- QEMU（`qemu-system-riscv64`，实测 11.1.0）—— 由你安装、由 RiscDom 自动探测
  （`RISCDOM_QEMU` / `QEMU_SYSTEM_RISCV64` → 常见路径 → `PATH`）；Windows 可用
  `winget install SoftwareFreedomConservancy.QEMU`。见 [docs/qemu-setup.md](docs/qemu-setup.md)。
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

### 提交信息一律用 ASCII —— Windows 下 `-m` 路径会丢字符

**实测**（2026-09-18，本机）：用中文写、经 `git commit -m "…"` 传递的信息**不可能**完整落库。
命令行要过一遍控制台的 ANSI 代码页，所有非 ASCII 字符在 **git 拿到之前**就被替换成 `?`
（`0x3F`）。探针 subject `test: 中文正文测试` 被存成 `test: ?????????`，逐字节为
`74 65 73 74 3a 20 3f 3f 3f 3f 3f 3f 3f 3f 3f`。

因此：

- `git commit -m "…"` 的信息一律用 ASCII（英文），这也是本仓库的常态。
- 确需非 ASCII 文本时**不要用 `-m`**：把信息写进文件（**UTF-8 无 BOM**），用
  `git commit -F <file>` 提交。
- 推送前先核对落库内容：`git log -1 --format=%B`；要看原始字节就加 `| Format-Hex`。

例：commit `363e5ab`（*docs: add v0.3.1 release notes (post-tag)*）的正文只能用英文，原因正是
这条约束——中文表述会被存成 `?`。

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

## 调试抖动测试（flaky tests）

抖动很耗时，所以**先定位层次，再加防御**。

1. **先拿证据。** 原样保存失败输出（断言消息、缓冲区内容、时间戳）。本项目里一次
   `read_serial` 失败只打印出 `HELLO RISCV` 的 **`H`** —— 这一个字节就锁定了 bug。
2. **不要假设第一个看起来合理的成因。** 同一个抖动最初被读成"等待窗口太短"，于是把窗口从
   2s 加宽到 5s；那次改动**完全无效**：工具一看到任何字节就返回，窗口根本没被用上。
   要在证据指向的层修，而不是在方便的层修。
3. **让失败自解释。** 返回空串会逼模型和日志去猜；返回一条提示（"guest 可能仍在启动"）就
   让下一次失败可以诊断。宁可先给失败路径补证据，也不要凭感觉改参数。
4. **防御性修复要标注。** 无法按需触发的重试（例如负载下的端口 TOCTOU）是**防御**，不是
   **证明**。在 commit 和报告里写清楚，并保留覆盖它的压力测试
   （`cargo test -p sandbox --test port_race -- --ignored`）。
5. **有意识地升级方案。** 若在同一层两次认真尝试后抖动仍在，就换设计（端口竞争可换 stdio
   传输），而不是继续叠加重试。
6. **轮次之间清理干净。** 被中断的测试会占用 `target/debug/deps/*.exe`，表现为
   `link.exe 1104`；重跑门禁前先清掉残留进程。

文档双语：英文为主文档（如 `README.md`），中文译本同目录 `*.zh-CN.md`，
首行加语言切换：

```markdown
[中文](README.zh-CN.md) | English
```

代码块、命令、路径、配置键名、API 名一律不译。gate 的双语检查会验证配对是否齐全、
切换行是否互相指向；它**只报告，不自动改文件**。
