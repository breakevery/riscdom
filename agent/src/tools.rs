//! Tool set exposed to the LLM, plus their execution.
//!
//! Every execution follows the same shape: parse args → policy check → audit
//! `agent.tool.call` → run → audit `agent.tool.result`. Policy denials also
//! audit `agent.policy.deny`. Failures return `Err`, which the agent loop
//! feeds back to the model as the tool result.

use crate::audit_hook::{record_policy_deny, record_tool_call, record_tool_result};
use crate::compiler::{compile_freestanding, CompilerConfig};
use crate::error::AgentError;
use crate::message::{FunctionCall, ToolCall};
use crate::policy::WorkspacePolicy;
use audit::AuditSink;
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, SerialObserver, VMConfig};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Cap for tool results handed back to the model / stored in audit detail.
pub const MAX_TOOL_RESULT: usize = 8 * 1024;

/// Guest memory size for agent-started VMs (MiB).
const VM_MEMORY_MB: u32 = 128;

/// How long `read_serial` waits for the first UART output before returning.
const SERIAL_READ_WAIT: Duration = Duration::from_millis(2000);

static CALL_SEQ: AtomicU64 = AtomicU64::new(1);

/// A tool description handed to the model.
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl ToolSpec {
    /// Render as an OpenAI/DeepSeek `tools[]` entry.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            }
        })
    }
}

/// Runtime context shared by tool executions.
///
/// NOTE: `vm` is `&mut Option<..>` (not `Option<&mut ..>`) so `start_vm` /
/// `stop_vm` can install/clear the VM in the caller-owned slot.
pub struct ToolContext<'a> {
    pub policy: &'a WorkspacePolicy,
    pub audit: Arc<Mutex<dyn AuditSink>>,
    pub vm: &'a mut Option<RiscVVirtualMachine>,
    pub compiler: &'a CompilerConfig,
    /// Live serial subscribers, fanned out to each started VM's observer.
    pub serial_observers: Arc<Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>>,
}

/// Build a serial observer that fans out to every live subscriber.
///
/// Closed receivers make `send` fail, and such senders are dropped from the
/// list on the next event.
pub fn serial_observer_for(
    senders: Arc<Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>>,
) -> SerialObserver {
    Arc::new(move |chunk: &[u8]| {
        let data = chunk.to_vec();
        if let Ok(mut list) = senders.lock() {
            list.retain(|tx| tx.send(data.clone()).is_ok());
        }
    })
}

/// The MVP tool catalogue.
pub fn tool_specs() -> Vec<ToolSpec> {
    let obj = |props: serde_json::Value, required: Vec<&str>| {
        serde_json::json!({
            "type": "object",
            "properties": props,
            "required": required,
        })
    };
    vec![
        ToolSpec {
            name: "write_source".into(),
            description: "Write a C or RISC-V assembly source file into the workspace. \
                          Only .c/.h/.S/.s are allowed. Paths are relative to the workspace."
                .into(),
            parameters: obj(
                serde_json::json!({
                    "path": {"type": "string", "description": "workspace-relative path"},
                    "content": {"type": "string", "description": "file contents"}
                }),
                vec!["path", "content"],
            ),
        },
        ToolSpec {
            name: "compile".into(),
            description: "Compile a workspace source file into a bare-metal RISC-V ELF at \
                          load address 0x80000000. Define `int main(void)`."
                .into(),
            parameters: obj(
                serde_json::json!({
                    "source_path": {"type": "string"},
                    "output_elf": {"type": "string", "description": "output path ending in .elf"}
                }),
                vec!["source_path", "output_elf"],
            ),
        },
        ToolSpec {
            name: "start_vm".into(),
            description: "Start the QEMU VM. If a VM is already running, this returns \
                          'already running' and does not start a second one."
                .into(),
            parameters: obj(
                serde_json::json!({ "elf_path": {"type": "string"} }),
                vec!["elf_path"],
            ),
        },
        ToolSpec {
            name: "read_serial".into(),
            description: "Return everything the guest has written to the UART so far.".into(),
            parameters: obj(serde_json::json!({}), vec![]),
        },
        ToolSpec {
            name: "stop_vm".into(),
            description: "Stop the running QEMU VM. Only call this when the user explicitly \
                          asks to stop the VM (e.g. 'stop the VM', 'shut down the sandbox'). \
                          After a task finishes, do NOT call stop_vm — the VM is a cross-run \
                          resource and is expected to stay running until the user stops it."
                .into(),
            parameters: obj(serde_json::json!({}), vec![]),
        },
        ToolSpec {
            name: "list_workspace".into(),
            description: "List files in the workspace (relative paths).".into(),
            parameters: obj(serde_json::json!({}), vec![]),
        },
    ]
}

