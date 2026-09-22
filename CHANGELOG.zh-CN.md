[English](CHANGELOG.md) | 中文

# 变更日志

本文件记录项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [未发布]

**控制平面会应答查询了，且事件只有一种 envelope。** `docs/control-plane-api.zh-CN.md` §5.1 的 26 个查询端点已可用，并新增错误码 `method_not_allowed`（405）、预留端点 `/v0/resources`（501）；同时，无论走哪个传输，每个事件现在都包在 `docs/control-plane-events.zh-CN.md` 定下的 envelope 里——SSE 流、Tauri webview（在唯一边界处解包）、以及 worker 的行协议。

### 新增

- **26 个查询端点**（`GET`）：审计状态与事件、运行（列表 / 单个 / 对比）、LLM 各类视图、会话、快照、VM 状态、工具链、QEMU、预检、设置、workspace 与串口——另加宿主本地端点 `/v0/health`、`/v0/status`、`/v0/events`，以及回 `501` 的预留端点 `/v0/resources`。必填参数会被校验（`400`，`cause` 指出参数名），路径参与 `405 method_not_allowed` 判定，每个端点的权限都在路由表里声明并交给 `Authn` 钩子。
- **envelope** 落于 `host/src/events.rs`：`version` / `kind` / `event` / `agent_id` / `task_id` / `ts` / `payload`，由各传输构建。`EventSink` trait 保持原签名，因此没有任何发射点改形状。
- **[docs/control-plane-client-guide.zh-CN.md](docs/control-plane-client-guide.zh-CN.md)**：客户端侧走查——逐端点 curl、应答示例、错误表、以及 SSE 订阅示例。

### 变更

- **三种事件 payload** 现在与文档一致：`vm:state` 恒带 `name`（非快照时为 `null`）、`audit:failed` 用 `message`（此前为 `error`）、`toolchain:download` 标签为 `state`（此前为 `kind`）。其余八种 payload 未变。webview 在 `ui/src/api/tauri.ts` 处解包 envelope，因此面板回调读到的仍是它们一贯读的 payload。
- **`TauriEventSink::new` 接收 agent 身份**，其发出的每个 envelope 都携带它。

**控制平面现在有进程了。** v0.9 的主线是控制平面：人监督 AI 与 AI 监督 AI 走同一套 HTTP + SSE 接口。本批落的是此后全部 API 赖以生长的骨架——新增 `server` crate、两个 smoke 端点、事件流、认证钩子——仅此而已：未改任何内核源码，未改任何事件发射点。

### 新增

- **`server`：新的 Layer 3 workspace crate**（可执行文件 `riscdom-server`）。它只依赖 `host`（Layer 2），不引用任何 Tauri 类型。它绑定 `GET /v0/health`、`GET /v0/status`、`GET /v0/events`（SSE），其余路径一律按 `docs/control-plane-api.md` §4 的错误模型应答，并装入 `Authn` 钩子、以 `NoAuth` 作为 v0.9 默认。宿主的事件经 `HttpEventSink` 抵达事件流——它在 `AppState::run_agent` 的 `emitter` 参数位传入，因此宿主无需任何改动即可被第二个进程驱动。`httpdate` 进入锁文件，是 hyper `server` feature 唯一新增的依赖。
- **`gap` 帧与 `Last-Event-ID` 补放尚未实现**：本批事件流只发 `hello` 与 `event` 两种 kind，第三种在设计上的位置已在 `docs/control-plane-events.zh-CN.md` 标注。

**环境 preflight 也改为按 agent 隔离。** `<workspace>/.riscdom/preflight` 是一个 workspace 里最后一条共享的
写入路径：共享同一 workspace 的两个进程会把 preflight guest 编译到同一对 `guest.c` / `guest.elf`，并同时把它们
的 preflight VM 启到同一个目录。新产物写入 `<workspace>/.riscdom/preflight/<agent_id>/`，要启动的 guest 先在本
agent 自己的目录里找，再回退共享根目录 —— 旧版本留下的 guest 仍然可用，不会被孤立。`settings.json` 里的缓存结果
本就随实例隔离，未动。

### 变更

- **preflight guest 与 preflight VM 的快照目录都改为 per-agent**（遗留项 A2）：
  `AppState::preflight_dir` / `preflight_root` / `write_preflight_guest` / `find_preflight_guest` 公开，
  布局由一条不编译、不启动任何 guest 的文件系统测试钉住。与 v0.8 批次 B 的快照同一套模式；与审计链无关。

