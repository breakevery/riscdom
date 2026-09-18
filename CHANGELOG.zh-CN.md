[English](CHANGELOG.md) | 中文

# 变更日志

本文件记录项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [未发布]

### 新增

- **主题切换**：浅色 / 深色 / 跟随系统，在「设置 → 外观」选择并存入 `settings.json`。样式表里所有颜色
  都是令牌，主题即一组令牌；串口终端也跟随同一套令牌。
- **`scripts/clean-temp.ps1` / `scripts/clean-temp.sh`**：清理 RiscDom 在系统临时目录下的条目。默认
  dry-run，加 `-Force` / `--force` 才真删。只匹配以 `riscdom-` 开头的条目，并显式排除
  `<temp>/riscdom`（兜底数据目录）。
- **端到端失败路径诊断日志**（阶段 5c-3）：e2e 运行现在会打印报告，点名第一个失败的步骤、该步骤的
  原始输出、串口状态（包含“guest 从未打印任何东西”这种情形）与审计链状态。怎么读见
  [docs/e2e-debugging.md](docs/e2e-debugging.md)。
- **环境能力预检**：工具链或 QEMU 路径变化后（以及缓存失效时的首次运行），宿主会在**真实路径**上
  编译一个最小 guest 并启动它，报告四步中卡在哪一步。它仅告警、结果按配置指纹缓存进
  `settings.json`、**绝不写入审计链**，并提供可记录的“仍要继续”。**有意不做版本号规则**：
  本仓库没有任何 QEMU × GCC 兼容矩阵可依据（见 `PROJECT_CONSTITUTION.md` §10）。

### 变更

- **relay 端口改由进程级租约分配**（v0.4 #1）：`sandbox::relay::lease_local_port` /
  `lease_local_ports` 取代 `free_local_port`，返回 `PortLease`，在交接并释放前一直占住该端口（以及一个
  已绑定的 listener）。`start_vm`、快照恢复与预检都从它取端口，只在 QEMU 启动前才放开 OS 层占用，于是
  本程序内部不可能再把同一端口发给两个持有者，端口在最后一刻之前也不会被外部抢走。三次重试保留：在
  QEMU 自己绑定端口的前提下，交接本身无法做成原子。详见 [docs/qemu-stdio.md](docs/qemu-stdio.md)。

### 修复

- **CI 改为调用 gate，不再自己维护命令清单**：`.github/workflows/ci.yml` 现在只安装 Rust + Node，然后
  调 `sh scripts/gate.sh`，于是检查项再也不会在 CI 与本机之间漂移（原先独立的前端 job 已并入）。改成这套
  之后的第一次运行就抓到真问题：某个测试里两条 `use` 只在 Windows 需要，Linux 上被 `-D warnings` 判为
  错误。`scripts/gate.sh` 现在也会检测平台并**打印**跳过的步骤（无系统库时的 Tauri lint/check；无 QEMU +
  RISC-V GCC 时的起 guest 测试），而不是失败、也不是静默跳过。
- **启动时清理过期的临时目录**：宿主启动时删除超过 24 小时的 `riscdom-*` **目录**（只删目录）。
  `<temp>/riscdom`（兜底数据目录）在白名单中、永不被删；examples 改用带 pid 的唯一名，避免清理时误删正在
  运行的实例。
- **启动时清理过期的构建临时目录**：构建会自己清理，而进程被 kill 后留下的那些（超过 24 小时）会在宿主
  启动时删除；只碰 `riscdom-build-*` 前缀。
- **构建完成后删除该次临时目录**：按次唯一路径解决了并发竞态，但每次编译会漏一个小目录；现在成功与
  失败都会清理。
- **并发构建不再共享文件**：注入的 `crt0.S` / 链接脚本原先位于一个固定的临时路径，同时进行的两次构建
  （一次 run 与预检，或并行测试）可能读到写了一半的文件。现在每次构建都有自己的目录。
- **预检的编译步骤加了守卫**：超过 30 秒未返回的编译器会被终止并报为超时，而不是把面板挂住。

## [0.3.1] - 2026-09-17

### 修复