/// The tool catalogue rendered for an LLM request.
pub fn tools_json() -> Vec<serde_json::Value> {
    tool_specs().iter().map(ToolSpec::to_json).collect()
}

/// Execute a tool call.
pub fn execute_tool(name: &str, args: &str, ctx: &mut ToolContext) -> Result<String, AgentError> {
    let parsed: serde_json::Value = if args.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(args)
            .map_err(|e| AgentError::Tool(format!("invalid tool arguments JSON: {e}")))?
    };

    // Audit the call exactly as the model issued it.
    let call = ToolCall {
        id: format!("tool-{}-{name}", CALL_SEQ.fetch_add(1, Ordering::SeqCst)),
        kind: "function".into(),
        function: FunctionCall {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    };
    record_tool_call(&ctx.audit, &call);

    let result = match name {
        "write_source" => tool_write_source(&parsed, ctx),
        "compile" => tool_compile(&parsed, ctx),
        "start_vm" => tool_start_vm(&parsed, ctx),
        "read_serial" => tool_read_serial(ctx),
        "stop_vm" => tool_stop_vm(ctx),
        "list_workspace" => tool_list_workspace(ctx),
        other => Err(AgentError::Tool(format!("unknown tool: {other}"))),
    };

    match &result {
        Ok(text) => record_tool_result(&ctx.audit, &call.id, text, true),
        Err(e) => record_tool_result(&ctx.audit, &call.id, &e.to_string(), false),
    }
    result.map(|s| truncate_result(&s))
}

fn arg_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, AgentError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AgentError::Tool(format!("missing string argument: {key}")))
}

/// Policy denial helper: audits and returns the error.
fn deny(ctx: &ToolContext, tool: &str, path: &str, err: AgentError) -> AgentError {
    record_policy_deny(
        &ctx.audit,
        &err.to_string(),
        serde_json::json!({ "tool": tool, "path": path }),
    );
    err
}

fn tool_write_source(
    args: &serde_json::Value,
    ctx: &mut ToolContext,
) -> Result<String, AgentError> {
    let path = arg_str(args, "path")?;
    let content = arg_str(args, "content")?;

    let abs = ctx
        .policy
        .check_write(Path::new(path))
        .map_err(|e| deny(ctx, "write_source", path, e))?;

    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&abs, content)?;
    Ok(format!("wrote {} bytes to {}", content.len(), path))
}

fn tool_compile(args: &serde_json::Value, ctx: &mut ToolContext) -> Result<String, AgentError> {
    let src_rel = arg_str(args, "source_path")?;
    let out_rel = arg_str(args, "output_elf")?;

    let src = ctx
        .policy
        .check_read(Path::new(src_rel))
        .map_err(|e| deny(ctx, "compile", src_rel, e))?;

    // The ELF is a build artifact, not AI-written source: enforce containment
    // (no extension allow-list) but require a `.elf` suffix.
    let out = ctx
        .policy
        .check_read(Path::new(out_rel))
        .map_err(|e| deny(ctx, "compile", out_rel, e))?;
    if out.extension().and_then(|e| e.to_str()) != Some("elf") {
        return Err(AgentError::Tool("output_elf must end in .elf".to_string()));
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }

    emit_compile(
        ctx,
        "agent.compile.start",
        serde_json::json!({
            "source": src_rel, "output": out_rel,
        }),
    );

    let res = compile_freestanding(ctx.compiler, &src, &out)?;

    emit_compile(
        ctx,
        "agent.compile.result",
        serde_json::json!({
            "ok": res.ok,
            "stderr": truncate_result(&res.stderr),
            "stdout": truncate_result(&res.stdout),
        }),
    );

    if res.ok {
        Ok(format!("compiled {} -> {} (ok)", src_rel, out_rel))
    } else {
        Err(AgentError::Tool(format!(
            "compile failed:\n{}",
            if res.stderr.trim().is_empty() {
                res.stdout.trim()
            } else {
                res.stderr.trim()
            }
        )))
    }
}

