//! The agent loop: drive an LLM through tool calls until it answers.

use crate::audit_hook::{host_of, record_llm_request, record_llm_response};
use crate::compiler::CompilerConfig;
use crate::config::AgentConfig;
use crate::error::AgentError;
use crate::llm::LlmClient;
use crate::message::{ChatMessage, ChatRequest, StreamEvent};
use crate::policy::WorkspacePolicy;
use crate::tools::{execute_tool, tools_json, SandboxRequester, ToolContext};
use audit::{AuditEvent, AuditSink};
use sandbox::vm::RiscVVirtualMachine;
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

/// Per-message byte cap (user input / tool results).
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;
/// Context is trimmed once it exceeds this many messages.
pub const CONTEXT_MAX: usize = 40;
/// Number of most-recent messages kept when trimming.
pub const CONTEXT_KEEP: usize = 30;

/// How a run finished.
///
/// Serialisable since v0.8 (main deliverable 1/2): a run's outcome crosses a
/// process boundary in [`crate::dispatch::TaskOutcome`] (stdio + JSON lines).
/// Plain `String` / `u32` fields, so the round trip is lossless, and the enum is
/// nowhere near the audit chain — the derive is purely additive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentOutcome {
    /// The model produced a final answer.
    Final { content: String, iterations: u32 },
    /// The iteration cap was reached.
    MaxIterations {
        last_content: String,
        iterations: u32,
    },
    /// The run failed (e.g. LLM/API error).
    Failed { reason: String, iterations: u32 },
}

/// The agent loop.
pub struct AgentLoop {
    llm: Box<dyn LlmClient>,
    config: AgentConfig,
    policy: WorkspacePolicy,
    audit: Arc<Mutex<dyn AuditSink>>,
    messages: Vec<ChatMessage>,
    vm: Option<RiscVVirtualMachine>,
    compiler: CompilerConfig,
    /// Live serial subscribers (one `Sender` per `subscribe_serial` call).
    serial_observers: Arc<Mutex<Vec<Sender<Vec<u8>>>>>,
    /// Live stream subscribers (one `Sender` per `subscribe_stream` call).
    stream_observers: Arc<Mutex<Vec<Sender<StreamEvent>>>>,
    /// When set, tools operate on this externally-owned VM slot instead of the
    /// loop's own `vm` (lets the host own the VM across runs).
    external_vm: Option<Arc<Mutex<Option<RiscVVirtualMachine>>>>,
    /// Host-injected QEMU executable (v0.3 5b-1b); `None` = discover it.
    qemu_exe: Option<std::path::PathBuf>,
    /// Who this loop is, for the audit chain's `agent_id` (v0.8 batch B).
    agent_id: String,
    /// The host's sandbox request surface (v0.9 sandbox F2c); `None` when the host
    /// injects none, and the two sandbox tools report exactly that.
    sandbox_requester: Option<Arc<dyn SandboxRequester>>,
}

impl AgentLoop {
    /// Build a loop that owns its VM (backwards-compatible behaviour).
    pub fn new(
        llm: Box<dyn LlmClient>,
        config: AgentConfig,
        policy: WorkspacePolicy,
        audit: Arc<Mutex<dyn AuditSink>>,
        system_prompt: String,
        agent_id: impl Into<String>,
    ) -> Result<Self, AgentError> {
        Self::build(
            llm,
            config,
            policy,
            audit,
            system_prompt,
            agent_id.into(),
            None,
        )
    }