**派发得到的 outcome 现在写明的是**真正跑了任务的那个执行者**。** 此前 `TaskOutcome.agent_id` 由分发器盖上它路由到的
`Task.target` —— 对子进程来说那是监工自造的标签，而不是真正干活的身份。现在 `AgentHandle::run` 返回
`TaskOutcome`（此前只返回 `AgentOutcome`），于是每个句柄自己填只有它知道的身份：本地 loop 填自己的 id、host
实例填自己的 id、`StdioExecutorHandle` 则填子进程在 `worker:ready` 事件里声明的身份（缺失即报
`DispatchError::Failed`，而非猜一个）。`LocalDispatcher` 透传句柄的 outcome，它仅剩的判断就是路由。审计链、
哈希公式与 append-only 触发器均无变化。

### 变更

- **`TaskOutcome.agent_id` 的语义是「谁跑了它」，不是「它被发给谁」**（主体交付 3/3）：
  `AgentHandle::run` 返回 `Result<TaskOutcome, DispatchError>` 并自行组装记录；`LocalAgent` 与
  `HostAgentHandle` 填自己的身份，`StdioExecutorHandle` 填子进程声明的那个，
  `LocalDispatcher` 不再组装任何东西。执行者是子进程时两者必然不同 —— 而这正是监工区分「executor-0」
  与「真正应答的那个进程」所需要的。

## [0.8.0] - 2026-09-22

**v0.8 批次 1 —— 面向多 Agent 运行时的技术债清理。** 架构重估点名的三个堵死点已清除；黄金路径上无可见
行为变化。

**v0.8 批次 2 —— 多进程可以共写同一个 `audit.db`，写入失败会响。** `audit.db` 是**有意共享**的（每个
workspace 一条链），而这正是多 Agent 运行时需要的：多个进程追加同一个文件。连接现在以 WAL 模式打开，
带 5 秒 busy timeout 与 `synchronous=NORMAL`；一次 append 在**读取 head 之前**先拿写锁
（`BEGIN IMMEDIATE`）；被锁住的库会先退避重试再报错。仍失败的写入**绝不静默丢弃**：
`AuditSink::record` 返回错误，sink 通知 host，host 记日志、发 `audit:failed` 事件，并（默认）在
*设置 → 审计*里给出横幅与弹窗。只有「告警」可以被关掉。

**v0.8 批次 3 —— 每条审计事件都写明是哪个 agent，快照不再相撞。** `agent_id` 此前只有字段没有生产者，
现在铺到了每一个写者。身份形状为 `<device>-<pid>-<seq>`（`agent::next_agent_id`；单机上即
`local-<pid>-<seq>`）—— 每进程一个，进程内每个 agent 一个，由进程级计数器发放。host 在构造时领一个，
交给它构建的 loop，于是 sandbox 的事件与 agent 的事件都带上与 host 相同的身份。快照改到
`<workspace>/.riscdom/snapshots/<agent_id>/`，共享同一 workspace 的两个 agent 各自保存 `snap1` 不再互相
覆盖；读取会回退到共享根目录，因此本次改动之前拍下的快照仍可列出、恢复与删除。

**v0.8 批次 4 —— 任务可以交给一个执行者，而调用方不必知道它在哪跑。** 多 Agent 运行时需要一件 host 从未
有过的东西：**派发**而不是内联调用。`agent::dispatch` 给出词汇 —— `Task`、`TaskId`、`AgentId`、
`TaskOutcome`、`DispatchError` —— 以及构成缝的两个 trait：`AgentHandle`（执行者：「跑这个任务，把结果给我」）
与 `Dispatcher`（「把任务变成结果」）。**本地**一半已实现：`LocalDispatcher` 按 target 把任务路由到持有该身份的
句柄，host 用 `HostAgentHandle` 把自己的 `run_agent` 路径包成句柄。**远程**一半有意缺席 —— 这个缺席本身就
是那道缝：将来跨进程或跨机器的句柄只需实现 `AgentHandle`，就能接进同一个分发器，上层一行不改。Tauri 命令
仍然照旧直接调 `run_agent`；派发路径是**新增的内部通路**，不是替换。

