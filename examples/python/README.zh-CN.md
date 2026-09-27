[English](README.md) | 中文

# examples/python — 参考监工，以及一个调度员

`dispatch.py` 是 RiscDom 自带的最小完整**监工**：一个不在内核里、自己不持有模型、通过 HTTP 上的控制平面驱动一个节点的进程。它就是 AI 监工包在模型外面的骨架——工具调用就是控制平面的端点（[`docs/tool-schema-control-plane.zh-CN.md`](../../docs/tool-schema-control-plane.zh-CN.md)），下面的请求就是一次工具调用会变成的东西。

`supervisor.py` 是另一种形状：同一个客户端外面套了一个**循环**——这正是 AI 调度员（纲领里的 **M**）作为一个程序的样子。下文有它自己的一节。

**只用标准库。** 没有 `requests`、没有 `httpx`、没有 SSE 库：`urllib.request`、`json`、`argparse`。参考实现不该教人一个它并不需要的依赖。

## 它做什么

三个端点，按监工遇到它们的顺序：

| 步骤 | 端点 | 为什么 |
|---|---|---|
| 1 | `GET /v0/executors` | 可以问谁。空列表是节点在说它没有队伍。 |
| 2 | `POST /v0/tasks` | 一条任务、一个执行者、一个 `TaskOutcome`——**同步**，与 `POST /v0/agent/run` 完全一样。没有任务表可轮询。 |
| 3 | `GET /v0/events`（`--follow`） | 干活时的事件流。 |

任务靠它的 `target` 路由：正是这一点让 `/v0/tasks` 与 `/v0/agent/run`（在节点自身上跑）成为两回事。目标无人拥有会被拒绝——`404`、`cause: "target"`——脚本把这报成 **refused**，而不是一次失败运行。

## 开始之前

1. **一个节点，带队伍。** 在它的 `settings.json` 里加 `executors`（见 [`server/README.zh-CN.md`](../../server/README.zh-CN.md) 与 [`docs/decisions.zh-CN.md`](../../docs/decisions.zh-CN.md) 的 E0 决策 §39）——每项是一个 label、一个 program 与它的参数，program 通常就是 `worker` 二进制：
   ```json
   { "version": 1,
     "executors": [ { "label": "executor-0", "program": "../target/debug/worker",
                      "args": ["--workspace", "./ws", "--data-dir", "./data-0"] } ] }
   ```
2. **一个控制平面。** 要么 `riscdom-server`，要么 CLI 的本地模式（`riscdom health` 会起一个然后退出——监工请用 `riscdom-server`）。
3. **一个 token。** `--token-file <path>` 或 `$RISCDOM_TOKEN`。默认是服务端的 `<data-dir>/token` 文件。**token 绝不作为命令行参数**：参数会落进 shell 历史与进程列表，CLI 自己的 `--token` 会为此告警。

## 跑起来

```bash
# 一条任务、一个执行者（队伍里先碰上的那个）。
export RISCDOM_TOKEN="$(cat ./data/token)"        # 或 --token-file ./data/token
python dispatch.py --target executor-0 "build the blink example"

# 一份任务清单，在队伍上轮询路由，事件流走 stderr。
python dispatch.py --tasks tasks.jsonl --follow

# 同一件事，对着别处的服务端，每条任务声明一个沙箱。
python dispatch.py --server 10.0.0.7:7821 --tasks tasks.jsonl --sandbox blink
```

`tasks.jsonl` 每行一条任务（与 worker 监工读的 JSON-lines 同形）：

```jsonl
{"target": "executor-0", "input": "compile the blink example"}
{"target": "executor-1", "input": "read the serial buffer and summarize it", "sandbox": "blink"}
```

参数：`--server`（默认 `127.0.0.1:7821`）、`--token-file`、`--tasks <file|->`、`--target`、`--sandbox`、`--follow`、`--timeout`、`--json`、`--self-test`。选项之后的裸参数是任务文本，此时只有不写 `--target` 才需要——不写就按队伍轮询。

**退出码**（CLI 的约定）：`0` 每条任务都以成功作答，`1` 至少一条没有（被拒、断掉、或一次失败的运行），`2` 用法错误，`3` 控制平面不可达或拒绝了这个凭据。

## 离线自证

```bash
python dispatch.py --self-test      # 批处理客户端
python supervisor.py --self-test    # 调度员
```

两者都不需要服务端、worker，或除回环外的任何网络：各自在 `127.0.0.1:0` 起一个自己的**假**节点（stdlib `http.server`），并对它跑真实的代码路径。

