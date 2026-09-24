[English](CHANGELOG.md) | 中文

# 变更日志

本文件记录项目的所有重要变更。

格式基于 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/spec/v2.0.0.html)。

## [未发布]

**控制平面能被驱动了，而且有门禁。** `docs/control-plane-api.zh-CN.md` §5.2 的 27 个控制端点已可用——跑 agent、管会话、存/恢复快照、停 VM、设置工具链与 QEMU 路径、跑预检、配置 LLM、导出审计——并且除非以 `--no-auth` 启动，服务端现在要求每个请求都出示 bearer token。事件流补上了 `Last-Event-ID` 补发与 `gap` 帧。

**现在每个端点都检查权限，且两种传输对身份一致。** 控制平面不再只是标注每条路由需要什么：服务端拿它与 `Authn` 钩子返回的 actor 比对，不持有即 `403`。sink 打的身份改从**来源**取，一个事件不可能看起来像两个 agent。

**宿主拆成「可移植半边」与「Tauri 半边」。** `host-core` 现在装着宿主所有不需要 webview 的部分，其依赖树里没有任何 Tauri crate；`host` 保留命令、Tauri 传输与 Tauri 依赖并再导出可移植面，因此本波其余一切未变。

**宿主的测试随拆分搬迁，两个守卫恢复完整。** 39 个集成测试文件现在住在 `host-core/tests`，即在可移植半边所在之处测试它；镜像常量守卫与 clippy 步骤也重新覆盖两个 crate。

**`worker` 与 `server` 不再链接 Tauri。** 两者改为依赖内核门面的可移植半边，于是任何无头进程都不再被拖进一套 GUI 工具链。

**宿主拆分完成：`host-core` + `host-tauri`。** 承载 Tauri 命令的 crate 现在按它的实质命名，桌面壳是它唯一的消费者，其下层没有任何东西链接 Tauri。

**文档与注释追上更名。** 所有仍然生效的旧 crate 引用都改指 `host-core`（内核能力、测试）或 `host-tauri`（Tauri 层）；历史一字未动。

**有 CLI 了，而且是控制平面客户端。** `riscdom` 在 shell 里驱动控制平面——与桌面应用同一套 HTTP 接口——本地模式把控制平面起在自己的进程里。

**Zig 是沙箱能编的第二种语言。** `.zig` 源经 `zig build-exe -target riscv64-freestanding` 编成同一个载入地址上的同一个裸机 ELF；Zig 自带交叉链接器，裸机目标无需外部工具链、无需 sysroot。语言按源扩展名分派，所以 `compile_freestanding` **签名字形不变**、C 路径一字未动，生成的链接脚本原样复用。**不**包含下载 Zig 归档：其 macOS/Linux 构建是 `.tar.xz`，现有下载器无法解包——那是独立批次，并同时服务 Rust。

**宪法不再与编译器矛盾。** `PROJECT_CONSTITUTION.md` 在**三处**禁 Zig——§3.6（在 `non-negotiable` 列表内）、§4.6、§5——并在 §9 的 v0.1 清单里又记了一次。三处现在各带一条**时效旁注**；原句与 `non-negotiable` 标题一字未动，因为那些条款禁 Zig 的条件是「**MVP 阶段**」，而 MVP 已于 v0.8.0 结束。只动了 Zig：C++、Rust、Python 仍在禁令内。§9 未动——v0.1 当时确实只支持 C。记入决策 §47。

**多了 `.tar.xz` 归档类型。** `ArchiveKind` 新增 `TarXz`，`extract_tar_xz` 用 `xz2::read::XzDecoder` 照抄 `extract_tar_gz`——同一个 Zip-Slip 守卫、同一个覆盖语义、同一套逐条目取消。该臂**不带平台门**：`.tar.xz` 是宿主自己 Zig 与 Rust 下载的形态，所以 Windows 主机也得能读。`xz2` 本就在 `Cargo.lock` 里（由 `zip` 带入）：锁只多一行、无版本变动，也没有任何平台新增系统库前提。目前还没有东西会去下载 xz 归档——那是 apply 批。

**Zig 编译器现在能从应用里安装，而下载终于会说自己在为哪种语言干活。** `DownloadSpec` 新增 `toolchain`（`C` / `Zig`）：它决定归档里由哪个定位器找产物，以及装完之后做哪一次「采用」——C 编译器走 `set_toolchain_path`，Zig 走 `set_zig_path`。语言以各条边都接受的**标签**传递（`--toolchain zig`；下载端点 body 里的 `{"toolchain":"zig"}`，body 是本批新加的，不发 body 仍然等于 C；Tauri 命令上一个可选参数），状态也把它报回来。Zig 的校验和与 xPack 一样钉在源码里。

**只要机器上有 `rustc` 与目标的 sysroot，Rust 也能编。** `compile` 第三次按扩展名分派：`.rs` 走 `rustc --target riscv64gc-unknown-none-elf --sysroot <dir>`，配上 C / Zig 路径同一份生成的 `link.ld`，以配置里的 RISC-V GCC 为链接器，并用 `panic=abort`。`rustc` 来自机器（同 QEMU 的先例），sysroot 是一项设置——一个 `rust-std-<target>/` 目录，因为 Rust 向我们要的是目标的 `core`。缺哪一半就**点名拒绝**，绝不静默。`rust-std` 的下载是独立批次。

**Rust 的 sysroot 也可以下载了，而且它是唯一「产物与版本硬绑定」的 pin。** `Toolchain` 新增 `Rust`，于是 `--toolchain rust`（或 body `{"toolchain":"rust"}`）会下载裸机目标所钉的 `rust-std` 组件：一个资产服务所有平台，因为 `rust-std` 是给**目标**而不是给宿主的。定位器返回归档嵌下一层的那个 **sysroot 目录**；当机器的 `rustc` 不是钉住的那个 release 时，宿主会在下载之前就拒绝——sysroot 只能由产出它的 `rustc` 使用。`rustc` 本身仍来自机器。

**宪法与编译器在 Rust 上也不再矛盾。** `PROJECT_CONSTITUTION.md` 在 §3.6（在 `non-negotiable` 列表内）、§4.6、§5 三处禁 Rust，而 §47 当时只把这三处的 Zig 半边标了时效、把 Rust 留作「待 F3b」。现在这三条旁注各自补上 Rust 那句，且 §5 的旁注把仍在禁的写明：**C++、Python 仍在禁令内**——Python 待 Linux 沙箱（v1.x），C++ 仍不在范围。原句与 `non-negotiable` 标题一字未动；§9（v0.1 完成情况）同样未动——v0.1 当时确实只支持 C。沙箱能用的语言现为 C / Zig / Rust。记入决策 §53。

**会话库会等锁，而不是当场失败。** SQLite 的 `busy_timeout` 默认是 0，所以第二个进程写 `sessions.db` 时会**立即**收到 `SQLITE_BUSY`——而这个失败落在 `SessionStore::open` 里，倒下的不是一条命令而是**整个实例**。两个进程确实可能撞上同一个文件（两个走默认路径的 CLI / 服务器进程，或显式共用 `--data-dir`），因此连接现在会等 5 秒，且在它**第一次写之前**就设好，与审计库同一个形状。**故意不开** WAL：审计库本来就是要跨进程共享的，会话库是 per-instance，所以两者的并发模型本就不同（决策 §54）。另外 `append_message` 现在是**一个事务**：消息插入与会话 `updated_at_ms` 的更新原本是两条语句，中途失败会留下一条「与其会话时间戳相矛盾」的消息。

**控制平面可以自己提供 Web UI 了。** `riscdom-server --web-root <dir>` 把构建好的前端挂在 `/` 与 `/assets/*`：与 API 同源（所以浏览器的 `fetch` 不需要 CORS 层），位置在路由表之前、且不做 capability 检查——资源不带秘密，而 `/v0/*` 下的一切仍然要认证。命名空间就是这两种形状、仅限 `GET`、没有 SPA fallback，也没有往被文档锁死的表里加任何路由；hash 资源缓存 `immutable`，`index.html` 是 `no-cache`，而名字从不做百分号解码（编码过的 `..` 是「不存在的文件」，不是穿越）。不给 `--web-root` 时，服务端与从前完全一样，而 `/` 会说清为何没有界面。同批修正：`docs/control-plane-events.md` 曾描述一个从不存在的 cookie 会话端点——这条流是用 `Authorization` 头认证的，所以浏览器用 `fetch` + `ReadableStream` 去读（文档现在给的正是这段代码，而不是带 cookie 的 `EventSource`）。

**一份构建产物现在同时服务桌面外壳与浏览器。** `ui/src/api/` 为自己的面长了第二个实现：`tauri.ts` 保留外壳的 `invoke` / `listen`，`http.ts` 调控制平面的 HTTP 端点，`index.ts` 依 Tauri 2 自己的全局量**在运行时选一次**——构建不用改，一份 `dist/` 同时服务外壳与服务端的 `--web-root`。**形状**搬到 `api/types.ts`（它们是宿主的，不是某个传输的），envelope 规则搬到 `api/envelope.ts`（一份拷贝、一条规则）。四个消费者现在都 import `../api`，而两份实现既由类型检查互相锁住，又由一个新探针锁住。26 个只读端点已实现——包括那 8 个以单字段包装应答的——而 26 个控制以一句话拒绝（「desktop control … arrive with D4」），四个订阅则返回空的退订函数而不是 reject。两条路径上的拒绝都是字符串，所以外壳与浏览器里的报错读起来一致。登录、新页面与实时流是接下来的 D2b 批次。

