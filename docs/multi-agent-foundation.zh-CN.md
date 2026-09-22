[English](multi-agent-foundation.md) | 中文

# RiscDom 多 Agent 地基 —— v0.8 已定的形状

快照：v0.8 定下的四项决策的落地状态，**按代码写**，时点为 v0.8 主体交付（批次 1/2 与 2/2）之后。
它记录已经存在的东西和**尚不存在**的东西；缺席的形状就写明缺席，不作暗示。

仓库 `D:\codeagent\breakevery\riscdom`，分支 `main`。它的姊妹文档是
[架构演进说明](architecture-evolution.zh-CN.md)，那份写的是计划；本文件写的是落地结果。

## 1. v0.8 交付了什么

| 批次 | 落地的形状 |
| --- | --- |
| 1 | app data 目录改为**注入**（`AppState::with_data_dir`）；`audit_events.agent_id` 列加在链旁边；每个 `AppState` 一个 VM 槽并用测试钉住 |
| 2 | 多进程可共写同一个 `audit.db`（WAL、`busy_timeout`、`BEGIN IMMEDIATE`、重试），且失败会**响**（`audit:failed` + 横幅 + 弹窗） |
| 3 | `agent_id` 处处有生产者；快照改到 per-agent 子目录并保留读取回退 |
| 4 | 最小派发抽象：`Task`、`AgentHandle`、`Dispatcher`，本地实现，远程一半有意缺席 |
| 5（1/2） | 两个进程：`worker` 执行者二进制 + `host_core::StdioExecutorHandle`；stdio + JSON lines；每执行者独立 data dir |
| 6（2/2） | 监工：一个**非 AI** 派发器，并发驱动多个执行者，输出报告与计数 |

v0.8 **没有**做的事，也不假装做了：AI 监工、远程（跨机器）执行者、preflight 目录隔离、以及无 Tauri
的执行者二进制。

## 2. 决策 1 —— B2：一台机器，多个进程

1. **一个 workspace，一条链。** `audit.db` 位于 `<workspace>/.riscdom/audit.db`，**有意共享**：一条链才能让
   最终的系统回答「谁让谁做了什么」。
2. **多写者是安全的。** 连接以 WAL 打开，带 5 秒 busy timeout 与 `synchronous=NORMAL`；一次 append 在读取
   head **之前**先拿写锁（`BEGIN IMMEDIATE` —— 没有它，两个写者会链到同一行并分叉链，这正是 v0.8 批次 2
   的并发测试抓到的）；被锁的 append 以 20/40/80/160 ms 退避重试 5 次。`AuditSink::record` 返回 `Result`；
   host 把失败变成 `audit:failed` 事件、日志行，以及（默认）横幅与弹窗。
3. **每个进程拥有自己的私有状态。** `AppState::with_data_dir(workspace, data_dir)` 把 `settings.json`、
   `sessions.db` 与工具链目录解析到 `data_dir` 之内，因此同一 workspace 的两个进程绝不共享它们。审计库按设计
   仍留在 workspace 下。
4. **监工与执行者是两个进程。** 执行者是 `worker` 二进制；监工是派发器（见 §5）。传输是 **stdio + JSON
   lines**：零新依赖，且子进程死掉表现为 EOF 而不是挂住的读。
5. **已知代价已偿付。** `worker` 此前依赖 `host`，于是执行者二进制会链接 Tauri——而它既不需要
   `AppHandle` 也不需要窗口。自 v0.9 A1 第 3 波起它依赖 `host-core`，执行者二进制因此完全不链接任何 Tauri crate（§7）。

## 3. 决策 2 —— agent 身份：`<device>-<pid>-<seq>`

1. **发放。** `agent::next_agent_id()`（`agent/src/identity.rs`）拼出 `DEVICE`（机器；现阶段 `local`）、
   `std::process::id()` 与一个进程级计数器：`local-12345-1`。两个进程不可能同 pid，同一进程内两个 agent
   不可能同计数器值。
2. **生产者。** `AgentLoop` 在构造时接收身份并盖在它写的每条事件上；`audit_hook` 的五个助手与工具层同样
   接收；host 把它为 `AppState` 领到的身份盖在自己的事件与 `run.start` / `run.end` / `run.abandoned`
   标记上。
3. **在链旁边，不在链里。** 哈希公式（`prev|ts|actor|action|detail`）、`prev_hash` 链接、每一行既有的
   `hash` 与 append-only 触发器全部不动。旧行为 `NULL`。JSONL 导出与 `list_audit_events` 都会带上它。
4. **监工看得见什么、看不见什么。** `TaskOutcome` 携带的是任务**被寻址到**的那个 `agent_id`（执行者标签），
   遵循派发接口的语义。子进程自己的身份是经它的**事件流**（以及它自己的审计库）回来的，不在 outcome 里。
   见 §7 第 1 条。

## 4. 决策 3 —— 快照按 agent 隔离

1. 新快照写入 `<workspace>/.riscdom/snapshots/<agent_id>/`，于是共享一个 workspace 的两个 agent 都能保存
   `snap1` 而不互相覆盖。
