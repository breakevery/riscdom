[English](handoff.md) | 中文

# 交接 —— 把 RiscDom 带进下一个对话

**本文件是跨对话交接文档。** 第 1 节是易变快照，正式版发布时更新；第 2–12 节是稳定约束：它们在各批次
之间没有变过，也是新对话必须守住的东西。

仓库 `D:\codeagent\breakevery\riscdom`，远端 `https://github.com/breakevery/riscdom.git`，分支
`main`。每个批次的收尾流程一致：gate 全绿 → `scripts\commit.ps1 "<msg>"`（它自己会跑 gate）→ push ——
而这些面向远端的动作，只在当轮请求明确授权时才做（见 §2）。

## 1. 快照 —— `v0.8.0` 是最新的发行版（下次正式发布时更新本节）

- **沙箱注册表有了控制平面表面：四条只读路由、四个 Tauri 命令、四个 CLI 子命令**（v0.9 沙箱 F2a-2）。`GET /v0/sandboxes` 答合并后的注册表加上 `current` 与 `default`；`/v0/sandboxes/current` 答这两个名字；`/v0/sandboxes/candidates` 答**原始扫描**（不是注册表，也不写回）；`/v0/sandboxes/{name}` 答一个定义，没有这个名字的定义时 `404` 并在 `cause` 指出参数。四条都声明第 29 个 capability `sandbox.read`。名字路由是继 `/v0/runs/{run_id}` 之后的第二条路径参数路由；其字面子路径（`current`、`candidates`，以及 F2 线后面才落的三个：`requests`、`switch`、`assemble`）永不被当作名字读——在 F2b/F2c 服务它们之前一律 `404`。四个 Tauri 命令（`list_sandboxes`、`current_sandbox`、`sandbox_candidates`、`get_sandbox`）已在桌面外壳注册，但**未与界面接线**——那是 D 线。四个 CLI 子命令（`sandboxes list` / `current` / `candidates` / `show <name>`）人类模式打表格，`--json` 原样透传。**F2a-1 报告的那处缺口已闭合**：§5.1 为 31 个查询，词汇表为 29 个名字，且每个名字现在都至少有一条路由。切换属 F2b，审批属 F2c，`Task.sandbox` 属 F2d。

- **沙箱注册表已存在：定义、扫描、合并**（v0.9 沙箱 F2a-1）。「沙箱」现在是可命名的：`host-core/src/sandbox_def.rs` 持有 `SandboxDef`（**存储**字段：`name` / `display_name` / `memory_mb` / `qemu_exe` / `toolchain_path` / `kernel` / `notes`，除名字外全部可选），以及 API **服务**的两种形状：`SandboxView`（定义加上只有宿主当场能回答的三项：`source`、`runnable`、`shadowed`）与 `CandidateView` / `CandidatesView`（一个已安装资源；两个互相独立的列表）。`LocalSettings` 新增 `sandboxes` 与 `default_sandbox`，均 `#[serde(default)]`，因此不做任何迁移，写于这两个字段存在之前的文件照样读得进。`AppState` 回答 `sandboxes()`（合并后的注册表）、`sandbox(name)`、`current_sandbox()`、`sandbox_candidates()` 与 `sandbox_default_name()`。合并优先级是手写 → 扫描 → 内置 `default`；同名时手写者胜，被遮的扫描项**留在列表里并标 `shadowed`**，所以合并是可见的而不是悄无声息的。扫描限于 `<data-dir>/toolchain/*` 与 `<data-dir>/qemu/*`（走下载器自己的 `find_compiler` / `find_qemu`，现已 `pub(crate)`，不再长第二份扫描）加上本机 QEMU，且**从不写回 `settings.json`**。`runnable` 是算出来的、从不存储：QEMU 存在且 `--version` 能跑、工具链存在、内核存在或可编译；因此卸掉一个资源只会让定义不可运行，不会让它消失。`Capability` 增至第 29 个名字 `sandbox.read`；四个端点、Tauri 命令与 CLI 属 F2a-2。已报告未修：API 文档 §5 表格仍只列出已有的 28 条路由的 capability，因为 `sandbox.read` 的端点在 F2a-2 才落——计数故意走在表格前面一格。