fn emit_compile(ctx: &ToolContext, action: &str, detail: serde_json::Value) {
    if let Ok(mut sink) = ctx.audit.lock() {
        sink.record(audit::AuditEvent::new("agent", action, detail));
    }
}

fn tool_start_vm(args: &serde_json::Value, ctx: &mut ToolContext) -> Result<String, AgentError> {
    if ctx.vm.is_some() {
        return Err(AgentError::Tool(
            "a VM is already running; reuse it (compile and read_serial) instead of starting \
             another one; call stop_vm only if the user asks to stop the VM"
                .into(),
        ));
    }
    let elf_rel = arg_str(args, "elf_path")?;
    let elf = ctx
        .policy
        .check_read(Path::new(elf_rel))
        .map_err(|e| deny(ctx, "start_vm", elf_rel, e))?;

    let (qmp_port, serial_port) = two_free_ports()?;
    let snapshot_dir = ctx.policy.root.join(".riscdom").join("snapshots");
    let config = VMConfig {
        kernel: elf,
        memory_mb: VM_MEMORY_MB,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir,
        serial_observer: Some(serial_observer_for(Arc::clone(&ctx.serial_observers))),
        incoming_snapshot: None,
        incoming_relay_addr: None,
    };
    let mut vm = RiscVVirtualMachine::new(config, Arc::clone(&ctx.audit))?;
    vm.start()?;
    *ctx.vm = Some(vm);
    Ok(format!("VM started (qmp={qmp_port}, serial={serial_port})"))
}

fn tool_read_serial(ctx: &mut ToolContext) -> Result<String, AgentError> {
    let vm = ctx
        .vm
        .as_ref()
        .ok_or_else(|| AgentError::Tool("no VM running; call start_vm first".into()))?;

    // Wait briefly for the guest to emit something (the model calls this right
    // after start_vm). Returns whatever is buffered once it is non-empty or the
    // wait elapses.
    let deadline = Instant::now() + SERIAL_READ_WAIT;
    loop {
        let out = vm.serial_output();
        if !out.is_empty() || Instant::now() >= deadline {
            return Ok(String::from_utf8_lossy(&out).to_string());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn tool_stop_vm(ctx: &mut ToolContext) -> Result<String, AgentError> {
    match ctx.vm.as_mut() {
        Some(vm) => {
            vm.stop()?;
        }
        None => return Err(AgentError::Tool("no VM running".into())),
    }
    *ctx.vm = None;
    Ok("VM stopped".into())
}

fn tool_list_workspace(ctx: &mut ToolContext) -> Result<String, AgentError> {
    let mut files = Vec::new();
    collect_files(&ctx.policy.root, &ctx.policy.root, &mut files)?;
    files.sort();
    if files.is_empty() {
        Ok("(workspace is empty)".into())
    } else {
        Ok(files.join("\n"))
    }
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), AgentError> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn two_free_ports() -> Result<(u16, u16), AgentError> {
    let a = TcpListener::bind("127.0.0.1:0").map_err(|e| AgentError::Tool(e.to_string()))?;
    let b = TcpListener::bind("127.0.0.1:0").map_err(|e| AgentError::Tool(e.to_string()))?;
    Ok((
        a.local_addr()
            .map_err(|e| AgentError::Tool(e.to_string()))?
            .port(),
        b.local_addr()
            .map_err(|e| AgentError::Tool(e.to_string()))?
            .port(),
    ))
}

fn truncate_result(s: &str) -> String {
    if s.len() <= MAX_TOOL_RESULT {
        return s.to_string();
    }
    let mut end = MAX_TOOL_RESULT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated {} bytes]", &s[..end], s.len())
}

/// A cheap path helper used by tests / callers.
pub fn workspace_path(policy: &WorkspacePolicy, rel: &str) -> PathBuf {
    policy.root.join(rel)
}
