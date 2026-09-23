[English](README.md) | 中文

# agent

智芯城（RiscDom）的 **AI 代理运行时**：用自然语言驱动 LLM 在 RISC-V 虚拟沙箱里
写 C / 汇编、编译、运行、读串口并循环迭代。

依赖方向：`agent → sandbox`，`agent → audit`。

## 模块

- `config` — `AgentConfig`（从环境变量读取；`Debug` 对 API key 打码）
- `message` — `ChatMessage` / `ChatRequest` / `ChatResponse` / `ToolCall`（DeepSeek 兼容）
- `llm` — `LlmClient` trait（`chat` + `chat_stream`）+ `OpenAiCompatClient`（OpenAI 兼容 HTTP 客户端；`DeepSeekClient` 为向后兼容别名）+ `MockLlm`（测试脚本）
- `audit_hook` — 统一的审计写入辅助
- `policy` — `WorkspacePolicy`（能力策略，默认拒绝）
- `compiler` — `compile_freestanding`（C → 裸机 ELF，注入 crt0）
- `tools` — 工具集与 `execute_tool`
- `prompt` — 系统提示词构建
- `agent` — `AgentLoop` / `AgentOutcome`

## 服务商预设

`presets` 模块是**纯数据**：描述如何到达一个 OpenAI 兼容端点，不含任何服务商专用逻辑。

| id | 名称 | base_url | 默认 model | 需 key | 本地 |
| --- | --- | --- | --- | --- | --- |
| `deepseek`（默认） | DeepSeek | `https://api.deepseek.com` | `deepseek-chat` | 是 | 否 |
| `openai` | OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` | 是 | 否 |
| `ollama` | Ollama（本地） | `http://localhost:11434/v1` | `qwen2.5-coder` | 否 | 是 |
| `lmstudio` | LM Studio（本地） | `http://localhost:1234/v1` | （用户填） | 否 | 是 |
| `custom` | 自定义 | （用户填） | （用户填） | 否 | 否 |

- `DEFAULT_PRESET_ID = "deepseek"`；`find_preset(id)` 查表；`builtin_presets()` 返回全部。
- `AgentConfig::from_preset(preset, api_key)`：按预设填 `base_url` / `model`；
  `requires_key=true` 且无 key → **明确报错**（不静默）；本地预设允许空 key；最后过一遍 `validate()`。
- “自定义”允许用户填**任意** OpenAI 兼容服务（含自建网关）。

## 配置

LLM 客户端为 **OpenAI 兼容协议**（`OpenAiCompatClient`）：只要 `base_url` / `model`
指向任何兼容服务商即可（DeepSeek 为默认预设）。`api_key` 对本地地址（localhost /
127.0.0.1）可为空，对云端地址必填。

> v0.2 OPENAI 兼容重构已完成（本阶段）；服务商预设下拉、本地模型（Ollama / LM Studio）
> 与无 key 降级体验见 [PROJECT_CONSTITUTION.md §10](../PROJECT_CONSTITUTION.md)。

| 环境变量 | 必需 | 默认 |
| --- | --- | --- |
| `DEEPSEEK_API_KEY` | 是 | — |
| `DEEPSEEK_BASE_URL` | 否 | `https://api.deepseek.com` |
| `DEEPSEEK_MODEL` | 否 | `deepseek-chat` |
| `RISCDOM_MAX_ITERATIONS` | 否 | `10` |
| `RISCDOM_REQUEST_TIMEOUT` | 否 | `120` |
| `RISCDOM_RISCV_GCC` | 否 | `D:\tools\...\riscv64-unknown-elf-gcc.exe` |
| `RISCDOM_QEMU` | 否 | `C:\Program Files\qemu\qemu-system-riscv64.exe` |