- **QEMU 与工具链同样接线，而拒绝本身是已定决策**（v0.9 沙箱 F1）。F 侦察发现一处不对称：`toolchain_download` 端到端跑通（宿主方法、端点、事件、CLI），而 `qemu_download` 是**有代码没 caller**。现在 caller 齐了，形状与工具链完全一致：`AppState::begin_qemu_download` / `qemu_download_status` / `cancel_qemu_download` / `download_qemu_now`（另加 `record_qemu_download_event` / `finish_qemu_download` 与 `qemu_dir()`）、审计事件 `host.qemu.download.start|done|failed|cancelled`、SSE 事件族 `qemu:download`、三个 Tauri 命令、三个端点（`GET`/`POST /v0/qemu/download`、`POST /v0/qemu/download/cancel`，capability `qemu.read` / `qemu.configure`）与三个 CLI 子命令（`qemu download [--wait]` / `cancel` / `status`）。**没接线的是「下载」本身**，这是决策：`spec_for_current_platform()` 在每个平台都拒绝（`docs/qemu-distribution.md` §5——RiscDom 引导用户自己安装 QEMU、不 pin 发布版，因为上游不提供 Windows 二进制，而猜一个摘要等于静默的完整性漏洞），所以 `POST /v0/qemu/download` 答 `503 unavailable`、`cause: "qemu"` 并附指引，也不占槽。拒绝之后的东西全部由回环夹具测试覆盖，因此**将来 pin 一个版本只是数据变更**：答案变成 `202`，其余已可用。两个形状合成了一个：`QemuDownloadEvent` 的 payload 标签现在是 `state`（曾是 `kind`），与 `toolchain:download` 同一套词汇；CLI 的 `--wait` 终止判定也变成一个函数（`download_terminal`）两族共用。`docs/control-plane-api.md` §5.1/§5.2 现为 27 查询与 29 控制（原 26/27），`docs/control-plane-events.md` 现为十二个事件（原十一个，§3 表第 12 行），`docs/decisions.md` §29 记录共用的装配形状。两处不对称保留未修（已报告）：`host-tauri` 在工具链下载失败时仍无条件 `eprintln!`（F1 的 QEMU 命令故意不写——C6 的日志开关只覆盖 `server`）；`paths.rs` 只加了 `qemu_dir_in`，没有进程级 `qemu_dir()`，因为工具链那个进程级对应物同样没有任何调用者。
- **接口说实话，库的日志变成可选项**（v0.9 CLI 批次 6/N）。C3/C4 留下的四件小事，合并修完。两条审计导出答的字段叫 `bytes_written`，返回的却是**事件数**（`write_events_jsonl` 返回 `events.len()`）：现在答 `events_exported`；而确实按字节写的 `/v0/serial/export` 保留 `bytes_written`——所以上面批次 5/N 那条读起来就是历史。workspace 策略拒绝的路径此前是 `403 forbidden`，读起来像授权判定，实际却是调用方参数问题：现在是 `400 bad_request` 加 `cause: "path"`，`403` 只留给认证与授权（`http.rs` 的 capability 检查与 `Authn` 钩子——钉这两者的测试未动）。库的运行日志行变成可选：`http.rs` 的 `connection … ended` 与 `accept failed`、`routes.rs` 的 `toolchain download failed` / `preflight failed` 都过 `ServerConfig::with_log_level`——`--log-level <off|error|info>`，**默认 off**——这正是把内嵌服务端挡在 CLI 的 stderr 之外的关键：批次 4/N 报告的粗糙点，现已闭环。`main.rs` 的启动横幅、用法文本与致命错误仍无条件输出：它们是二进制自己的控制台输出，内嵌场景根本不会跑到那个 `main`。另外本节两处失效的 `../host/src/run_diff.rs` 链接也已修正。
- **CLI 线收尾：五批，且每一个控制端点都有了子命令**（v0.9 CLI 批次 5/N）。`docs/control-plane-api.md` §5.2 剩下的十七个端点全部落地——三个导出（`export audit-jsonl`、`export run-audit <run_id>`、`export serial-log`）与十四个按批次 4 定的分组方式归类的管理配置命令（`llm set|clear|load-key`、`qemu path|clear`、`toolchain download|cancel|path|clear`、`preflight run|ack`、`audit alert set <on|off>`、`theme set`、`language set`）。加上批次 4 的十二个，27 个控制端点已全部可从 shell 触及，另加表外的两个（`vm start` → 预留的 `501`、`runs abandon-stale`）。查询半边未动：仍是批次 2/N 的六个只读命令。
  随之而来两件事。**`--api-key` / `--api-key-file` / `--remember`**：模型的 key 可以直接写在命令行（会警告，与 `--token` 完全一样）或从文件读，只有加 `--remember` 才会存进操作系统凭据存储。**`--wait`**：`toolchain download --wait` 与 `preflight run --wait` 在发请求**之前**先订阅 `/v0/events`，打印该工作所属事件族的帧（`toolchain:download` / `preflight:progress`），并以**工作本身**的判定退出——下载失败或预检某一步失败即 `3`。
  途中记下三个事实，每一个都塑造了实现：**`--out` 是服务端的路径**，相对于 workspace 根解析（逃出 workspace 即策略的 `403`），CLI 从头到尾拿不到文件；**两条审计导出的 `bytes_written` 是事件数、不是字节数**（序列日志导出才是字节数），所以人类模式那行会说明是哪种；**`preflight:progress` 没有“结束”事件**，所以 `--wait` 结束于 fail-fast 的第一个 `failed`，或最后一步的 `ok`（步骤表取自宿主自己的 `preflight::STEPS`）。已知粗糙处，只报告未修：内嵌 `--follow`/`--wait` 若在流仍打开时退出，可能把服务端那行 `connection … ended` 留在 CLI 的 stderr 上。
