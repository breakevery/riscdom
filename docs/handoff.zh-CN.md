[English](handoff.md) | 中文

# 交接 —— 把 RiscDom 带进下一个对话

**本文件是跨对话交接文档。** 第 1 节是易变快照，正式版发布时更新；第 2–12 节是稳定约束：它们在各批次
之间没有变过，也是新对话必须守住的东西。

仓库 `D:\codeagent\breakevery\riscdom`，远端 `https://github.com/breakevery/riscdom.git`，分支
`main`。每个批次的收尾流程一致：gate 全绿 → `scripts\commit.ps1 "<msg>"`（它自己会跑 gate）→ push ——
而这些面向远端的动作，只在当轮请求明确授权时才做（见 §2）。

## 1. 快照 —— `v0.8.0` 是最新的发行版（下次正式发布时更新本节）

- **Web 客户端能实时刷新了**（v0.9 D2b-3）：浏览器用 `fetch` + `ReadableStream` 读 `GET /v0/events`——`EventSource` 带不了 `Authorization` 头——并用纯模块 `ui/src/lib/sse.ts` 解帧（`parseSseLine` + `SseReader`：保住未到齐的半帧、合并一帧内的多行 `data:`）。**一条流服务所有订阅者**，由第一个订阅打开、最后一个订阅关掉；断开后按倍延迟重连（1 秒 → 15 秒），并把最后看到的 `id:` 回填为 `Last-Event-ID`——服务端的重放缓冲正是为此而存在。envelope 按 `kind` 分流：`event` → 该 `event` 名字的订阅者，`gap` → 单独的 **`onGap`** 回调（`hello` 不分发：它描述的是流本身，而其 `id` 已经成为游标）。`onHostEvent` 保持名字、签名与「返回退订函数」的契约，因此商店的**10 处订阅一行未改**；`gap` 现在驱动一个新的 `refreshAll`（status/audit/runs/sessions/snapshots/vm/toolchain/preflight——丢帧会弄脏的那些；**流式聊天文本与串口字节不可恢复**，文档里明写）。**新增 1 个探针**（`probe-ui-sse.mjs`，ui 探针现为 11 个），21 项检查——15 项线上格式 + 6 项传输层（`fetch` 用替身 `ReadableStream` 替换）。`scan-encoding.py` 的 `print` 不再依赖控制台代码页。决策 §61。

- **源文件已清空 Windows 代码页事故的残留，而且 gate 现在会查它**（v0.9 编码清账）。Windows PowerShell 5.1 会把无 BOM 的 UTF-8 文件按 ANSI 代码页读入、再按 UTF-8 写回：`E2 80 xx`（破折号、省略号）变成 `U+9225` 加一个丢掉的字节，`C2 A7`（`§`）变成 `U+6402`，而 `Set-Content -Encoding utf8` 会添一个 BOM——这些**任何检查都看不到**，因为每一个受损字符都坐在注释里。它发生过**两次**：v0.7 的 `sandbox/`（4 处）与 v0.9 D2b-1 的 `ui/src/api/`（18 处加 3 个 BOM）。五个文件、**24 处**已全部修好，**零行为变更**；`scripts/scan-encoding.py` 新增 BOM 类与 `§` 残码、扫描范围加 `.py`/`.sh`/`.ps1`、并跳过自己的模式表；而 **`--check` 已进 gate**——只对**不可能误报**的两类（mojibake、BOM）失败，模糊类继续报告（决策 §59、§60）。其中 4 处是**不可见字符**（3 个 BOM、1 个私用区码点），需要字节级修复：`edit` 会在匹配前把这类字符规整掉，所以那条例外连带边界一起写进了账本（两侧 hex dump、`git diff` 作证）。

- **工具探针会重试「内核以『text file busy』拒绝的 `exec`」**（v0.9 ETXTBSY 修复——本账本记录的第**四**个「本地绿、CI 红」机制）。`state.rs` 的四个可运行性探针（`zig_runs`、`rustc_release`、`rust_runs`、`toolchain_runs`）现在都走 `exec_with_busy_retry`，它**只**重试 `ErrorKind::ExecutableFileBusy`——最多 5 次、每次相隔 10 毫秒——其它错误一律立即返回。这个拒绝与工具本身无关：`ETXTBSY` 的含义是「**某个**进程正以写方式打开这个文件」，而在 Unix 上这包括「已 **fork** 但尚未 **exec**」的进程（`CLOEXEC` 只在 exec 那一刻关闭继承来的描述符），因此兄弟线程的一次 spawn 可能在本进程已丢开自己的写句柄之后再持有几百微秒——这正是 D2b-2 那次在 `a_mock_zig_download_installs_and_adopts_the_compiler` 上变红、而同一份代码上一次却是绿的原因。匹配用 `kind()`，**绝不**用 `raw_os_error() == 26`：26 在 Unix 上是 `ETXTBSY`，在 Windows 上是毫不相干的错误码（决策 §58）。**新增 3 个测试**——一个跨平台（不存在的文件不重试）、两个仅 unix（errno 26 确实映射到该 kind；一次真正繁忙的 `exec` 会被重试到成功）。Windows：681 → **682**；在 unix 上另两个不会被 `cfg` 掉。

- **Web 客户端能打开、能登录、能看**（v0.9 D2b-2）。`App.tsx` 现在是一道**门，不是路由**：没有 token 时它只渲染 `<Login>`，而外壳（`useAppStore()` 所在处）只在拿到 token 后才挂载——这也正是「一屏未认证请求在任何人输入之前就飞出去」被挡掉的原因。token 在**装入之前**先用一次 `GET /v0/health` 验证，存放于 `sessionStorage`（勾选「记住此设备」时改存 `localStorage`；**绝不进 URL**），而 `verifyToken` 有**四种**答案——`ok` / `unauthorized` / `unreachable` / `other`——因为「令牌不对」与「服务没应答」对人是两句不同的待办。状态页（`panels/StatusPanel.tsx`）是外壳的**第三个视图**（`main | settings | status`），且**只在 Web 端**出现（`isTauriRuntime()` 判），因为桌面端没有可问的 `/v0/status`；它的措辞住在纯函数 `lib/statusView.ts` 里，`agents` 那个数旁边带着诚实的说明：尚未接入执行者名册的节点报的就是 1。**新增 24 个 i18n 键**（8 登录 + 16 状态；193 → **217**，两语言完全一致）与 **1 个探针**（`probe-ui-login.mjs`——ui 探针现为 10 个）。Web 独有的 API 名字（`clearToken` / `verifyToken` / `getHealth` / `getStatus`）在 `api/index.ts` 的 `Omit` 名单里声明，并由 `probe-ui-api.mjs` 断言（决策 §57）。产物 **630,388 → 637,030 字节（+6,642）**。SSE 与其余只读页属 D2b-3 / D2b-4。

- **一份构建产物现在同时服务桌面外壳与浏览器**（v0.9 D2b-1——管理程序这条线的适配层）。`ui/src/api/` 对同一个面有两个实现：`tauri.ts`（外壳的 `invoke` / `listen`）与 `http.ts`（控制平面的端点）；**形状**搬到 `api/types.ts`，使它们不属于任何一种传输；envelope 规则搬到 `api/envelope.ts`，因此只有一份。`api/index.ts` 在两份实现之间**只在运行时选一次**，依据是 Tauri 2 自己的全局量（`window.__TAURI_INTERNALS__`，即 `@tauri-apps/api/core.js` 调 `invoke` 时用的那个对象）——所以 `npm run build` 不用改，一份 `dist/` 既服务桌面外壳（`frontendDist`）也服务 `--web-root`（决策 §56）。四个消费者（`appStore`、`ChatPanel`、`CanvasPanel`、`ModelTab`——注意是**四个**，不是一个）现在都 import `../api`；类型标注 `impl: Omit<typeof http, …>` 让「只在某一份实现里存在的名字或签名」直接变成**编译错误**，新探针又在源码文本上重申同一件事。**26 个只读端点已实现**（8 个单字段包装在唯一一处解包）；**26 个控制一律以一句话拒绝**（「desktop control … controls arrive with D4」）而不假装成功；四个订阅返回空的退订函数而不是 reject，因为它们是从 React effect 里注册的。拒绝以**字符串**形式到达，与桌面的 `Result<_, String>` 完全一致，所以商店里的 `String(e)` 在两端都显示宿主自己的那句话。**新增 1 个探针**（`probe-ui-api.mjs`，ui 探针现为 9 个，已进 `gate.sh`）；产物大小 **623,347 → 630,388 字节（+7,041，+1.1%）**，因为这份单一产物现在也带上了 Web 客户端。登录、新页面与事件流属 D2b-2 / D2b-3 / D2b-4。

