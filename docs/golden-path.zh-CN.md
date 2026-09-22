[English](golden-path.md) | 中文

# 黄金路径 —— v0.5 设计提案

> **状态：提案（v0.5 批次 0）。** 本批次不写代码。§2 是侦察：从本仓库里读出来的事实，每条都附上出处
> 文件。§3–§7 每一项都给出**推荐方案、备选方案与代价**。§8 列出只能由项目所有者拍板的决定。

## 1. 黄金路径是什么

v0.5–v0.6 的核心主线，按 `PROJECT_CONSTITUTION.md` §10 现在的写法：让一个从没见过本仓库的开发者，从一台
干净机器走到「比对过的重新运行」。

```text
安装 → 创建环境 → 跑 Agent 任务 → 存 snapshot →
得审计记录 → rollback → 换配置重跑 → 比较两次结果
```

前七步**手动完成**属于 **v0.5**；第八步（自动比较）属于 **v0.6**，本提案明确不做。

## 2. 现在已有什么

### 2.1 安装

两个环境依赖，都在「设置 → 工具链」里处理：

- **RISC-V GCC** —— 自动探测（`probe_toolchain`：`RISCDOM_RISCV_GCC` / `RISCV_GCC` → 常见路径 →
  `PATH`）、原生文件对话框手动指定，或一键下载 xPack 版本（`start_toolchain_download`，带进度与取消）。
- **QEMU** —— 自动探测与手动指定（`probe_qemu` / `set_qemu_path`）。自 v0.4 #4 起**不下载**：应用只把用户
  引向 `winget` 或官网（[qemu-distribution.md](qemu-distribution.md) §5）。

Rust / Node 工具链是**开发者**的前置条件，不是用户的前置条件 —— 它属于构建，不属于黄金路径。

### 2.2 创建环境

今天**没有「环境」这个一等对象**。存在的是配置加一次检查：

| 组成 | 位置 | 后端 |
|---|---|---|
| 服务商 + key + 模型 | 「设置 → 模型」 | `get_provider_presets`、`probe_local_llm`、`set_llm_config`、`has_stored_key` / `load_stored_key`、`get_llm_readiness` |
| 编译器路径 | 「设置 → 工具链」 | `probe_toolchain` / `set_toolchain_path` / `clear_toolchain_path` |
| QEMU 路径 | 「设置 → 工具链」 | `probe_qemu` / `set_qemu_path` / `clear_qemu_path` |
| 能力检查 | 「设置 → 工具链 → 环境预检」 | `preflight_status` / `run_preflight` / `acknowledge_preflight` |

key 可以存在操作系统钥匙串里；路径与预检缓存存在应用数据目录的 `settings.json`。预检会编译一个四行
guest 并在真实路径上启动它，报告四步中哪一步失败，并记录一次「仍要继续」（[preflight.md](preflight.md)）。

### 2.3 跑 Agent 任务

宿主的 `run_agent(user_input)`：铸造 run id、计算配置指纹、写入 `run.start`、跑 Agent 循环（编译 → 启动
VM → 读串口 → 迭代）、写入 `run.end`，返回结果（`final` / `max_iterations` / `failed`）与流式对话。

### 2.4 存 snapshot

| 动作 | 命令 | 说明 |
|---|---|---|
| 存 | `save_snapshot_real(name)` | 需要**宿主持有的运行中 VM**；文件落在 `.riscdom/snapshots/<name>.mig`（`tcp-relay`）。更早的 `reboot-fallback` `.json` 快照仍会被列出。 |
| 列 | `list_snapshots()` | 名称 / 大小 / 时间 / 模式 —— 即「设置 → 快照」里的列表。 |
| 恢复 | `resume_from_snapshot_real(name)` | 先停当前 VM，再通过 `-incoming tcp:` 推流。 |
| 删 | `delete_snapshot(name)` | |

取名用的是 `window.prompt` —— 这是最后一个还在用文本输入框的路径类取值（工具链与 QEMU 早已用原生对话框）。