- **CLI 现在能驱动控制平面了**（v0.9 CLI 批次 4/N）。八个只读子命令之外又添十二个控制类子命令：`run <task>`（一轮 agent 的结果）、`vm stop`、`vm start`（预留的 `501`）、`snapshots save|resume|delete`、`sessions create|open|rename|delete|clear-all` 与 `runs abandon-stale`——全部是对控制平面的 HTTP `POST`，没有一个直接摸 `AppState`。随之而来两件事。**确认**：五条销毁状态的命令（`vm stop`、`snapshots resume`、`snapshots delete`、`sessions delete`、`sessions clear-all`）动手前先问——`--yes` 提前回答，终端上弹提示，stdin 不是终端即拒绝（退出码 `2`），因为沉默不是同意。**`--follow`**：`run --follow` 在发起运行**之前**先订阅 `/v0/events`，事件一到就打印（事件名加一小段 payload；带 `--json` 时是原样的 envelope），最后再打结果，因此运行的流从不需要事后补读。`cli/src/sse.rs` 为新增，负责读帧；`client::confirm` 拥有提问；`cli/tests/control.rs` 用真实二进制对着一个未配置模型的控制平面跑，因此不联网、不碰 QEMU。已知粗糙处，只报告未修：内嵌 `--follow` 若在流仍打开时退出，可能把服务端那行 `connection … ended` 留在 CLI 的 stderr 上，排在 CLI 自己的错误体之前。
- **`server` 已纳入门禁 clippy，且已 clippy-clean**（v0.9 CLI 批次 3/N）。该 crate 此前从未被 lint 过——门禁只选可移植 crate 与 `host-core`/`host-tauri`——把 `-p cli` 加进该步骤才暴露了它。6 处 `clippy::result_large_err`（`http.rs:379`、`routes.rs:489/495/503/513/522`）按 lint 的建议修：错误类型改为 `Box<Response<RespBody>>`，所有调用方返回 `*response`——同一个 Response 值，只是装箱了。随之一并修掉的还有 `routes.rs` 测试里 3 处 `bool_assert_comparison` 与 `tests/smoke.rs` 里 1 处 `filter_next`。现在 `cargo clippy -p server --all-targets --no-deps -- -D warnings` 静默通过，门禁步骤选 `-p cli -p server -p host-core -p host-tauri` 并保留 `--no-deps`。行为未变：同样的 `400` 对象、同样的构造、同样的传播路径。
- **CLI 现在是控制平面客户端**（v0.9 CLI 批次 2/N）。新增 `cli` crate（bin `riscdom`），只对控制平面说 HTTP：加 `--remote host:port` 就连已在运行的 `riscdom-server`；不加则在**自己进程内**把控制平面起在 `127.0.0.1:0`（由内核挑端口）——两种模式同一条代码路径，因此 `riscdom` 从不直接调 `AppState`。八个只读子命令（`health`、`status`、`agents`、`runs list|get`、`audit status|events`、`snapshots list`）；`--json` 原样透传控制平面的 JSON，人类模式打表格；退出码 `0`/1/2/3/4（成功 / 本地 / 用法或 400 / 拒绝或 5xx / 401-403）。token 本地取自 `<data-dir>/token`（首次由 `riscdom-server` 同一套代码生成），远程按 `--token-file` > `RISCDOM_TOKEN` > `--token`；从不被打印。锁文件未新增任何 crate，且门禁的 clippy 步骤现在也覆盖 `-p cli`。
- **更名遗留的失效引用已清空**（v0.9 A1 第 5 波）。所有**仍然生效**的旧 crate 提及都已改指真正拥有该物的 crate：`cargo test -p host` → `-p host-core`（测试住在那里）、`host/tests/…` → `host-core/tests/…`、`host/src/state.rs` 等 → `host-core/src/…`、`host/src/commands.rs` → `host-tauri/src/commands.rs`、`host/README.md` → `host-tauri/README.md`、`host::` 前缀按所指对象改为 `host_core::` 或 `host_tauri::`。CONTRIBUTING、SECURITY、PROJECT_CONSTITUTION 与 ci.yml 注释里的 crate 清单现在都写两个 crate，`README.md` 的 crate 索引也列出 `host-core` 与 `host-tauri`。历史未动：CHANGELOG、RELEASE_NOTES、决策账本、本文件 §1 与 architecture-evolution 快照仍按当时的事实使用 `host`。非 Windows 的 clippy 缺口本波**刻意不补**。
- **宿主拆分完成：`host-core` + `host-tauri`**（v0.9 A1 4 波中的第 4 波）。`host` 更名为 `host-tauri`（目录、`[package] name`、workspace member），桌面壳依赖它：`ui/src-tauri/src/lib.rs` 里 57 处 `host::` 全部改为 `host_tauri::`，且每一处都经门面解析，因此外壳只依赖一个 crate，无需直接依赖 `host-core`。清单里的 `tokio` 死依赖已删除（本 crate 与其测试从未用过它）。`cargo tree`：`-p host-core` 0 行 Tauri、`-p host-tauri` 15 行、`-p worker` 与 `-p server` 仍 0 行。`worker` 补上了它一直缺的 README 双语对；`host-tauri/README.md` 说明两 crate 边界；账本新增一条（`docs/decisions.md` §27）。
- **`worker` 与 `server` 不再链接 Tauri**（v0.9 A1 4 波中的第 3 波）。两者都从 `host` 切到 `host-core`：30 处 `host::` 路径改为 `host_core::`（worker 4 文件 7 处，server 7 文件 23 处——按出现次数全量改，不只 `use` 行），各自的 `Cargo.toml` 依赖也改成可移植半边。`cargo tree -p worker` 与 `-p server` 现在**一个 Tauri crate 都没有**；此前各列出 15 行 `tauri`。逻辑未变——只是 import 路径与各一行依赖。两个 crate 的端到端测试（worker 协议、HTTP + SSE 控制平面）原样通过，工作区测试仍为 418 条。还剩一个消费者：第 4 波把 `host` 更名 `host-tauri` 并搬桌面壳。
- **宿主的测试随拆分搬迁，且被拆弱的两个守卫恢复完整**（v0.9 A1 4 波中的第 2 波）。39 个集成测试文件全部由 `host/tests` 搬到 `host-core/tests`（`git mv`，保留历史），其 133 处 `host::` 路径改为 `host_core::`——67 条 `use host::`、65 处函数体内路径、1 处文档链接。`host` 现在没有测试、也没有 `[dev-dependencies]`；测试用的是 `host-core` 自己的依赖（`agent`、`audit`、`sandbox`、`serde_json`、`sha2`、`rusqlite`、`zip` / `flate2` + `tar`），因此没有任何依赖被声明两次。第 1 波里悄悄失效的两个守卫在此修好：`scripts/check-mirrored-constants.mjs` 现在同时扫描 `host-core/src` **与** `host/src`（17 个文件，此前只有 3 个），`scripts/gate.sh` 在同一个 clippy 步骤里跑 `-p host-core -p host`（此前 host-core 完全逃过 clippy）。消费者仍未动——那是第 3、4 波。
- **宿主已拆：`host-core` + 门面**（v0.9 A1 4 波中的第 1 波）。内核门面的可移植半边——审计接线、VM 槽、快照、会话、两条下载路径、预检、事件 envelope 与 `EventSink` trait——现在在 `host-core` crate，且**其依赖树里没有任何 Tauri crate**（`cargo tree -p host-core` 一个都没有；`-p host` 仍有 15 行）。`host` 保留 53 个 Tauri 命令、`TauriEventSink` 传输与 Tauri 依赖，并再导出可移植面（`pub use host_core::*;` 加 `host::events::*`），因此**本波没有任何消费者或测试改动**：`worker` / `server` / `ui/src-tauri` 仍对着 `host` 编译、仍链接 Tauri。后续三波依次：把测试搬到 `host-core/tests`；把 `worker` + `server` 搬去 `host-core`（两者从此不链 Tauri）；把 `host` 更名 `host-tauri` 并搬桌面壳。见 [host-core/README.zh-CN.md](../host-core/README.zh-CN.md)。
- **权限已强制，两种传输对身份一致**（v0.9 批次 5/N）。`Capability` 现在是路由表（`server/src/routes.rs`）的类型化列：写不出一条不声明它的路由，也就没有跳过检查的路径。`Authn` 钩子返回 `Actor` 之后，请求路径会问它是否 `allows` 该 capability，不持有即 `403 forbidden`、`cause` 为 `"capability"`（`server/src/http.rs`）——默认拒绝、28 个名字、`Actor` 携带该集合。v0.9 只有两种 actor 形状且都持有全部（token 持有者记为 `operator`，以及 `--no-auth` 的钩子），故 `403` 只来自返回更窄 actor 的钩子。身份复审：每个 sink 打的都是**来源**的身份——`AppState::agent_id()`——且 `Server::sink()` 不再接受身份参数，故同进程内的两种传输不可能不一致；新测试把同一事件经两个 sink 发出，在线上断言 `agent_id` 一致。文档：API 文档 §3 现为「认证与权限」，客户端指南新增 capability 一节与安全部署示例（nginx 前置、流用 `proxy_buffering off`），[server/README.zh-CN.md](../server/README.zh-CN.md) 说明 token 管什么、不管什么。
- **控制端点、token 与事件补发已落**（v0.9 批次 4）。`docs/control-plane-api.zh-CN.md` §5.2 的 27 个 `POST` 端点全部可调，另加预留的 `POST /v0/vm/start`（501）与已实装的 `POST /v0/runs/abandon-stale`（G4）；服务端现在默认装入 `TokenAuth`——在 `<data-dir>/token` 生成 32 字节随机 token、仅属主可读、常量时间比对，可用 `--no-auth` 关闭——事件流的 id 改为服务端全局帧序号，带 `Last-Event-ID` 时从 1024 帧的有界缓冲补发，游标过旧则发 `gap` 帧。每条路由都声明所需权限并交给钩子（强制已于批次 5/N 落地，见上）。
- **审计存储的打开路径已并发安全**（v0.9 批次 3 之后的修复）。两个进程同时打开同一个全新的 `audit.db`，过去必有一个失败：`PRAGMA journal_mode = WAL` 需要独占访问，且绕过 busy_timeout 直接回 `SQLITE_BUSY`，因此覆盖写路径的 5 秒超时从未覆盖这次切换——这正是 gate 曾在 `audit::concurrency` 上 flake 一次的原因。现在 `AuditStore::open` 会对整条序列重试（每次新连接，`OPEN_MAX_ATTEMPTS` / `OPEN_BACKOFF_BASE`，定义为写路径常量的别名），且只对锁错误重试。写路径的 `BEGIN IMMEDIATE` + 重试、哈希公式、链上历史行与触发器均未动。由两条新的竞态测试钉住（8 线程 × 25 轮，一个新文件、一个已在 WAL）。
- **查询 API 与统一事件 envelope 已落**（v0.9 批次 3/N）。`docs/control-plane-api.zh-CN.md` §5.1 的 26 个查询端点全部可经 HTTP 调用——另加宿主本地端点 `/v0/health`、`/v0/status`、`/v0/events`、预留的 `/v0/resources`（501）、以及新增错误码 `405 method_not_allowed`——并且每个传输现在都把要发的东西包进同一个 envelope，构建于 `host/src/events.rs`。事件文档标为「有变」的三种 payload 形状均已落地（`vm:state` 恒带 `name`、`audit:failed` 用 `message`、`toolchain:download` 标签为 `state`）；其余八种未动。webview 在唯一边界处解包（`ui/src/api/tauri.ts`）；worker 的行协议与 SSE 流按原样承载 envelope。客户端走查见 [docs/control-plane-client-guide.zh-CN.md](control-plane-client-guide.zh-CN.md)。此条列出的三项缺口——控制类、`gap` / `Last-Event-ID`、权限强制——已分别由批次 4 与批次 5/N 闭环。
- **控制平面骨架已落**（v0.9 批次 2/N）。新增 `server` crate——可执行文件 `riscdom-server`——绑定了已定稿的接口面：`GET /v0/health`、`GET /v0/status`、`GET /v0/events`（SSE，`hello` 与 `event` 两种帧）、B1 错误模型、以及 `Authn` 钩子（当时默认 `NoAuth`；批次 4 起默认 `TokenAuth`）。它是架在 `host` 之上的 Layer 3，不引用任何 Tauri 类型（仍会链接 `tauri`——已知代价）。它不改 `host` 任何源码：`HttpEventSink` 从 `run_agent` 的 `emitter` 参数位注入。`gap` 帧与 `Last-Event-ID` 补放**尚未实现**（下一批）；设计上给它们留的位置已在 `docs/control-plane-events.zh-CN.md` 标注。见 [server/README.zh-CN.md](../server/README.zh-CN.md)。
- **控制平面协议已定稿**（v0.9 批次 1/N，仅设计，无代码）。两份双语成对的文档固定了管理侧要照着实现的接口：HTTP 命令/查询面见
  [control-plane-api.zh-CN.md](control-plane-api.zh-CN.md)——53 个命令变成 53 个端点（26 个 `GET` / 27 个 `POST`），四项内核能力缺口逐项明确处置；推送侧见
  [control-plane-events.zh-CN.md](control-plane-events.zh-CN.md)——SSE 分帧，加一个十一个事件共用的 envelope（`version` / `kind` / `event` / `agent_id` / `task_id` / `ts` / `payload`）。十一个 payload 里有三个改了形状（`vm:state`、`audit:failed`、`toolchain:download`），其余八个是恒等映射。**传输选 HTTP + SSE，不用 WebSocket**：推送是单向（服务端到客户端），SSE 是纯 HTTP（无升级握手、无额外 crate），且自带 `Last-Event-ID` 重连。设计定稿，尚无任何实现。