- **控制平面可以自己提供 Web UI 了**（v0.9 D2a——管理程序这条线的服务端半边）。`riscdom-server --web-root <dir>` 把构建好的前端的 `index.html` 挂在 `/`、把它的 hash 文件挂在 `/assets/*`，位置在路由表**之前**、且**不**做认证检查：文档、样式表与脚本不带任何秘密，而 `/v0/*` 下的一切仍要过 `Authn`。命名空间就是这两种形状、仅限 `GET`、**没有 SPA fallback**，并且**没有往 `ROUTES` 里加任何路由**——`docs/control-plane-api.md` 那两张被文档锁死的表一字未动，它们的两个测试仍过。目录是请求时读盘（不内嵌任何东西，所以前端重建不需要重编 Rust），`.js` / `.css` / 图片类型显式映射，hash 资源标 `immutable` 而 `index.html` 标 `no-cache`，路径穿越被拦两道——名字不得含 `..`/根/前缀组件（`workspace_io::safe_relative` 的规矩），且真正找到的文件必须解析在解析后的 root 之内——客户端给的名字**从不**做百分号解码。不给 `--web-root` 时，`/` 回一个 404，消息里点名该旗标。**新增 6 个测试**（`server/tests/web_ui.rs`：入口文档；hash 资源的类型与缓存头；三种穿越形状；不存在的资源；未配置的命名空间；以及「Web UI 免 token 而 API 仍要」）。决策 §55。另修：`docs/control-plane-events.md` 曾描述一个从不存在的 cookie 会话端点——浏览器读这条流用的是 `fetch` + `ReadableStream`，文档现在就是这么写的。页面本身属 D2b。

- **会话库会等锁，且一次 append 就是一个事务**（v0.9 会话批次——A3 那项遗留）。`SessionStore::init` 现在会在它**第一次写之前**设好 5 秒的 `busy_timeout`，与审计库同一个形状：SQLite 默认是 0，第二个写者当场就收到 `SQLITE_BUSY`，而因为这个失败落在 `SessionStore::open` 里，倒下的不是一条命令而是**整个实例**。两个进程确实可能撞上 `sessions.db`（两个走默认路径的 CLI / 服务器进程，或显式共用 `--data-dir`）；per-instance 仍是默认。**WAL 与 `synchronous` 故意不设**——审计库本来就是要跨进程共享的，会话库不是，所以这份不对称是决策而不是疏漏（决策 §54）。`append_message` 现在是**一个事务**（插入 + 会话 `updated_at_ms` 更新）：两条语句原本可能只落一半，留下一条「与其会话时间戳相矛盾」的消息。**新增 2 个测试**（`a_file_database_gets_a_busy_timeout`；以及 `a_failed_append_leaves_no_message_behind`——它用一个拒绝 `sessions` 表任何 `UPDATE` 的触发器注入失败，**修前会红**；同样的形状在 autocommit 下确实会把那条消息留下来）。

- **宪法与编译器在 Rust 上也不再矛盾——F 线到此收尾**（v0.9 多语言批 F3b-3）。`PROJECT_CONSTITUTION.md` 在 §3.6（在 `non-negotiable` 列表内）、§4.6、§5 三处禁 Rust——而 §47 当时只把这三处的 Zig 半边标了时效、把 Rust 留作「待 F3b」。现在这三条旁注各自补上 Rust 那句（`v0.9 F3b 解除 Rust 禁令——见 §53`），且 §5 的旁注把仍在禁的写明：**C++、Python 仍在禁令内**——Python 待 Linux 沙箱（v1.x），C++ 仍不在范围。原句与 `non-negotiable` 标题一字未动；§9（v0.1 完成情况）同样未动：那是历史，v0.1 当时确实只支持 C。沙箱能用的语言现为 C / Zig / Rust。决策 §53；本批**只动文档**，生产代码零改动。