恢复**自成一个 run**：`run.start` 记录 `resumed_from_snapshot`，`parent_run_id` 指向本进程内最近一次 run
（重启后为 `None`）。这个链接是**单向**的：run 知道自己恢复了哪个快照，快照不知道自己是哪次 run 产生的 ——
`snapshot_producers` 是内存里的映射，重启即失。

### 2.5 得审计记录

- 「设置 → 审计」显示事件数、链状态（`Intact` / `Broken`）、按 actor 过滤、最近事件列表，以及一个**只读的
  run 列表**（状态、指纹短码、时间、parent）。
- `export_audit_jsonl(path)` **存在且可用** —— 它把每个事件按 id 序写成 JSONL，行含 `id` /
  `timestamp_ms` / `actor` / `action` / `detail` / `prev_hash` / `hash`，外部工具可以据此重新验链 ——
  但**UI 里没有任何地方调用它**。UI 唯一提供的导出是串口日志（`exportSerialLog`，控制台面板在用）。
- `audit-verify <path-to-db> [--runs]` 是那个独立校验器：只读，打印 `Intact { length: N }` 或
  `Broken { at_id, reason }`，退出码 `0` / `1` / `2`。重建派生索引是**另一个**二进制
  （`audit-rebuild`），有意不放进校验器。

**缺口**不是「没有导出」，而是这个够不着的导出**没有按 run 划界**：

- `RunRecord`（在 `audit` 里）带着 `start_seq` / `end_seq`，即该 run 的审计 id 区间 —— 区间在链上是存在的。
- `RunView`（UI 看到的形状）**丢掉了** `start_seq`、`end_seq` 与 `fingerprint_schema`。
- `EventFilter` 只按 actor、action 前缀与时间戳过滤，**不能按 id** —— 所以今天的 `list` 也表达不出
  「这个 run 的区间」。

### 2.6 rollback

`resume_from_snapshot_real` 加上能在端口冲突后活下来的重试。详见 §2.4。

### 2.7 换配置重跑

要紧的东西都已经在指纹文档里（[run-provenance.md](run-provenance.md) §1.2）：`schema`（指纹 schema、应用
版本）、`llm`（provider、base URL、model —— 绝不含 key）、`vm`（内存、machine、cpu、QEMU 路径 + 实测版本、
快照模式）、`toolchain`（解析到的 GCC 路径 + 版本、发现来源）。改动其中任何一项再跑一次，确实会得到不同的
指纹，两次 run 也都会出现在 run 列表里。**缺的是可读性**：没有任何地方显示两次 run **差在哪些字段**。
（v0.5 后续已为每个 run 显示指纹、为恢复产生的 run 显示来源快照，并支持两个 run 并排对照 —— 见
下文 §7/§8；逐字段比对仍属 v0.6。）

### 2.8 已经走通一部分路径的东西

| 产物 | 覆盖 | 运行方式 |
|---|---|---|
| `host-core/tests/e2e_ui.rs` | 第 3 步端到端（编译 → VM → 串口 → 审计），用 **mock LLM**，并打印 5c-3 失败报告 | `--ignored` |
| `host-core/tests/diagnosis/mod.rs`（+ `run_diagnosis.rs`） | 报告本身，措辞被钉住 | 常规 |
| `sandbox/tests/snapshot_real.rs`、`snapshot.rs` | 快照的**真实**存 / 恢复，带 QEMU | 常规 / ignored |
| `host-core/tests/snapshot_commands.rs`、`run_provenance.rs`、`qemu_path_snapshot.rs` | host 层的快照命令、run 记录、恢复使用所配置的 QEMU | 常规 |
| `audit/tests/verify_bin.rs`、`run_verify_cli.rs`、`run_rebuild_cli.rs` | `audit-verify` / `audit-rebuild` 的 CLI 行为 | 常规 |
| `agent/tests/real_api.rs` | 真实 API key 的端到端 | `--ignored` |

**没有任何东西把七步串起来。** 从「存一个快照」经过「导出记录」到「换配置再跑一次」，没有测试、脚本或
清单覆盖过。

## 3. 七步的逐步定义

每步：输入是什么、用户做什么、之后必须成立什么、以及它会怎么失败。

