[English](README.md) | 中文

# worker

两进程原型的**执行者进程**，以及驱动一整队执行者的监工半边（v0.8 主要交付物 1/2 与 2/2）。

一任务进，一结果出。二进制把单个 `Task` 作为 **stdin 上的一行 JSON** 读入，走宿主自己的 `AppState::run_agent` 路径——与桌面应用跑的是同一条路径，而不是第二套驱动 agent 的实现——然后把一个 `TaskOutcome` 作为 **stdout 上的一行 JSON** 写出。它的事件（JSON lines）走 **stderr**，因此 stdout 保持为一条纯协议通道，监工无需过滤即可解析。

它依赖 [`host-core`](../host-core/README.zh-CN.md)，即内核门面的可移植半边，因此这个二进制**不链接任何 Tauri crate**（`cargo tree -p worker` 里一个都没有）。v0.9 A1 第 3 波之前它依赖 `host`，从而继承了 Tauri 这一已知代价。

## 运行

```text
worker --workspace <dir> --data-dir <dir> [--sleep-ms <n>]
```

| 参数 | 含义 |
|---|---|
| `--workspace <dir>` | 必填。本执行者的工作区；审计链位于 `<dir>/.riscdom/audit.db`。 |
| `--data-dir <dir>` | 必填。本执行者的 `settings.json`、`sessions.db` 与工具链目录所在。 |
| `--sleep-ms <n>` | 诊断钩子（测试与人工计时）：读完任务后先等这么久再作答。不改其它行为。 |

`--workspace` 与 `--data-dir` **刻意没有环境变量回退**：执行者的身份就是它的命令行，因此共用同一工作区的两个执行者仍各有各的状态，也不会有被继承的变量让两个 worker 悄悄撞在一起。

**退出码。** 只要写出了 `TaskOutcome` 就是 `0`——包括失败的运行，因为结果是答案、退出状态不是。用法错误退出 `2` 且不向 stdout 写任何行；监工把它报告为协议失败。请求畸形也仍有应答：结果里带着稳定的占位身份 `task-unparsed` / `unparsed`，于是监工得到的是「你的任务读不出来」，而不是苦等一行永不到来的输出。

## 监工半边

库目标（`src/lib.rs`、`src/supervisor.rs`）是一个**非 AI 派发器**：读一份任务清单，按 `Task.target` 把每个任务路由到它点名的执行者，并汇报回来的东西。里面没有任何模型。路由是显式的、从不猜测——点名了不在队伍里的执行者会被拒绝（`DispatchError::NoSuchAgent`），而不是随手丢给某个空闲的执行者。

可运行示例：

```text
cargo build -p worker                       # 执行者二进制必须先存在
cargo run  -p worker --example dispatch     # 两个执行者，各一个演示任务
cargo run  -p worker --example dispatch -- --tasks tasks.jsonl --executors 3
```

`--executors <n>`（默认 2）个执行者**共用一个工作区**——一条审计链、按 agent 的快照——但各自有**自己的 data dir**。`--base <dir>` 指定这些目录的位置，`--worker <path>` 覆盖执行者二进制，`--tasks <file>`（或 `-` 读 stdin）给出一份 JSON-lines 任务清单；不给就由示例自行编出每个执行者一个任务。没有配置 LLM 的执行者会回 `Failed`：这个示例展示的是管道，不是模型。

## 测试

```text
cargo test -p worker
```

- `tests/stdio.rs` —— 进程边界，对着**真实的** `worker` 二进制（`CARGO_BIN_EXE_worker`）：任务跨过边界并以结果回来；畸形任务得到失败而非崩溃；起不来的 worker 被报告而不是挂住；永不作答的 worker 被杀掉并报告；两个 worker 各自保有自己的 data dir。
- `tests/supervisor.rs` —— 路由与计数：每个执行者标签都被登记；不在队伍里的执行者被以 `DispatchError::NoSuchAgent` 拒绝；空计划报空计数；拒绝与中断分开计数；任务清单是一行一个 JSON 对象。

worker 在测试里无法*跑完*一轮——它没有配置 LLM，QEMU 也从不在这里调用——因此子进程回的是宿主报告的失败，而这就是断言：拒绝以一个完好成形的结果抵达。完整运行的行为仍由 [`host-core`](../host-core/tests) 自己的测试在进程内覆盖。