- **Rust 的 sysroot 现在也一键可装——而且它是唯一「产物与版本硬绑定」的 pin**（v0.9 多语言批 F3b-2）。`Toolchain` 新增 `Rust`，于是 `LABELS` = `c` / `zig` / `rust`，Tauri 命令、HTTP body 与 CLI 都由同一个 `Toolchain::parse` 获得 `--toolchain rust`。这条件规格是第一个**没有 `(os, arch)` 分支**的：`rust-std` 组件是给**目标**而非宿主的，所以一个 11.9 MB 资产（`rust-std-1.98.1-riscv64gc-unknown-none-elf.tar.xz`，sha256 `32ff8091…`，取自旁边的 `.sha256` 文件）就服务所有平台——`rustc` 本身仍由机器提供（F3b 第一条裁决）。`find_rust_std` 是第三个定位器，也是唯一一个产物是**目录**的：归档把 sysroot 嵌下一层（`rust-std-<版本>-<目标>/rust-std-<目标>/`），定位器在深度 1 返回内层（手工解开的则在深度 0），既有的安装逻辑会把这段相对路径保留下来——所以**不需要「择层抠取」**，`product_locator` 的 `PathBuf` 契约也一字未改。采用走 `set_rust_sysroot`，它会校验 `lib/rustlib/<目标>/lib` 并一并报出 `rustc` 版本。**版本是硬门**：sysroot 只能由产出它的那个 release 使用，因此 `begin_toolchain_download`——每条边都必经的那个方法——在机器 `rustc -vV` 报出的 release 不等于钉住的 `1.98.1` 时**在下载之前**就拒绝。**新增 5 个测试**（单资产规格；`find_rust_std` 两种深度；走回环的 Rust 端到端；经 `download_toolchain_now` 的采用并断言 C 与 Zig 的固定值仍为 null；版本拒绝），另把标签测试扩到 `rust`，并加了一个会输出 GNU 长名条目的 fixture builder——真实归档正是那个形状。决策 §52。
- **logging 测试的 stdout 管道现在被读到 EOF，而 CI 那两次红就是它引起的**（v0.9 logging 修根因批）。`read_banner` 以前按值拿走子进程的 stdout，并在看到 banner 行的瞬间丢弃它；server 随后还要写**三行**（`server/src/main.rs:81-83`）到一个读端已关闭的管道——EPIPE，而在 Unix 上就是 SIGPIPE，它会**无声地**终止进程（无 panic 消息、stderr 到 EOF），从而来不及写出测试等的 `connection from … ended`。这正是前两批测到的形态：`0 line(s) unreadable`、`reader already stopped (EOF)`、子进程干脆消失。现在 `read_banner` **把读取器交还**，`start()` 用与 stderr 同一个 `drain_reader` 把 stdout 读到 EOF（两根管道从此对称），失败信息也在子进程退出状态旁多了 `server stdout: N line(s), reader …`。另有一个不依赖 server 的测试把形状钉住：写四行的子进程被读到结束，且必须报 `success()`（被 SIGPIPE 杀死不算成功）。Windows 一直是绿的，因为它没有 SIGPIPE。**生产代码零改动**——server 自己的 `println!` 没问题，问题在读取端。
- **logging 测试的失败信息现在会说子进程是怎么离开的**（v0.9 logging 诊断批次）。上一批把**读取器**排除了（`0 line(s) unreadable`、`reader already stopped (EOF)`），只剩一个事实：**服务器进程自己已退出**，且从未写出 `connection from … ended`。本批给那条信息补上另一半——`child: exited with code N` / `killed by signal N` / `still running`——并用一个单测把报告本身钉住（`cmd /c exit 1` / `sh -c 'exit 1'`）。**侦察排除的**：全仓**没有** `[profile.*]` 段（因此没有 `panic = "abort"`，否则任一 task panic 都会致命）、没有 `.cargo/config.toml`、没有 `RUSTFLAGS`，而 server 仅有的两处 `process::exit` 都是启动失败路径——两处都先 `eprintln!`。**剩下最强的线索在测试侧**：`read_banner` 按值拿走子进程的 stdout，并在看到 banner 行的瞬间丢弃它，随后 server 还要写三行 `println!`（`server/src/main.rs:81-83`）到一个读端已关闭的管道——这个竞态与每一条已观察到的事实相符（间歇、子进程消失、没有 `connection from`、子进程 stderr 上没有异常）且 Windows 比 Unix 更容易赢。至于到底是 `exited` 还是 `killed by signal`，下一次 CI 失败就会说出。**只改诊断：未碰生产代码、未改超时、未动 profile。**（v0.9 多语言批 F3b-1）。`compile` 第三次按扩展名分派：`.rs` 走 `rustc --target riscv64gc-unknown-none-elf --sysroot <dir>`，配上生成的 `link.ld`（与 C、Zig 共用）、把**配置里的** RISC-V GCC 当链接器（`-Clinker=…`——**故意不**用草案命令里写死的 `riscv64-unknown-elf-gcc`，因为 xPack 装出来的叫 `riscv-none-elf-gcc`），再加 `-Cpanic=abort`、`-Crelocation-model=static`、`-Ccode-model=medany` 与 `-Clink-arg=-{march,mabi,nostartfiles,T}`。`RustConfig` 是第三种语言的配置，也是第一个**两半都可能缺**的：`rustc` 靠探测（`RISCDOM_RUSTC` → `PATH`，同 Zig），sysroot 来自 `settings.rust_sysroot`（或 `RISCDOM_RUST_SYSROOT`）——它是一个**目录**而非可执行文件，因为 Rust 向我们要的是目标的 `core`。缺哪一半就**点名拒绝**（`RustConfig::require`），绝不静默失败。`RUSTUP_TOOLCHAIN` **只对该子进程**清空，所以 `rust-toolchain.toml` 无法在我们给的 sysroot 之下换掉编译器，并行构建也不受影响。宿主会拒绝不带 `lib/rustlib/<target>/lib` 的 sysroot（`set_rust_sysroot`），并把 `rustc` 版本一并报出——两者必须匹配。**新增 6 个测试**（4 个在 `compiler.rs`：精确的 `rustc` argv、扩展名臂、两半缺失各自的拒绝、Rust 搜索日志；`policy::allows_rust_inside_root`；以及宿主侧「sysroot 必须带目标库」）。真正的编译测试在这里打印 `skip: compiles_hello_rs_fixture -- no rustc + target sysroot`：**本机没有目标 `std`**（`rustup target list --installed` 只有宿主三元组）。`rust-std` 仍不可下载（F3b-2），§5 对 Rust 的禁令也仍未加时效旁注（F3b-3）。（v0.9 logging 批次）。`the_connection_line_appears_at_info` 在 Linux CI 上**连失败两次**（两次都是 5.09 s，报错相同：捕获到的 stderr 里只有 `--no-auth` 的 WARNING），而那个测试二进制的读取器正具备「丢行」的能力：`for line in reader.lines() { let Ok(line) = line else { break }; … }`——一行读不出来就结束线程，并把其后所有行丢掉，包括测试等的 `connection from … ended`。现在坏行**只计数、继续读**（`InvalidData` / 其它 I/O 错误计入坏行数，`Interrupted` 重试，EOF 才结束），且 `wait_for_line` 失败时打印**读取器状态**——已捕获行数、坏行数、仍在读还是已停止——下次变红就能区分「那行根本没来」与「读取器早就停了」。**仍开着的**：这是否**就是** CI 的根因。我们的改动不可能造成该失败（`789f196` 对 `server` 的唯一改动是 `Action::ToolchainDownloadStart` 臂，`+11/-1`，健康检查路径没有动），可是同一测试在它之前那两次 Linux run 都是绿的。5 秒超时与 25 ms 轮询是**故意不动**的——本仓两个 flake 先例（relay 端口租约、`audit::concurrency`）都是靠消除竞态修好的，而不是等更久。决策 §50。
- **Zig 编译器现在只差一次点击，而下载终于会说自己在装哪种语言**（v0.9 多语言批 F3a-download-apply）。`DownloadSpec` 新增 `toolchain: Toolchain`（`C` / `Zig`，serde，缺省即 C），随之带来模块自己猜不出的**两件事**：**哪个定位器**找产物（`product_locator`：C 走 `find_compiler`，Zig 走新的 `find_zig`——深度 0/1，用 `agent::ZIG_NAMES`，因为 Zig 发行包是浅结构）以及宿主做**哪次采用**（`set_toolchain_path` vs `set_zig_path`）。`zig_spec_for_current_platform()` 是 C 规格的兄弟函数：Zig 的五个平台资产 + 自己那 5 条硬编码 0.16.0 校验和，取自 `ziglang.org/download/index.json` 的 `shasum` 字段（Zig 不像 xPack 那样发行逐资产 `.sha`）。语言以**标签**形式传递，只在一处解析（`Toolchain::parse`）：CLI 的 `--toolchain zig`、`POST /v0/toolchain/download` 的 body `{"toolchain":"zig"}`（该端点在本批之前**没有** body；不发 body 仍然等于 C）、Tauri 命令的 `Option<String>`。`ToolchainDownloadStatus` 新增 `toolchain: Option<Toolchain>`，UI 因此能说出在跑哪一个。**新增 5 个测试**（3 个单测：标签往返、Zig 五平台臂 + 校验和、`find_zig` 两种深度；2 个集成：Zig 走回环下载全链、Zig 经 `download_toolchain_now` 的采用并断言 C 的固定值仍为 `null`）；CLI 的 `--toolchain` 断言写进本就拥有命令表的两个测试里。`install_subdir` **仍是**死字段——本批需要的不是它，而把它当解压根相接法反而会搞坏 Zig 的安装（决策 §49）。
- **多了 `.tar.xz` 归档类型，其解包器就是 gzip 那个换个解码器**（v0.9 多语言批 F3a-download）。`ArchiveKind` 新增 `TarXz`；`extract_tar_xz` 逐行照抄 `extract_tar_gz`，只把 `flate2::read::GzDecoder` 换成 `xz2::read::XzDecoder`——同一个 Zip-Slip 守卫（`safe_relative`）、同一个 `set_overwrite(true)`、同一套逐条目取消——且分派臂**不带平台门**：`.tar.gz` 在这里只限非 Windows，因为它只是我们自制下载里的 unix 资产；而 `.tar.xz` 是**宿主自己** Zig 发行包与 Rust `rust-std-*.tar.xz` 的形态，所以 Windows 主机也得能读。`host-core` 新增直接依赖边 `xz2 = "0.1"`：它与 `lzma-sys` 本来就在 `Cargo.lock` 里（由 `zip` 带入），因此锁只**多一行**（那条边）、无版本变动；`lzma-sys` 在 MSVC 上编译其 vendored C，unix 主机没有 `liblzma` 时也回落到同一份 vendored 构建，所以**两个平台都不新增系统库前提**。**新增 5 个测试**，全部离线：4 个单测（解包 / 拒穿越条目 / 覆盖 / 取消）+ 1 个走回环下载全链的端到端。归档是**内存里现造**的——这就是本仓的归档测试惯例（`host-core/tests/common` 造 zip/gzip fixture 也一样；仓里没有 `tests/fixtures/`，也没有签入的二进制），所以**不需要改 `.gitattributes`**。不在这里的：目前还没有任何东西会去下载 Zig / Rust 归档——`spec_for_current_platform()` 仍只返回 xPack 那一条，Zig 定位器（`find_zig`）属 apply 批（决策 §48）。`PROJECT_CONSTITUTION.md` 在**三处**禁 Zig——§3.6（在 `non-negotiable` 列表内）、§4.6、§5——并在 §9 的 v0.1 清单里又记了一次。三处现在各带一条**时效旁注**（原句完整、`non-negotiable` 标题完整）：那些条款禁 Zig 的条件是「**MVP 阶段**」，而 MVP 已于 v0.8.0 结束。§5 的旁注写明只动了 Zig——C++、Rust、Python 仍在禁令内（Rust：F3b；Python：Linux 沙箱，v1.x）。§9 未动：v0.1 当时确实只支持 C。记入决策 §47。两项**只报不修**：宪法是一份停在 v0.5 roadmap 节之后的活文档；本批也没有补 v0.6–v0.9 节。
- **Zig 成了沙箱的第二种语言，语言按源扩展名选**（v0.9 多语言批 F3a）。`compile` 把 `.c` / `.h` / `.S` / `.s` 交给 GCC（未改），把 `.zig` 交给 `zig build-exe -target riscv64-freestanding -O ReleaseSmall -fno-stack-check -T <link.ld> --image-base 0x80000000 -femit-bin=<out>`。Zig 自带交叉链接器，裸机目标无需外部工具链、无需 sysroot，生成的 `link.ld` 原样复用。Zig 不注入任何东西：源自己写 `_start`，因为 `-bios none` 的客机跳到载入地址而不是 ELF 入口点，启动代码必须排最前（`.text.start` 正是让一份脚本同时服务两种语言的原因）。`compile_freestanding` 保持原签名，`CompilerConfig` 多带一个值（`zig: ZigConfig`），`settings.json` 新增 `zig_path`（`AppState::set_zig_path` / `clear_zig_path`；**故意不**让 preflight 缓存失效）。`Policy.allowed_extensions` 放行 `.zig`，工具 schema 文档、其人类表格与 `agent/README.md` 都点名两种语言。**这台机器没有 Zig**，所以编译测试打印 `skip: compiles_hello_zig_fixture -- no Zig found`；Zig→guest 启动测试既带标记**又**自我防护——gate 的 `--include-ignored` 会跑在有 QEMU 与 GCC、但不一定有 Zig 的机器上。下载 Zig 归档**不**在本批（其 macOS/Linux 构建是 `.tar.xz`，而下载器只认 `Zip` / `TarGz`），所以那是独立批次 F3a-download，与 Rust 共用（决策 §46）。具体体积要到那一批才有定论：本批没有下载任何 Zig 二进制。
- **最后四个环境依赖测试也点名了，两个 fixture 不再只在 Windows 上能跑**（v0.9 gate 一致性批 B-3b-fix）。B-3b 的仿真有个洞：它只把 `RISCDOM_*` 指向不存在的文件——那能打断 `discover()`，但打断不了 `CompilerConfig::from_env()`，它的回退是**裸可执行名、靠 `PATH` 解析**，而这台机器的 PATH 上正好有工具链。把 `PATH` 也滤掉后又找到 **4** 个（三个会编 C，一个 preflight 步骤），标记总数现在是 **54**（37 + 8 + 9）。七个红 target 里有两个**不是缺前提，而是只在 Windows 成立的 fixture**：`build_archive` 的 tar 分支在 `../` 条目上崩掉，因为 `tar` 自己的 `append_data` 拒绝 `..`（现在名称直接写进 header，于是拒它的变成*安装器*——而测试要考的正是安装器）；以及 `qemu_archive()` 的内容是个 `#!` 脚本，在 Unix 上 0755 的文件会真的*运行*，于是「不能运行的模拟器」竟被采纳了。Windows：**643 passed / 0 ignored**；没有工具的 gate：**584 passed / 62 ignored**。
- **需要 guest 的测试会自己声明，gate 改为按能力分叉**（v0.9 gate 一致性批 B-3b）。50 个需要 QEMU guest 或 RISC-V GCC 的测试现在都带一个 `#[ignore]`，其理由写明前提（37 个 `requires a QEMU guest and a RISC-V GCC`、8 个 `requires a discoverable QEMU`、5 个 `requires a discoverable RISC-V GCC`），因此没有这些工具的机器跑 `cargo test --workspace --no-fail-fast`——**588** 个测试，而本系列开始时是 10。有工具的机器跑 `cargo test --no-fail-fast -- --include-ignored`，再加三个 `--skip`（对应需要 `DEEPSEEK_API_KEY` 或会真写 OS 钥匙串的测试）——**643 passed / 0 ignored**。`--skip` 匹配的是测试的**函数名**——最初那版旗标里的文件名（`real_api`、`stream_real`、`keyring_os`）一个都匹配不上，这个错误就是这样被抓住的。`scripts/gate.sh` 已无按平台分叉的测试分支，账本的 §44 记下了这个惯例。
- **两个真的会编 C 的单元测试，在没有工具链时会明说**（v0.9 gate 一致性批 B-3a-fix）。B-3a 在 Linux 上当场就红：`agent/src/compiler.rs` 的 `compiles_hello_fixture` 与 `reports_compile_failure_without_panicking` 调 `compile_freestanding(...).expect("run gcc")`，而 CI 没有 RISC-V GCC。现在它们探测 `CompilerConfig::discover()`，缺失时打印 `skip: ... -- no RISC-V GCC found` 而不是失败；有工具链的机器仍然真跑。两处 `cargo test` 也加上了 `--no-fail-fast`，一个测试二进制失败不会遮掉其余。**值得记住的教训**：Windows 机器**有**工具链，所以在那里跑一遍套件**不可能**暴露「需工具链」这类前提——本地检查一路都是绿的。
- **没有 guest 的 gate 现在跑**每个** crate 的单元测试**（v0.9 gate 一致性批 B-3a）。该分支原本跑 `cargo test -p audit -p sandbox --lib`——638 个测试里的 **10** 个——于是 `agent`、`host-core`、`cli`、`server`、`worker` 以及 audit/sandbox 的 `tests/` 目录在非 Windows 上完全无覆盖。现在是 `cargo test --workspace --lib`：**204** 个单元测试，零 QEMU 风险（全是纯逻辑；唯一带平台门的是 `sandbox/src/platform.rs`（unix）与 `server/src/token.rs`（unix + windows））。仍开着：需要 guest 的集成测试还没与可移植的分离，而且可移植的**集成**测试（`tests/*.rs`）在非 Windows 上仍然不跑（B-3b）。
- **gate 现在在每个平台都 lint 并 check 每个 workspace crate**（v0.9 gate 一致性批 B-2）。最后两处跳过没了——`host-tauri` 与 `ui/src-tauri` 在 Linux 也被 lint——而 `worker`（此前两个平台的任何 clippy 列表都没提到它）也加了进来。`scripts/gate.sh` 里的 OS 分支随跳过一起删掉，`ci.yml` 的 `gate` job 现在在原有的 `libdbus-1-dev` 之外再装 Tauri 的系统库（webkit2gtk / gtk / librsvg / libsoup）。`ui/dist` **不是** `cargo check` / `clippy` 的前置条件：只要 `custom-protocol` 特性没开（`tauri-macros/src/context.rs`），`tauri::generate_context!()` 就走 dev 分支，而裸 `cargo check` 正是这种情形；已用「把 `ui/dist` 挪开再 check」实测确认。仍开着：非 Windows 的 gate 只跑 `cargo test -p audit -p sandbox --lib`——638 个测试里的 10 个（B-3）。
- **B-1 的后续：它暴露的那个 lint 已修**（v0.9 gate 一致性批 B-1-fix）。在 Linux 上解开 `cli` 后当场就红：`cli/tests/control.rs` 的 `write_executor_settings` 只被一个 `#[cfg(windows)]` 派发测试调用，自己却没有 cfg 门，于是在 Linux 上是死代码，`-D warnings` 把它变成了编译失败——而只有它和假执行者 helper 用到的 `std::path::Path` 导入也因为同一原因没有门。现在两者都是 `#[cfg(windows)]`。**生产代码零改动。CI 已回绿。**
- **gate 现在在 Linux 上也 lint `cli` / `server` / `host-core`**（v0.9 gate 一致性批 B-1）。`scripts/gate.sh` 的非 Windows 分支原本把这四个 crate 合在一起跳过；现在它在**每个**平台都跑 `cargo clippy -p cli -p server -p host-core --all-targets --no-deps -- -D warnings`，只跳过两个 Tauri crate（`host-tauri`、`ui/src-tauri`）。**没有新增系统包**：CI 已装的 `libdbus-1-dev` + `pkg-config` 正是 `host-core` 的 `keyring` 后端在 Linux 上所需。这个缺口是**付了代价才发现的**——E4 的 `worker` example 步骤是 Linux 上第一个编译 `host-core` 的 gate 步骤，它连红了四个提交。`ci.yml` 未改。仍开着：两个 Tauri crate（B-2），以及非 Windows 的 gate 只跑 `cargo test -p audit -p sandbox --lib`——638 个测试里的 10 个（B-3）。
- **crate 示例已记载，贡献者模板已就位**（v0.9 小改批次）。`agent/README` 与 `audit/README`（两个语言）各加了一节 `## 示例`，分别对应 `examples/audit_demo.rs` 与 `examples/chain_demo.rs`——它们是唯一两个没在本 crate README 里点名的示例（`sandbox` 的 `run_hello`、`worker` 的 `dispatch` 与 `remote_executor` 早就有了）。`.github/` 新增两个 issue 表单（`ISSUE_TEMPLATE/bug_report.yml`、`ISSUE_TEMPLATE/feature_request.yml`）与一份双语 PR 模板（`PULL_REQUEST_TEMPLATE.md` + `.zh-CN.md`）。表单刻意用 `.yml`：`scripts/check-bilingual.sh` 扫每一个 `*.md`，所以 Markdown 模板会需要一个 `.zh-CN.md` 兄弟文件——而 GitHub 又会把它当成第二个模板列出来。**决策 §43** 撤销了 §20 的 DCO 条款：两件工具干的事不同，CLA（权利授予）更强、也是 open-core 必需的，而 DCO 只是来源声明。
- **Linux 上的 CI 红已在配置层修好**（v0.9 CI 修复）。`gate` job 自 `00fca17` 起在 Linux 上连续红了**四个提交**：它的 `remote executor example` 步骤是 Linux 上第一个编译 `host-core` 的 gate 步骤，而 `host-core` 在 Linux 上的 `keyring` 后端会编译 `libdbus-sys`，后者经 pkg-config 需要系统 `dbus-1` 库。`gate` job 现在安装 `libdbus-1-dev`，Linux 的 `bundle` job 的依赖列表也加上了它。本地（Windows）gate 走 `keyring` 的 `windows-native` 后端，从不需要它——于是四个提交在 CI 全红的情况下被推了出去。**流程教训**：本地 gate 绿不等于 CI 绿；每次 push 后跑 `gh run list --limit 3`。
- **文档有了入口**（v0.9 文档批次）。[`docs/README.zh-CN.md`](README.zh-CN.md) 是那张导航图：全仓每一份 Markdown，按 [decisions](decisions.zh-CN.md) §21 的五个受众分组（从这里开始 · 内核开发者 · 发行集成者 · 管理员 · 终端用户 · 贡献者），每行写明这份文档做什么、它的**状态**（*活跃* / *快照* / *历史*）与版本——历史被标为历史（`architecture-evolution.md` 是 v0.7 快照；较早的 `CHANGELOG` 段与已发布的 `RELEASE_NOTES` 不重写）。根 `README` 的目录树落后了好几个批次（它只认识 `host/` 与四个 crate；工作区有八个），测试清单还把 `host-core` 写成「Tauri 后端命令」——自 A1 拆分后那是 `host-tauri` 的活；两个语言都已改正，而「更多」节现在以导航开头并列全九个 crate README。规范文档里两处陈旧计数（`control-plane-api` 说 35 个控制端点、客户端指南说 31 个查询端点）也已改。**只报不修**：四个 crate README（`audit`、`cli`、`host-core`、`ui`）没有任何指向邻居的链接——它们唯一的链接是语言切换行。（那一批报的另外两项——crate 示例未在本 crate README 记载、以及 §20 的模板与 DCO——已在 2026-09-24 的小改批次里结清；§20 的 DCO 条款事后看是判断错了，§43 已撤销它。）
- **行尾现在归仓库管**（v0.9 行尾批次，机械性）。一份 `.gitattributes`（`* text=auto eol=lf`；`*.sh` 明确写出；六个被跟踪的二进制标 `binary`；**没有 `*.ps1` 例外**——五个 PowerShell 脚本今天就是 LF，而且每批的 gate 与提交都是经它们跑的）。工作树里躺着 38 个 CRLF 或混用行尾的文件（最重 `sandbox/src/relay.rs`，413 行里 401 行），而 git 看不见：**索引里一直就是 LF**，所以 `git add --renormalize .` 一无所获，**本提交不含任何行尾改动**——它是一次工作树修理，并已被证明与内容无关（356 个被跟踪文件逐个与其已提交 blob 对比，逐字节相同）。七处相对链接是错的（从 `docs/` 指向根级文件或反之漏了前缀：CHANGELOG 的 `multi-agent-foundation`、`handoff(.zh-CN).md` → `RELEASE_NOTES`、`qemu-distribution(.zh-CN).md` → `THIRD_PARTY_NOTICES`、`toolchain-setup(.zh-CN).md` → `ENVIRONMENT`）；全仓 365 条相对链接现在全部有效。
- **小债还清**（v0.9 清账）。四件事，没有新表面。（一）**测试里不再有需要手改的计数**：`every_control_endpoint_answers` 通过一个新的只读访问子（`server::routes::control_paths()`）从路由表算出预期，于是少一条用例的控制端会点名缺失的路径而失败，而测试不管的七个是被**点名**而不是被数出来的；`the_table_has_the_documented_endpoints` 从 API 文档的 §5 标题读计数，两个语言都读。（二）**忽略 `__pycache__/` 与 `*.pyc`**。（三）数端点的注释（「26 查询 / 27 控制」、「两条带路径参数的路由」）已改正或去掉数字。（四）`agent/README.zh-CN.md` 的工具表变成与它英文兄弟一样的索引。扫描带出两个发现，按本批规矩**只报不修**：**七处相对链接失效**（从 `docs/` 指向根级文件或反之——`CHANGELOG.zh-CN.md` → `docs/multi-agent-foundation.zh-CN.md`、`handoff(.zh-CN).md` → `../RELEASE_NOTES(.zh-CN).md`、`qemu-distribution(.zh-CN).md` → `../THIRD_PARTY_NOTICES.md`、`toolchain-setup(.zh-CN).md` → `../ENVIRONMENT.md`），以及**11 个文件在工作树里行尾 CRLF/LF 混用**（`sandbox/src/relay.rs` 及邻居）——编辑工具与 `core.autocrlf=true` 的产物，git 看不见。本批要找的两个缺陷**并不存在**：85 份 Markdown 里没有 lone `\r`，也没有不配对的围栏。
- **那条缝有了另一半：远程执行者**（v0.9 接口交付 E4）。`agent/src/dispatch.rs` 自 v0.8 起就写着「远程实现……正好实现这个 trait。**尚未写**」；`worker/examples/remote_executor.rs` 就是它。`HttpExecutorHandle` 持一个本地 label、节点的 base URL、可选的 bearer token 与**远端 target**，它的 `run` 把一个任务形状的 body POST 到 `POST /v0/tasks`——那个把任务路由给**远端**节点拥有的执行者、并回 `TaskOutcome` 的端点，这正是它是真正执行者而非形状演示的原因。应答里的身份是节点的，绝不是句柄的 label；一条指向别的任务的应答是协议破裂；`404` 是 `NoSuchAgent`（缺的是对面的队伍），其它失败都是 `Failed`。**没有传输层 crate**：`worker` 只依赖 `host-core`、`agent` 与 `serde_json`，请求是用 `std::net::TcpStream` 手写的——`server/tests/smoke.rs` 与 `host-core/tests/common/mod.rs` 已在用的手法，于是读者看得见究竟过了什么。**没有生产 crate 被改动**：`LocalDispatcher::new(vec![Arc::new(handle) as Arc<dyn AgentHandle>])` 就是全部集成，这正是那条缝承诺的。示例零配置即可跑（`127.0.0.1:0` 上的进程内替身节点），并用 `--self-test` 自证（七条断言）；`scripts/gate.sh` 会跑这一步。客户端指南多了 §9（怎么写一个句柄），README 写明它不是：回环 HTTP，不是跨设备方案（决策 §42）。