    /// Build a loop that operates on an **externally-owned** VM slot.
    ///
    /// The slot outlives the loop, so a VM started during a run stays alive
    /// afterwards (host-owned lifecycle). `system_prompt` is a sixth parameter
    /// on purpose: without it the constitution could not be injected.
    ///
    /// `agent_id` is the last parameter for the same reason it exists at all
    /// (v0.8 batch B): the caller owns the identity — the host mints one per
    /// instance — and every event this loop writes carries it.
    pub fn with_vm(
        llm: Box<dyn LlmClient>,
        config: AgentConfig,
        policy: WorkspacePolicy,
        audit: Arc<Mutex<dyn AuditSink>>,
        vm: Arc<Mutex<Option<RiscVVirtualMachine>>>,
        system_prompt: String,
        agent_id: impl Into<String>,
    ) -> Result<Self, AgentError> {
        Self::build(
            llm,
            config,
            policy,
            audit,
            system_prompt,
            agent_id.into(),
            Some(vm),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        llm: Box<dyn LlmClient>,
        config: AgentConfig,
        policy: WorkspacePolicy,
        audit: Arc<Mutex<dyn AuditSink>>,
        system_prompt: String,
        agent_id: String,
        external_vm: Option<Arc<Mutex<Option<RiscVVirtualMachine>>>>,
    ) -> Result<Self, AgentError> {
        Ok(Self {
            llm,
            config,
            policy,
            audit,
            messages: vec![ChatMessage::text("system", system_prompt)],
            vm: None,
            agent_id,
            compiler: CompilerConfig::from_env(),
            serial_observers: Arc::new(Mutex::new(Vec::new())),
            stream_observers: Arc::new(Mutex::new(Vec::new())),
            external_vm,
            qemu_exe: None,
            sandbox_requester: None,
        })
    }

    /// Use the host's sandbox request surface (v0.9 sandbox F2c).
    ///
    /// The host owns the queue and injects the handle the way it injects the
    /// compiler and the QEMU path; `None` (the default) means this host has no
    /// such surface, and the tools say so rather than quietly doing nothing.
    pub fn with_sandbox_requester(&mut self, requester: Option<Arc<dyn SandboxRequester>>) {
        self.sandbox_requester = requester;
    }

    /// Hand out a handle to the serial subscriber list so it can outlive this
    /// loop (host keeps it and re-attaches it to later runs).
    pub fn detach_serial(&mut self) -> Arc<Mutex<Vec<Sender<Vec<u8>>>>> {
        Arc::clone(&self.serial_observers)
    }

    /// Use an externally-owned subscriber list (shared across runs).
    pub fn attach_serial(&mut self, senders: Arc<Mutex<Vec<Sender<Vec<u8>>>>>) {
        self.serial_observers = senders;
    }

    /// Replace the compiler configuration (the host injects a manually chosen
    /// RISC-V toolchain here; auto-discovery stays the default).
    pub fn set_compiler(&mut self, compiler: CompilerConfig) {
        self.compiler = compiler;
    }

    /// Replace the QEMU executable (the host injects a manually chosen path
    /// here; auto-discovery stays the default).
    pub fn set_qemu_path(&mut self, path: std::path::PathBuf) {
        self.qemu_exe = Some(path);
    }

    /// Append restored messages to the conversation.
    ///
    /// Used to rehydrate a persisted session: it only extends `messages`, it
    /// never calls the LLM and never replays tool calls.
    pub fn push_history(&mut self, history: Vec<ChatMessage>) {
        self.messages.extend(history);
    }

    /// Subscribe to live LLM stream events.
    ///
    /// Each call returns a fresh channel carrying [`StreamEvent`]s for runs
    /// started afterwards. Closed receivers are dropped automatically.
    pub fn subscribe_stream(&self) -> Receiver<StreamEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        if let Ok(mut list) = self.stream_observers.lock() {
            list.push(tx);
        }
        rx
    }

    /// Subscribe to live serial output.
    ///
    /// Each call returns a fresh channel: the receiver gets **data that arrives
    /// after subscribing** (no history replay). The sender is dropped from the
    /// internal list automatically once the receiver is closed.
    pub fn subscribe_serial(&self) -> Receiver<Vec<u8>> {
        let (tx, rx) = std::sync::mpsc::channel();
        if let Ok(mut list) = self.serial_observers.lock() {
            list.push(tx);
        }
        rx
    }