- **快照恢复会写入手动指定的 QEMU 路径**：恢复路径自建 `VMConfig` 且写死 `qemu_exe: None`，于是静默
  回落到自动探测，可能用与「设置 → 工具链」中配置的不同的二进制引导。现在 agent 循环与恢复共用同一
  注入逻辑。（复现说明：未修时函数**不报错**而是返回 `Ok(())` 并忽略该路径；回归测试断言“恢复必须
  走所配置的二进制”，因此拿到 `Ok` 时该断言失败 panic。）
- **QEMU 进程已退出时不再报 running**：`vm_is_running` 只看槽位，guest 关机（或 QEMU 被杀/崩溃）后
  残留的句柄会让顶栏徽标永远停在“VM 运行中”。现在同时检查子进程，并丢弃死句柄。
- **guest 持续输出时 `read_serial` 返回已捕获数据**：整体等待到期时此前会答“无串口输出”，即使缓冲区
  已满（循环打印的 guest 永远等不到 150ms 静默期）。现在只有缓冲区真的为空才报空。
- **任务结束后不再把聊天拉回底部**：完成处理此前强制滚到底，即使用户已上翻。现在尊重滚动状态并显示
  “回到最新”，与串口侧一致。
- **双栏布局不再溢出窗口**：聊天列此前只按固定 240–900px 夹紧，窄窗下会把串口列挤出屏幕。现在拖动
  上限由实测容器宽度推导，串口列最小宽度自适应。

> 注：RELEASE_NOTES 的 v0.3.1 条目在 tag 后一个 commit 补入。

## [0.3.0] - 2026-09-16

### 新增

- **一键下载 RISC-V GCC（xPack）**：SHA-256 校验、Zip Slip 防护、可取消。
- **QEMU 自动探测与手动指定**：`RISCDOM_QEMU` → 常见路径 → `PATH`，并可在「设置」手动指定
  路径；持久化到 `settings.json`，注入 AgentLoop 真正生效。
- **顶栏 VM 状态徽标**（跨 run 保持可见）。
- **设置页 tab 化**：模型 / 工具链 / 快照 / 审计 / 插件。

### 变更

- **主视图改为双栏**（聊天 + 串口）；设置移至独立页面（Esc 返回）。
- **system prompt 全英文。**
- **AI 不再在任务结束后自动停止 VM**：prompt、`stop_vm` 工具描述与 VM 状态徽标三重保证。

### 修复

- **聊天与串口自动滚动**到最新输出；用户上翻不被打断，并出现“回到最新”浮按钮。
- **`read_serial` 返回前等待约 150ms 静默期**，首字节不再被截断。
- **快照恢复在中继端口失败时重试**（QMP 10054 / 绑定失败，最多 3 次）。
- **门禁稳定性**：`start_vm` 的端口 TOCTOU 重试。

### 说明

- 残留事项记录于 `PROJECT_CONSTITUTION.md` §10（v0.4）。

## [0.2.2] - 2026-09-15

### 修复

- **Windows 钥匙串此前是"静默空操作"**：`keyring` crate **没有**默认后端，所以裸写
  `keyring = "3"` 会编译成一个空实现——`set` 返回成功，但什么都没写进凭据管理器，重启即丢 key。
  现在 `host` 在 Windows 上启用 `windows-native`（macOS/Linux 分别启用
  `apple-native` / `linux-native-sync-persistent`），API key 真正持久化，并在启动时读回。

## [0.2.1] - 2026-09-15

### 新增

- **手动工具链路径可持久化**：在「设置 → 工具链」选定的编译器会写入应用数据目录的
  `settings.json`（不进仓库、不含任何 key），重启后自动生效。写盘失败记审计
  `host.settings.save_failed`，**不阻塞**运行。

### 修复

- **RISC-V 工具链的自动探测、友好引导与可配置入口**（阶段 24a–24c）：解析顺序为
  `RISCDOM_RISCV_GCC` → `RISCV_GCC` → 常见安装路径 → `PATH`，同时接受
  `riscv64-unknown-elf-gcc` 与 xPack 的 `riscv-none-elf-gcc`。全部未命中时，错误信息会列出
  搜索过的每个路径、给出下载链接与"如何指路"；`run_agent` 前置以结构化错误
  `toolchain_missing` 拒绝，UI 显示红色横幅并提供"重新探测 / 手动指定"
  （见 `docs/toolchain-setup.md`）。