`dispatch.py` 断言：队伍列表、一个成功结果、一个失败结果、一个被拒的目标（`404` 带 `cause: "target"`）、`--follow` 的订阅、错 token 触发凭据错误、以及一行畸形任务被当作用法错误。`supervisor.py` 断言：状态快照真的读了它声称的每一源；空闲的一轮**不发**任何控制请求；读失败会终止这一轮且不发生任何控制请求、而下一轮能恢复；动作表里每个工具都打到自己的端点；事件读取记住最后一个 `id` 并用 `Last-Event-ID` 续订；以及除标准库外什么都没 import。`scripts/gate.sh` 在 `PATH` 上有 Python 解释器时会跑这两步（没有就打印一条 skip）。

## 调度员：`supervisor.py`

`dispatch.py` 是被塞一份任务清单然后发出去。调度员是**常驻**的：读节点的状态，决定要不要动手：

    读状态（一次快照）  →  决策  →  行动  →  报账

**决策层是一个有上限的工具调用循环，而且它不自带策略。** 模型拿到节点的状态与控制平面自己的工具 schema，然后靠**调用工具**来决策 —— 由进程去发那次 HTTP 请求、把 JSON 交回去，也就是客户指南 §8 描述的那个循环。有两件事刻意留给你：**策略**（什么样的决定算好）与**模型**。不给 `--llm-model` 就没有模型，M 什么都不决定 —— 那是保守的默认，而不是降级模式。

```bash
python supervisor.py --agent-id m-admin --once         # 一轮，不给模型：什么都不决定
python supervisor.py --agent-id m-admin --interval 30  # 常驻，每 30 秒一轮
python supervisor.py --agent-id m-admin --once --llm-base-url http://127.0.0.1:11434/v1 \
    --llm-model qwen2.5:7b                             # 一轮，带模型（这里用 Ollama）
python supervisor.py --agent-id m-admin --events       # 只要事件流（只读）
python supervisor.py --self-test                       # 离线：假节点**加**假模型
```

以上每一条都需要 **`--agent-id`**（或 `$RISCDOM_AGENT_ID`）：M 把它的名字作为 `X-RiscDom-Agent` header 发在**每一个**请求上，而节点会把这个名字写进该调度员引起的每一行审计（`m.sandbox.spawn`、`m.task.dispatch` ……）。它是**必填**而不是默认值，因为「没有名字的调度员」正是当初让 M 在链上不可见的原因：带上 header，行里写的是**谁要的**；不带，行里写的是节点。

M 的模型是 **M 自己的**，不是节点的：`--llm-base-url`、`--llm-model`、`--llm-api-key-file`（或 `$RISCDOM_LLM_API_KEY`）从命令行与环境读，从不取自节点的 `llm_configs`。节点的模型跑**任务**；M 的模型决定**哪些任务**。`--max-rounds`（默认 6）限制一轮里允许的调用数，所以一个一直要工具的模型会结束这一轮，而不是永远跑下去。

`--server` 与 `--token-file` 的含义与上文相同，退出码也是同一张表：`0` 这一轮完成了（动了手，或确实无事可做），`1` 这一轮没完成 —— 状态读不到，或决策层失败，而 M **没有进一步**动手 —— `2` 用法错误，`3` 控制平面不可达或拒绝了这个凭据。

`import dispatch` 是故意的：传输、token 规矩与错误分类已经写过一遍，再抄一份线格式就是多一样要维持为真的东西。Python 并不总是把脚本自己的目录放进 `sys.path`（`-P`、`PYTHONSAFEPATH=1`），所以文件里显式加了它——两行，带理由。

### 工具，以及刻意不给的工具

M 的工具清单在启动时**读** [`docs/tool-schema-control-plane.zh-CN.md`](../../docs/tool-schema-control-plane.zh-CN.md)（`--tool-schema` 指向它），并过滤出调度员应有的十八个：它决策所依据的状态（`status`、`capabilities`、`executors`、`sandboxes`、`instance_list`、`sandboxes_requests`、`runs`、`run_get`、`audit_status`、`audit_events`、`vm_status`）与属于它自己的动作（`tasks`、`instance_create`、`instance_delete`、`sandboxes_switch`、`sandboxes_requests_post`、`sandbox_request_approve`、`sandbox_request_reject`）。白名单里的名字若文档没有定义，那是**启动报错**，不是一条安静的短清单。

有三条排除值得点名：

- **不给 `agent_run`。** 它在**本节点**用执行者自己的工具跑一轮 agent —— 客户指南 §8 的第一条铁律就是「监工不是执行者」。
- **不给 `events`。** 事件流是长开的，所以它是上下文（`--events`），而不是一个永不返回的工具调用 —— 指南的第三条铁律。
- **不给导出、导入、快照与 `vm_*`。** 它们在节点上写文件或搬它的运行时；M 决定的是**什么在哪里跑**，仅此而已。

### system prompt 与状态

