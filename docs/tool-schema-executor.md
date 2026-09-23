[中文](tool-schema-executor.zh-CN.md) | English

# Tool schema: the executor's tools

> **Applies to v0.9 (unstable until v1.0).** The JSON below is what the **executor's own
> model** is offered on every turn: the `tools[]` array of the request the kernel sends to
> the LLM. It is generated from the code, and a test fails when this file and the code
> disagree.

## What this is

An executor is one `AgentLoop` (an in-process loop, a `worker` child process, or anything
else that implements `AgentHandle`). The model inside it can call exactly eight tools, and
the kernel sends their schemas with every request — `agent/src/agent.rs` puts
`tools_json()` into the `tools` field of the chat request. This document is that array,
written out.

**Who needs it**

- **A kernel developer** changing a tool: the description is part of the prompt, so it is
  part of the behaviour (see the `stop_vm` text: the model is told *not* to stop the VM
  when a task ends).
- **A supervisor author** who wants to know what a dispatched task's model may do — this
  is the vocabulary a `Task`'s input is answered with.

**Who does *not* need it**: an AI supervisor. A supervisor is an external process; its
interface is the control plane over HTTP, not these tools. Its tool schema is
[tool-schema-control-plane.md](tool-schema-control-plane.md). The two are deliberately
different things: these run *inside* an executor, those run an executor.

## The eight tools

The array below is the exact value of `tools_json()`, in the order the kernel sends it
(which is the order `tool_specs()` declares them).

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

## The same eight, for a human

| Tool | Arguments | What it does |
|---|---|---|
| `write_source` | `path`, `content` | Writes a source file. Only `.c` / `.h` / `.S` / `.s`, and only inside the workspace. |
| `compile` | `source_path`, `output_elf` | Compiles one source file to a bare-metal RISC-V ELF at `0x80000000`. |
| `start_vm` | `elf_path` | Starts QEMU with that ELF. A second start is refused, not queued. |
| `read_serial` | — | Everything the guest has printed to the UART so far. |
| `stop_vm` | — | Stops the VM. The description tells the model **not** to do this at the end of a task: the VM outlives a run. |
| `list_workspace` | — | The workspace's files, as relative paths. |
| `request_sandbox` | `action`, `sandbox`?, `reason`? | Leaves a sandbox request. It switches nothing by itself (v0.9 sandbox F2c). |
| `sandbox_status` | — | What this node runs, and what is waiting for a decision. |

## How this file is kept true

`agent/tests/tool_schema_doc.rs` reads the JSON block above, parses it, and compares it
with `agent::tools::tools_json()` — the same value `AgentLoop` puts into the request. A
change to a tool's name, description or parameters therefore fails the test until this file
is regenerated, and a typo in this file fails it too. The older per-description assertions
live in `agent/tests/tool_descriptions.rs`.

The block is **the** copy: `agent/README.md`'s tool table points here rather than keeping a
second list.