- **错误文案不再重复**：手动指定的工具链无法运行时，只报一次 `not runnable: …`。

## [0.2.0] - 2026-09-14

### 变更

- **VM 生命周期归 host**：VM 从 `AgentLoop` 解绑到 `AppState::vm_slot`，run 结束后 VM 仍
  留在槽内，下一次 run 复用同一台 guest（`AgentLoop::with_vm` 注入；无注入时行为不变）。
  串口转发器改为**长驻**（应用启动时创建），订阅**跨 run 连续**。
- **串口来源改为 sandbox 主动推送**：`sandbox` 的串口读取线程经 `VMConfig.serial_observer`
  实时扇出分帧 → `agent::AgentLoop::subscribe_serial()`（`std::sync::mpsc`）→
  host 转发为 `serial:chunk` 并累加到 `get_serial_buffer()`。**不再**从审计里
  `read_serial` 的工具结果派生（旧的 `serial_full_text` / `SerialDiff` 已删除）。
  `read_serial` 工具语义不变；observer panic 被 `catch_unwind` 拦截并记审计事件
  `sandbox.serial.observer_panic`。

### 新增

- **真实快照保存 / 恢复**：host `save_snapshot_real` / `resume_from_snapshot_real`
  （审计 `host.snapshot.save` / `host.snapshot.resume`），UI 提供“保存当前状态”与每项
  “恢复”按钮（二次确认）。
- **`real_api` 断言审计链完整性**（阶段 21）：真实 API 测试的审计后端由 in-memory 改为
  **文件 SQLite**，run 结束后用独立句柄 `audit::verify_chain` 断言
  `ChainStatus::Intact { length > 0 }`，并断言 `agent.llm.request` / `agent.tool.call` /
  `agent.tool.result` 各 >= 1；临时库用 `Drop` 守卫清理（含失败路径）。
- **快照面板**（列表 / 删除；真实快照标注“真实”、重启式标注“重启式”），host 命令
  `list_snapshots` / `delete_snapshot`（审计 `host.snapshot.delete`）。
- **会话持久化**（列表 / 打开 / 重命名 / 删除 / 清空）：host `SessionStore`（SQLite，复用
  `rusqlite`）+ 7 个 Tauri 命令；会话自动保存到应用数据目录，重启后可恢复；恢复只注入
  历史消息（不重放工具调用），**不持久化** API key / system prompt / 审计事件。
- **流式 LLM 响应（agent + host + UI 全链路）**：`LlmClient::chat_stream`（默认退化到
  `chat`）+ `OpenAiCompatClient` 的 SSE 实现 + `sse` 解析模块；`AgentLoop::subscribe_stream`；
  host `agent:stream:delta` / `agent:stream:done`；UI 逐字追加渲染（最终 content 覆盖）。
  审计只记 `agent.llm.stream.start` / `.end`，不逐 chunk 记。
- CI workflows（`.github/workflows/ci.yml`）：secret scanning（gitleaks，全历史）、
  Rust 检查（`fmt --check` / `clippy -D warnings` / `check` / `audit` 单测，仅可移植 crate）、
  前端构建（`npm ci` + `npm run build`）。
- 本地预检脚本：`scripts/preflight.ps1`（Windows）与 `scripts/preflight.sh`（Unix）。
- `SECURITY.md`、`.env.example`，并完善 `.gitignore`（`.env*` / `*.db` / `*.jsonl` 等）。

### 安全

- 依赖审计（2026-09-14）：`cargo audit` 扫描 470 个 crate，**0 个漏洞**；7 条信息性警告
  （6 个 unmaintained：`proc-macro-error`、`unic-char-property` / `unic-char-range` /
  `unic-common` / `unic-ucd-ident` / `unic-ucd-version`；1 个 unsound：`glib 0.18.5`，
  仍 Linux/GTK 传递依赖，Windows 不构建）。`npm audit --omit=dev`：**0 个漏洞**。