`SYSTEM_PROMPT` 是骨架，不是规则手册：M 是谁（调度员不是执行者）、它在看什么（第一条 user 消息里的节点状态）、怎么决定（先读、选最小的动作、看不全就不动），以及它不能弯的约束（只能通过这些工具行动；工具报了错就是告诉了你一件事）。**策略** —— 哪个执行者、哪个沙箱、批谁的请求 —— 由你来写，写在你的 prompt 里或者包一层 `LLMDecider`。

状态以 **JSON** 出行（`snapshot()`，键排序）：无损，而且它的形状早被工具 schema 记下来了，所以不用再养第二份描述。

### 离线自证决策层

self-test 会起一个假节点**和一个假模型**：一个按脚本作答的服务器，先回一条 `tool_calls`，再回一条收尾消息。它断言：快照与十八个工具真的出行了；工具调用被执行、结果被回喂；M 从未提供过的工具**不会**被执行；模型失败或不可达会以**零**控制请求结束这一轮；而一个永不停下要工具的模型会撞上 `--max-rounds` 而不是无限循环。

### 已知边界

写出来而不是藏起来。M2c 的侦察逐条找到它们；改动其中任何一条都是内核批次，不是这个文件：

- **M 在链上没有自己的身份。** token 客户端以 `operator` 行事，所以 M 的行与人的行长得一样，而节点自己写的 `m.sandbox.*` 行带的是 `actor: "host"`。
- **一次决定不写进链。** `approve` / `reject` 只发一条 `sandbox:request` 事件，不耐久地记任何东西。
- **实例表与待批槽是内存里的。** 节点重启两者都丢；审计链是唯一的耐久源。
- **审计读取没有窗口也没有分页。** `GET /v0/audit/events` 只收 `limit`（必填）、`actor`、`action_prefix`，所以「从 X 以后的一切」是一次导出，不是一次查询。
- **五个能力名只是词汇**（`task.dispatch`、`task.dispatch.remote`、`sandbox.instantiate.remote`、`audit.read.remote`、`request.approve`）：没有任何路由要求它们。派发要 `agent.run`；派生实例要 `sandbox.instantiate`；决定一个请求要 `sandbox.read` **加上**该请求自己的动作所隐含的那个能力。
- **工具清单不是新文档。** M 的工具就是 [`docs/tool-schema-control-plane.zh-CN.md`](../../docs/tool-schema-control-plane.zh-CN.md)——与客户指南 §8 交给监工的是同一份「每个端点一个函数」的清单。

| | `dispatch.py` | `supervisor.py` |
|---|---|---|
| 形状 | 批处理：一份任务清单进，一份报告出 | 循环：状态进，一轮出去 |
| 决策 | 没有——`--target` 或轮询 | 一个模型，在十八个工具上 |
| 写 | `POST /v0/tasks` | 派发、实例、切换、待批槽 |
| 读 | 队伍 | status / 能力 / 沙箱 / 实例 / 待批请求 / 链 |
| 事件 | `--follow`，不续订 | `--events`，用 `Last-Event-ID` 续订 |
| 复用 | —— | `dispatch.py` 的传输、token 规矩与错误 |

## 与 `worker/examples/dispatch.rs` 对照

两者都是监工；它们是同一幅画的两半，谁也不替代谁。

| | `worker/examples/dispatch.rs` | `examples/python/dispatch.py` |
|---|---|---|
| 语言 | Rust，cargo example | Python 3，仅标准库 |
| 怎么到达执行者 | 直接，作为子进程（stdin/stdout） | 经节点的控制平面（HTTP） |
| 需要跑着的节点 | 不需要——它自己起执行者 | **需要**——队伍归节点所有 |
| 需要 token | 不需要 | 需要（控制平面是要鉴权的） |
| 并发 | `dispatch_all` 并行跑队伍 | 一条一条，按顺序 |
| 展示什么 | 任务协议与进程边界 | HTTP 表面与事件流 |
| 任务来源 | `--tasks <file>` / stdin（JSON lines） | `--tasks <file>` / stdin（JSON lines） |

## 这不是什么

- **不是 AI。** 这里没有模型；它展示的是监工的模型会架在上面的那套管道。
- **不是 `agent` 工具的客户端。** 执行者的模型可以调用的八个工具（[`docs/tool-schema-executor.zh-CN.md`](../../docs/tool-schema-executor.zh-CN.md)）在执行者内部，从这里够不着。
- **不是调度器。** 任务一条一条按顺序发出，脚本等每一次应答。真正的监工会让它们重叠；这里有意思的是那份契约，而一个队列会把契约藏起来。

关于 `--follow` 的一处实话：信封里的 `task_id` 对宿主事件是 `null`，所以一帧无法归因到造成它的那条任务——帧上的 `agent_id` 是跑它的执行者。需要归因的监工应当读审计链（`GET /v0/audit/events`，以及那些 `agent.file.write` 行），那里记着 actor。