**Web 客户端能打开、能登录、能看。** `App.tsx` 变成了一道**门**：没有 token 时它只渲染登录页，别的什么都不渲染，所以外壳——以及外壳挂载时商店发出的每一个读——都等到有 token 才发生。token 在**装入之前**先用一次 `GET /v0/health` 证明，默认存放于 `sessionStorage`（勾选「记住此设备」时改存 `localStorage`，**绝不进 URL**），而一次失败会被分成四种不同的回答：令牌不对、服务没应答、其它状态、或成功。门后面是状态页——外壳的第三个视图，且只在 Web 端提供，因为桌面端没有可问的这个端点——它显示节点对自己的说法，并诚实地注明 `agents` 在执行者名册接入前就是 1。本次新增 24 个注册表键（两个语言），并加了一个探针：它检查这道门始终在外壳之外、且这两个屏幕用到的每个键在两个语言里都存在。

### 新增

- **Web 客户端能实时刷新了**（v0.9 D2b-3）：浏览器用 `fetch` + `ReadableStream` 读事件流（`EventSource` 设不了 `Authorization` 头），并用纯模块 `lib/sse.ts` 解帧。一条流服务所有订阅者；断开后按倍延迟重连，并以 `Last-Event-ID` 从最后看到的 `id:` 续传；`event` envelope 到达各自订阅者，`gap` 到达一个 `onGap` 回调，由它触发对「可重读内容」的全量重读。`onHostEvent` 契约不变，因此商店的 10 处订阅无需改动。

- **Zig 可编译（v0.9 F3a）**：`compile` 按源扩展名分派——`.c` / `.h` / `.S` / `.s` 走 GCC，`.zig` 走 `zig build-exe -target riscv64-freestanding`。Zig 不注入任何东西：源自己写 `_start`（`-bios none` 的客机跳到载入地址，所以启动代码必须排最前），生成的 `link.ld` 原样复用。`ZigConfig` 负责探测（`RISCDOM_ZIG` → 已知路径 → `PATH`），`settings.json` 新增 `zig_path`，由 `AppState::set_zig_path` / `clear_zig_path` 固定与清除。Zig 归档**不**在本批下载：其 macOS/Linux 构建是 `.tar.xz`，现有下载器无法解包（独立批次，与 Rust 共用）。
- **`cli`，新的 workspace crate，含 `riscdom` 二进制**：八个只读子命令（`health`、`status`、`agents`、`runs list` / `runs get <id>`、`audit status`、`audit events`、`snapshots list`），每条都是对控制平面的 HTTP 调用。
- **两种模式、一条代码路径**：`--remote host:port` 连已在运行的 `riscdom-server`；不加则在**本进程内**把控制平面起在 `127.0.0.1:0` 并对其说 HTTP。CLI 从不直接调 `AppState`。
- **`--json`** 原样透传控制平面的应答（失败时把错误体打到 stderr）；人类模式打印表格与 `key value` 行。
- **退出码**：`0` 成功、`1` 本地失败、`2` 用法或 `400`、`3` 被拒或 `5xx`、`4` `401`/`403`。见 `cli/README.md`。
- **token 处理**：本地模式经 `riscdom-server` 同一套代码读取（首次运行时生成）`<data-dir>/token`；远程模式优先 `--token-file`，其次 `RISCDOM_TOKEN`，最后才是 `--token`——且会警告，因为它会落入 shell history。token 从不被打印或记录。

**`server` 已 clippy-clean，且门禁会 lint 它。** 该 crate 此前从未被 lint 过——把 `-p cli` 加进 clippy 步骤才暴露了它。

### 修复

- **`server/tests/logging.rs` 现在把 server 的 stdout 读到 EOF**（v0.9 logging 修根因批）：`read_banner` 以前一拿到 banner 就丢掉了子进程的 stdout，而 server 随后那三行 `println!` 就落到已关闭的管道上——在 Unix 上那是 SIGPIPE，会在进程写出测试要等的那行之前把它杀掉。现在读取器会被交还，并用 stderr 已在用的同一个 `drain_reader` 读到 EOF；失败信息还会带上子进程退出状态与 stdout 行数。
- **`server/tests/logging.rs` 失败时会说子进程是怎么离开的**（v0.9 logging 诊断批次）：在读取器自身状态旁打印 `child: exited with code N` / `killed by signal N` / `still running`，并用一个单测把报告本身钉住。**只改诊断**——未碰生产代码、未改超时、未动 profile。
- **`server/tests/logging.rs` 的 stderr 读取循环不再因第一行读不出来就停下**（v0.9 logging 批次）。原循环是 `let Ok(line) = line else { break };`——一行读不出来就结束线程，并把其后所有行一并丢掉，包括 `the_connection_line_appears_at_info` 等的 `connection from … ended`。这正是该测试两次 CI 失败的形态（两次都是 5.09 s、报错逐字相同，而本地绿）。现在坏行**只被计数、读取继续**：`InvalidData`（非 UTF-8）与其它 I/O 错误计入坏行数，`Interrupted` 重试，EOF 才结束。等待失败时的输出也扩展为**读取器自身状态**——已捕获行数、坏行数、仍在读还是已停止——于是下次失败能区分「那行根本没来」与「读取器早就停了」。5 秒超时与轮询未动，生产代码一行未改。
- **`server` 的 6 处 `clippy::result_large_err`**：`http.rs:379` 与 `routes.rs:489/495/503/513/522` 返回 `Result<_, Response<RespBody>>`，而 hyper 的 `Response` 有 128+ 字节。错误类型改为 `Box<Response<RespBody>>`，所有调用方返回 `*response`——同一条路径上同一个 Response 值。随之一并修的还有 `routes.rs` 测试里 3 处 `bool_assert_comparison` 与 `tests/smoke.rs` 里 1 处 `filter_next`。行为未变。
- **`scripts/gate.sh`** 选 `-p cli -p server -p host-core -p host-tauri`（仍带 `--no-deps`），控制平面与我们自己的其它 crate 一样被 lint。
- **工具探针现在会重试「内核因文件忙而拒绝的 `exec`」**（v0.9）：`state.rs` 的四个「这个产物能不能跑」探针（`zig_runs`、`rustc_release`、`rust_runs`、`toolchain_runs`）都经 `exec_with_busy_retry` 跑命令，而该助手**只**重试 `ErrorKind::ExecutableFileBusy`——最多 5 次、每次相隔 10 毫秒。`ETXTBSY` 的含义是：内核不会 `exec` 一个正被**某个**进程以写方式打开的文件；在 Unix 上这包括「已 fork 但尚未 exec」的进程（`CLOEXEC` 只在 exec 那一刻关闭继承来的描述符），因此兄弟线程的一次 spawn 可能在本进程已关掉自己的写句柄之后**再持有几百微秒**。其它任何失败仍然立即返回，而超出预算的 busy 拒绝会被报出，不会吞掉。
- **所有源文件已清空 Windows 代码页事故的残留，而且 gate 现在会查它**（v0.9 编码清账）：五个文件、**24 处**——18 处 mojibake 残码（`U+9225`）、3 处 `U+6402`（`§`）、3 个 BOM——每一处都坐在注释里，这正是产生它们的两个批次（v0.7 与本次线的 D2b-1）里没有任何检查看到的原因。`scripts/scan-encoding.py` 新增 BOM 类、`§` 残码、三种扩展名，以及一个**只对不可能误报的类**失败的 `--check` 模式；gate 会跑它（没有 Python 时会大声说明）。行为未变。

### 变更

- **节点页有三个子 tab，而且浏览器能读本节点的执行者名册与沙箱**（v0.9 D2b-4b）：`StatusPanel` 变成容器，下面是 `panels/node/{NodeStatus,NodeExecutors,NodeSandboxes}.tsx`，用的是设置页自己那一行 tab，而 `AppShell` 没有新增 view。四个读接口以**共有名字**加入 API——`listExecutors`、`listSandboxes`、`currentSandbox`、`sandboxCandidates`——它们每一个都已经是 Tauri 命令，所以两种传输都携带它们，适配层的 Web 独有名单也没动。
- **浏览器端是只读看板**（v0.9 D2b-4a）：设置页各 tab、聊天输入框与串口导出按钮的每一个控制，都包在新的 `DesktopOnly` 组件里（六个文件、共 12 处包裹），因此桌面端渲染与以前完全一致，而 Web 端看到的是同一批界面、少了那些属于桌面的控件。模型表单在那里根本不提供，且每个界面都会说明原因。**有两个控制刻意实现了 HTTP**：主题与语言是**显示偏好**而不是节点配置，而它们所替代的行为——本地已生效、宿主调用失败并报错——本就是坏的（决策 §62）。`probe-ui-web-readonly.mjs` 逐屏数包裹点，所以漏包一个会让 gate 变红，而不是上线一个点了就报错的按钮。
- **`scripts/gate.sh`** 用 `--no-deps` lint `-p cli -p host-core -p host-tauri`：新 crate 被覆盖，同时不把（从未被 lint 过的）`server` crate 自身的问题拖进门禁。