- **一个可以跑起来的监工**（v0.9 接口交付 E3）。`examples/python/dispatch.py` 是最小的完整外部监工：一个在内核之外、自己不持有模型、通过控制平面驱动一个节点的进程——用 `GET /v0/executors` 拿队伍，每条任务一次 `POST /v0/tasks`（同步、一条一条），`--follow` 时用 `GET /v0/events` 在干活过程中看事件流。**只用标准库**（`urllib.request`、`json`、`argparse`，加上手写的 SSE 拆帧），因为参考实现不该教人一个它并不需要的依赖；token 来自 `--token-file` 或 `$RISCDOM_TOKEN`，绝不来自参数。它的退出码沿用 CLI 的约定（`0` 全部成功、`1` 有没成功、`2` 用法错误、`3` 不可达或被拒），而它的 `--self-test` 用一个 stdlib `http.server` 假控制平面离线跑通整条路径——`scripts/gate.sh` 现在会在 `PATH` 上有 Python 解释器时跑这一步（没有就打印 skip；这是 gate 的第一处可选工具链）。随行两份文档：`examples/python/README.zh-CN.md` 与客户端指南 §8，后者现在指向它。Rust 那位兄弟保持原样（`worker/examples/dispatch.rs`：执行者是子进程、不走 HTTP、并行），README 特意把差别列成表：它们是同一幅画的两半（决策 §41）。