- **v0.8.0 发布之后：preflight 目录也改为 per-agent**（遗留项 A2 —— 共享 workspace 里最后一条写入路径）。
  新产物写入 `<workspace>/.riscdom/preflight/<agent_id>/`；要启动的 guest 先在 agent 自己的目录里找、再回退共享
  根目录，因此 A2 之前的 guest 仍然可用、不会被孤立。`settings.json` 里的缓存本就随实例隔离。v0.8.0 发布说明中
  「preflight 目录仍共享」这一条据此闭环；那边仅剩 `tauri` 仍未 optional。
- **v0.8.0 发布之后：派发得到的 outcome 写明的是真正跑了任务的执行者**（主体交付 3/3 —— v0.8.0 发布说明里
  三条「已知限制」的第一条已闭环）。`AgentHandle::run` 返回 `TaskOutcome`，并填入只有它知道的身份：本地 loop
  自己的 id、host 实例自己的 id、或子进程声明的 id（从它的 `worker:ready` 事件读出，缺失则上报而不是猜）。
  `LocalDispatcher` 改为透传而不再盖 `Task.target`，因此 `TaskOutcome.agent_id` 现在回答的是「谁跑了它」而不是
  「它被发给谁」。v0.8.0 还有一个边角未结：`tauri` 仍未 optional（共享 preflight 那条已由遗留项 A2 闭环）。
  `RELEASE_NOTES.md` 是已发布的正文、**不改**：它的「已知限制」里仍带着已被本行与上一行取代的边角。