### 变更

- **清理失效的 `host` 引用**：根 `README`、`CONTRIBUTING`、`SECURITY`、`PROJECT_CONSTITUTION`、`THIRD_PARTY_NOTICES`、`docs/` 下 11 对双语文件、`ui/README`、`ci.yml` 注释，以及四处源码文档注释——`-p host` → `-p host-core`、`host/tests` → `host-core/tests`、`host/src/…` → `host-core/src/…`（属于 Tauri 层的文件则 → `host-tauri/src/commands.rs`）、`host/README.md` → `host-tauri/README.md`、`host::` → `host_core::` / `host_tauri::`。CHANGELOG、RELEASE_NOTES、决策账本、handoff §1 与 architecture-evolution 快照保留其历史表述。
- **`ui/scripts/probe-ui-*.mjs`** 与门禁脚本已在第 4 波改好；工作区现已不含任何仍生效的失效引用（`host/src`、`host/tests`、`-p host`、`host::`）。

### 变更

- **`host` 更名为 `host-tauri`**（目录、`[package] name`、workspace member），桌面壳依赖它：`ui/src-tauri/src/lib.rs` 里 57 处 `host::` 全部改为 `host_tauri::`，经门面解析，因此 `ui/src-tauri` 无需直接依赖 `host-core`。`cargo tree`：`-p host-core` 0 行 Tauri、`-p host-tauri` 15 行、`-p worker` / `-p server` 0 行。
- **`host-tauri/Cargo.toml` 里的 `tokio` 死依赖已删除**：本 crate 与其测试从未用到它（`cargo tree` 仍会经 Tauri 显示 tokio）。
- **`host-tauri/README.md`** 现在说明两 crate 边界，并把内核能力指向 `host-core`；**`worker/README.md`** 为新增（此前没有），覆盖 CLI、stdio 协议、监工半边与示例。

### 变更

- **`worker` + `server` 改依赖 `host-core`（而非 `host`）**（A1 第 3 波）：30 处 `host::` 路径改写为 `host_core::`（worker 4 文件 7 处、server 7 文件 23 处），依赖行也改到可移植半边。`cargo tree -p worker` 与 `-p server` 现在不含任何 Tauri crate；此前各列出 15 行 `tauri`。逻辑未变。
- **`server/README.md`** 的 Layer 3 边界不再带「tauri 仍被链接」的旧注；`worker` 与 `server` 的 `Cargo.toml` 注释同此。

### 变更

- **`host/tests` → `host-core/tests`**（39 文件，`git mv`，保留历史），133 处 `host::` 路径改写为 `host_core::`。`host` 现在没有测试，`[dev-dependencies]` 随之删除：测试用的是 `host-core` 自己的依赖。
- **`scripts/check-mirrored-constants.mjs` 同时扫描 `host-core/src` 与 `host/src`**——17 个文件，而第 1 波搬迁后守卫内只剩 3 个。`--dir` 可重复给出。
- **`scripts/gate.sh` 也 lint `host-core`**：`cargo clippy -p host-core -p host --all-targets -- -D warnings`。第 1 波拆分曾把宿主的绝大部分放到所有 clippy 步骤之外。

### 新增

- **`host-core`，新的 workspace crate**：审计接线、`AppState`、快照、会话、工具链与 QEMU 两条下载路径、预检、`run_diff`、`paths`、`settings`、`keyring`、`error`、`dispatch`、`executor`，以及事件 envelope 与 `EventSink` trait。它只依赖 `agent` / `sandbox` / `audit`，且 `cargo tree -p host-core` 里没有任何 Tauri crate。

### 变更

- **`host` 变成架在 `host-core` 之上的门面**（A1 第 1 波；后续还有三波）。它仍提供 `host::commands`、`host::events::TauriEventSink` 与 Tauri 依赖，并再导出可移植面（`pub use host_core::*`）——因此 `worker`、`server`、`ui/src-tauri` 与 39 个 `host/tests` 文件原样编译，这正是本波能全程绿、没有「暂时红」中间态的原因。`host-core/README.md` 记录该拆分与「无 Tauri」约束。

### 新增

- **权限强制。** 每条被服务的路由恰好声明一个 capability，且它是路由表（`server/src/routes.rs`）的类型化列——写不出一条不声明它的路由，也就没有跳过检查的路径。处理器运行前，请求路径会问 `Actor` 是否 `allows` 该 capability，不持有即 `403 forbidden`、`cause` 为 `"capability"`（`server/src/http.rs`）。默认拒绝；词汇表就是 API 文档 §5 表格里的 28 个名字。
- **`Actor::capabilities`**：钩子返回的 actor 现在携带它能用的集合，因此钩子可以收窄调用者而无需新增管道。v0.9 只有两种形状——token 持有者（`operator`）与 `--no-auth` 的钩子——且都持有全部 28 项，故 `403` 只来自返回更窄 actor 的钩子。按能力细分的 token 属 v1.0。

### 变更

- **身份取自来源，文档也这么写。** `Server::sink()` 不再接受 `agent_id`：它用 `AppState::agent_id()`，因此同进程内的两种传输（「两个 sink，同一个事件」）不可能给同一事件打上不同身份。新增一条端到端测试：把同一事件经两个 sink 发出，在线上断言 `agent_id` 一致。API 文档 §3 现为「认证与权限」，明写钩子负责认证、服务端负责授权；客户端指南新增 capability 一节与安全部署示例（绑窄、token 文件只给属主、由代理终止 TLS、且绝不拿 `--no-auth` 配公网绑定）。

### 新增

- **27 个控制端点**（`POST`），另加 `POST /v0/runs/abandon-stale`（预留缺口 G4）与仍预留的 `POST /v0/vm/start`（501）。请求体为 JSON 对象，按文档定义的字面量集合校验，问题出在请求侧时回 `400`/`409`/`404`，宿主自身的错误也映射进同一模型。
- **`TokenAuth` 成为默认**：首次启动把 32 字节随机值（`getrandom`）写入 `<data-dir>/token`，Unix 为 `600`、Windows 为仅属主 ACL（用 `icacls` 校验；无法限制则服务端拒绝启动），以常量时间比对（`subtle`）。token 从不被打印或记入日志。`--no-auth` 取消该要求并打印警告；`--auth` 是默认。
- **`Last-Event-ID` 补发与 `gap` 帧**：帧带服务端全局序号，hub 在内存里保留最近 1024 帧，游标比这更旧时先回一个 `gap` 帧、其中给出仍持有的最旧 id。

### 变更

- **帧的 `id` 按服务端生成，而非按连接**（`<ts>-<seq>`）：游标在重连后必须指同一件事。文档中对 `seq` 的描述已同步更正。

**审计存储的打开路径不再是并发隐患。** 两个进程同时打开同一个全新的 `audit.db`，过去必有一个失败：`PRAGMA journal_mode = WAL` 需要独占访问，且会**绕过** busy_timeout 直接回 `SQLITE_BUSY`（等待可能死锁时 SQLite 不调用 busy handler），因此覆盖写路径的 5 秒超时从未覆盖这次切换。现在「连接 → 配置 → 建 schema → 迁移列」整条打开序列在遇到数据库锁时会重试，预算与退避形状沿用 v0.8 起写路径的那一套。序列本身的动作未变，链的一切未变。

### 修复

- **`AuditStore::open` 遇到数据库锁会重试**（`OPEN_MAX_ATTEMPTS` / `OPEN_BACKOFF_BASE`，定义为写路径常量的别名，两者不会漂移）：每次重试用新连接，只对 `SQLITE_BUSY` / `SQLITE_LOCKED` 重试，预算耗尽后仍**如实报错**——绝不静默降级。由一条「8 线程抢开同一新文件 × 25 轮」的测试、一条「同样方式打开已是 WAL 的文件」的测试、以及一条「超过预算的锁是错误」的测试钉住。

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

**CLI 能驱动控制平面了，而且在销毁前会先问。** `riscdom` 现在也覆盖了 API 的控制半边——`run`、`vm stop`、`vm start`（仍是预留的 `501`）、快照与会话的写操作、`runs abandon-stale`——随之而来两件事：五条销毁类命令要求的确认（终端上弹提示，脚本里用 `--yes`，没人可问时直接拒绝），以及 `--follow`：在运行期间打印 `/v0/events` 事件流。

### 新增

- **`riscdom` 新增十二个控制类子命令**：`run <task>`、`vm stop`、`vm start`、
  `snapshots save|resume|delete <name>`、`sessions create|open|rename|delete|clear-all` 与
  `runs abandon-stale`。每一个都是 HTTP `POST`，走的是只读命令同一份客户端；人类模式下，
  运行的输出是 `kind`/`iterations` 加答案，保存是写入字节数，删快照是是否真的删掉了。