- **两份工具 schema 都是文档，且都被校对**（v0.9 接口交付 E2）。这条接口的另一半是词汇：执行者的模型可以叫什么，AI 监工可以叫什么。`docs/tool-schema-executor.zh-CN.md` 是 `tools_json()` 构建的八个工具——逐字，就是 `AgentLoop` 放进请求 `tools` 字段的那个数组——而 `docs/tool-schema-control-plane.zh-CN.md` 把每个端点写成一条 OpenAI 风格函数定义（32 查询 + 36 控制 + 3 本机 + 4 带路径参数 = 75 条工具），名字从路径机械推导（去掉 `/v0/`、折叠分隔符；同时服务两种方法的路径给它的 `POST` 加 `_post`；四条带 id 的路由用一个动词）。区分正是要点，两份文档都说了：执行者的工具在**执行者内部**跑，监工的工具是**驱动**一个节点的方式。手写的 schema 会漂，所以两半各有一个归属：`agent/tests/tool_schema_doc.rs` 把执行者文档与 `tools_json()` 对校；`server/src/routes.rs` 自己的测试把控制平面那三张带标记的路由表与 `ROUTES` + `LOCAL_ROUTES` + `resolve` 对校（于是端点不可能没带工具就落地）；而新增的 `scripts/check-tool-schema.mjs`——`scripts/gate.sh` 里的一步——owns 两者都看不见的东西：译文必须携带相同的带标记块，表里每个名字必须是文档 §2 的推导结果，且每个名字既有定义又有表行。`agent/README.md` 的工具表现在只是指向 schema 文档的索引，不再是清单的第二份（决策 §40）。

- **节点可以被派任务，而它的执行者队伍是配置**（v0.9 接口交付 E0）。本批把 `Dispatcher` 变得可以从 HTTP 抵达。`settings.json` 里的 `executors`——一个 label、一个 program 与它的参数，**没有 `env`**，因为设置文件不是密钥库——在 `load_settings` 之后一次性变成 `StdioExecutorHandle`，而 `AppState::dispatch_task(target, input, sandbox, id)` 把一条任务送进 worker 监工所建的那个同一个 `LocalDispatcher`。登记**不起任何子进程**：在任务到来之前 handle 只是数据，所以它不需要懒初始化（也因此，不存在的 program 是第一个任务的失败，而不是启动错误）。`POST /v0/tasks` 接 `Task` 的四个标量字段，调用方没给 `TaskId` 就补一个，并以该执行者的 `TaskOutcome` 作答——**同步**，和 `/v0/agent/run` 一样：没有任务表，也没有 `GET /v0/tasks/{id}`，所以没有可轮询的东西。阶梯是分开的两件事：无人拥有的目标是调用方的 `404 cause "target"`，断掉的派发是宿主的 `500 cause "task"`，而一次只是*跑失败*的运行仍是 `200`，它的 `outcome` 会说 `failed`。`GET /v0/executors` 列出可抵达的都有谁，按配置顺序——空列表也一样。**节点故意不是它自己的执行者之一**——目标写它是 `404`，因为在这里跑是 `POST /v0/agent/run`——所以两个端点是兄弟而非同义词，而这正是让 E0 值得单独一批的那个区分。两条路由都声明 `agent.run`（E0 裁决三：不加任何 capability——能跑 agent 的人就是能问「能问谁」的人），各有一条 Tauri 命令（已注册、未接到界面：D 线）与两个 CLI 子命令（`executors list`、`tasks dispatch --target <agent_id> --input <text> [--sandbox <name>]`）。随行八份文档，含决策 §39。