    /// The current conversation (for inspection/tests).
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    fn emit(&self, action: &str, detail: serde_json::Value) {
        if let Ok(mut sink) = self.audit.lock() {
            if let Err(error) =
                sink.record(AuditEvent::new("agent", action, detail).with_agent(&self.agent_id))
            {
                audit::report_failure(&error);
            }
        }
    }

    /// Who this loop is (v0.8 batch B): the `agent_id` every event it writes carries.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Run one user turn to completion.
    pub fn run(&mut self, user_input: &str) -> Result<AgentOutcome, AgentError> {
        // Readiness gate: refuse to start when the configuration is unusable.
        // Fails before any LLM call, with iterations = 0.
        if let Err(e) = self.config.validate() {
            return Ok(AgentOutcome::Failed {
                reason: e.to_string(),
                iterations: 0,
            });
        }

        let user = truncate(user_input, MAX_MESSAGE_BYTES);
        self.emit(
            "agent.user.input",
            serde_json::json!({ "chars": user_input.len(), "text": user }),
        );
        self.messages.push(ChatMessage::text("user", user));

        let mut iterations: u32 = 0;
        loop {
            self.trim_context();

            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: self.messages.clone(),
                tools: Some(tools_json()),
                tool_choice: Some("auto".into()),
                temperature: Some(0.0),
                stream: None,
            };
            record_llm_request(
                &self.audit,
                &self.agent_id,
                &request,
                &self.config.model,
                &self.config.base_url,
            );

            iterations += 1;

            self.emit(
                "agent.llm.stream.start",
                serde_json::json!({
                    "model": self.config.model,
                    "base_url_host": host_of(&self.config.base_url),
                }),
            );
            let started = std::time::Instant::now();

            // Fan stream events out to live subscribers (never per-chunk audit).
            // Scoped so the borrow of `chunks` ends before it is read below.
            let (response, chunks) = {
                let mut chunks = 0usize;
                let observers = Arc::clone(&self.stream_observers);
                let mut on_event = |event: StreamEvent| {
                    match &event {
                        StreamEvent::Delta(_) | StreamEvent::ToolCallDelta { .. } => chunks += 1,
                        StreamEvent::Done => {}
                    }
                    if let Ok(mut list) = observers.lock() {
                        list.retain(|tx| tx.send(event.clone()).is_ok());
                    }
                };
                let response = match self.llm.chat_stream(request, &mut on_event) {
                    Ok(r) => r,
                    Err(e) => {
                        return Ok(AgentOutcome::Failed {
                            reason: e.to_string(),
                            iterations,
                        })
                    }
                };
                (response, chunks)
            };

            self.emit(
                "agent.llm.stream.end",
                serde_json::json!({
                    "chunks": chunks,
                    "duration_ms": started.elapsed().as_millis() as u64,
                    "has_tool_calls": response.tool_calls().is_some(),
                }),
            );
            record_llm_response(&self.audit, &self.agent_id, &response);

            let message = match response.first_message() {
                Some(m) => m.clone(),
                None => {
                    return Ok(AgentOutcome::Failed {
                        reason: "model returned no choices".into(),
                        iterations,
                    })
                }
            };
            let tool_calls = message.tool_calls.clone().unwrap_or_default();

            self.messages.push(ChatMessage {
                role: "assistant".into(),
                content: message.content.clone(),
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls.clone())
                },
                tool_call_id: None,
            });

            if tool_calls.is_empty() {
                return Ok(AgentOutcome::Final {
                    content: message.content.clone().unwrap_or_default(),
                    iterations,
                });
            }

            for call in &tool_calls {
                let text = if let Some(slot) = self.external_vm.clone() {
                    // Host-owned VM: operate directly on the shared slot.
                    let mut guard = slot
                        .lock()
                        .map_err(|_| AgentError::Other("vm slot poisoned".into()))?;
                    let mut ctx = ToolContext {
                        policy: &self.policy,
                        audit: Arc::clone(&self.audit),
                        vm: &mut guard,
                        compiler: &self.compiler,
                        serial_observers: Arc::clone(&self.serial_observers),
                        qemu_exe: &self.qemu_exe,
                        agent_id: &self.agent_id,
                        requester: self.sandbox_requester.as_ref(),
                    };
                    match execute_tool(&call.function.name, &call.function.arguments, &mut ctx) {
                        Ok(result) => result,
                        Err(e) => format!("error: {e}"),
                    }
                } else {
                    let mut ctx = ToolContext {
                        policy: &self.policy,
                        audit: Arc::clone(&self.audit),
                        vm: &mut self.vm,
                        compiler: &self.compiler,
                        serial_observers: Arc::clone(&self.serial_observers),
                        qemu_exe: &self.qemu_exe,
                        agent_id: &self.agent_id,
                        requester: self.sandbox_requester.as_ref(),
                    };
                    match execute_tool(&call.function.name, &call.function.arguments, &mut ctx) {
                        Ok(result) => result,
                        Err(e) => format!("error: {e}"),
                    }
                };
                self.messages.push(ChatMessage::tool_result(
                    &call.id,
                    truncate(&text, MAX_MESSAGE_BYTES),
                ));
            }

            if iterations >= self.config.max_iterations {
                let last_content = self
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == "assistant")
                    .and_then(|m| m.content.clone())
                    .unwrap_or_default();
                return Ok(AgentOutcome::MaxIterations {
                    last_content,
                    iterations,
                });
            }
        }
    }

    /// Keep the system message + the most recent `CONTEXT_KEEP` messages.
    fn trim_context(&mut self) {
        if self.messages.len() <= CONTEXT_MAX {
            return;
        }
        let system = if self
            .messages
            .first()
            .map(|m| m.role == "system")
            .unwrap_or(false)
        {
            Some(self.messages[0].clone())
        } else {
            None
        };
        let start = self.messages.len() - CONTEXT_KEEP;
        let tail: Vec<ChatMessage> = self.messages[start..].to_vec();

        let mut trimmed = Vec::new();
        if let Some(s) = system {
            trimmed.push(s);
        }
        trimmed.push(ChatMessage::text(
            "system",
            format!(
                "[context truncated: earlier messages omitted, most recent {CONTEXT_KEEP} kept]"
            ),
        ));
        trimmed.extend(tail);
        self.messages = trimmed;
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated {} bytes]", &s[..end], s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_caps_long_text() {
        let long = "a".repeat(MAX_MESSAGE_BYTES + 100);
        let t = truncate(&long, MAX_MESSAGE_BYTES);
        assert!(t.len() < long.len());
        assert!(t.contains("truncated"));
    }

    #[test]
    fn truncate_keeps_short_text() {
        assert_eq!(truncate("hi", MAX_MESSAGE_BYTES), "hi");
    }

    #[test]
    fn trim_context_bounds_growth() {
        // Cannot build an AgentLoop without a real LLM easily here, so exercise
        // the trimming rule directly on a message vector mirroring the logic.
        fn trim(messages: &mut Vec<ChatMessage>) {
            if messages.len() <= CONTEXT_MAX {
                return;
            }
            let system = messages[0].clone();
            let start = messages.len() - CONTEXT_KEEP;
            let tail: Vec<ChatMessage> = messages[start..].to_vec();
            let mut trimmed = vec![system, ChatMessage::text("system", "[context truncated]")];
            trimmed.extend(tail);
            *messages = trimmed;
        }

        let mut messages = vec![ChatMessage::text("system", "sys")];
        for i in 0..100 {
            messages.push(ChatMessage::text("user", format!("m{i}")));
        }
        trim(&mut messages);
        assert_eq!(messages.len(), CONTEXT_KEEP + 2);
        assert_eq!(messages[0].role, "system");
        assert!(messages[1]
            .content
            .as_deref()
            .unwrap()
            .contains("truncated"));
    }
}