PowerShell：

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."
```

**API key 从不写入**代码、仓库、审计事件、错误信息或 Debug 输出（仅显示前 4 后 4）。

## 运行测试

Mock（无需网络 / key）：

```text
cargo test -p agent
```

真实 DeepSeek 端到端（需 key + 网络 + QEMU + 工具链）：

```powershell
$env:DEEPSEEK_API_KEY = "sk-..."
cargo test -p agent -- --ignored --nocapture
```

该测试的审计后端是**文件 SQLite**（`%TEMP%\riscdom-audit-real-*.db`）：run 结束后用**新的
句柄**重开同一个库，`audit::verify_chain` 必须返回 `ChainStatus::Intact`（且 `length > 0`），
并要求 `agent.llm.request` / `agent.tool.call` / `agent.tool.result` 各至少 1 条。临时库用
`Drop` 守卫清理（失败/panic 时同样删除）。

## 工具清单

| 工具 | 说明 |
| --- | --- |
| `write_source(path, content)` | 写 C / 汇编源文件（仅 `.c/.h/.S/.s`） |
| `compile(source_path, output_elf)` | 编译成裸机 ELF（加载地址 `0x80000000`） |
| `start_vm(elf_path)` | 用 `sandbox` crate 启动 QEMU |
| `read_serial()` | 读取当前串口缓冲 |
| `stop_vm()` | 停止 QEMU |
| `list_workspace()` | 列出工作区文件 |
| `request_sandbox(action, sandbox?, reason?)` | 为有切换权限的人留一条沙箱申请（v0.9 沙箱 F2c） |
| `sandbox_status()` | 本节点在跑什么，以及什么在等 |

## agent 写下的审计事件

每件发生过的事一行，都经 `audit_hook`：

| action | 何时 | detail |
| --- | --- | --- |
| `agent.user.input` | 一轮开始 | 输入，哈希并截断 |
| `agent.llm.request` / `.response` | 每次模型调用 | 大小与哈希，从不存原始 body——**从不存 API key** |
| `agent.llm.stream.start` / `.end` | 流式回答 | 与块式路径同一纪律 |
| `agent.tool.call` | 工具运行前 | `{id, name, arguments, arguments_len}`——arguments **截到 4 KiB**，一个源文件轻易就到 |
| `agent.tool.result` | 工具运行后 | `{call_id, ok, result_len, result}` |
| `agent.file.write` | **`write_source` 写好一个文件**（v0.9 项目进出） | `{path, bytes}`——工具给的工作区相对路径，以及落了多少 |
| `agent.compile.start` / `.result` | 一次构建 | 编译器、源与结果 |
| `agent.policy.deny` | 策略拒绝的路径 | 工具、路径、原因 |

`agent.file.write` 存在，是因为「AI 写过哪些文件」是关于项目的问题，而不应当从
`agent.tool.call` 被截断的 arguments 里反推。它是一条**审计**行，不是 SSE 事件：
它属于链（项目的来龙去脉在那里可证），不属于事件流。

## 能力策略

`WorkspacePolicy` **默认拒绝**：

1. 任何含 `..` 的路径直接拒绝（防穿越）。
2. 解析为绝对路径后必须位于工作区根内，否则拒绝。
3. 写操作额外要求扩展名在白名单内（`.c/.h/.S/.s`）。
4. 每次拒绝都写 `agent.policy.deny` 审计事件。

工具执行前必经策略检查；agent 从不直接调用 QEMU，一律通过 `sandbox` crate。

## 循环与上下文

- 每轮：`llm.chat` → 若有 `tool_calls` 则逐个执行并回填 `tool` 消息，否则返回 `Final`。
- **入口即校验**：`run()` 第一件事是 `config.validate()`；不通过直接返回
  `AgentOutcome::Failed { iterations: 0 }`，不会发出任何 LLM 请求。
- 上限 `max_iterations`（默认 10），达到后返回 `MaxIterations`，绝不无限循环。
- 上下文裁剪：超过 40 条消息时保留 system + 最近 30 条，并插入 truncation 标记。
- 单条消息（用户输入 / 工具结果）上限 8 KB。
- 串口内容视为**数据**，绝不当指令处理（prompt injection 防线）。

## 编译器说明

`compile` 在 spec 建议的 `-Ttext=0x80000000` 之外，额外使用 `-mcmodel=medany`
并链接一个生成的链接脚本 + 注入的 `crt0`：`crt0` 提供 `_start`（放在镜像最前），
设置 `sp`，再 `call main`。因此 AI 只需写 `int main(void) { ... }`。

原因：本环境下 `medlow` 模型在 `0x80000000` 会截断重定位；且 `-bios none` 下 QEMU
从 `0x80000000` 起跳，入口必须位于镜像最前。详见 `sandbox/README.md` 与 `ENVIRONMENT.md`。

## 串口订阅

`AgentLoop::subscribe_serial() -> std::sync::mpsc::Receiver<Vec<u8>>`：每次调用创建一个新的
channel（sender 内部保存），返回 receiver。启动 VM 时会把 `VMConfig.serial_observer`
设为一个统一 observer，把沙箱**实时推送**的串口分帧扇出给所有订阅者。

- 只收到**订阅之后**的数据（不做历史回放）；需要全量请调用 `read_serial` 工具。
- receiver 关闭后，下一次事件会自动把对应 sender 从列表移除（不会 panic）。
- 无 tokio 依赖（纯 `std::sync::mpsc`）。

## 流式响应

`LlmClient::chat_stream(req, on_event) -> ChatResponse`：`on_event` 收到 `StreamEvent`：

- `Delta(String)` — 增量文本
- `ToolCallDelta { index, id, name, args_delta }` — **工具调用分片**（不逐字显示，按 `index` 缓冲到完整后再处理）
- `Done` — 流结束

- 默认实现**退化到 `chat`**（整段 content 作为单个 `Delta`），因此任何 `LlmClient` 都可用；
  `OpenAiCompatClient` 覆盖为真实 SSE（`stream: true` + `data:` 逐行解析），
  `MockLlm` 按 8 字符切片以便测试。
- SSE 解析在 [`sse`] 模块（`parse_sse_line` / `SseAccumulator`），已覆盖空行、注释、
  `data:` 有无空格、多行 data 拼接、`[DONE]`。
- 网络/JSON 错误返回 `Err`，不 panic。
- 真实 API 流式测试：`cargo test -p agent --test stream_real -- --ignored --nocapture`。

### 订阅（`AgentLoop`）

`AgentLoop::subscribe_stream() -> std::sync::mpsc::Receiver<StreamEvent>`：每次调用新建一个
channel（sender 内部保存），`run` 中把同一个 `StreamEvent` 扇出给所有订阅者；
receiver 关闭后自动剔除。

### 审计事件

| action | detail |
| --- | --- |
| `agent.llm.stream.start` | `{ model, base_url_host }` |
| `agent.llm.stream.end` | `{ chunks, duration_ms, has_tool_calls }` |
| `agent.llm.request` / `agent.llm.response` | 保持不变（response 仍记完整响应） |

**不**逐 chunk 记审计事件。

## v0.2 TODO

- `OpenAiCompatClient` + 本地模型支持：`DeepSeekClient` 重构为通用 OpenAI 兼容客户端，
  内置 DeepSeek（默认）/ OpenAI / Ollama（本地）/ LM Studio（本地）预设，支持无 key 的本地模型
- gdbstub 接入（调试）
- 多轮会话持久化（跨轮上下文）
- ~~`real_api` 补 `verify_chain` 断言~~（已完成，阶段 21：文件 SQLite + 独立句柄验证链完整）