**v0.8 主体交付 1/2 —— 同一套派发接口，执行者换到了另一个进程。** 新增 `worker` crate 作为执行者
二进制：它在 stdin 上读**一行** `Task` JSON，用给定的 data 目录跑 host 自己那条 `run_agent` 路径，然后在
stdout 上写**一行** `TaskOutcome` JSON。它的事件以 JSON 行写到 stderr，因此 stdout 是一条监工不必过滤就能
直接解析的通道。监工侧新增 `host::StdioExecutorHandle`：一个 `AgentHandle`，其执行者就是那个子进程 ——
它起进程、写任务、读结果、吸干事件，到时不答就杀掉。句柄之上的东西一行未改：分发器分不出执行者是本地还是
跨进程，而这正是 v0.8 批次 4 留开的那道缝。为上线上，`AgentOutcome`、`TaskOutcome`、`DispatchError`
加了 `Serialize` / `Deserialize`（纯附加：字段全是 `String` / `u32`，与链无关）。传输是 stdio + JSON
lines，**零新依赖**。两个已知边角：worker 会链接 Tauri（因为 `host` 无条件依赖它 —— v0.9 改为
optional），以及子进程自己的 agent 身份是经**事件**回来的，而非经监工的 `TaskOutcome`（后者的
`agent_id` 沿用批次 4 语义：该任务被寻址到的那个执行者）。

**v0.8 主体交付 2/2 —— 一个监工同时驱动多个执行者。** 原型现在端到端可演示：`worker` 增加了库半边
（`worker::supervisor`）与可运行演示（`cargo run -p worker --example dispatch`）：它起 N 个执行者进程，
**共享一个 workspace**、**各自拥有一个 data dir**，把任务按 `Task.target` 路由到同名执行者，每个任务打印
一行并给出计数。任务**并发**派发（`std::thread::scope` 每任务一线程 —— 句柄是 `Send + Sync`，不需要线程池
依赖）；指向不在机群里的执行者会被**拒绝**，而不是发给「猜一个」的执行者。监工里没有任何模型：这一阶段的
监工是派发器，不是 agent。同批收尾：`worker` 的 `audit` 依赖声明了却从未使用（执行者经 `host::AppState`
触链），已删除；监工逻辑放进 `worker` 的库，使演示与测试共用一份实现而非两份循环；四项已定决策写进了
[docs/multi-agent-foundation.zh-CN.md](multi-agent-foundation.zh-CN.md)。

### 变更

- **`host::dispatch::outcome_from_view` 改为公开**（v0.8 主体交付 1/2）：唯一的
  `AgentOutcomeView` → `AgentOutcome` 映射此前是私有的；跨进程 worker 复用它，而不是再养一份副本。行为
  无变化。
- **`agent_id` 有生产者了**（v0.8 批次 3）：`AgentLoop` 在构造时接收身份（`new` / `with_vm` 新增了该
  参数），并把它盖到自己写的每条事件上 —— 包括经 `audit_hook` 与工具层写出的事件，它们也改为接收该
  id。host 为每个 `AppState` 领一个（`local-<pid>-<seq>`），并盖在自己的事件与 run 标记上。该字段仍位于
  链**旁边**：哈希公式、`prev_hash` 链接、历史行全部不动。
- **快照改为 per-agent**（v0.8 批次 3）：新快照写入
  `<workspace>/.riscdom/snapshots/<agent_id>/`；`list_snapshots` 同时读该目录与共享根目录（同名时以此
  agent 的为准），`resume_from_snapshot_real` 与 `save_snapshot_real` 走同一回退，`delete_snapshot` 两边
  都删。v0.8 之前的快照继续可用。
- **审计写入支持多进程**（v0.8 批次 2）：`journal_mode=WAL`、`busy_timeout=5s` 与
  `synchronous=NORMAL` 集中在连接打开处设置（`audit::store`）；被锁的 append 最多重试 5 次，退避
  20/40/80/160 ms；而「读 head + 插入」这一对现在跑在 `BEGIN IMMEDIATE` 里 —— 没有后者，单靠 WAL 仍然会
  让两个写者链到同一行上、把链分叉（新增的并发测试抓住了这一点）。`AuditSink::record` 改为返回
  `Result<(), AuditError>`，不再把失败的写入丢在地上；sandbox 与 agent loop 的调用点经
  `audit::report_failure` 上报。链结构、哈希公式、历史行与 append-only 触发器全部未动。