- **确认机制与 `--yes`（别名 `-y`）**：五条销毁状态的命令（`vm stop`、`snapshots resume`、
  `snapshots delete`、`sessions delete`、`sessions clear-all`）动手前先问。`--yes` 提前
  回答；终端上弹提示，除 `y`/`yes` 外均算拒绝；stdin 不是终端时直接拒绝并退出 `2`，
  因此脚本无法因沉默而销毁状态。
- **`--follow`（别名 `-f`），仅 `run`**：CLI 在发起运行**之前**先订阅 `GET /v0/events`，
  因此一个事件也不会漏；每个帧打一行（事件名加一小段 payload；带 `--json` 时是原样的
  envelope），最后再打结果。用在其他命令上是用法错误。
- **`cli/src/sse.rs`**：帧读取器——`id:` / `data:` 行、空行分帧、注释心跳跳过、
  多行 `data:` 合并为一个 payload。

### 变更

- **`cli/src/lib.rs` 负责分发，`cli/src/client.rs` 负责会话**：每次调用一个 `Session`
  （远程，或内嵌在自身进程里的控制平面，并把宿主自己的凭证回递给它），三个超时，
  因为读立即应答、控制可能跑半小时、事件流则绝不能超时。
- **`cli/tests/control.rs`** 用真实二进制对着真实控制平面跑，并清空模型环境变量，
  因此测试不联网、不碰 QEMU。

**CLI 收尾：导出、管理配置，以及看着工作跑完的 `--wait`。** `riscdom` 又落下最后十七个端点——三个导出与十四个管理配置命令——至此 API 文档里的每一个控制都是 shell 命令。

### 新增

- **导出类子命令**：`export audit-jsonl [--out <path>]`、`export run-audit <run_id> [--out <path>]`、
  `export serial-log [--out <path>]`。`--out` 是**服务端的**路径——相对于 workspace 根解析，
  逃出 workspace 则 `403` 拒绝——CLI 从头到尾拿不到文件。默认值为 `audit.jsonl`、
  `run-<run_id>.jsonl`、`serial.log`。人类模式会说明数的是什么：
  `exported 3 events to audit.jsonl` / `wrote 4096 bytes to serial.log`（两条审计导出给的是
  **事件数**，序列日志导出是**字节数**；控制平面对该字段一律叫 `bytes_written`）。
- **十四个管理配置子命令**，已分组：`llm set|clear|load-key <provider_id>`、
  `qemu path <file>|clear`、`toolchain download|cancel|path <file>|clear`、
  `preflight run|ack`、`audit alert set <on|off>`、`theme set <light|dark|system>`、
  `language set <system|en|zh>`。`llm set` 接收端点的五个字段：`--api-key`（或
  `--api-key-file`）、`--base-url`、`--model`、`--provider-id`、`--remember`。
- **`--api-key-file`** 从文件读 key——不落入 shell history 与 `ps` 的形状——而
  **`--remember`** 才是把它固化到操作系统凭据存储的那个开关。
- **`--wait`**，用于两条异步控制：在发请求**之前**先订阅 `/v0/events`，打印该工作的帧
  （`toolchain:download` / `preflight:progress`），最后一行是 `download ok` / `preflight failed`；
  退出码是**工作本身**的判定，下载失败或预检失败即 `3`。
- **确认机制覆盖三条 clear**：`llm clear`、`qemu clear`、`toolchain clear` 动手前先问，
  因为它们拿走的东西无法从宿主里读回来。

### 变更

- **`cli/src/lib.rs` 在 `--follow` 之外多了第二条流式路径**（`wait`），两者共用同一个
  `subscribe` 助手：开流并吃掉首帧 `hello`；预检的结束判定是第一个 `failed`（fail-fast）
  或最后一步的 `ok`，取自 `host_core::preflight::STEPS` 而不是字面量。
- **`cli/src/client.rs`** 负责读 `--api-key-file`，并按 `--token` 的方式对 `--api-key` 告警；
  **`cli/src/render.rs`** 新增三个渲染分支（导出的计数、`202` 确认、预检步骤表），
  而十四条 `204` 应答沿用既有的空 body 路径继续打 `ok`。

**接口说实话，库的日志变成可选项。** CLI 几批留下的四件小事，合并修完：两个导出字段改名以说明数的是什么、workspace 路径拒绝从 `403` 改成 `400`、服务端的运行日志放到一个默认关闭的开关之后——这正是把内嵌服务端挡在 CLI 的 stderr 之外的关键。

### 变更

- **`POST /v0/audit/export` 与 `POST /v0/runs/export` 答 `events_exported`**，不再是
  `bytes_written`：宿主的 `write_events_jsonl` 返回 `events.len()`，旧字段名描述的是该端点
  从未做过的事。`POST /v0/serial/export` 确实按字节写，保留 `bytes_written`。CLI 据此渲染
  `exported N events to <path>` 与 `wrote N bytes to <path>`。
- **workspace 策略拒绝的路径是 `400 bad_request`、`cause: "path"`**，不再是 `403 forbidden`：
  它属于调用方参数不可用，而 `403` 留给认证与授权（capability 检查与 `Authn` 钩子，钉它们的
  测试未动）。该规则覆盖两条审计导出、序列日志导出与 `/v0/workspace/file`。
- **库的运行日志默认关闭、需显式开启**：`riscdom-server` 上是
  `--log-level <off|error|info>`，库调用方用 `ServerConfig::with_log_level`。`error` 写失败
  （accept 失败、下载或预检以错误告终），`info` 再加每条异常结束的 `connection … ended`。
  此前无条件输出的四处现在只由该开关决定；二进制自己的启动横幅、用法文本与致命错误原样保留，
  因为内嵌场景根本不会跑那个 `main`。
- **`docs/handoff.md` §1** 去掉两处失效的 `../host/src/run_diff.rs` 链接（该文件自 A1 拆分起就住在 `host-core`）。

**两套装配就是一个形状：QEMU 与工具链同样接线，而它的拒绝是已定决策。** 侦察发现 `qemu_download` 是有代码没 caller；F1 把 caller 补齐——槽、状态、取消、采用、审计、事件族、Tauri 命令、端点、CLI——形状与工具链完全一致，而把「下载」本身按决定留在未接线状态（没有 pin 任何 QEMU 发布版；本项目引导安装而不代为下载）。

### 新增

- **QEMU 下载接线**（`host-core`）：`AppState::begin_qemu_download` / `qemu_download_status` /
  `cancel_qemu_download` / `download_qemu_now`，另加 `record_qemu_download_event`、
  `finish_qemu_download` 与 `qemu_dir()`。槽是独立的（`qemu_download`），所以工具链下载
  进行中不会阻塞 QEMU 下载。
- **审计事件** `host.qemu.download.start|done|failed|cancelled`，与工具链那四个一一对应。
- **`qemu:download`**，第十二个 SSE 事件，payload 形状与工具链一致（内部标签枚举，标签为
  `state`）：`started` / `progress` / `verifying` / `extracting` / `done` / `failed` / `cancelled`。
- **三个 Tauri 命令**（`start_qemu_download`、`cancel_qemu_download`、`qemu_download_status`），
  已在桌面外壳注册；本批不接 UI。
- **三个端点**：`GET /v0/qemu/download`（`qemu.read`）、`POST /v0/qemu/download` 与
  `POST /v0/qemu/download/cancel`（`qemu.configure`）——API 表现已为 27 查询 + 29 控制。
- **三个 CLI 子命令**：`qemu download [--wait]`、`qemu cancel`、`qemu status`。
- **`QemuDownloadStatus`** 与 **`paths::qemu_dir_in`**（`<data-dir>/qemu`，版本并列、不清理）。

### 变更

- **`QemuDownloadEvent` 的 payload 标签改为 `state`**，不再是 `kind`：两种下载用同一套词汇
  自报，客户端因此只用读一种形状。
- **CLI 的 `--wait` 只剩一个终止判定**（`download_terminal`），两个下载族共用，而不是每资源一份。
- **`spec_for_current_platform` 就是平台分支，而它在每个平台都拒绝**：没有 pin 任何 QEMU 发布版
  （`docs/qemu-distribution.md` §5），因此 `POST /v0/qemu/download` 答 `503 unavailable`、
  `cause: "qemu"` 并附安装指引，也不占槽。拒绝之后的每一层都由回环夹具测试覆盖，所以将来 pin
  一个版本只是数据变更。

**沙箱先是名字，然后才是运行时；扫描从不写回。** F2a-1 落下定义层：人在 `settings.json` 里写的定义、控制平面将来服务的形状、对这台机器上实际已安装资源的扫描，以及三者的合并。切换、审批与 `Task.sandbox` 属后续批次（F2b / F2c / F2d），端点、Tauri 命令与 CLI 也一样（F2a-2）——本批停在「沙箱可以被命名、可以被读取」。

### 新增

- **`host-core/src/sandbox_def.rs`**：`SandboxDef`——`name` 加可选的 `display_name` /
  `memory_mb` / `qemu_exe` / `toolchain_path` / `kernel` / `notes`——以及 API 服务的两种形状：
  `SandboxView`（定义加上 `source`、`runnable`、`shadowed`）与 `CandidateView` /
  `CandidatesView`（一个已安装资源；两个互相独立的列表，永不笛卡尔积）。`DEFAULT_SANDBOX_NAME`
  为 `"default"`。