2. 读取回退到共享根目录（`<workspace>/.riscdom/snapshots`），因此 v0.8 之前拍下的快照仍可列出、恢复与删除；
   同名时以 per-agent 的为准。
3. 这与机群给 data 目录用的是同一招：该共享处共享（链、workspace），不该共享处私有（快照、settings、
   sessions、工具链）。

## 5. 决策 4 —— 派发抽象

1. **位置。** `agent::dispatch`（`agent` crate）：不依赖 Tauri，因此该抽象不强迫做 host-core /
   host-tauri 拆分。host 与 worker 都建立在它之上。
2. **类型。** `Task { id, target, input }`、`TaskId`（`task-<pid>-<seq>`）、`AgentId`、`TaskOutcome`、
   `DispatchError::{NoSuchAgent, Failed}`。全部可序列化（线上需要的派生由 v0.8 主体交付 1/2 补上）。
3. **trait。** `AgentHandle`（「跑这个任务，返回结果」）与 `Dispatcher`（「把任务变成结果」）。**远程一半有意
   缺席** —— 另一个进程或另一台机器上的执行者只需实现 `AgentHandle`，即可接进同一个分发器。
4. **已存在的实现。** 本地：`agent::LocalAgent`（包一个 loop）、`agent::LocalDispatcher`（按
   `Task.target` 路由）、`host_core::HostAgentHandle`（host 自己的 `run_agent` 路径）、以及
   `host_core::StdioExecutorHandle`（经 stdio 的子进程）。
5. **路由规则。** 任务自己声明要哪个执行者；声明一个不在机群里的执行者会被 `NoSuchAgent` 拒绝。绝不发给
   「猜一个」的执行者 —— 发错执行者比不发更糟。
6. **并发。** `AgentHandle: Send + Sync`，分发器持有 `Arc<dyn AgentHandle>`，因此一个分发器可跨线程共享。
   监工用 `std::thread::scope` 每任务一线程地派发：多个执行者同时干活，一个慢的执行者不拖住其余，且不需要
   线程池依赖。

## 6. 端到端的原型

1. **执行者**（`worker`，`worker/src/main.rs`）：stdin 上读**一行** `Task` JSON，构造
   `AppState::with_data_dir`，跑 host 自己那条 `run_agent` 路径，在 stdout 上写**一行** `TaskOutcome`
   JSON，事件以 JSON 行写到 **stderr**（于是 stdout 不必过滤即可解析）。用法：
   `worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]`。
2. **退出码。** 只要写出了 outcome 就是 `0` —— 失败的一次运行也是答案，不是错误；用法错误退 `2` 且不写
   stdout 行，由监工报协议失败。读不出来的任务用文档化的占位 id `task-unparsed` / `unparsed` 作答。
3. **监工**（`worker::supervisor`，由 `worker/examples/dispatch.rs` 驱动）：

   ```text
   cargo build -p worker
   cargo run  -p worker --example dispatch                     # 两个执行者，各一个演示任务
   cargo run  -p worker --example dispatch -- --executors 3 --tasks tasks.jsonl
   ```

   未配置 LLM 的执行者会答 `Failed`（readiness 检查先拒绝），这正是离线演示所展示的：路由、并发、汇总 ——
   而不是一次模型调用。
4. **任务清单**是 JSON lines，一行一个 `Task`，与执行者协议同一套分帧。某一行坏掉会让整份清单失败，并报出
   它的行号。

## 7. 交给 v0.9 的未结项

1. **子进程的身份到不了 `TaskOutcome`。** `AgentHandle::run` 返回 `AgentOutcome`，因此 `LocalDispatcher`
   盖的是被寻址的 target。有两条收口路径：给 `TaskOutcome` 加一个 `executor` 字段，或让
   `AgentHandle::run` 返回子进程的 `TaskOutcome`（后者会改动已发布的 trait 签名，所以 v0.8 只报告不做）。
2. ~~**`tauri` 不是可选的**，因此执行者二进制会链接它。~~ **已闭环**（v0.9 A1 第 3 波）：执行者依赖 `host-core`，是 crate 拆分——而不是 feature 开关——移除了这条链接。保留于此，作为当初代价的记录。
3. **preflight 目录**（`<workspace>/.riscdom/preflight`）在同一 workspace 的进程间仍**共享且无锁**；可以像
   快照那样隔离，也可以接受 last-writer-wins。
4. **没有 AI 监工。** 这一阶段的监工是派发器而不是 agent：没有 LLM 循环、没有提示词、没有预算策略。
5. **没有远程执行者。** trait 的缝在；除本地 stdio 句柄外，没有任何实现面向另一个进程或另一台机器。
6. **会话 DB 没有 WAL 与 busy timeout。** 在每个进程各自拥有 data 目录时无害；若将来会话变为共享，值得再看。
7. **临时目录代码从不清理。** 测试与演示会在系统临时目录留下 `riscdom-*` 条目；
   `scripts/clean-temp.ps1` / `clean-temp.sh` 负责清理（默认 dry-run，`-Force` / `--force` 才删）。手工调试
   的一次性产物（比如 `wdbg-*` 前缀）不在该过滤器范围内，由操作者自行删除。