- **app data 目录改为注入，不再是全局**（v0.8 批次 1）：`host::paths` 此前把默认 data 目录存在
  `OnceLock` 里，谁先调用谁生效，之后的调用被静默忽略 —— 同一进程里的第二个 `AppState` 无法拥有自己的
  data 目录。默认值现为可重复设置的 `RwLock`，并且 `AppState::with_data_dir(workspace, data_dir)` 把
  `settings.json`、会话 DB 与工具链下载目录都解析到实例自己的目录下。Tauri 外壳已改用它；一条 host 测试
  钉住两个实例各自写入不同文件。
- **每个 `AppState` 一个 VM 槽**（v0.8 批次 1）：host 持有的 VM 槽本来就已是 per-instance 字段
  （`AppState::vm_slot`），而非进程级单例。本批次用一条测试把它钉住 —— 两个实例各自独立的链、各自独立
  的槽 —— 不会有东西悄悄把它们重新共享。

### 新增

- **`worker`**（v0.8 主体交付 1/2）：执行者进程。用法
  `worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]` —— 两个路径必填、无环境变量回退：执行者的
  身份就是它的命令行，而一个会把它的数据目录静默指到别处的继承变量，比缺参更糟。它构造
  `AppState::with_data_dir`，于是每个执行者自己拥有 `settings.json`、`sessions.db` 与工具链目录。退出码：
  **只要写出了 outcome 就是 0** —— 失败的一次运行也是答案，写成 `TaskOutcome { outcome: Failed { .. } }`；
  用法错误退出 2（不写 stdout 行，由监工报协议失败）。非法任务不是崩溃而是答复：outcome 带约定好的占位
  id `task-unparsed` / `unparsed`。
- **`host::StdioExecutorHandle`**（v0.8 主体交付 1/2）：面向子进程的监工侧 `AgentHandle` —— 子进程 stdin
  进一行任务、stdout 出一行结果；有超时截至，到时不答就杀掉；子进程 stderr 被吸入 `events()`；每条失败消息
  都带子进程退出状态；答的若是另一个任务则拒绝向上传递而非当作本次结果。`with_env` / `with_env_removed`
  让监工决定执行者继承什么环境。
- **派发线上类型加 serde**（v0.8 主体交付 1/2）：`AgentOutcome`、`TaskOutcome`、`DispatchError` 现在派生
  `Serialize` + `Deserialize`（前两者本就形状可序列化；第三个上 `thiserror` 的 `#[error]` 与 serde 可共存）。
  纯附加。
- **`agent::dispatch`**（v0.8 批次 4）：派发词汇与缝。`Task { id, target, input }`；`TaskId`
  （`task-<pid>-<seq>`，来自进程级计数器，因此在进程内与跨进程都唯一）；`AgentId`（批次 3 的
  `<device>-<pid>-<seq>` 形状，包成 newtype）；`TaskOutcome { task_id, agent_id, outcome }`，其中
  `outcome` 就是执行者自己的 `AgentOutcome`，原样不包装；以及 `DispatchError::NoSuchAgent` /
  `::Failed`。两个 trait 是 `AgentHandle`（`agent_id`、`run(&self, &Task)`）与 `Dispatcher`
  （`dispatch(Task) -> Result<TaskOutcome, DispatchError>`）。`agent::LocalAgent` 把一个 loop 包成
  执行者，`agent::LocalDispatcher` 按 target 路由；host 侧新增 `HostAgentHandle`（内部即 `run_agent`
  路径）与 `host::local_dispatcher`。**远程实现不写** —— 那就是有意留开的缝。
- **`agent::next_agent_id`**（v0.8 批次 3）：身份助手 —— `DEVICE`（机器标识，现阶段为 `local`）
  加一个进程级序号，于是 `local-<pid>-1`、`local-<pid>-2`、… 在进程内与跨进程都唯一。
- **审计失败告警**（v0.8 批次 2）：`settings.json` 增加 `alert_on_audit_failure`（默认 `true`，因此升级
  不会把它关掉），*设置 → 审计*里有开关与横幅，新失败还会弹出对话框 —— 为此授予
  `dialog:allow-message` 权限，并由对话框探针钉住权限集合。`audit:failed` 事件与日志行不受该设置影响。
  新增命令：`set_audit_alert`。