- **`sandbox_def::discover_in(data_dir)`**，只读扫描：`<data-dir>/toolchain` 与 `<data-dir>/qemu`
  下的每一个版本目录，经下载器自己的 `find_compiler` / `find_qemu` 找到，再加上本机已有的 QEMU。
  安装器的 `.download-tmp` / `.extract-tmp` 副产物被跳过，目录不存在就是空列表，找到的东西从不写回。
- **`LocalSettings::sandboxes` 与 `LocalSettings::default_sandbox`**，均 `#[serde(default)]`：
  纯附加，因此 `SETTINGS_VERSION` 仍为 1，写于这两个字段存在之前的 `settings.json` 照样读得进。
- **五个 `AppState` 方法**：`sandboxes()`（合并后的注册表：手写 → 扫描 → 内置 `default`）、
  `sandbox(name)`、`current_sandbox()`、`sandbox_candidates()` 与 `sandbox_default_name()`。
- **`Capability::SandboxRead`**，第 29 个名字（`sandbox.read`）：纯附加，因此持有其余 28 项的 actor
  自动持有它，过去成功的请求不会被拒。

### 变更

- **`host-core::find_compiler` 与 `host-core::find_qemu` 改为 `pub(crate)`**，沙箱扫描因此复用下载器对
  「已安装」的定义，而不另长第二份。
- **同名时手写定义胜出，被遮的扫描项仍然可见**，标为 `shadowed`——合并会自报，而不是躲在胜者背后。
- **`runnable` 每次读取时现算，从不存储**：QEMU 存在且 `--version` 能跑、工具链存在、内核存在或可编译。
  资源被卸载的定义仍是定义，只是跑不了。
- **文档中的 capability 计数为 29**（`docs/control-plane-api.md` 及其中文版、`server/README.md` 及其中文版、
  `docs/control-plane-client-guide.md` 及其中文版）。§5 表格本身仍只列出已有的 28 条路由：
  `sandbox.read` 的端点在 F2a-2 才落，所以计数故意走在表格前面一格。

**沙箱注册表可读——只读——且两个表面都能访问。** F2a-2 补上定义层需要的四条查询（合并后的注册表、current/default 对、原始扫描、按名取一个定义），
以及它们背后的四个 Tauri 命令与四个 CLI 子命令。切换仍属 F2b：这里什么都改不了。

### 新增

- **四条查询**（`docs/control-plane-api.md` §5.1）：`GET /v0/sandboxes`（`sandbox.read`）、
  `/v0/sandboxes/current`、`/v0/sandboxes/candidates` 与 `/v0/sandboxes/{name}`——继
  `/v0/runs/{run_id}` 之后的第二条路径参数路由，也是服务第 29 个 capability 的四条路由。
- **四个 Tauri 命令**（`host-tauri`）：`list_sandboxes`、`current_sandbox`、`sandbox_candidates`
  与 `get_sandbox`，已在桌面外壳注册。本批不把它与界面接线。
- **四个 CLI 子命令**：`sandboxes list` / `current` / `candidates` / `show <name>`，各是一次 HTTP `GET`，
  人类模式打表格，`--json` 原样透传。

### 变更

- **§5.1 为 31 个查询，词汇表的 29 个名字现在全部有路由。** F2a-1 先落了 `sandbox.read`、路由晚一批；
  计数与表格重新自洽。
- **`/v0/sandboxes/{name}` 永不把字面子路径当作名字。** `current` 与 `candidates` 是它们自己的路由，
  而 `requests` / `switch` / `assemble`——F2 线后面才落的三个——答 `404`，而不是解析成一个恰好叫这个名字的定义。
- **`render` 新增四种人类模式形状**（`sandboxes`、`sandbox_current`、`sandbox_candidates`、
  `sandbox_detail`），`args.rs` 新增四个命令的路径、方法与编码：名字像 run id 一样百分号编码，
  所以斜杠无法逸出路由匹配的那一段。

**无版本的资源得到的是一个名字，而不是一次拼接。** F2a-1 把每个扫描出的定义命名为 `format!("{kind}-{version}")`，而机器自带的 QEMU 没有版本号——于是合并后的注册表里列出一个叫 `qemu--` 的定义，F2a-2 又把它经 HTTP 与 CLI 服务出去。现在它是 `qemu-system-riscv64`。扫描**确实**知道版本的资源不受影响。

### 修复

- **`qemu--` 没了。** 扫描不知道该资源版本时，定义按资源本身命名（`SandboxDef::for_resource` 背后新增的 `resource_name`），而不是按一个缺失的版本：机器自带的 QEMU 在每个平台都叫 `qemu-system-riscv64`。名字取自 `sandbox::qemu_discover::exe_name()` 并裁掉各平台的 `.exe` 后缀，因此不留第二份名字，Windows 的文件名也不会变成一个定义名。

### 变更

- **`"-"` 哨兵现在只有一个定义**：`host-core::sandbox_def::NO_VERSION`，由扫描的 `CandidateView::system_qemu` 与命名规则共用，而不是两个必须彼此一致的字面量。它与定义层其余类型一同导出。
- **已安装资源仍为 `<kind>-<version>`**（`toolchain-15.2.0-1`、`qemu-11.1.0`）。名为 `-` 的版本目录会被当作哨兵、取无版本的名字；安装器从不写这种目录，这一点记在这里而不是被防御。扫描与合并的其余部分未变。

**节点可被切到另一个沙箱，且切换是先校验后停止。** F2b-1 落下核心：节点*正在跑*的沙箱变成运行时状态（存储的 `default_sandbox` 是重启的起点），定义在动到正在运行的 VM **之前**先校验，同一时刻只允许一次切换，运行中拒绝切换。端点、Tauri 命令、CLI 与 `sandbox:switch` 事件属 F2b-2。

### 新增

- **`AppState::switch_sandbox(name)`**：取定义 → 校验（QEMU、工具链、内核）→ 定内核 → 停当前 VM → 按定义启一台新的（三次尝试、每次新端口，与 `tool_start_vm` 同形）→ 采用它 → 最后才把名字记为当前。
- **`AppState::sandbox_check(def)`**：定义不能跑的原因，即四个新的 `HostError` 变体——`sandbox_not_found`、`sandbox_qemu_missing`、`sandbox_toolchain_missing`、`sandbox_kernel_missing`。`sandbox_runnable` 现在是它的 bool 面：一条规则、两种形状、行为不变。
- **`AppState::run_in_flight()`**：是否有调用正处在 `run_agent` 里，读自 `begin_run` 置、`finish_run` 清的运行记账。
- **切换槽**：`begin_sandbox_switch` / `cancel_sandbox_switch` / `finish_sandbox_switch`，与下载槽四个同形——同一时刻只允许一次，第二次以 `already in progress` 拒绝。

### 变更

- **`AppState::current_sandbox()` 现在是运行时状态**（v0.9 F2b 决策 1，账本 §34）：切换成功前为 `None`，且**从不**写进 `settings.json`；`sandbox_default_name()` 仍回答存储的 `default_sandbox`。切换改的是正在跑的，不是配置。
- **切换失败 = 已停，不是半切换**：新句柄被丢弃，`Drop` 杀掉它 spawn 的东西，当前值不变。校验失败的定义则完全不碰正在跑的 VM——这个顺序本身就是重点。
- **注册表的合并只剩一份实现**（`merged_sandbox_defs`），`sandboxes()` 与切换共用，因此切换用的定义就是列表里胜出的那个。

**切换有了对外表面：一个事件、一个端点、一个 capability 与一个 CLI 命令。** F2b-2 把 F2b-1 造的东西露出来。`sandbox:switch` 是第 13 个事件（`{from, to, ok, reason}`，每次尝试一条，成功失败都发）；`POST /v0/sandboxes/switch` 是沙箱表面上唯一的一写，为每种失败各答一个状态而不是一句话；`sandbox.switch` 是第 30 个 capability；`riscdom sandboxes switch <name>` 是它的 CLI，属破坏性家族。`events.rs` 的守卫恢复完整——列全 13 个名字，F1 的 `qemu:download` 不再遗漏。

### 新增

- **`sandbox:switch`**，第 13 个事件，每次出口带 `{from, to, ok, reason}`——`from` 是切换前正在用的定义（没有则为 `null`），`ok` 为 `false` 时 `reason` 携带原因码。
- **`POST /v0/sandboxes/switch`**（capability `sandbox.switch`）：`200` 带 `{from, to}`；`404` `cause: "name"`；`409` `cause: "run"` 或 `cause: "sandbox"`；`503` 的 `cause` 就是原因码；`500` `cause: "sandbox_start_failed"`。
- **`HostError::SandboxStart`**，让「每项校验都过而 VM 仍起不来」那种情形以原因码而不是散文作答（此时节点是已停，不是半切换）。
- **`AppState::sandbox_switch_in_progress()`**，决定 `409 cause: "sandbox"` 的探针——即 `toolchain_download_status().in_progress` 已有的形状。
- **`riscdom sandboxes switch <name>`**，先问（它停掉正在跑的 VM，且运行中拒绝）；`--yes` 提前回答，非终端 stdin 拒绝并退出 `2`。成功时打印 `switched from <old> to <new>`。
- **一个 Tauri 命令** `switch_sandbox`，已在桌面外壳注册。界面未与它接线（那是 D 线）。

