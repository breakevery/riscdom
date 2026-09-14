//! Stage 16a — SSE parsing and streaming client behaviour.

use agent::llm::{LlmClient, MockLlm, OpenAiCompatClient};
use agent::message::{ChatMessage, ChatRequest, ChatResponse, Choice, StreamEvent};
use agent::sse::{parse_sse_line, SseAccumulator, SseEvent};
use agent::{AgentConfig, AgentError};
use std::io::{Read, Write};
use std::net::TcpListener;

fn req() -> ChatRequest {
    ChatRequest {
        model: "mock".into(),
        messages: vec![ChatMessage::text("user", "hi")],
        tools: None,
        tool_choice: None,
        temperature: Some(0.0),
        stream: Some(true),
    }
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        id: None,
        model: None,
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }
}

// ---- SSE line parsing ------------------------------------------------------

#[test]
fn sse_edge_cases() {
    assert_eq!(parse_sse_line(""), SseEvent::Ignore);
    assert_eq!(parse_sse_line("\r"), SseEvent::Ignore);
    assert_eq!(parse_sse_line(": ping"), SseEvent::Ignore);
    assert_eq!(parse_sse_line("event: x"), SseEvent::Ignore);
    assert_eq!(parse_sse_line("data: {}"), SseEvent::Data("{}".to_string()));
    assert_eq!(parse_sse_line("data:{}"), SseEvent::Data("{}".to_string()));
    assert_eq!(parse_sse_line("data: [DONE]"), SseEvent::Done);
}

#[test]
fn sse_multi_line_data_is_joined_with_newline() {
    let mut acc = SseAccumulator::new();
    assert_eq!(acc.feed("data: a"), None);
    assert_eq!(acc.feed("data: b"), None);
    assert_eq!(acc.feed(""), Some("a\nb".to_string()));
}

// ---- Default trait implementation -----------------------------------------

/// Implements only `chat`; `chat_stream` must degrade to a single delta.
struct PlainLlm;

impl LlmClient for PlainLlm {
    fn chat(&self, _req: ChatRequest) -> Result<ChatResponse, AgentError> {
        Ok(text_response("plain fallback"))
    }
}

#[test]
fn default_chat_stream_degrades_to_chat() {
    let mut events = Vec::new();
    let resp = PlainLlm
        .chat_stream(req(), &mut |e| events.push(e))
        .expect("stream");

    assert_eq!(resp.content(), Some("plain fallback"));
    assert_eq!(
        events,
        vec![
            StreamEvent::Delta("plain fallback".to_string()),
            StreamEvent::Done
        ]
    );
}

// ---- MockLlm streaming -----------------------------------------------------

#[test]
fn mock_llm_streams_in_fixed_size_chunks() {
    let mock = MockLlm::new(vec![text_response("abcdefghij")]); // 10 chars
    let mut events = Vec::new();
    let resp = mock
        .chat_stream(req(), &mut |e| events.push(e))
        .expect("stream");

    let deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Delta(d) => Some(d.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec!["abcdefgh".to_string(), "ij".to_string()]);
    assert_eq!(events.last(), Some(&StreamEvent::Done));
    assert_eq!(resp.content(), Some("abcdefghij"));
}

// ---- Real SSE over a local HTTP server -------------------------------------

/// A one-shot SSE server that replies with `body` to the first request.
fn spawn_sse_server(body: String) -> (u16, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf); // consume the request headers
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}"
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, handle)
}

fn client_for(port: u16) -> OpenAiCompatClient {
    let config = AgentConfig {
        api_key: "test-key".into(),
        base_url: format!("http://127.0.0.1:{port}"),
        model: "mock".into(),
        provider_id: "custom".into(),
        max_iterations: 10,
        request_timeout_secs: 30,
    };
    OpenAiCompatClient::new(config).expect("client")
}

#[test]
fn openai_client_parses_a_text_sse_stream() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
        "data: [DONE]\n\n"
    )
    .to_string();
    let (port, handle) = spawn_sse_server(body);

    let mut events = Vec::new();
    let resp = client_for(port)
        .chat_stream(req(), &mut |e| events.push(e))
        .expect("stream");
    handle.join().ok();

    let deltas: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::Delta(d) => Some(d.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(deltas, vec!["Hel".to_string(), "lo".to_string()]);
    assert_eq!(events.last(), Some(&StreamEvent::Done));
    assert_eq!(resp.content(), Some("Hello"));
    assert!(resp.tool_calls().is_none());
}

#[test]
fn openai_client_buffers_tool_call_fragments() {
    let body = concat!(
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",",
        "\"function\":{\"name\":\"write_source\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,",
        "\"function\":{\"arguments\":\"\\\"a.c\\\"}\"}}]}}]}\n\n",
        "data: [DONE]\n\n"
    )
    .to_string();
    let (port, handle) = spawn_sse_server(body);

    let mut events = Vec::new();
    let resp = client_for(port)
        .chat_stream(req(), &mut |e| events.push(e))
        .expect("stream");
    handle.join().ok();

    // Tool-call fragments are reported as deltas, never as text deltas.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolCallDelta { index: 0, .. })),
        "expected ToolCallDelta events: {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(e, StreamEvent::Delta(_))),
        "tool calls must not produce text deltas"
    );

    let calls = resp.tool_calls().expect("tool calls");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_1");
    assert_eq!(calls[0].function.name, "write_source");
    assert_eq!(calls[0].function.arguments, "{\"path\":\"a.c\"}");
}