- **`v0.8.0` 已发布**（2026-09-22）：版本 bump 到 `0.8.0`（7 个文件：`Cargo.toml`、两个 `Cargo.lock`、
  `ui/package.json`、`ui/package-lock.json`、`ui/src-tauri/Cargo.toml`、`ui/src-tauri/tauri.conf.json`
  —— wix 守卫要求纯数字正式版不带 `bundle.windows.wix.version`，当前确实没有），`CHANGELOG` 的
  `[Unreleased]` 归入 `[0.8.0] - 2026-09-22`，[RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) 按正式发布
  重写 —— **GitHub Release 的正文就是该文件（英文版 `RELEASE_NOTES.md`）的逐字拷贝**（v0.7.0 的发布就是
  这么做的）。附件：本机构建的 `RiscDom_0.8.0_x64_en-US.msi` 与 `RiscDom_0.8.0_x64-setup.exe`，以及从 CI
  `bundle` job 下载的 macOS/Linux 包。
  v0.8 是什么：完整双语的界面，以及
  多 Agent 地基（多进程审计写入、每条事件带身份、per-agent 快照、派发抽象、两进程雏形）。
- **`v0.7.0` 已发布**（2026-09-21，本次发布之前的那一次）。 版本已 bump 到 `0.7.0`（7 文件 / 15 处 —— 与 v0.6.0-preview.1 相同
  的落点），预览版专用的 `bundle.windows.wix.version` 覆盖再次**删除**（包版本已是纯数字，wix 守卫要求
  如此），`CHANGELOG` 与 `RELEASE_NOTES` 已按正式发布重写，Windows 安装包已构建：
  `RiscDom_0.7.0_x64_en-US.msi` 与 `RiscDom_0.7.0_x64-setup.exe`。次日发布了：annotated tag `v0.7.0`
  （→ `2bddae6b0897bb5fe262af2b7e4bf4b3733ec7eb`）、正文为 `RELEASE_NOTES.md` 逐字拷贝的 GitHub Release，以及
  6 个附件（两个 Windows 安装包 + 一次重新 dispatch 的 `bundle` 产出的、名字为 `0.7.0` 的 macOS/Linux 包）。
  v0.7 落地了三块：**自建 i18n
  设施**（批次 1–2 —— `ui/src/i18n/`、gate 里的 `scripts/check-ui-strings.mjs`、以及*设置 → 外观*里写入
  `settings.json` 并把 `<html>` 的 `lang` 跟着改的语言切换）、**macOS/Linux 构建**（批次 A 的平台工作 +
  批次 B 的 `bundle` job，见下一条），以及让 `host` 在非 Windows 上能编译的修复（批次 8）。**用注册表铺开
  其余约 190 条界面字符串这件事有意不做**：设施与那 4 条 diff 字符串保留，全量翻译不做。