- **一个任务声明它在哪个沙箱下跑，而节点一点不动**（v0.9 沙箱 F2d，沙箱线的最后一块）。`Task` 多出 `sandbox: Option<String>`（`#[serde(default)]`，旧 supervisor 的行仍可读）与 `Task::with_sandbox` builder；三个能携带声明的表面都带上它：`POST /v0/agent/run`（请求体字段 `sandbox`）、Tauri 的 `run_agent` 命令（新的可选参数）与 worker（`Task.sandbox`，本就在它读的那一行上——协议改动恰好是一个可选字段，因为 `Task` 自 v0.8 起就可序列化）。它落在 `AppState::run_agent_for(emitter, input, sandbox)`——`run_agent` 保留旧签名并改以 `None` 委派，所以直接调用点一个都没动。优先顺序是**任务 > `current_sandbox` > 配置默认 > 内置兕底**，未定义名字是 `404`（`cause: "name"`）而不是静默回落，解析出的定义被注入为编译器、QEMU 路径与客户机内存（新增 `agent.set_memory_mb`，因为 `VM_MEMORY_MB` 是硬编码的 128）。**它不到内核**：`start_vm` 的 `elf_path` 仍旧选 ELF，`def.kernel` 仍是*切换*时启的（F2d 裁决二）。两条拒绝守着声明：未定义的名字，以及不是**正在跑的** VM 所来自的名字——`409` `cause: "sandbox"`，因为任务只声明、只有切换才改变，而报文指名两条出路（停掉它，或 `POST /v0/sandboxes/switch`）。这个检查需要一个宿主原本没有的事实：`current_sandbox` 只由成功的切换写入，所以由 `start_vm` **工具**启起的 VM 没有记录下来的来源。现在 `active_sandbox` 记录它——在运行的 VM 出现时、由 `switch_sandbox`、以及（作为节点自己的沙箱）快照恢复时写入，由 `stop_current_vm` 清除。随之而来的是：运行端点的整条拒绝阶梯（先是声明，再是就绪）移进了 `run_agent_for`——坏的*参数*先于环境被回答，所以 `POST /v0/agent/run` 带一个未定义沙箱时是没有配置模型也拿到的调用方 `404`；就绪拒绝变成类型化的 `HostError::NotConfigured`，路由因此不再重复检查就绪。随行七份文档，含决策 §38。

- **项目以一个文件的形式行走**（v0.9 项目进出）。`POST /v0/workspace/export` 把 workspace 以 `tar.gz` 作答——**字节，不是 JSON**，是那个表面上除事件流之外的第一个此类 body——而 `POST /v0/workspace/import` 把归档当请求体收（zip、tar.gz 或 tar，由 `Content-Type` 选、不认知时看字节）。宿主自己的 `.riscdom/` 状态从不打包、从不解包；逃出 workspace、以链接到访、或读不出来的 entry → `400`、`cause: "archive"`；已有同名文件 → `409`、`cause: "exists"`，除非 `?force=true`；而 import 带**自己的 64 MiB 上限**（`413`），而不是抬高每个 JSON body 共用的那个 64 KiB。capability 上两者分开：import 需要新的 **`workspace.write`**（第 32 个），export 需要 `workspace.read`。AI 的写入也变可见了：`write_source` 现在往审计链记一条 **`agent.file.write`** `{path, bytes}`（不是 SSE 事件——事件计数仍为 14），于是「模型写过哪些文件」是一行记录，而不是去重剖一个被截断的工具参数。两个打包器（`zip`、`flate2`+`tar`）本来就在锁文件里，现在对每个平台都声明：Windows 宿主能读 `.tar.gz`，unix 宿主能读 `.zip`。另有两条 Tauri 命令（已注册、未接线——D 线）与两个 CLI 子命令（`workspace import <archive> [--force]`、`workspace export [--out <file>]`；导出把归档写到 `--out` 或 stdout，计数走 stderr）。随行八份文档，含决策 §37。

- **AI 可以申请改动沙箱，而由别人来决定**（v0.9 沙箱 F2c）。控制平面长出一条申请队列：`POST /v0/sandboxes/requests`（声明 `agent.run`——能跑 agent 的 actor 就是可以表达它所想的 actor）落一条申请并答 `201` 带它的 id；`GET /v0/sandboxes/requests?status=`（`sandbox.read`）读回来，新的在前。`approve` / `reject` 只改记录，**别的什么都不做**：切换仍是另一次带授权的 `POST /v0/sandboxes/switch` 调用（F2c 裁决 4）。决策需要该请求自己的 `action` 所隐含的 capability——`switch` 要 `sandbox.switch`，`define` / `assemble` 要新的 **`sandbox.assemble`**（第 31 个）——而静态路由表表达不了这个，所以这两条路由以 `sandbox.read` 为门，由处理器针对 `dispatch` 现在收到的 actor 做精确检查（F2c 裁决 1）；id 未知是 `404`，再次决策是 `409`。**没有 TTL**（F2c 裁决 2）：`expired` 为预留，没有任何路径产生它。队列是队列、不是单槽，id 是 `req-<pid>-<seq>`——自己的命名空间，不与 `task-` 共用。AI 拿到两个让这个表面可达的工具：`request_sandbox` 与 `sandbox_status`，经新增的 `agent::SandboxRequester` trait 接线（宿主用克隆的子句柄实现它：`agent` 不能命名 `AppState`，而 loop 就住在它里面——F2c 裁决 4），另有四条 Tauri 命令（已注册、未接到界面：那是 D 线）与三个 CLI 子命令（`sandboxes requests`、`requests approve|reject`；两条决策和 `sandboxes switch` 一样先问）。`sandbox:request` 是第 14 个 SSE 事件——`{id, status, requester, action}`，每次变化一条——`all_events_are_named` 守卫重新列全十四个。随行的八份文档：API 表格（§5.1 32、§5.2 33，且词汇表那句「每个 capability 都至少有一条路由」改成「都在某处被强制」——因为 `sandbox.assemble` 在那个处理器里被强制，直到它自己的端点落地）、事件表、客户端指南、两份 README、本节、CHANGELOG 与决策 §36。

- **relay 的端口租约契约写下来了，而它那个偶发失败的测试现在断言的正是它**（v0.9 relay-fix 2/N）。`concurrent_leases_never_repeat_a_port` 失败过两次，两次 `sandbox` crate 都未被改动。侦察结论：分配器是对的——检查与登记在同一锁作用域（`relay::reserve`），且 `PortLease` 只有一个构造点——而断言过强：它记录了本次运行中*曾*取到的每个端口，于是一个已结束线程释放、又被 OS 重新发出的号码，看起来就像两个持有者同时持有。现在测试把每个线程的租约停放到八个线程全部取完再比对，即代码真正保持的不变量；新增的 `a_released_port_is_free_to_come_back` 钉住另一半（被销毁的租约让端口对任何人可 bind）。库侧卫生，行为不变：`HELD_PORTS` 改为 `LazyLock` 包着的 `HashSet`（`HashSet::new` 无法初始化 `static`；`relay::reserve` 现在就是一次 `insert`），`Drop` 只删**自己的**号码而不是所有相同项。API 与调用点均未变（`agent/src/tools.rs`、`host-core/src/state.rs`、`sandbox/src/vm.rs` 未动），`sandbox/README.md` 双语写下契约，`port_race.rs`（默认忽略的真 QEMU 压力测试）仍在走调用方用重试绕开的跨进程窗口。更强的「进程存活期间绝不复用」不变量已被考虑并否决（账本 §35）。

