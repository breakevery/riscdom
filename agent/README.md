# agent

智芯城（RiscDom）的 **AI 代理运行时**：用自然语言驱动 LLM 在 RISC-V 虚拟沙箱里
写 C / 汇编、编译、运行、读串口并循环迭代。

依赖方向：`agent → sandbox`，`agent → audit`。

## 模块

- `config` — `AgentConfig`（从环境变量读取；`Debug` 对 API key 打码）
- `message` — `ChatMessage` / `ChatRequest` / `ChatResponse` / `ToolCall`（DeepSeek 兼容）
- `llm` — `LlmClient` trait + `OpenAiCompatClient`（OpenAI 兼容 HTTP 客户端；`DeepSeekClient` 为向后兼容别名）+ `MockLlm`（测试脚本）
- `audit_hook` — 统一的审计写入辅助
- `policy` — `WorkspacePolicy`（能力策略，默认拒绝）
- `compiler` — `compile_freestanding`（C → 裸机 ELF，注入 crt0）
- `tools` — 工具集与 `execute_tool`
- `prompt` — 系统提示词构建
- `agent` — `AgentLoop` / `AgentOutcome`

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

## 工具清单

| 工具 | 说明 |
| --- | --- |
| `write_source(path, content)` | 写 C / 汇编源文件（仅 `.c/.h/.S/.s`） |
| `compile(source_path, output_elf)` | 编译成裸机 ELF（加载地址 `0x80000000`） |
| `start_vm(elf_path)` | 用 `sandbox` crate 启动 QEMU |
| `read_serial()` | 读取当前串口缓冲 |
| `stop_vm()` | 停止 QEMU |
| `list_workspace()` | 列出工作区文件 |

## 能力策略

`WorkspacePolicy` **默认拒绝**：

1. 任何含 `..` 的路径直接拒绝（防穿越）。
2. 解析为绝对路径后必须位于工作区根内，否则拒绝。
3. 写操作额外要求扩展名在白名单内（`.c/.h/.S/.s`）。
4. 每次拒绝都写 `agent.policy.deny` 审计事件。

工具执行前必经策略检查；agent 从不直接调用 QEMU，一律通过 `sandbox` crate。

## 循环与上下文

- 每轮：`llm.chat` → 若有 `tool_calls` 则逐个执行并回填 `tool` 消息，否则返回 `Final`。
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

## v0.2 TODO

- 给 `AgentLoop` 暴露最小串口访问接口（当前 host 从审计派生串口内容，依赖脆弱）
- 流式 LLM 响应（SSE 逐字）
- `OpenAiCompatClient` + 本地模型支持：`DeepSeekClient` 重构为通用 OpenAI 兼容客户端，
  内置 DeepSeek（默认）/ OpenAI / Ollama（本地）/ LM Studio（本地）预设，支持无 key 的本地模型
- gdbstub 接入（调试）
- 多轮会话持久化（跨轮上下文）
- `real_api` 测试补 `verify_chain` 断言（当前只断言串口输出）