| # | 步骤 | 输入 | 动作 | 预期结果 | 失败模式 |
|---|---|---|---|---|---|
| 1 | 安装 | 一台 Windows 机器 | 装 QEMU（按引导）、让应用找到编译器（或下载它） | 编译器与 QEMU 都解析到，路径记进 `settings.json` | 什么都没找到 → 应用列出搜索过的每个位置；已设置但文件不存在是**报错**，不是回退 |
| 2 | 创建环境 | LLM key（或本地服务） | 选服务商、粘贴 key、跑预检 | 服务商/模型已保存，readiness 为真，预检四步全绿 | key 不对 → readiness 说明原因；预检失败 → 点名步骤并给建议 |
| 3 | 跑 Agent 任务 | 一句自然语言请求 | 发送 | 一次带 id、指纹与区间的 run；guest 被编译、启动、读取 | `max_iterations` / `failed` → 诊断报告点名第一个失败步骤 |
| 4 | 存 snapshot | 一台运行中的宿主 VM | 取名、保存 | `.riscdom/snapshots/<name>.mig`，列表中带大小与时间 | 没有运行中的 VM → 按钮禁用并说明原因；保存失败 → 报 QEMU/中继错误 |
| 5 | 得审计记录 | 一次已结束的 run（含被链标记为 abandoned 的） | 在 run 列表里**导出它** | 一个能**独立**验证的文件（空库 + `audit-verify`）：从链的第一条事件起，到该 run 的 `run.end`（或其 `host.run.abandoned` 标记）为止 | 链已断 → 导出不得假装没断 |
| 6 | rollback | 一个快照 | 恢复 | 一次**新的** run：`parent_run_id` 指向上一次，`run.start` 点名快照 | 端口被占 / QMP 重置 → 重试三次后报错；快照缺失 → 在停 VM 之前就被拒 |
| 7 | 换配置重跑 | 指纹里的任意一个字段 | 改它，用同样的请求再跑 | 第二次 run，同一请求，**不同指纹** | 差异不可读：用户看到两个 64 位十六进制串，而不是「只改了 QEMU」 |

## 4. 审计导出

**推荐：按「一次 run 的记录」导出，格式就是纯事件 JSONL，落到工作区，由审计页的一个按钮触发。**

- **导出什么**：从链的**第一条事件**起到**结束该 run 的那条事件**为止 —— 其中就包含那次 run 的
  `run.start`（带着 `fingerprint_schema` 与被哈希的规范 JSON 文本 `fingerprint_json`）与 `run.end`，
  以及它所处的上下文。不新增：run 的元数据**本来就在**被导出的事件里，所以文件能自解释，并且仍可
  逐行验证。被链标记为 **abandoned** 的 run 没有 `run.end`，它的记录改以该 run 的
  `host.run.abandoned` 事件结尾，所以文件的最后一行仍然说明了它为何停下。
  *（v0.5 批次 4：导出从 genesis 而非 `run.start` 开始 —— 只有如此，从 genesis 链接起步的
  `verify_chain` 才能在**一个空数据库**里判定该文件。切片形式被移除而非并列保留：「导出」有两种含义
  就是一种太多，值得留下的那种要能自己回答「这份记录是否完整」。）*
- **什么格式**：`export_jsonl` 已经在写的那个 JSONL，原样不动 —— 每行 `id` / `timestamp_ms` / `actor` /
  `action` / `detail` / `prev_hash` / `hash`，按 id 序。读者无需本应用即可逐行重算哈希并检查链接。
- **导出到哪**：工作区下的一个默认路径，用原生**另存为**对话框让用户自选。应用向宿主索取工作区根路径，
  默认名为 `<根>/<run_id>.audit.jsonl`；工作区在哪是宿主知道的事。（工作区正是导出命令被允许写入的
  范围；`.riscdom/` 会被文件列表跳过，所以默认放一个可见路径比放隐藏的内部目录更好。）
- **由谁触发**：用户，按 run，在「设置 → 审计」的 run 列表里。

备选方案与代价：