- **macOS 与 Linux：CI 能出包，但尚未人工走查。** `ci.yml` 有 `bundle` job（dispatch 或 `v*` tag；
  macOS + Linux 两个 runner），执行 `npm run tauri build` 并把安装包作为 artifact 上传 —— macOS aarch64
  的 `.app` + `.dmg`，Linux amd64 的 `.deb` + `.rpm` + `.AppImage` —— 在 run `35572294916` 全绿。批次 A
  还补齐了 `icons/icon.icns`、让 QEMU 安装指引随平台变化（`winget` / Homebrew / 发行版包，见
  `sandbox::qemu_discover::install_hint_for`），并用一条跨平台单测钉住 Unix 的 `-qmp unix:` 参数。
  **尚未做**：没有人启动过这些安装包；它们**未签名**（macOS Gatekeeper 会拦下首次运行；Developer ID
  签名与公证属于商业化层）；真实 Unix socket 的 QEMU 运行仍需一台 Mac 或 Linux 机器。Windows 仍是黄金
  路径验证过的平台。当时的 CI 包是在 `0.6.0-preview.1` 名字下构建的，所以 v0.7.0 发布时重新 dispatch 了
  `bundle` 以取得 `0.7.0` 名字的包 —— 它发布的就是那些。
- **`v0.6.0-preview.1` 已作为预发布版发布**（v0.6 批次 1–2，批次 4 发布）：两次 run 逐字段对比 —— 数据层
  与 API（[../host-core/src/run_diff.rs](../host-core/src/run_diff.rs)、`AppState::compare_run_fingerprints`、
  `compare_run_fingerprints` 命令）以及审计页两 run 面板下方那个默认折叠的区块。预发布版**不持有 Latest
  标记**，因此当时 Latest 仍在 `v0.5.0`（此后已先后移到 `v0.7.0`、`v0.8.0`）。附件：`RiscDom_0.6.0-preview.1_x64_en-US.msi` 与
  `RiscDom_0.6.0-preview.1_x64-setup.exe`，构建时带 `bundle.windows.wix.version = "0.6.0"`（WiX 不接受
  预发布版 `ProductVersion`），所以「应用和功能」里显示 `0.6.0`，而产物名保留包版本。它证明了什么、
  没证明什么写在 [RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md) —— 它所点的第一条缺口如今已闭环：
  **第 8 步的界面已经人工走查、结论通过**（由项目所有者走查；没有单独归档记录文件，因此
  `walkthroughs/` 里仍只有 v0.5 那次本地走查）。
