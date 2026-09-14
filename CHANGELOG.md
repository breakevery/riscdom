# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **串口来源改为 sandbox 主动推送**：`sandbox` 的串口读取线程经 `VMConfig.serial_observer`
  实时扇出分帧 → `agent::AgentLoop::subscribe_serial()`（`std::sync::mpsc`）→
  host 转发为 `serial:chunk` 并累加到 `get_serial_buffer()`。**不再**从审计里
  `read_serial` 的工具结果派生（旧的 `serial_full_text` / `SerialDiff` 已删除）。
  `read_serial` 工具语义不变；observer panic 被 `catch_unwind` 拦截并记审计事件
  `sandbox.serial.observer_panic`。

### Added

- CI workflows（`.github/workflows/ci.yml`）：secret scanning（gitleaks，全历史）、
  Rust 检查（`fmt --check` / `clippy -D warnings` / `check` / `audit` 单测，仅可移植 crate）、
  前端构建（`npm ci` + `npm run build`）。
- 本地预检脚本：`scripts/preflight.ps1`（Windows）与 `scripts/preflight.sh`（Unix）。
- `SECURITY.md`、`.env.example`，并完善 `.gitignore`（`.env*` / `*.db` / `*.jsonl` 等）。

### Security

- 依赖审计（2026-09-14）：`cargo audit` 扫描 470 个 crate，**0 个漏洞**；7 条信息性警告
  （6 个 unmaintained：`proc-macro-error`、`unic-char-property` / `unic-char-range` /
  `unic-common` / `unic-ucd-ident` / `unic-ucd-version`；1 个 unsound：`glib 0.18.5`，
  仍 Linux/GTK 传递依赖，Windows 不构建）。`npm audit --omit=dev`：**0 个漏洞**。
- README 新增“安全声明”；v0.2 路线图新增 **f 条**（公开前安全清单）。
- 未自行升级任何依赖（存在警告均未处理，待人工决策）。

### Planned (v0.2) — 多模型接入与密钥安全

- **LLM 客户端重构**：`DeepSeekClient` → `OpenAiCompatClient`（`base_url` / `api_key` /
  `model` 全部用户可配；保持 OpenAI 兼容协议，DeepSeek 降为默认预设之一）
- **内置服务商预设**：DeepSeek（默认）/ OpenAI / Ollama（本地，无需 key）/
  LM Studio（本地）/ 自定义；UI 服务商下拉自动填 `base_url` / `model`
- **本地离线模型支持**：Ollama / LM Studio 复用同一客户端；离线模式 = QEMU +
  RISC-V GCC + 审计 + 沙箱 + 本地 LLM，全程无网络
- **无 key 降级体验**：无 key 不崩溃，UI 引导配置；自动探测 `localhost:11434`
  提示使用本地 Ollama；新用户首次启动不能直接报错
- **API key 持久化：OS keyring**（Windows Credential Manager / macOS Keychain /
  Linux Secret Service，Rust `keyring` crate）；绝不使用 `localStorage` / 明文文件 /
  `.env`；“仅内存”降级为 fallback
- **公开前安全清单**：`.env.example` 只放占位符；`.gitignore` 覆盖 `.env` / `*.db` /
  `*.jsonl`；CI 加 secret scanning（gitleaks 或 GitHub 原生）；README 声明不提供 API key

### Planned (v0.2) — 其他

- 给 `AgentLoop` 暴露最小串口访问接口（当前 host 从审计派生，依赖脆弱）
- QEMU 真实快照 `savevm` / `loadvm`（替代重启式降级）
- 流式 LLM 响应
- 会话持久化
- `real_api` 测试补 `verify_chain` 断言（当前只断言串口输出）
- host 串口轮询改为 sandbox 主动回调
- gdbstub 接入（调试）
- Unix socket（macOS / Linux）与 virtio 设备
- 审计日志分片与远程备份
- **公开前完成中英双语文档**：README / CHANGELOG / PROJECT_CONSTITUTION / AGENTS /
  Release notes 双语；英文为主文档（GitHub 默认展示），中文为 `*.zh-CN.md`；顶部加
  语言切换链接；LICENSE 无需翻译