| 备选 | 为何不作为 v0.5 默认 | 若选择它的代价 |
|---|---|---|
| 保留**导出整条链**（现在的命令），只加个按钮 | 它回答的是「都给我」，不是「给我这一次 run」。同一任务跑两次，差别只是大文件里的两个摘要 | 最省（约等于一个按钮）；但 v0.6 的比对会更难做 |
| 导出 run **外加一行合成头**（`run.export`） | 格式就不再是「恰好等于链」；要么往文件里塞一个假事件，要么让格式与 `export_jsonl` 分叉 | 多一份要文档化、要验证、要保持同步的格式 |
| 导出时**同时写入链状态结论** | 把断言混进记录；结论属于校验器，不属于文件 | 改动小，但文件开始宣称它无法自证的东西 |
| 用 CLI 子命令（`audit-export <db> --run <id>`）替代 UI 按钮 | 黄金路径是人在应用里走的；只有 CLI 导出会让第 5 步**看不见** | 表面积更大；仍然需要按 id 区间的查询 |

**任选其一都有的前置条件**：id 区间必须能到调用方手里。要么 `RunView` 带上 `start_seq` / `end_seq`，要么
导出命令接收 run id 并在 audit 层解析区间。**推荐后者（接收 run id）** —— UI 不该需要懂序号。

## 5. 「创建环境」：一个概念，还是就是「配好了」？

**推荐：v0.5 不引入新的一等对象。** 把第 2 步定义为「三项配置 + 一次被记录下来的预检」，并在文档与预检的
缓存结果里把这个定义写清楚。一次 run 的指纹已经捕获了生效中的环境，所以「这属于哪个环境」从链上就能回答，
不需要新造东西。

备选方案与代价：

| 备选 | 为何不作为 v0.5 默认 | 若选择它的代价 |
|---|---|---|
| 具名**环境**（名字 → 服务商、路径、VM 设置；一键切换；一台机器可多个） | 这是一个有自己 UX、存储与迁移故事的真功能；而 v0.5 的目标是让一个人走到「比对过的重新运行」，不是管理环境 | 大：新状态、新 settings 形状、新 UI、新测试 |
| 轻量**环境记录**：给 run 挂一个名字，好给两次 run 打标签 | 等于上面一半的功能，而指纹已经把活干了；名字还多出「标签与配置不一致」的问题 | 中：存储、迁移、UI，以及「标签 vs 现实」的判定 |
| 什么都不做（维持现状，但写进文档） | 那样「创建环境」就一直是路线图上一句含糊的话 | 免费 |

## 6. 七步怎么被证明可复现

**推荐：两层 —— 一个 `--ignored` 端到端测试走第 3–7 步，外加一份书面人工清单覆盖第 1–2 步，由人在干净
机器上照做（[golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md)）。** 第 1–2 步是机器装配，在本仓库的 CI 里无法诚实自动化（runner 没有 QEMU、没有 GUI）；第
3–7 步则正是现有的 mock-LLM 夹具已经做了大半的事。

| 选项 | 是否推荐 | 理由与代价 |
|---|---|---|
| **(a) `--ignored` e2e 测试，mock LLM，真实 QEMU**：跑一次任务 → 存快照 → 恢复它（第二次 run 带 parent）→ 导出该 run 的区间 → 对导出文件跑 `audit-verify` → 改一个指纹字段 → 再跑 → 断言两个指纹不同 | **推荐** | 复用 `host-core/tests/e2e_ui.rs` 与 5c-3 诊断夹具。代价：一个新测试模块 + 一条小的导出通路。它覆盖不到安装/配置（第 1–2 步），也不使用真实模型 |
| **(b) 文档里的**人工清单（[golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md)），每个版本由人用真实 API key 走一遍 | **推荐，用于第 1–2 步** | 这是覆盖「装 QEMU」与「粘贴 key」的唯一诚实方式。代价：每版的人工时间，且必须被**记录下来**才算数 |
| (c) 用脚本无头驱动应用命令（不走 GUI） | v0.5 不推荐 | 相对 e2e 测试只是多一层机器，覆盖没有增加 |
| (d) GUI 自动化（点界面） | v0.5 不推荐 | 脆弱，且仓库里没有任何此类夹具 |