- **切换有了对外表面：一个事件、一个端点、一个 capability 与一个 CLI 命令**（v0.9 沙箱 F2b-2）。`sandbox:switch` 是第 13 个 SSE 事件——每次出口发一条 `{from, to, ok, reason}`，成功与失败都发——而 `POST /v0/sandboxes/switch`（capability `sandbox.switch`，第 30 个）为每种失败各答一个状态：`200` 带 `{from, to}`；没有这个名字的定义时 `404`、`cause: "name"`；运行中 `409`、`cause: "run"`，另一次切换进行中 `409`、`cause: "sandbox"`；定义不能跑时 `503`，`cause` 就是原因码（`sandbox_qemu_missing` / `sandbox_toolchain_missing` / `sandbox_kernel_missing`）；而每项校验都过、VM 起不来时 `500`、`cause: "sandbox_start_failed"`——即「已停而非半切换」那种情形，它变成了自己的 `HostError`（`SandboxStart`）以便应答带上原因码。路由答的是两端而不是空的 `204`，所以只读应答的客户端也能知道发生了什么。`switch_sandbox` 现在接收调用方自己的 `EventSink`（路由注入 `HttpEventSink`，Tauri 命令注入 `TauriEventSink`；界面仍未接线——那是 D 线），`sandbox_switch_in_progress()` 是决定 `409` 的探针。CLI 有 `sandboxes switch <name>`——属破坏性家族：先问，`--yes` 提前回答，非终端 stdin 直接拒绝（退出 2），成功时打印 `switched from <old> to <new>`。**`events.rs` 的守卫恢复完整**：`all_events_are_named` 列全 13 个名字并断言互不重复，F1 的 `qemu:download` 不再遗漏；`docs/control-plane-events.md` §3 现有 13 行。自 F2b-1 起报告、仍未关闭：*成功*路径无 hermetic 测试（需要真 QEMU 与内核 ELF，即黄金路径的 `--ignored` 票）；端点的 `409 cause: "run"` 分支同样无法在测试里走到（运行中需要 LLM 与工具链），因此只在状态层守住。

- **节点可被切到另一个沙箱，先校验后停止**（v0.9 沙箱 F2b-1）。`AppState::current_sandbox()` 变成**运行时状态**（F2b 决策 1，账本 §34）：切换成功前为 `None`，从不写进 `settings.json`；`sandbox_default_name()` 仍回答存储的 `default_sandbox`——切换改的是*正在跑的*，不是*配置*。`AppState::switch_sandbox(name)` 取定义（列表服务的同一份合并，现已收为一份实现 `merged_sandbox_defs`）、校验它（`sandbox_check` 答四个新 `HostError`：`SandboxNotFound` / `SandboxQemuMissing` / `SandboxToolchainMissing` / `SandboxKernelMissing`；`sandbox_runnable` 是它的 bool 面，行为不变）、定内核（`def.kernel`，否则工作区最新 ELF），**之后才**停当前 VM、按定义启一台新的（三次尝试、每次新端口，与 `tool_start_vm` 同形）并把名字记为当前。切换进行中拒绝第二次（`begin_sandbox_switch` / `cancel_sandbox_switch` / `finish_sandbox_switch`，与下载槽同形，`already in progress`），运行中拒绝（`run_in_flight()`，读自 `begin_run` 置、`finish_run` 清的运行记账——loop 共享这个 VM 槽并每次工具调用取一次锁，运行中切换会把它交给另一台 guest）。切换失败 = 节点**已停**而非半切换（`Drop` 杀掉失败句柄 spawn 的东西）；校验失败的定义完全不碰正在跑的 VM。本批尚无事件、端点、Tauri 命令、CLI 与 `sandbox:switch` 审计行——那是 F2b-2，连同 capability（29 → 30）。已报告、已在 F2b-2 关闭：`events.rs` 的守卫只列十一个名字而文档已算十二个（列表里漏了 `qemu:download`）。仍未关闭：成功路径无 hermetic 测试（需要真 QEMU 与内核 ELF，即黄金路径的 `--ignored` 票）。

- **无版本的扫描资源按资源本身命名，而不是按一个缺失的版本**（v0.9 沙箱 F2a-3）。F2a-1 的 `format!("{kind}-{version}")` 把机器自带的 QEMU——扫描不记录其版本——变成了一个叫 `qemu--` 的定义，F2a-2 又把这个名字经四条端点与 CLI 服务出去。它现在在每个平台都叫 `qemu-system-riscv64`：即 `sandbox::qemu_discover` 搜索的那个主干名，裁掉 Windows 构建的 `.exe` 后缀，因此文件里不留第二份名字。扫描**确实**知道版本的资源仍为 `<kind>-<version>`（`toolchain-15.2.0-1`、`qemu-11.1.0`）；`"-"` 哨兵变成一个导出的常量 `NO_VERSION`，不再是两个必须彼此一致的字面量。扫描与合并的其余部分未变——这是一次命名修复。

- **沙箱注册表有了控制平面表面：四条只读路由、四个 Tauri 命令、四个 CLI 子命令**（v0.9 沙箱 F2a-2）。`GET /v0/sandboxes` 答合并后的注册表加上 `current` 与 `default`；`/v0/sandboxes/current` 答这两个名字；`/v0/sandboxes/candidates` 答**原始扫描**（不是注册表，也不写回）；`/v0/sandboxes/{name}` 答一个定义，没有这个名字的定义时 `404` 并在 `cause` 指出参数。四条都声明第 29 个 capability `sandbox.read`。名字路由是继 `/v0/runs/{run_id}` 之后的第二条路径参数路由；其字面子路径（`current`、`candidates`，以及 F2 线后面才落的三个：`requests`、`switch`、`assemble`）永不被当作名字读——在 F2b/F2c 服务它们之前一律 `404`。四个 Tauri 命令（`list_sandboxes`、`current_sandbox`、`sandbox_candidates`、`get_sandbox`）已在桌面外壳注册，但**未与界面接线**——那是 D 线。四个 CLI 子命令（`sandboxes list` / `current` / `candidates` / `show <name>`）人类模式打表格，`--json` 原样透传。**F2a-1 报告的那处缺口已闭合**：§5.1 为 31 个查询，词汇表为 29 个名字，且每个名字现在都至少有一条路由。切换已在上方落地（F2b），审批属 F2c，`Task.sandbox` 属 F2d。

- **沙箱注册表已存在：定义、扫描、合并**（v0.9 沙箱 F2a-1）。「沙箱」现在是可命名的：`host-core/src/sandbox_def.rs` 持有 `SandboxDef`（**存储**字段：`name` / `display_name` / `memory_mb` / `qemu_exe` / `toolchain_path` / `kernel` / `notes`，除名字外全部可选），以及 API **服务**的两种形状：`SandboxView`（定义加上只有宿主当场能回答的三项：`source`、`runnable`、`shadowed`）与 `CandidateView` / `CandidatesView`（一个已安装资源；两个互相独立的列表）。`LocalSettings` 新增 `sandboxes` 与 `default_sandbox`，均 `#[serde(default)]`，因此不做任何迁移，写于这两个字段存在之前的文件照样读得进。`AppState` 回答 `sandboxes()`（合并后的注册表）、`sandbox(name)`、`current_sandbox()`、`sandbox_candidates()` 与 `sandbox_default_name()`。合并优先级是手写 → 扫描 → 内置 `default`；同名时手写者胜，被遮的扫描项**留在列表里并标 `shadowed`**，所以合并是可见的而不是悄无声息的。扫描限于 `<data-dir>/toolchain/*` 与 `<data-dir>/qemu/*`（走下载器自己的 `find_compiler` / `find_qemu`，现已 `pub(crate)`，不再长第二份扫描）加上本机 QEMU，且**从不写回 `settings.json`**。`runnable` 是算出来的、从不存储：QEMU 存在且 `--version` 能跑、工具链存在、内核存在或可编译；因此卸掉一个资源只会让定义不可运行，不会让它消失。`Capability` 增至第 29 个名字 `sandbox.read`；四个端点、Tauri 命令与 CLI 属 F2a-2。已报告未修：API 文档 §5 表格仍只列出已有的 28 条路由的 capability，因为 `sandbox.read` 的端点在 F2a-2 才落——计数故意走在表格前面一格。

