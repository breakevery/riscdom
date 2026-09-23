[English](tool-schema-executor.md) | 中文

# 工具 schema：执行者的工具

> **适用于 v0.9（在 v1.0 之前不稳定）。** 下面的 JSON 是**执行者自己的模型**每一轮都会拿到的东西：内核发给 LLM 的那个请求里的 `tools[]` 数组。它由代码生成，而当本文件与代码不一致时，会有一个测试失败。

## 这是什么

一个执行者就是一个 `AgentLoop`（进程内的 loop、一个 `worker` 子进程，或任何实现了 `AgentHandle` 的东西）。它里面的模型只能调用八个工具，而内核把它们的 schema 随每个请求一起发出——`agent/src/agent.rs` 把 `tools_json()` 放进聊天请求的 `tools` 字段。本文件就是那个数组。

**谁需要它**

- **改工具的内核开发者**：描述是 prompt 的一部分，所以它也是行为的一部分（看 `stop_vm` 的文本：模型被告知任务结束时**不要**停 VM）。
- **监工作者**，想知道一条被派发的任务，它的模型可以做什么——这就是一条 `Task` 的 input 会被用什么词汇来回应。

**谁*不*需要它**：AI 监工。监工是外部进程；它的接口是 HTTP 上的控制平面，不是这些工具。它的工具 schema 在 [tool-schema-control-plane.zh-CN.md](tool-schema-control-plane.zh-CN.md)。两者是刻意分开的东西：这些在**执行者内部**跑，那些用来**驱动**执行者。

## 八个工具

下面的数组就是 `tools_json()` 的确切取值，顺序也是内核发送它的顺序（即 `tool_specs()` 声明它们的顺序）。

```json
[
  {
    "type": "function",
    "function": {
      "name": "write_source",
      "description": "Write a C or RISC-V assembly source file into the workspace. Only .c/.h/.S/.s are allowed. Paths are relative to the workspace.",
      "parameters": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "workspace-relative path" },
          "content": { "type": "string", "description": "file contents" }
        },
        "required": ["path", "content"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "compile",
      "description": "Compile a workspace source file into a bare-metal RISC-V ELF at load address 0x80000000. Define `int main(void)`.",
      "parameters": {
        "type": "object",
        "properties": {
          "source_path": { "type": "string" },
          "output_elf": { "type": "string", "description": "output path ending in .elf" }
        },
        "required": ["source_path", "output_elf"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "start_vm",
      "description": "Start the QEMU VM. If a VM is already running, this returns 'already running' and does not start a second one.",
      "parameters": {
        "type": "object",
        "properties": { "elf_path": { "type": "string" } },
        "required": ["elf_path"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "read_serial",
      "description": "Return everything the guest has written to the UART so far.",
      "parameters": { "type": "object", "properties": {}, "required": [] }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "stop_vm",
      "description": "Stop the running QEMU VM. Only call this when the user explicitly asks to stop the VM (e.g. 'stop the VM', 'shut down the sandbox'). After a task finishes, do NOT call stop_vm — the VM is a cross-run resource and is expected to stay running until the user stops it.",
      "parameters": { "type": "object", "properties": {}, "required": [] }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "list_workspace",
      "description": "List files in the workspace (relative paths).",
      "parameters": { "type": "object", "properties": {}, "required": [] }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "request_sandbox",
      "description": "Ask the user to switch this node to another sandbox, or to assemble one. This does NOT switch anything by itself: it leaves a request the user (or a supervisor) approves or rejects. Use it when the user asks for a sandbox you do not have, and tell them the id it returns.",
      "parameters": {
        "type": "object",
        "properties": {
          "action": { "type": "string", "description": "switch, define or assemble" },
          "sandbox": { "type": "string", "description": "the sandbox to switch to" },
          "reason": { "type": "string", "description": "why the change is wanted" }
        },
        "required": ["action"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "sandbox_status",
      "description": "Report the sandbox this node is running and the sandbox requests still waiting for a decision.",
      "parameters": { "type": "object", "properties": {}, "required": [] }
    }
  }
]
```

## 同样八个，给人看

| 工具 | 参数 | 它做什么 |
|---|---|---|
| `write_source` | `path`、`content` | 写一个源文件。只允许 `.c` / `.h` / `.S` / `.s`，且只在 workspace 内。 |
| `compile` | `source_path`、`output_elf` | 把一个源文件编到 `0x80000000` 的裸机 RISC-V ELF。 |
| `start_vm` | `elf_path` | 用那个 ELF 启 QEMU。第二次启动会被拒绝，不排队。 |
| `read_serial` | —— | 客户机到目前为止写到 UART 的一切。 |
| `stop_vm` | —— | 停 VM。描述明确告诉模型**不要**在任务结束时做这件事：VM 比一次运行活得久。 |
| `list_workspace` | —— | workspace 的文件，相对路径。 |
| `request_sandbox` | `action`、`sandbox`?、`reason`? | 落一条沙箱申请。它自己不切换任何东西（v0.9 沙箱 F2c）。 |
| `sandbox_status` | —— | 本节点在跑什么，以及什么在等裁决。 |

## 本文件如何保持为真

`agent/tests/tool_schema_doc.rs` 读上面的 JSON 块、解析它，并与 `agent::tools::tools_json()`——`AgentLoop` 放进请求里的同一个值——比对。因此工具的名字、描述或参数一改，本文件没同步就会测试失败；本文件里打错一个字，也会失败。按描述逐条钉的老断言仍在 `agent/tests/tool_descriptions.rs`。

这个块就是**唯一**的那一份：`agent/README.md` 的工具表指向这里，不再保留第二份清单。