- **每条审计事件带 `agent_id`**（v0.8 批次 1）：`AuditEvent` 增加可选字段 `agent_id`（用 builder
  `with_agent` 设置），存入新增的 `audit_events.agent_id` 列，旧库在下次打开时自动补上。它位于链**旁边**：
  哈希公式、`prev_hash` 链接、以及每一行既有的 `hash` 全部不动，因此 v0.8 之前的链仍按原样通过校验。
  在多 Agent 运行时给生产者身份之前，生产者一律留 `None`。JSONL 导出与 `list_audit_events` 都会带上它。

## [0.7.0] - 2026-09-21

**v0.7 已在 `main` 上、尚未发布：自建 i18n 设施、语言切换，以及 macOS/Linux 构建。** Latest 正式版
仍是 `v0.6.0-preview.1`。

### 新增

- **自建 i18n 设施**（v0.7 批次 1–2）：`ui/src/i18n/` 是一套双语字符串注册表，**不引第三方库**；
  `scripts/check-ui-strings.mjs`（在 gate 里，带自测）要求每个键都有两种语言；*设置 → 外观*里新增
  语言选择器（跟随系统 / 中文 / English），写入 `settings.json`、改写 `<html>` 的 `lang`，并即时重渲染。
  v0.6 那四条 diff 字符串是试点。**把注册表铺开到其余约 190 条界面字符串这件事有意不做** —— 价值在
  设施，而不在把一个内核形态的工具翻一遍。
- **macOS 与 Linux 构建**（v0.7 批次 B）：`ci.yml` 新增 `bundle` job（dispatch 或 `v*` tag；macOS + Linux
  两个 runner），执行 `npm run tauri build` 并把产物作为 artifact 上传 —— macOS `.app` + `.dmg`，Linux
  `.deb` + `.rpm` + `.AppImage`。已在 run `35572294916` 端到端验证。安装包**未签名**：Developer ID 签名
  与公证属于商业化层，因此 macOS Gatekeeper 会拦下首次运行。
- **随平台变化的 QEMU 指引**（v0.7 批次 A）：“未找到 QEMU”的指引随平台变化（`winget` / Homebrew /
  发行版包，见 `sandbox::qemu_discover::install_hint_for`），macOS 打包所需的 `icons/icon.icns` 已补齐，
  Unix 的 `-qmp unix:` 参数由一条跨平台单测钉住。

### 修复

- **`host` 在非 Windows 上编译不过**（v0.7 批次 8）：`fn extract_zip` 是 Windows 专属，但调用它的
  `ArchiveKind::Zip` 分支没有加同样的 cfg，于是所有 macOS/Linux 构建都死在 `error[E0425]: cannot find
  function 'extract_zip' in this scope`。现在非 Windows 平台得到一个同名存根，返回“zip archives are not
  supported on this platform”，而 `zip` 仍是 Windows-only 依赖。这个缺陷是新 `bundle` job 发现的：那是
  `host` 第一次在非 Windows 上被编译 —— Linux 的 gate 完全跳过 `host`。

## [0.6.0-preview.1] - 2026-09-19

**黄金路径第 8 步以预览版发布：两次 run 逐字段对比。** 它尚未经人工走查 —— 既没在干净机器上，也没
在本机 —— 所以它是预览版，而 `v0.5.0` 仍是 Latest 正式版：预发布版不持有 Latest 标记。这个预览版
是什么、没证明什么，写在 [RELEASE_NOTES.zh-CN.md](RELEASE_NOTES.zh-CN.md)。

### 新增

- **两个 run 的指纹，逐字段对比**（v0.6 批次 1，数据层 + API）：`host/src/run_diff.rs` 把两份指纹
  文档变成一张有序列表 —— 字段名、两侧的值、是否不同 —— 顺序即 `AppState::run_fingerprint` 的声明
  顺序（**不**按字母排序）。嵌套值整体比较；两个文档携带的字段一个不落地全部返回（无差异也不例外）；
  文档里没有的字段不凭空出现。`AppState::compare_run_fingerprints(run_a, run_b)` 从链上的 `run.start`
  事件取这两份文档，`compare_run_fingerprints` 命令把它交给 UI。
- **审计页里的字段级差异**（v0.6 批次 2）：两 run 并排面板**下方**是一个默认折叠的区块，标题给出字段
  数与差异数（`字段级差异 · 7 个字段 · 3 处不同`；两次 run 配置完全相同时为 `· 0 处不同`）。展开后
  每行是字段名、第一个 run 的值、第二个 run 的值；不同的行高亮，相同的行灰显，值**完整显示** ——
  等宽字体、自动换行、绝不截断。面板照宿主返回的顺序渲染，不做任何重排。