- **QEMU 与工具链同样接线，而拒绝本身是已定决策**（v0.9 沙箱 F1）。F 侦察发现一处不对称：`toolchain_download` 端到端跑通（宿主方法、端点、事件、CLI），而 `qemu_download` 是**有代码没 caller**。现在 caller 齐了，形状与工具链完全一致：`AppState::begin_qemu_download` / `qemu_download_status` / `cancel_qemu_download` / `download_qemu_now`（另加 `record_qemu_download_event` / `finish_qemu_download` 与 `qemu_dir()`）、审计事件 `host.qemu.download.start|done|failed|cancelled`、SSE 事件族 `qemu:download`、三个 Tauri 命令、三个端点（`GET`/`POST /v0/qemu/download`、`POST /v0/qemu/download/cancel`，capability `qemu.read` / `qemu.configure`）与三个 CLI 子命令（`qemu download [--wait]` / `cancel` / `status`）。**没接线的是「下载」本身**，这是决策：`spec_for_current_platform()` 在每个平台都拒绝（`docs/qemu-distribution.md` §5——RiscDom 引导用户自己安装 QEMU、不 pin 发布版，因为上游不提供 Windows 二进制，而猜一个摘要等于静默的完整性漏洞），所以 `POST /v0/qemu/download` 答 `503 unavailable`、`cause: "qemu"` 并附指引，也不占槽。拒绝之后的东西全部由回环夹具测试覆盖，因此**将来 pin 一个版本只是数据变更**：答案变成 `202`，其余已可用。两个形状合成了一个：`QemuDownloadEvent` 的 payload 标签现在是 `state`（曾是 `kind`），与 `toolchain:download` 同一套词汇；CLI 的 `--wait` 终止判定也变成一个函数（`download_terminal`）两族共用。`docs/control-plane-api.md` §5.1/§5.2 现为 27 查询与 29 控制（原 26/27），`docs/control-plane-events.md` 现为十二个事件（原十一个，§3 表第 12 行），`docs/decisions.md` §29 记录共用的装配形状。两处不对称保留未修（已报告）：`host-tauri` 在工具链下载失败时仍无条件 `eprintln!`（F1 的 QEMU 命令故意不写——C6 的日志开关只覆盖 `server`）；`paths.rs` 只加了 `qemu_dir_in`，没有进程级 `qemu_dir()`，因为工具链那个进程级对应物同样没有任何调用者。
- **接口说实话，库的日志变成可选项**（v0.9 CLI 批次 6/N）。C3/C4 留下的四件小事，合并修完。两条审计导出答的字段叫 `bytes_written`，返回的却是**事件数**（`write_events_jsonl` 返回 `events.len()`）：现在答 `events_exported`；而确实按字节写的 `/v0/serial/export` 保留 `bytes_written`——所以上面批次 5/N 那条读起来就是历史。workspace 策略拒绝的路径此前是 `403 forbidden`，读起来像授权判定，实际却是调用方参数问题：现在是 `400 bad_request` 加 `cause: "path"`，`403` 只留给认证与授权（`http.rs` 的 capability 检查与 `Authn` 钩子——钉这两者的测试未动）。库的运行日志行变成可选：`http.rs` 的 `connection … ended` 与 `accept failed`、`routes.rs` 的 `toolchain download failed` / `preflight failed` 都过 `ServerConfig::with_log_level`——`--log-level <off|error|info>`，**默认 off**——这正是把内嵌服务端挡在 CLI 的 stderr 之外的关键：批次 4/N 报告的粗糙点，现已闭环。`main.rs` 的启动横幅、用法文本与致命错误仍无条件输出：它们是二进制自己的控制台输出，内嵌场景根本不会跑到那个 `main`。另外本节两处失效的 `../host/src/run_diff.rs` 链接也已修正。
- **CLI 线收尾：五批，且每一个控制端点都有了子命令**（v0.9 CLI 批次 5/N）。`docs/control-plane-api.md` §5.2 剩下的十七个端点全部落地——三个导出（`export audit-jsonl`、`export run-audit <run_id>`、`export serial-log`）与十四个按批次 4 定的分组方式归类的管理配置命令（`llm set|clear|load-key`、`qemu path|clear`、`toolchain download|cancel|path|clear`、`preflight run|ack`、`audit alert set <on|off>`、`theme set`、`language set`）。加上批次 4 的十二个，27 个控制端点已全部可从 shell 触及，另加表外的两个（`vm start` → 预留的 `501`、`runs abandon-stale`）。查询半边未动：仍是批次 2/N 的六个只读命令。
  随之而来两件事。**`--api-key` / `--api-key-file` / `--remember`**：模型的 key 可以直接写在命令行（会警告，与 `--token` 完全一样）或从文件读，只有加 `--remember` 才会存进操作系统凭据存储。**`--wait`**：`toolchain download --wait` 与 `preflight run --wait` 在发请求**之前**先订阅 `/v0/events`，打印该工作所属事件族的帧（`toolchain:download` / `preflight:progress`），并以**工作本身**的判定退出——下载失败或预检某一步失败即 `3`。
  途中记下三个事实，每一个都塑造了实现：**`--out` 是服务端的路径**，相对于 workspace 根解析（逃出 workspace 即策略的 `403`），CLI 从头到尾拿不到文件；**两条审计导出的 `bytes_written` 是事件数、不是字节数**（序列日志导出才是字节数），所以人类模式那行会说明是哪种；**`preflight:progress` 没有“结束”事件**，所以 `--wait` 结束于 fail-fast 的第一个 `failed`，或最后一步的 `ok`（步骤表取自宿主自己的 `preflight::STEPS`）。已知粗糙处，只报告未修：内嵌 `--follow`/`--wait` 若在流仍打开时退出，可能把服务端那行 `connection … ended` 留在 CLI 的 stderr 上。
- **CLI 现在能驱动控制平面了**（v0.9 CLI 批次 4/N）。八个只读子命令之外又添十二个控制类子命令：`run <task>`（一轮 agent 的结果）、`vm stop`、`vm start`（预留的 `501`）、`snapshots save|resume|delete`、`sessions create|open|rename|delete|clear-all` 与 `runs abandon-stale`——全部是对控制平面的 HTTP `POST`，没有一个直接摸 `AppState`。随之而来两件事。**确认**：五条销毁状态的命令（`vm stop`、`snapshots resume`、`snapshots delete`、`sessions delete`、`sessions clear-all`）动手前先问——`--yes` 提前回答，终端上弹提示，stdin 不是终端即拒绝（退出码 `2`），因为沉默不是同意。**`--follow`**：`run --follow` 在发起运行**之前**先订阅 `/v0/events`，事件一到就打印（事件名加一小段 payload；带 `--json` 时是原样的 envelope），最后再打结果，因此运行的流从不需要事后补读。`cli/src/sse.rs` 为新增，负责读帧；`client::confirm` 拥有提问；`cli/tests/control.rs` 用真实二进制对着一个未配置模型的控制平面跑，因此不联网、不碰 QEMU。已知粗糙处，只报告未修：内嵌 `--follow` 若在流仍打开时退出，可能把服务端那行 `connection … ended` 留在 CLI 的 stderr 上，排在 CLI 自己的错误体之前。
- **`server` 已纳入门禁 clippy，且已 clippy-clean**（v0.9 CLI 批次 3/N）。该 crate 此前从未被 lint 过——门禁只选可移植 crate 与 `host-core`/`host-tauri`——把 `-p cli` 加进该步骤才暴露了它。6 处 `clippy::result_large_err`（`http.rs:379`、`routes.rs:489/495/503/513/522`）按 lint 的建议修：错误类型改为 `Box<Response<RespBody>>`，所有调用方返回 `*response`——同一个 Response 值，只是装箱了。随之一并修掉的还有 `routes.rs` 测试里 3 处 `bool_assert_comparison` 与 `tests/smoke.rs` 里 1 处 `filter_next`。现在 `cargo clippy -p server --all-targets --no-deps -- -D warnings` 静默通过，门禁步骤选 `-p cli -p server -p host-core -p host-tauri` 并保留 `--no-deps`。行为未变：同样的 `400` 对象、同样的构造、同样的传播路径。
- **CLI 现在是控制平面客户端**（v0.9 CLI 批次 2/N）。新增 `cli` crate（bin `riscdom`），只对控制平面说 HTTP：加 `--remote host:port` 就连已在运行的 `riscdom-server`；不加则在**自己进程内**把控制平面起在 `127.0.0.1:0`（由内核挑端口）——两种模式同一条代码路径，因此 `riscdom` 从不直接调 `AppState`。八个只读子命令（`health`、`status`、`agents`、`runs list|get`、`audit status|events`、`snapshots list`）；`--json` 原样透传控制平面的 JSON，人类模式打表格；退出码 `0`/1/2/3/4（成功 / 本地 / 用法或 400 / 拒绝或 5xx / 401-403）。token 本地取自 `<data-dir>/token`（首次由 `riscdom-server` 同一套代码生成），远程按 `--token-file` > `RISCDOM_TOKEN` > `--token`；从不被打印。锁文件未新增任何 crate，且门禁的 clippy 步骤现在也覆盖 `-p cli`。
- **更名遗留的失效引用已清空**（v0.9 A1 第 5 波）。所有**仍然生效**的旧 crate 提及都已改指真正拥有该物的 crate：`cargo test -p host` → `-p host-core`（测试住在那里）、`host/tests/…` → `host-core/tests/…`、`host/src/state.rs` 等 → `host-core/src/…`、`host/src/commands.rs` → `host-tauri/src/commands.rs`、`host/README.md` → `host-tauri/README.md`、`host::` 前缀按所指对象改为 `host_core::` 或 `host_tauri::`。CONTRIBUTING、SECURITY、PROJECT_CONSTITUTION 与 ci.yml 注释里的 crate 清单现在都写两个 crate，`README.md` 的 crate 索引也列出 `host-core` 与 `host-tauri`。历史未动：CHANGELOG、RELEASE_NOTES、决策账本、本文件 §1 与 architecture-evolution 快照仍按当时的事实使用 `host`。非 Windows 的 clippy 缺口本波**刻意不补**（后来在 v0.9 的 gate 一致性批 B-1 里**部分**补上：`cli`、`server` 与 `host-core` 在 Linux 也 lint；两个 Tauri crate 留 B-2）。
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
  `0.5.0`）。它证明了什么、没证明什么，写在 [RELEASE_NOTES.zh-CN.md](../RELEASE_NOTES.zh-CN.md)。
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