### 变更

- **`switch_sandbox` 改为接收调用方自己的 `EventSink`**，事件因此与其它宿主事件同路：路由注入 `HttpEventSink`，Tauri 命令注入 `TauriEventSink`。切换本身的逻辑未变。
- **`events.rs` 的守卫列全 13 个事件**（`all_events_are_named`，原 `all_eleven_events_are_named`），并断言名字互不重复——它自 F1 起一直漏着 `qemu:download`。
- **文档里的 capability 计数为 30，§5.2 多了切换一行**：API 文档（双语）、`server/README`（双语）与客户端指南（双语）。

**端口租约说的就是它的意思，而测试现在断言的就是那个意思。** relay 的并发测试一直偶发失败——两次，且两次 `sandbox` crate 都未被改动：它把本次运行中**曾取到过**的每个端口都记下来，断言没有端口出现过两次，但租约只承诺**同时存活的**租约互不同号。已结束的持有者释放掉的号码可以回来，OS 也确实会再发出去。现在测试把每个线程的租约停放住、直到八个线程全部取完再比对，于是断言的正是代码真正保持的不变量；分配器一行未改，因为它本来没有错（检查与登记在同一锁作用域，`PortLease` 只有一个构造点）。

### 修复

- **`concurrent_leases_never_repeat_a_port` 断言对了东西**：八个线程各取四个端口，所有租约活到最后一个线程取完，然后比对号码——因此一个已结束线程释放掉（并被 OS 重新发出）的端口，不会再被误认为两个持有者同时持有。文案未变。

### 新增

- **`a_released_port_is_free_to_come_back`** 钉住契约的另一半：被销毁的租约注销自己的号码，并让该端口对任何人都可 bind。
- **`sandbox/README.md`**（双语）新增「端口租约的契约」：它承诺什么、故意不承诺什么、以及为什么。

### 变更

- **`PortLease` 的释放只删自己的号码一次**（`HashSet::remove`，原为 `Vec::retain`——那会把所有相同项一起删掉，属潜在隐患，现在连误写都不可能）。
- **`HELD_PORTS` 改为集合**：`LazyLock<Mutex<HashSet<u16>>>`，因为 `HashSet::new` 无法初始化 `static`（其 hasher 需要运行时种子）。`relay::reserve` 就是一次 `insert`；`leased_ports()` 签名不变，而它的顺序从来不是契约（无人读取顺序）。

**一个 actor 可以申请改动沙箱，由另一个 actor 裁决。** 沙箱表面原本只有一写（切换，需要 `sandbox.switch`）。现在多了一条**申请队列**：agent——或任何能跑 agent 的 actor——可以提出申请，而持有该请求 `action` 所隐含 capability 的 actor 来裁决。裁决不切换任何东西：切换仍是另一次带授权的调用，所以申请是一份意图账本，不是排好队的命令。

### 新增

- **申请队列**（v0.9 沙箱 F2c）。`POST /v0/sandboxes/requests`（`agent.run`）落一条申请——`{action: switch|define|assemble, sandbox?, reason?}`——并答 `201` 带 `req-<pid>-<seq>`（自己的命名空间，不是任务那个）。`GET /v0/sandboxes/requests?status=`（`sandbox.read`）列出它，新的在前。两条决策（`…/{id}/approve`、`…/{id}/reject`）无请求体，答 `200` 带记录，且是终局：再决一次是 `409`，id 未知是 `404`。
- **`sandbox.assemble`，第 31 个 capability**：把节点搬走、与给它一个要跑的新定义，是两种权力，所以一条决策需要它的请求所要求的那一个——`switch` 申请要 `sandbox.switch`，`define` / `assemble` 要 `sandbox.assemble`。
- **`SandboxRequester`，agent crate 的两个沙箱工具**：`request_sandbox`（落一条申请、答出 id）与 `sandbox_status`（现在跑什么、什么在等）。宿主用克隆的子句柄实现该 trait 并注入 loop——loop 不能持有 `Arc<AppState>`，因为它就住在里面。
- **四条 Tauri 命令**（`request_sandbox`、`list_sandbox_requests`、`approve_sandbox_request`、`reject_sandbox_request`），已注册但未与界面接线（D 线）。
- **三个 CLI 子命令**：`sandboxes requests [--status <s>]`、`sandboxes requests approve <id>`、`sandboxes requests reject <id>`。两条决策像 `sandboxes switch` 一样先确认，非终端 stdin 直接拒绝。
- **`sandbox:request`，第 14 个 SSE 事件**：`{id, status, requester, action}`，每次变化一条（`pending`，然后 `approved` / `rejected`）。

### 变更

- **路由声明的 capability 不总是全部检查。** 两条决策声明 `sandbox.read`——决策者先要看得见队列——由处理器再拿该请求自己的 `action` 所隐含的 capability 对 actor 做检查，这也是 `dispatch` 现在接收它的原因。API 文档从「每个 capability 都至少有一条路由」改成「都在某处被强制」：`sandbox.assemble` 在那个处理器里被强制，直到装配端点落地。
- **文档里的 capability 计数为 31**，§5.1 / §5.2 携带 32 / 33 个端点：API 文档（双语）、`server/README`（双语）与客户端指南（双语）。
- **`all_events_are_named` 重新列全十四个事件**，事件文档 §3 表格有十四行。

**一个项目以一个文件的形式行走，而 AI 的写入就在链上。** workspace 现在可以以 `tar.gz` 离开、
以一个归档回来——两个端点、两个打包器、那些守卫与 capability 的划分——而 `write_source`
会记下它写了什么。

### 新增

- **`POST /v0/workspace/export`**：workspace 作为 `tar.gz`，以**字节**作答（那个表面上除事件流
  之外的第一个非 JSON body）并带 `Content-Disposition`。空 workspace 导出合法空归档。宿主自己的
  `.riscdom/` 状态不打包。
- **`POST /v0/workspace/import`**：归档就是请求体——zip、tar.gz 或 tar，由 `Content-Type` 选、
  不认知时看字节——答 `{files, bytes}`。带**自己的 64 MiB 上限**（`413`），所以共用的
  64 KiB JSON 上限原地不动。workspace 里已有同名文件 → `409`、`cause: "exists"`，除非
  `?force=true`；逃出 workspace、以符号/硬链接到访、命名 `.riscdom/`、或不是可读归档的 entry
  → `400`、`cause: "archive"`。
- **`workspace.write`，第 32 个 capability**：导入替换项目，导出读它，两者不是同一种权限。
- **`agent.file.write` `{path, bytes}`**：每次 `write_source` 写文件一行审计，于是项目的来龙去脉
  是一行记录，而不是去重剖一个被截断的工具参数。它是审计事件，不是 SSE 事件——事件计数仍为 14。
- **两条 Tauri 命令**（`import_workspace`、`export_workspace`）与**两个 CLI 子命令**
  （`workspace import <archive> [--force]`、`workspace export [--out <file>]`；归档写到 `--out`
  或 stdout，计数走 stderr）。

### 变更

- **`zip` 与 `flate2` + `tar` 对每个平台都声明。** 它们本来就在 `Cargo.lock` 里——Windows 宿主
  只读 zip，unix 宿主只读 tar.gz——而项目归档是用户工具链产出什么就是什么。
- **文档里的 capability 计数为 32**，§5.2 携带 35 个端点：API 文档（双语）、`server/README`（双语）
  与客户端指南（双语）。

**一个任务声明它的沙箱，而节点不被切换。** 沙箱线的最后一块：一次运行可以说它用哪个定义，
节点自己的沙箱只在什么都没声明时作答，而声明从不搬动节点。

### 新增

- **`Task.sandbox`**（v0.9 沙箱 F2d）：`Option<String>` 带 `#[serde(default)]`——旧 supervisor
  的任务行仍可读——加 `Task::with_sandbox`。worker 本就读整条 `Task`，所以跨进程协议改动就是那一个字段。
- **运行上的 `sandbox`**：`POST /v0/agent/run` 接受可选的 `sandbox`；Tauri `run_agent` 命令接它作为参数；
  CLI 多出 `run <task> --sandbox <name>`。
- **`AppState::run_agent_for(emitter, input, sandbox)`**：唯一解析并拒绝一次运行沙箱的地方。
  `run_agent` 保留签名并委派。
- **`AppState::active_sandbox()`**：**在跑的** VM 来自哪个定义——在运行的 VM 出现时、由
  `switch_sandbox`、以及快照恢复时写入；由 `stop_current_vm` 清除。单靠 `current_sandbox` 答不了它：
  只有切换会写它，所以由 `start_vm` 启起的 VM 没有记录下来的来源。
- **`agent.set_memory_mb`**，使定义的 `memory_mb` 能抵达工具启的那台 VM。

### 变更