- **`v0.5.0` 已发布，当时它持有 Latest 标记。**（Latest 此后已先后移到 `v0.7.0`、`v0.8.0`。）
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
- 近期提交（新→旧）：`20f2052`（v0.7.0 发布准备：版本 bump、变更日志、发布说明）← `57dd25e`（v0.7 文档
  快照）← `202dd75`（非 Windows 的 `extract_zip` 存根）← `344fd2b`（Linux 包需要的 rpm）← `0633bdc`
  （macOS/Linux 的 bundle CI job）← `833f9c3`（随平台变化的 QEMU 指引、icon.icns、Unix QMP 单测）←
  `06fef0a`（语言切换）← `6abcb44`（i18n 试点）← `b0efeb8`（v0.6.0-preview.1 发布）。
- tag：`v0.8.0` 是最新的 tag，也是**持有 Latest 标记**的那次发布（已用 `gh release list` 确认：
  2026-09-22T07:37:50Z，annotated tag 对象 `0b018081de1a4e89e04e7bc1570d595d38ab4b4b` →
  `8a5381436b62fa84b4f4a972a630061ce2203373`）；`v0.7.0` = annotated tag 对象
  `f267f13dc6f8df9a3ff196b05d3bb9b7724f2d60` → `2bddae6b0897bb5fe262af2b7e4bf4b3733ec7eb`（Latest
  标记原在它身上，本次发布接管）；`v0.6.0-preview.1` 与 `v0.5.0-preview.1` 为预发布版；
  `v0.5.0` =
  `cea44f7b9920a079422217f811afb49350e08477` → `287ffdb095e1659b89a8cafe040647ada64d0026`；
  `v0.4.0` = `25bd3da3c31c1d1ec7e163f3835b0c2bbb74546d` → `15fda1f6d76d53a4ff1b621c2d3d91f0b4b87311`；
  `v0.3.1` = `d8fdba66a366632ca569d8db2657ab5a566b991c` → `b9be9111c620faad686c7a9d095e0ebc04b31225`。
- `main`（本次发布，`602f402`）处的测试总况：**330 passed / 0 failed / 8 ignored / 90 suites**
  —— 监工批次之前为 324 / 0 / 8 / 87，`v0.7.0` 发布提交处为 295 / 0 / 8 / 80，`v0.6.0-preview.1` 处为
  291 / 0 / 8 / 80，`v0.5.0` 处为 281 / 0 / 8 / 79。gate 共 13 步（v0.7 批次 1 新增了 UI 字符串注册表），
  本地与 CI 均全绿（`ubuntu-latest` 上跑 `scripts/gate.sh`，另加 gitleaks）。
- **架构演进文档已定稿并落盘**：[architecture-evolution.md](architecture-evolution.md)（双语，与
  [architecture-evolution.zh-CN.md](architecture-evolution.zh-CN.md) 成对）记录了 v0.7.0 之后做的架构重估 ——
  四层分层与 syscall 层的「机制/策略」划分、已定决策（Tauri 解耦 A3 → A1、B2 多进程模型、审计单链 +
  agent_id）、为多设备预留的缝，以及通往 v1.0 内核 API 冻结的里程碑路径。只写文档：未改代码。
- **v0.8 主体交付完成 —— 监工派发器 + 多执行者，可演示**（v0.8 主体交付 2/2）：端到端可跑。`worker`
  增加了库半边（`worker::supervisor`）与可运行演示（`cargo run -p worker --example dispatch`）：它起多个
  执行者进程，**共享一个 workspace**、各自拥有一个 **data dir**；任务按 `Task.target` 路由到同名执行者；
  每个任务打印一行，并给出计数。任务**并发**派发（`std::thread::scope` 每任务一线程 —— 句柄是
  `Send + Sync`，因此不需要线程池依赖）；指向不在机群里的执行者会被**拒绝**，不会发给「猜一个」的执行者。
  监工里没有任何模型：这一阶段的监工是派发器，不是 agent。同批收尾：`worker` 的 `audit` 依赖声明了却从未
  使用（执行者经 `host::AppState` 触链）已删除；监工逻辑放进 `worker` 的库，使演示与测试共用一份实现；
  四项已定决策写进了 [多 Agent 地基文档](multi-agent-foundation.md)。
- **两进程雏形已落地**（v0.8 主体交付 1/2）：监工与执行者是两个进程，经 **stdio + JSON lines** 对话。
  `worker`（新 crate，执行者二进制）在 stdin 上读一行 `Task` JSON，用注入的 data 目录跑 host 自己那条
  `run_agent` 路径，在 stdout 上写一行 `TaskOutcome`；它的事件以 JSON 行写到 stderr。监工侧
  `host::StdioExecutorHandle`（一个 `AgentHandle`）起那个二进制、发任务、读结果、吸干事件，到时不答就杀掉
  —— 句柄之上没有改动，这正是 v0.8 批次 4 留开的缝。传输零新依赖；但 worker **确实会链接 Tauri**
  （host 无条件依赖它 —— 改为 optional 是 v0.9 的清理）。两个值得知道的边角：子进程自己的 `agent_id` 是经
  它的事件回来的，而分发器的 `TaskOutcome.agent_id` 保持批次 4 的语义（被寻址到的那个执行者）；以及环境里
  有 key 的执行者**会真的跑**（探针测试移除了该变量，因此这里不会调模型）。