### 备注

- **本预览版的 MSI 单独钉了安装器版本。** `0.6.0-preview.1` 是合法的语义化版本，但不是合法的 MSI
  `ProductVersion`（WiX 只接受 `major.minor.patch.build`，纯数字），因此 `tauri.conf.json` 里的
  `bundle.windows.wix.version = "0.6.0"` 提供数字形式，而包版本 —— 也就是所有产物名 —— 仍是
  `0.6.0-preview.1`。等包版本重新变成纯数字后删除或更新该字段。

## [0.5.0] - 2026-09-19

**黄金路径已完整，本版就是「预览版的工作 + 其走查发现已修」的那一版。** 全路径已被走查过一次 ——
在开发机上，而非干净环境 —— 记录见
[walkthroughs/2026-09-19-preview1-local.md](walkthroughs/2026-09-19-preview1-local.md)；它证明了什么、
没证明什么，写在 [RELEASE_NOTES.zh-CN.md](RELEASE_NOTES.zh-CN.md)。

### 新增

- **七步走查的记录**（v0.5 批次 10–11）：安装 → 创建环境 → 跑任务 → 存快照 → 导出审计记录 → rollback
  → 改一个配置字段再跑，用真实模型、真实 API key 与**安装版 MSI**。每一步都通过；记录里列出了这次走查
  产生的五项发现及其处理结果。
- **`scripts/scan-encoding.py`**（v0.5 批次 13）：针对 0x3F 家族编码事故的手动诊断工具（U+FFFD、连续
  `?`、只含 `?` 的字面量、CJK 旁的 `?`、UTF-8 被当 GBK 读）。**刻意不入门禁** —— 它的「只含 `?` 的
  字面量」判据无法区分真损坏与合法代码。

### 修复

- **审计页的 run 行除了复选框什么都看不见**（走查 S-1）：全局 `input { width: 100% }` 也作用于复选框，
  它撑满整行，把状态、指纹、时间与「导出」按钮全挤出可视区。现在复选框有自己的尺寸、行内文本改用省略号
  收缩、控件永不收缩，设置页也改用窗口宽度而不是自身内容宽度。
- **快照弹窗默认值写死 `snap1`**（G-1）：默认值改为取自时钟（`snap-YYYYMMDD-HHMM`），直接确认不再
  让每个快照同名。
- **工具调用行显示成 `?? write_source ?`**（E-1）：那几行 JSX 里原本的标记在写入时被吃成了字面量
  `?`；现在改为一枚 CSS 状态点加一个中文词。
- **预检句子看似可点其实不可点**（E-2）、**审计 actor 过滤器失焦才生效**（E-3）：句子改为指向它右侧的
  按钮，过滤器改为随输入实时生效。
- **QEMU 会在应用上方弹出控制台窗口**（G-4）：sandbox 在 Windows 上以 `CREATE_NO_WINDOW` 启动它。
  `-display none` 关掉的是 guest 显示，不是那个控制台。

### 变更

- **预览版的 MSI 版本覆盖已删除。** `bundle.windows.wix.version` 只因 `preview.1` 不是合法的 MSI
  `ProductVersion` 而存在；`0.5.0` 是纯数字，因此 MSI 现在直接携带包版本，「应用和功能」里显示 `0.5.0`。
  若该覆盖日后与纯数字版本一起回归，gate 里的 `scripts/check-wix-version.mjs` 会让构建失败。

## [0.5.0-preview.1] - 2026-09-19

**这是预览版，给准备在干净机器上走黄金路径的人。** 预览版尚未在干净环境验证过：测试者需要什么、
如何回报，写在 [RELEASE_NOTES.zh-CN.md](RELEASE_NOTES.zh-CN.md)。

### 新增

- **黄金路径全部七步**（v0.5）：安装 → 创建环境 → 跑任务 → 存快照 → 得审计记录 → rollback →
  改一个配置字段再跑。设计、已拍板的决定与最小诚实范围见
  [docs/golden-path.zh-CN.md](docs/golden-path.zh-CN.md)。