- **一次运行的拒绝阶梯移进了 `run_agent_for`，按此顺序**：未定义沙箱名 → `404` `cause: "name"`；
  不是正在跑的 VM 所来自的名字 → `409` `cause: "sandbox"`；就绪 → `503` `cause: "llm"`，现在来自
  类型化的 `HostError::NotConfigured`，而不是路由检查两次。于是坏的参数先于环境被回答，
  且 `POST /v0/agent/run` 带一个未定义沙箱时，即使没有配置模型也是 `404`。
- **一次运行的解析顺序**：声明的名字，否则节点在跑的，否则其配置默认，否则内置兕底
  （后者意味着「去发现宿主自己的 QEMU 与工具链」——上一批之前的行为）。

**节点可以被派任务，而它的执行者队伍是配置。** `Dispatcher` 自 v0.8 起从 Rust 可达，除此以外哪里都不可达；任务端点就是把它变成接口的那一步。执行者队伍来自 `settings.json` 里的 `executors`，**仅此一处**。

### 新增

- **`settings.json` 里的 `executors`**（`ExecutorSpecSettings`：一个 label、一个 program 与它的参数）：在设置文件读完之后一次性登记为 `StdioExecutorHandle`。**没有 `env`**——设置文件不是密钥库。登记不起任何子进程，所以不存在的 program 是第一个任务的失败，而不是启动错误。
- **`POST /v0/tasks`**（`agent.run`）：进 `Task` 的四个标量字段，出该执行者的 `TaskOutcome`。缺 `id` 由服务端补。同步，与 `/v0/agent/run` 相同：没有任务表，也没有 `GET /v0/tasks/{id}`。
- **`GET /v0/executors`**（`agent.run`）：任务可抵达的身份，按配置顺序——空列表是一个事实，不是错误。
- **`AppState::dispatch_task`**（以及 `dispatch_task_value` / `executors`），另有两条 Tauri 命令（`dispatch_task`、`list_executors`）已注册但**未**接到界面——D 线。
- **两个 CLI 子命令**：`executors list` 与 `tasks dispatch --target <agent_id> --input <text> [--sandbox <name>]`，新增 `--target` / `--input` 两个 flag。

### 变更

- **两种拒绝，彼此分开**：无人拥有的目标是 `404` `cause: "target"`（调用方的参数——没有队伍的节点对每个目标都这样拒绝），而断掉的派发是 `500` `cause: "task"`。一次只是*跑失败*的运行仍是 `200`；它的 `outcome` 会说 `failed`。
- **节点不是它自己的执行者之一**：目标写它就是 `404`，因为在这里跑是 `POST /v0/agent/run`。两个端点是兄弟，不是同义词。
- **没有新增 capability**：两条路由都声明 `agent.run`（E0 裁决三）。

**两份工具 schema 都是文档，且都被校对。** 这条接口的另一半是词汇：执行者的模型可以叫什么，AI 监工可以叫什么。

### 新增

- **`docs/tool-schema-executor.zh-CN.md`**：`tool_specs()` 声明的八个工具，写成 `tools_json()` 放进请求 `tools` 字段的那个确切 JSON 数组，并附一张给人看的表。
- **`docs/tool-schema-control-plane.zh-CN.md`**：每个端点写成一条 OpenAI 风格函数定义——32 查询 + 36 控制 + 3 本机 + 4 带路径参数 = 75 条工具——分节，每行带 capability 与参数，每组各有一个可直接用的 `tools[]` 数组。名字从路径推导（去掉 `/v0/`、折叠分隔符；双方法路径的 `POST` 侧加 `_post`；四条带 id 的路由用一个动词）。
- **`scripts/check-tool-schema.mjs`**，接进 `scripts/gate.sh`：译文必须携带逐字相同的带标记块；表里每个名字必须是文档自身的推导结果；每个名字既要有表行也要有定义。它带自证：植一个漂移，要求被拒绝。

### 变更

- **`agent/README.md` 的工具表变成索引，不再是第二份清单**：它指向 `docs/tool-schema-executor.zh-CN.md`，而后者由测试与 `tools_json()` 对校（`agent/tests/tool_schema_doc.rs`）。控制平面的那些表则由 `server/src/routes.rs` 自己的测试与服务端路由表对校。

**一个可以跑起来的监工。** 外部那一半的参考实现：一个在内核之外、靠 HTTP 驱动节点、自己不带模型的进程。

### 新增

- **`examples/python/dispatch.py`**：`GET /v0/executors` → 每条任务一次 `POST /v0/tasks` → `--follow` 时 `GET /v0/events`。仅标准库（`urllib.request`、`json`、`argparse`，加手写的 SSE 拆帧）。token 来自 `--token-file` 或 `$RISCDOM_TOKEN`，绝不来自参数。退出码沿用 CLI：`0` 全部成功、`1` 有没成功、`2` 用法错误、`3` 不可达或被拒。
- **`--self-test`**：在 `127.0.0.1:0` 起一个 stdlib `http.server` 假控制平面，于是脚本能离线自证（队伍列表、一个成功、一个失败、一个被拒目标、`--follow` 的订阅、错 token、畸形任务行）。
- **`examples/python/README.zh-CN.md`**：它演示什么、两份文档怎么分工、以及一张与 `worker/examples/dispatch.rs` 对照的表。
- **gate 一步**：`PATH` 上有 `python3`/`python` 时 `scripts/gate.sh` 跑这个自证，没有就打印 skip——gate 的第一处可选工具链。

### 变更

- **`docs/control-plane-client-guide.zh-CN.md` §8** 指向这个示例，作为 AI 监工的可跑形状。

**那条缝有了另一半。** `AgentHandle` 自 v0.8 起就说，远程实现正好实现它，而尚未有人写过。现在有了。

### 新增

- **`worker/examples/remote_executor.rs`**：`HttpExecutorHandle`，一个经 HTTP 把执行者放在另一个节点上的 `AgentHandle`。它的 `run` 把一个任务形状的 body POST 到 `POST /v0/tasks`，并返回远端执行者产出的 `TaskOutcome`——身份是**节点的**，绝不是句柄的 label。`404` 映射为 `NoSuchAgent`，其它失败都是 `Failed`；一条指向别的任务的应答是协议破裂。
- **`--self-test`**：`127.0.0.1:0` 上的替身节点，以及跨真实句柄路径的七条断言（请求体形状、解析、节点身份、未知目标、指错的应答、不可达节点、指向别人的任务）。`scripts/gate.sh` 会跑它。
- **`worker/README.zh-CN.md` 的一节**与**客户端指南 §9**：远程句柄是什么、它带的两个名字、以及为什么那次派发是 `POST /v0/tasks`（而不是 `/v0/agent/run`）。

### 变更

- **生产代码一行未动**：本工作区没有任何 crate 被改动。集成就是那条缝承诺的那一行——`LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn AgentHandle>])`。

**小债还清，地面干净。** 没有新东西：四件事收拾好。

### 变更

- **测试里不再有需要手改的计数。** `server/tests/smoke.rs` 的 `every_control_endpoint_answers` 把它的预期从路由表算出来（`server::routes::control_paths()`，一个对路径的只读访问子），不再断言一个字面量，于是没带用例就落地的控制端会带着**缺失的路径**失败——而测试刻意不管的七个端点是被**点名**的，不是被数出来的。`routes.rs` 的 `the_table_has_the_documented_endpoints` 现在从 API 文档自己的 §5 标题里读计数（两个语言都读），不再把它们当字面量带着。
- **忽略 `__pycache__/` 与 `*.pyc`**：Python 参考监工的字节缓存弄脏了工作树。
- **注释里的陈旧计数**——`server/src/lib.rs` 的「26 个查询 / 27 个控制端点」、`routes.rs` 的「两条带路径参数的路由」——现在是正确的，或者不在了：一个数端点的注释是一个会烂的注释。
- **`agent/README.zh-CN.md` 的工具表**变成与它的英文兄弟一样的东西：八个名字加上一个指向 schema 文档的指针，而不是描述的第二份。

**机械性的一批：行尾现在归仓库管。** 没有行为改动、没有正文改动——一份 `.gitattributes`、一次工作树归一、七处链接路径。

### 新增

- **`.gitattributes`**：`* text=auto eol=lf`，`*.sh` 明确写出，六个被跟踪的二进制（`*.png`、`*.ico`、`*.icns`）标 `binary`。**没有 `*.ps1` 例外**：`scripts/` 里五个 PowerShell 脚本今天就是 LF，**而且**每一批的 gate 与提交都是经它们跑的——声明 CRLF 等于把这五个文件拿去重写，而不是保住现状。

### 修正

- **七处相对链接**：从 `docs/` 指向根级文件（或反之）时漏了前缀——CHANGELOG 里那条 `multi-agent-foundation`、`handoff(.zh-CN).md` → `RELEASE_NOTES`、`qemu-distribution(.zh-CN).md` → `THIRD_PARTY_NOTICES`、`toolchain-setup(.zh-CN).md` → `ENVIRONMENT`。全仓 365 条相对链接重扫 → **0 失效**。

### 变更