该推荐假定导出落在工作区：这样 e2e 测试可以直接断言文件存在、对它跑 `audit-verify` 的逻辑、再删掉它 ——
中间没有对话框挡路。

## 7. v0.5 范围

| 步骤 | 现状 | v0.5 工作量 |
|---|---|---|
| 1 安装 | 已有（QEMU 引导；GCC 探测/下载） | 无，只需把一行清单写进文档 |
| 2 创建环境 | 已有（配置 + 预检） | 把定义写下来（§5）；不引入新概念 |
| 3 跑 Agent 任务 | 已有 | 无 |
| 4 存 snapshot | 已有 | 无（`window.prompt` 取名是候选，不是硬需求） |
| 5 得审计记录 | **三分之二缺失** | **本版本的主要工作**：把 run 区间暴露出来、让导出按区间划界、加 UI 入口、并用测试证明 |
| 6 rollback | 已有 | 无 |
| 7 换配置重跑 | 机制已有，可读性没有 | **最小化**：每个 run 显示指纹、可选两个 run 并排看指纹、并显示它来自哪个快照；**不做**差异引擎 |

用一句话概括最小且诚实的 v0.5：**把第 5 步做实、把第 7 步做可读、把第 1–2 步的定义写下来。**

## 8. 已拍板的决定

八项均已于 2026-09-19 由项目所有者裁决；§4–§6 的推荐即为决定，另有两处补充见下。

1. **导出范围：一次 run 的区间。** 整链导出保留给「都给我」（`export_audit_jsonl`）；黄金路径第 5 步
   用按 run 的导出。
2. **导出落点：工作区里一个可见路径**，由原生**另存为**对话框选择。工作区之外的路径仍会被宿主拒绝，
   而这个拒绝会**显示给用户**，不会被吞掉。*（决定：UI 用 `save()`，所以能力集里多了
   `dialog:allow-save` —— 由 `ui/scripts/probe-ui-dialog.mjs` 钉住，防止权限悄悄变大。）*
3. **区间从 run id 来。** `export_run_audit(run_id, path)` 在 audit 层解析 `[start_seq, end_seq]`；
   **`RunView` 不动** —— UI 不接触序号。
4. **v0.5 不做 CLI 导出。** `audit-verify` 与 `audit-rebuild` 形状不变；导出子命令是 v0.6 的问题。
5. **「环境」继续是一个说法，不是对象。** 第 2 步 = 三项配置 + 一次被记录的预检，并照此写进文档。
6. **发布门槛包含「由人用真实 API key 走一遍第 1–2 步」**，且必须**对照一份清单模板记录下来**：
   仓库里放一份模板，由人每版填写 —— 机器与日期、操作系统版本、QEMU 与 GCC 的版本**及其来源**
   （`winget` / 手动 / 应用内下载）、服务商与模型、预检结论、两次 run 的指纹、以及导出文件与其
   `audit-verify` 结论。没人写下来的行走，没人能复核。模板见
   [golden-path-checklist.zh-CN.md](golden-path-checklist.zh-CN.md)。
7. **第 7 步在 v0.5 保持最小化**：每个 run 显示指纹与它来自哪个快照，并可选两个 run 并排看到它们的短指纹、
   全量指纹、开始时间与状态。逐字段比对指纹属于 v0.6。
8. **快照取名在 v0.5 继续用 `window.prompt`。** 它与旁边的原生对话框不一致、值得修，但不在黄金路径上。

## 9. 不做

- **自动比较（第 8 步）。** 属于 v0.6。v0.5 停在「两次 run 都被记录下来，且它们的指纹不同」；自动比较需要
  指纹文档的差异计算，那本身是一份设计。
- **任何跨平台工作。** 这条路径只在 Windows 上成立；macOS / Linux 是 v0.5 路线图里独立的一条（第 3 条），
  它改变的是**谁**能走这条路径，而不是路径本身。
- **另一套 VM/LLM 方案、新的快照格式、会话加密。** 都是各自独立的路线图条目。