- **一次 run 的记录可自足导出**（v0.5 批次 1–4）：*设置 → 审计* 把一次 run 导成 JSONL，从链的
  **第一条事件**写到**收束该 run 的那条事件**，因此首行锚定 genesis，空库 + `audit-verify` 即可判定
  它，无需从产出它的机器上带任何东西。abandoned 的 run，其文件以 `host.run.abandoned` 标记结尾；
  进行中的 run 会被拒绝，而不是导出到链恰好停下的地方。链中间的「切片」形式被移除而非并列保留
  （批次 4）：「导出」有两种含义就是一种太多。
- **run 显示它来自哪个快照**（v0.5 批次 3）：`resumed_from_snapshot` 进入派生 `runs` 索引 ——
  与其他列一样从链重建 —— 经 `RunView` 到 run 列表，并在审计页与两 run 对比面板中显示。更早写入的
  数据库通过 `ALTER TABLE runs ADD COLUMN` 迁移获得该列。
- **两个 run 并排对照**（v0.5 批次 2）：审计页的 run 列表可选两条，并排显示短指纹、全量指纹、开始
  时间、状态与来源快照。逐字段指纹差异仍属 v0.6。
- **贡献者许可协议**（v0.5 批次 5）：[CLA.zh-CN.md](CLA.zh-CN.md)（以英文版为准）、
  [CONTRIBUTING.zh-CN.md](CONTRIBUTING.zh-CN.md) 的 CLA 章节、在 `pull_request_target` 上运行且
  **不检出 PR 代码**的 [`.github/workflows/cla.yml`](.github/workflows/cla.yml)，以及预创建的
  `signatures/version1/cla.json`。
- **黄金路径人工清单**（v0.5 批次 3）：[docs/golden-path-checklist.zh-CN.md](docs/golden-path-checklist.zh-CN.md)
  —— 在干净机器上走第 1–2 步时要填的字段，附填写示例。
- **第 3–7 步的 `--ignored` 走查**（`host/tests/golden_path.rs`）：真实 QEMU guest + mock LLM，依次
  跑任务 → 存快照 → 导出 → 恢复 → 改一个字段 → 再跑，最后对导出文件跑 `audit-verify`。

### 变更

- **README 的许可证说明又变短**（v0.5 批次 6）：代码为 Apache-2.0；提交贡献需 [CLA](CLA.md)。
  「开放核心」的措辞已从公开 README 撤下；授权条款本身留在 CLA.md 里，那才是贡献者真正签署的东西。
- **CONTRIBUTING 的 CLA 章节改为条件式**（v0.5 批次 6）：「若您向本仓库提交贡献」，因为贡献流程
  日后可能迁往别处。

### 备注

- **本预览版的 MSI 单独钉了一个安装器版本号。** `0.5.0-preview.1` 是合法的语义化版本，但不是合法的
  MSI `ProductVersion`（WiX 只接受 `major.minor.patch.build`，纯数字），因此 `tauri.conf.json` 里的
  `bundle.windows.wix.version = "0.5.0.1"` 提供数字形式，而包版本 —— 也就是产物名 —— 仍是
  `0.5.0-preview.1`。待包版本重新变成纯数字时，请删除或更新该字段。

## [0.4.0] - 2026-09-19

### 新增

- **QEMU 改为引导安装、不下载；并补上第三方声明**（v0.4 #4）：应用只告诉你该跑什么 —— 有 `winget` 时
  给 `winget install SoftwareFreedomConservancy.QEMU`，没有就给官网下载页 —— 而不是自己去拉取 QEMU。
  上游没有可钉的 Windows 二进制；第三方打包者会变成一段没被点名的供应链；自建 QEMU 构建会让我们成为
  GPL-2.0 二进制的分发者（[docs/qemu-distribution.md](docs/qemu-distribution.md) §5）。顺便写出来的
  下载器（`host/src/qemu_download.rs`）留在仓库里不接线、规格表为空，因为一个谁都无法复现的摘要比不下载
  更糟。我们依赖的许可证写在 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
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

[未发布]: https://github.com/breakevery/riscdom/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/breakevery/riscdom/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/breakevery/riscdom/compare/v0.6.0-preview.1...v0.7.0
[0.5.0]: https://github.com/breakevery/riscdom/compare/v0.5.0-preview.1...v0.5.0
[0.4.0]: https://github.com/breakevery/riscdom/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/breakevery/riscdom/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/breakevery/riscdom/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/breakevery/riscdom/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/breakevery/riscdom/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/breakevery/riscdom/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/breakevery/riscdom/releases/tag/v0.1.0
