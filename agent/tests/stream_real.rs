//! Stage 16a — real streaming end-to-end against the DeepSeek API.
//!
//! ```text
//! set DEEPSEEK_API_KEY=***
//! cargo test -p agent --test stream_real -- --ignored --nocapture
//! ```
//!
//! Requires network access and a valid key (read from the environment; never
//! printed).

use agent::llm::{LlmClient, OpenAiCompatClient};
use agent::message::{ChatMessage, ChatRequest, StreamEvent};
use agent::AgentConfig;

#[test]
#[ignore = "requires DEEPSEEK_API_KEY and network"]
fn real_api_streams_content_deltas() {
    let config = AgentConfig::from_env().expect("DEEPSEEK_API_KEY must be set");
    let client = OpenAiCompatClient::new(config.clone()).expect("client");

    let request = ChatRequest {
        model: config.model.clone(),
        messages: vec![ChatMessage::text("user", "Reply with exactly: HELLO RISCV")],
        tools: None,
        tool_choice: None,
        temperature: Some(0.0),
        stream: Some(true),
    };

    let mut deltas = 0usize;
    let mut text = String::new();
    let resp = client
        .chat_stream(request, &mut |event| {
            if let StreamEvent::Delta(d) = event {
                deltas += 1;
                text.push_str(&d);
            }
        })
        .expect("stream");

    println!("deltas: {deltas}");
    println!("text  : {}", text.trim());
    assert!(deltas >= 1, "expected at least one delta");
    assert_eq!(resp.content().unwrap_or_default(), text);
    assert!(!text.trim().is_empty());
}