- **工作树已全为 LF。** 38 个文件带着 CRLF 或混用行尾（27 个纯 CRLF、11 个混用——最重的是 `sandbox/src/relay.rs`，413 行里 401 行），因为 `core.autocrlf=true` 的 checkout 与编辑工具写法不一致，而 git 看不见这个差别。**索引里一直是 LF**，所以 `git add --renormalize .` 一无所获，而**本提交不含任何行尾改动**：它是一次工作树修理，并且已被证明与内容无关——356 个被跟踪文件逐个与其已提交 blob 对比，**逐字节相同**（0 处差异）。

**文档有了入口。** 一页点名全仓每一份文档、它给谁看、以及它是活跃、快照还是历史。

### 新增

- **`docs/README.zh-CN.md`**（及英文对偶）：文档导航。五节受众——从这里开始 · 内核开发者 · 发行集成者 · 管理员 · 终端用户 · 贡献者——另有一节写明刻意不在导航内的东西（`IDENTITY.md` / `SOUL.md` / `USER.md`、`LICENSE`、CLA 签署存储）。全仓每一份 Markdown 都出现，并带它做什么、它的**状态**（*活跃* / *快照* / *历史*）与适用版本。历史会被标为历史：`architecture-evolution.md` 是 v0.7 快照，较早的 `CHANGELOG` 段与已发布的 `RELEASE_NOTES` 都标为不重写。
- **两个没在本 crate README 里点名的示例现在点名了。** `agent/README` 与 `audit/README`（两个语言）各加了一节 `## 示例`，对应 `examples/audit_demo.rs` 与 `examples/chain_demo.rs`，形状与 `sandbox/README` 已有的那一节一致（`sandbox` 的 `run_hello`、`worker` 的 `dispatch` 与 `remote_executor` 早就写上了——这两个是最后两个）。
- **贡献者模板。** `.github/ISSUE_TEMPLATE/bug_report.yml` 与 `feature_request.yml`（GitHub issue 表单），以及 `.github/PULL_REQUEST_TEMPLATE.md` 及其 `.zh-CN.md` 译文。表单刻意用 `.yml`：`scripts/check-bilingual.sh` 扫每一个 `*.md`，所以 Markdown 模板会需要一个 `.zh-CN.md` 兄弟文件——而 GitHub 又会把它当成第二个模板列在选择器里。PR 模板是 Markdown（GitHub 只读一个，而双语是本仓规矩），所以它的首行是语言切换行，因此那行也会出现在新建 PR 的正文里。

### 修正

- **根 `README` 的目录树**描述的是 `host/` 与四个 crate；工作区现有八个（`cli`、`host-core`、`host-tauri`、`sandbox`、`audit`、`agent`、`worker`、`server`）加上 `docs/`、`examples/`、`scripts/`、`walkthroughs/`。它的测试清单还把 `host-core` 写成「Tauri 后端命令 + 串口增量」——自 v0.9 的 A1 拆分后，Tauri 那一半是 `host-tauri`——且只列了八个 crate 中的四个。
- **规范文档里的两处陈旧计数**：`control-plane-api.md` 说 §5.2 有 35 个控制端点（实为 36），客户端指南说查询面是 31 个端点（实为 32）——两个语言都改。
- **自 `00fca17` 起，CI 在 Linux 上连续红了四个提交。** `gate` job 的 `remote executor example` 步骤是 Linux 上第一个编译 `host-core` 的步骤，而 `host-core` 在 Linux 上的 `keyring` 后端会编译 `libdbus-sys`，它需要系统 `dbus-1` 库——而 runner 并不自带。`gate` job 现在会安装 `libdbus-1-dev`（Linux 的 `bundle` job 的依赖列表也加上了它）。Windows 开发机走 `keyring` 的 `windows-native` 后端，所以本地 gate 根本不可能发现这个问题。
- **`cli` 的测试二进制在 Linux 上又能编译了。** 在 Linux 上解开 `cli`（即上面的改动）当场就红：`cli/tests/control.rs` 的 `write_executor_settings` 只被一个 `#[cfg(windows)]` 派发测试调用，自己却没有 cfg 门，于是在 Linux 上是死代码——而 `-D warnings` 会把 `dead_code` 升级成编译失败。现在它带 `#[cfg(windows)]`，只有它和假执行者 helper 用到的 `std::path::Path` 导入也同样加上。**生产代码零改动。**

### 变更

- **根 `README` 的「更多」节**改为以导航开头，并列全九个 crate README，而不是六个。
- **[决策 §43](docs/decisions.zh-CN.md) 撤销了 §20 的 DCO 条款。** 账本自己的规矩是：被推翻的决策以**追加**一条新条目的方式记录、并在其中点名旧条目，所以 §43 记下这件事，§20 的状态行指向 §43。CLA 是权利授予（再许可、专利），正是它让一个 open-core 项目能以商业专有许可分发派生作品；DCO 只是来源声明，而 §20 的措辞——「DCO（CLA 已有）」——本身就带着矛盾。CI 里不接任何 `Signed-off-by` 校验。
- **gate 现在在非 Windows 平台上也 lint `cli`、`server` 与 `host-core`。** `scripts/gate.sh` 的那个分支原本把这四个（`cli` / `server` / `host-core` / `host-tauri`）合在一起跳过——因为 `host-tauri` 在那里需要 webkit2gtk / gtk / librsvg。现在不带 Tauri 的三个 crate 在**每个**平台都 lint，只有两个 Tauri crate（`host-tauri`、`ui/src-tauri`）仍是 Windows 专属。`ci.yml` 没改：`host-core` 的 Linux 系统依赖是 `libdbus-1-dev`（经 `keyring` 抵达），而 gate job 早就装了它。这件事拖到现在要怪 E4——它的 `worker` example 步骤是 Linux 上第一个编译 `host-core` 的 gate 步骤，而它连红了四个提交才有人去看。
- **每个 workspace crate 在**每个**平台都被 lint 与 check。** `scripts/gate.sh` 里的非 Windows 分支没了（OS 探测也一并删去）：`host-tauri` 与 `ui/src-tauri` 在 Linux 也被 lint 与 check，而 `worker`——此前两个平台的任何 clippy 列表都没有它——也并入了同一条命令。Linux 的 `gate` job 在原有的 `libdbus-1-dev` 之外，再装 Tauri 的系统库（webkit2gtk / gtk / librsvg / libsoup）。前端构建顺序**不需要**改：只要 `custom-protocol` 没开，`tauri::generate_context!()` 就走 dev 分支，而裸 `cargo check` / `cargo clippy` 正是这种情形。
- **没有 guest 的 gate 会跑**每个** crate 的单元测试。** 非 Windows 分支原本跑 `cargo test -p audit -p sandbox --lib`——638 个里的 10 个——现在跑 `cargo test --workspace --lib`：八个 crate 共 **204** 个单元测试。它们是纯逻辑（没有一个会起 guest），因此不需要 QEMU；这也是 `agent`、`host-core`、`cli`、`server`、`worker` 第一次在非 Windows 上获得覆盖。可移植的**集成**测试在那里仍不跑；把它们与需要 guest 的那些分开是 B-3b 的事。
- **`agent` 两个真编 C 的单元测试在没有工具链时会打印跳过。** 它们调 `compile_freestanding(...).expect("run gcc")`，所以在没有 RISC-V GCC 的 Linux job 上一旦开启 `cargo test --workspace --lib` 就立即失败。现在它们先探测 `CompilerConfig::discover()`，缺失时打印 `skip: <测试名> -- no RISC-V GCC found`；有工具链的机器仍然真编。两处 `cargo test` 也带上了 `--no-fail-fast`，一个测试二进制失败不再遮掉 workspace 其余部分。
- **gate 的测试分叉改为按**能力**，而不是按**平台**。** 需要 QEMU guest 或 RISC-V GCC 的测试都带 `#[ignore = "<它需要什么>; run with --include-ignored"]`，共 54 个（37 个需要 guest + 编译器，8 个需要可发现的 QEMU，9 个需要可发现的编译器）。没有这些工具的机器跑 `cargo test --workspace --no-fail-fast`——**584** 个测试，而本系列之前非 Windows 分支只跑 10 个；有工具的机器跑 `--include-ignored`（**643**），再加三个 `--skip`（对应需要 API key 或会真写 OS 钥匙串的测试）。重用这些旗标前值得知道：`--skip` 匹配的是**测试函数名**，所以文件名永远匹配不上，而短子串可能连带带走可移植测试。
- **两个测试 fixture 不再只在 Windows 上成立。** `host-core/tests/common/mod.rs` 的 tar 构造函数用 `../` 条目调了 `tar::append_data`，而那个 crate 在**写入**时就拒绝它——于是 Linux 上 fixture 先崩，测试根本没走到它要考的安装器；现在名称直接写进 header，转义条目能抵达被测代码。另外 `qemu_archive()` 的模拟器内容是 `#!` 脚本：配 0755 权限在 Unix 上**会真的运行**，于是「不能运行的模拟器」被采纳、测试在那里失败（Windows 上文本 `.exe` 从不运行，所以一直绿）。现在它是普通的非程序字节。

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
[docs/multi-agent-foundation.zh-CN.md](docs/multi-agent-foundation.zh-CN.md)。

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