- **最小派发抽象已落地**（v0.8 批次 4）：任务现在可以**派发**，而不再只能内联调用。
  `agent::dispatch` 持有词汇 —— `Task`、`TaskId`（`task-<pid>-<seq>`）、`AgentId`（批次 3 的身份形状）、
  `TaskOutcome`、`DispatchError` —— 以及构成缝的两个 trait：`AgentHandle`（执行者：跑这个任务、返回结果）
  与 `Dispatcher`。**本地**一半已实现：`agent::LocalAgent` 包一个 loop，`agent::LocalDispatcher` 按 target
  路由，host 在其既有 `run_agent` 路径上新增 `HostAgentHandle` + `host::local_dispatcher`。**远程**一半
  有意缺席 —— 这个缺席本身就是那道缝。它放在 `agent` 而不是 `host`，正是为了让该抽象不归 Tauri 外壳所有、
  **不**强迫做 host-core / host-tauri 拆分（[架构演进](architecture-evolution.zh-CN.md) §7 缝 2）。Tauri
  命令仍照旧直接调 `run_agent`：派发路径是新增的内部通路。
- **v0.8 技术债批次已落地**（v0.8 批次 1）：[架构演进文档](architecture-evolution.md) §8 列为债务的三个
  堵死点已清除。app data 目录改为**注入** —— `AppState::with_data_dir(workspace, data_dir)` 取代了
  `host::paths` 里进程级的 `OnceLock`，于是同一进程内的两个实例各自拥有 `settings.json`、会话 DB 与工具链
  目录。审计链新增 **`agent_id`** 列，位于链**旁边**：哈希公式、`prev_hash` 链接与每一行既有的 `hash`
  全不动，因此 v0.8 之前的链仍能通过校验。以及**每个 `AppState` 一个 VM 槽**，以测试钉住而非假定。改的是
  代码不是文档：审计侧两条新测试、host 侧一条新测试（`multi_instance.rs`），黄金路径无变化。
- **每条审计事件都写明是哪个 agent，快照改为 per-agent**（v0.8 批次 3）：身份为
  `local-<pid>-<seq>`（`agent::next_agent_id`），每个 `AppState` 领一个并传给它构建的 `AgentLoop`，
  于是 host、sandbox 与 agent 的事件都带上它；`AgentLoop` 与 `audit_hook` 助手改为接收该参数。
  `agent_id` 仍是链**旁边**字段 —— 哈希公式、`prev_hash` 链接、历史行均不动。快照改到
  `<workspace>/.riscdom/snapshots/<agent_id>/`，读取回退到共享根目录，因此共享同一 workspace 的两个
  agent 不会互相覆盖，v0.8 之前的快照仍可列出、恢复与删除。
- **审计写入现已支持多进程，且失败会响**（v0.8 批次 2）：`audit.db` 是有意共享的（每个 workspace 一条
  链），因此连接以 **WAL** 打开，带 5 秒 `busy_timeout` 与 `synchronous=NORMAL`；一次 append 在读取 head
  **之前**先拿写锁（`BEGIN IMMEDIATE` —— 没有它两个写者会链到同一行并把链分叉，新增的并发测试抓住了这一
  点）；被锁的 append 先以 20/40/80/160 ms 退避重试 5 次，再报错。`AuditSink::record` 改为返回
  `Result<(), AuditError>`，不再丢弃事件；sink 通知 host，host 记日志、发 `audit:failed`，并（默认）在
  *设置 → 审计*给出横幅与弹窗。**产品决策：告警默认开、用户可关；事件与日志行不可关。** 链结构、哈希
  公式、历史行与触发器全部未动。
- 未完成项：`%TEMP%` 下的临时目录仍未清理（删除确认始终未被放行；2026-09-21 统计到 144 个
  `riscdom-*` 条目）；CLA.md 待律师过目；**由他人进行的干净机器走查尚未发生** —— 仍是 v0.5.x 的补强项，
  而不是阻塞项；**macOS/Linux 的安装包从未被启动过**且未签名（签名属商业化层）；v0.5 走查留给真人的两项
  （G-2、真实键盘输入中文）仍未做。**v0.7.0 的发布本身就是下一批**：push、tag、Release，以及一次重新
  dispatch 的 `bundle`（产出 `0.7.0` 名字的 macOS/Linux 包）。

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
那份本地走查代位。第 3–7 步由 `cargo test -p host-core --test golden_path -- --ignored` 覆盖；第 8 步的
比较目前**没有**这样的自动走查（§9）。「实现完成」不是门槛。

## 9. v0.6 从黄金路径第 8 步开始 —— 该步已交付

v0.6 是两次 run 的自动比较 —— 它们的指纹差在哪些字段 —— 这正是 v0.5 刻意不做完的那一步。**已交付**
（批次 1–2：`host-core/src/run_diff.rs`、`AppState::compare_run_fingerprints`、`compare_run_fingerprints`
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
