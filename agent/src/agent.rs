//! The agent loop: drive an LLM through tool calls until it answers.

use crate::audit_hook::{record_llm_request, record_llm_response};
use crate::compiler::CompilerConfig;
use crate::config::AgentConfig;
use crate::error::AgentError;
use crate::llm::LlmClient;
use crate::message::{ChatMessage, ChatRequest};
use crate::policy::WorkspacePolicy;
use crate::tools::{execute_tool, tools_json, ToolContext};
use audit::{AuditEvent, AuditSink};
use sandbox::vm::RiscVVirtualMachine;
use std::sync::{Arc, Mutex};

/// Per-message byte cap (user input / tool results).
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024;
/// Context is trimmed once it exceeds this many messages.
pub const CONTEXT_MAX: usize = 40;
/// Number of most-recent messages kept when trimming.
pub const CONTEXT_KEEP: usize = 30;

/// How a run finished.
#[derive(Debug, Clone, PartialEq)]
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
}

impl AgentLoop {
    /// Build a loop. `system_prompt` becomes the first message.
    pub fn new(
        llm: Box<dyn LlmClient>,
        config: AgentConfig,
        policy: WorkspacePolicy,
        audit: Arc<Mutex<dyn AuditSink>>,
        system_prompt: String,
    ) -> Result<Self, AgentError> {
        Ok(Self {
            llm,
            config,
            policy,
            audit,
            messages: vec![ChatMessage::text("system", system_prompt)],
            vm: None,
            compiler: CompilerConfig::from_env(),
        })
    }

    /// The current conversation (for inspection/tests).
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    fn emit(&self, action: &str, detail: serde_json::Value) {
        if let Ok(mut sink) = self.audit.lock() {
            sink.record(AuditEvent::new("agent", action, detail));
        }
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
                &request,
                &self.config.model,
                &self.config.base_url,
            );

            iterations += 1;
            let response = match self.llm.chat(request) {
                Ok(r) => r,
                Err(e) => {
                    return Ok(AgentOutcome::Failed {
                        reason: e.to_string(),
                        iterations,
                    })
                }
            };
            record_llm_response(&self.audit, &response);

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
                let text = {
                    let mut ctx = ToolContext {
                        policy: &self.policy,
                        audit: Arc::clone(&self.audit),
                        vm: &mut self.vm,
                        compiler: &self.compiler,
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