- README 新增“安全声明”；v0.2 路线图新增 **f 条**（公开前安全清单）。
- 未自行升级任何依赖（存在警告均未处理，待人工决策）。

### 说明

- **真实快照：已用方案 A′（TCP 迁移 + 本地文件中继）实现。** 阶段 18a 实测 `migrate` → `file:`
  在 Windows + QEMU 11.1.0 不可用，但 `migrate` → `tcp:` 成功；19b 用本地 TCP 中继把迁移流
  落盘为 `<name>.mig`，恢复时由中继反向喂给 `-incoming tcp:` 的 QEMU。
  详见 `sandbox/docs/snapshot-experiment.md`。
  残留限制：旧的重启式降级（`.json`）仍保留兼容；恢复用的 `-kernel` 取工作区内最新的
  `*.elf`（迁移流会覆盖内存，内核仅用于让 QEMU 起机）。

### 计划中（v0.2）— 多模型接入与密钥安全

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

### 计划中（v0.2）— 其他

- 给 `AgentLoop` 暴露最小串口访问接口（当前 host 从审计派生，依赖脆弱）
- QEMU 真实快照 `savevm` / `loadvm`（已由方案 A′ 满足；待办：启用 `AppState.vm` 槽，
  让 UI 可保存/恢复）
- host 串口轮询改为 sandbox 主动回调
- gdbstub 接入（调试）
- Unix socket（macOS / Linux）与 virtio 设备
- 审计日志分片与远程备份
- **公开前完成中英双语文档**：README / CHANGELOG / PROJECT_CONSTITUTION / AGENTS /
  Release notes 双语；英文为主文档（GitHub 默认展示），中文为 `*.zh-CN.md`；顶部加
  语言切换链接；LICENSE 无需翻译

## [0.1.0] - 2026-09-14

> RiscDom v0.1.0 — AI-native RISC-V sandbox MVP

### 新增

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

### 已知限制（MVP 降级项）

- **快照是降级方案**：`save_snapshot` / `load_snapshot` 保存/读取启动参数并重启，
  **不是**真实 VM 内存+设备状态（v0.2 换 `savevm`/`loadvm`）。
- **仅 Windows + TCP**：QMP/串口走 TCP；Unix socket、macOS/Linux 未实现。
- **无流式输出**：LLM 响应整块返回。
- **无会话持久化**：每轮 `run_agent` 为独立上下文。
- **编译器注入 crt0**：AI 只需写 `int main(void)`；入口 `_start` 与栈由编译器注入
  （原因见 `agent/README.md` 与 `ENVIRONMENT.md`）。
- **API key 仅内存**：不落盘、不进审计；关闭应用即失效。

### 构建产物（Windows x64）

由 `npm run tauri build` 生成（构建输出，位于 `target/`，不入库）：

- `ui/src-tauri/target/release/bundle/msi/RiscDom_0.1.0_x64_en-US.msi` （约 5.16 MB）
- `ui/src-tauri/target/release/bundle/nsis/RiscDom_0.1.0_x64-setup.exe` （约 3.65 MB）

### GitHub Release

仓库保持**私有**。GitHub Release 未发布；安装包仅本地保留。（早先创建的 draft 已删除，tag `v0.1.0` 保留。）

### 验证

- 全 workspace `cargo test` 通过（sandbox/audit/agent/host + doc tests）。
- `npm run build`（tsc + vite build）通过。
- `cargo check --manifest-path ui/src-tauri/Cargo.toml` 通过。
- mock LLM 端到端：`cargo test -p host -- --ignored --nocapture` →
  `agent:final` 到达、`serial:chunk` 含 `HELLO RISCV`、`verify_chain` 为 Intact。
- 真实 DeepSeek API 端到端：**已执行通过**（2026-09-14，`iterations = 6`，串口捕获 `HELLO RISCV`；结果见 `host/README.md`）。

[未发布]: https://github.com/breakevery/riscdom/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/breakevery/riscdom/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/breakevery/riscdom/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/breakevery/riscdom/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/breakevery/riscdom/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/breakevery/riscdom/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/breakevery/riscdom/releases/tag/v0.1.0