## [0.1.0] - 2026-09-14

> RiscDom v0.1.0 — AI-native RISC-V sandbox MVP

### Added

- **sandbox**：QEMU RISC-V `virt` 裸机沙箱。进程生命周期、平台端点抽象
  （QMP / 串口 → QEMU 参数）、最小 QMP 客户端（greeting / `qmp_capabilities` /
  `stop` / `cont` / `quit`）、串口捕获与增量缓冲、快照/回滚（MVP 降级）、
  所有对外操作写入审计。
- **audit**：append-only SQLite + SHA-256 hash chain。`BEFORE UPDATE` /
  `BEFORE DELETE` 触发器硬保证不可改写；无 UPDATE/DELETE API、无关闭开关；
  查询/过滤/JSONL 导出；`audit-verify` CLI（exit 0/1/2，可定位首个断裂事件）。
- **agent**：LLM 循环与工具。DeepSeek 客户端 + MockLlm；能力策略
  `WorkspacePolicy`（默认拒绝、防穿越、扩展名白名单）；工具集
  `write_source` / `compile` / `start_vm` / `read_serial` / `stop_vm` /
  `list_workspace`；freestanding RISC-V 编译器封装（注入 crt0 + 链接脚本）；
  系统提示词；上下文裁剪与迭代上限；全链路审计事件。
- **host**：Tauri 后端。10 个 command（审计状态/列表、LLM 配置、运行 agent、
  工作区、串口、导出）；事件 `agent:iteration` / `agent:tool_call` /
  `agent:tool_result` / `agent:final` / `serial:chunk` / `vm:state`。
- **ui**：React + TypeScript + Vite 三栏桌面界面（对话框 / 设置 / 串口画布），
  xterm.js 串口画布，可拖拽分栏，无第三方分栏库。
- 项目文档：`AGENTS.md`（宪法）、`PROJECT_CONSTITUTION.md`（完整宪法 + 架构 +
  审计事件类型）、`ENVIRONMENT.md`（工具链与平台限制）、各 crate README、根 README。

### Known limitations (MVP 降级项)

- **快照是降级方案**：`save_snapshot` / `load_snapshot` 保存/读取启动参数并重启，
  **不是**真实 VM 内存+设备状态（v0.2 换 `savevm`/`loadvm`）。
- **仅 Windows + TCP**：QMP/串口走 TCP；Unix socket、macOS/Linux 未实现。
- **无流式输出**：LLM 响应整块返回。
- **无会话持久化**：每轮 `run_agent` 为独立上下文。
- **编译器注入 crt0**：AI 只需写 `int main(void)`；入口 `_start` 与栈由编译器注入
  （原因见 `agent/README.md` 与 `ENVIRONMENT.md`）。
- **API key 仅内存**：不落盘、不进审计；关闭应用即失效。

### Build artifacts (Windows x64)

由 `npm run tauri build` 生成（构建输出，位于 `target/`，不入库）：

- `ui/src-tauri/target/release/bundle/msi/RiscDom_0.1.0_x64_en-US.msi` （约 5.16 MB）
- `ui/src-tauri/target/release/bundle/nsis/RiscDom_0.1.0_x64-setup.exe` （约 3.65 MB）

### GitHub Release

仓库保持**私有**。GitHub Release 未发布；安装包仅本地保留。（早先创建的 draft 已删除，tag `v0.1.0` 保留。）

### Verification

- 全 workspace `cargo test` 通过（sandbox/audit/agent/host + doc tests）。
- `npm run build`（tsc + vite build）通过。
- `cargo check --manifest-path ui/src-tauri/Cargo.toml` 通过。
- mock LLM 端到端：`cargo test -p host -- --ignored --nocapture` →
  `agent:final` 到达、`serial:chunk` 含 `HELLO RISCV`、`verify_chain` 为 Intact。
- 真实 DeepSeek API 端到端：**已执行通过**（2026-09-14，`iterations = 6`，串口捕获 `HELLO RISCV`；结果见 `host/README.md`）。

[Unreleased]: https://github.com/breakevery/riscdom/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/breakevery/riscdom/releases/tag/v0.1.0
