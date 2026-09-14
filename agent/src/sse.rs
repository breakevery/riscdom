//! Minimal Server-Sent Events (SSE) parsing for streaming chat completions.

/// One parsed SSE line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A `data:` payload (already stripped of the prefix and one optional space).
    Data(String),
    /// The stream terminator (`data: [DONE]`).
    Done,
    /// Anything else: blank line, comment, or an unknown field.
    Ignore,
}

/// Parse a single SSE line.
pub fn parse_sse_line(line: &str) -> SseEvent {
    let without_eol = line.trim_end_matches(['\r', '\n']);
    if without_eol.trim().is_empty() {
        return SseEvent::Ignore;
    }
    let trimmed = without_eol.trim_start();
    if trimmed.starts_with(':') {
        // Comment / keep-alive.
        return SseEvent::Ignore;
    }
    if let Some(rest) = trimmed.strip_prefix("data:") {
        let payload = rest.strip_prefix(' ').unwrap_or(rest);
        if payload.trim() == "[DONE]" {
            return SseEvent::Done;
        }
        return SseEvent::Data(payload.to_string());
    }
    SseEvent::Ignore
}

/// Accumulates `data:` lines until an event boundary (a blank line), joining
/// multiple `data:` lines with `\n` as the spec requires.
#[derive(Debug, Default)]
pub struct SseAccumulator {
    parts: Vec<String>,
    done: bool,
}

impl SseAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw line; returns the completed payload, if the event ended.
    pub fn feed(&mut self, line: &str) -> Option<String> {
        match parse_sse_line(line) {
            SseEvent::Done => {
                self.done = true;
                None
            }
            SseEvent::Data(payload) => {
                self.parts.push(payload);
                None
            }
            SseEvent::Ignore => self.flush(),
        }
    }

    /// Emit any buffered payload (used at EOF, where some servers omit the
    /// trailing blank line).
    pub fn flush(&mut self) -> Option<String> {
        if self.parts.is_empty() {
            return None;
        }
        let joined = self.parts.join("\n");
        self.parts.clear();
        Some(joined)
    }

    /// Whether `[DONE]` has been seen.
    pub fn is_done(&self) -> bool {
        self.done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_comment_lines_are_ignored() {
        assert_eq!(parse_sse_line(""), SseEvent::Ignore);
        assert_eq!(parse_sse_line("   "), SseEvent::Ignore);
        assert_eq!(parse_sse_line(": keep-alive"), SseEvent::Ignore);
        assert_eq!(parse_sse_line("event: message"), SseEvent::Ignore);
    }

    #[test]
    fn data_prefix_forms() {
        assert_eq!(
            parse_sse_line("data: {\"a\":1}"),
            SseEvent::Data("{\"a\":1}".to_string())
        );
        // No space after the colon.
        assert_eq!(
            parse_sse_line("data:{\"a\":1}"),
            SseEvent::Data("{\"a\":1}".to_string())
        );
    }

    #[test]
    fn done_marker() {
        assert_eq!(parse_sse_line("data: [DONE]"), SseEvent::Done);
        assert_eq!(parse_sse_line("data:[DONE]"), SseEvent::Done);
    }

    #[test]
    fn multi_line_data_is_joined() {
        let mut acc = SseAccumulator::new();
        assert_eq!(acc.feed("data: line1"), None);
        assert_eq!(acc.feed("data: line2"), None);
        assert_eq!(acc.feed(""), Some("line1\nline2".to_string()));
        assert!(!acc.is_done());
    }

    #[test]
    fn done_is_reported_after_feeding() {
        let mut acc = SseAccumulator::new();
        assert_eq!(acc.feed("data: {\"x\":1}"), None);
        assert_eq!(acc.feed(""), Some("{\"x\":1}".to_string()));
        assert_eq!(acc.feed("data: [DONE]"), None);
        assert!(acc.is_done());
    }

    #[test]
    fn flush_emits_a_trailing_payload() {
        let mut acc = SseAccumulator::new();
        assert_eq!(acc.feed("data: tail"), None);
        assert_eq!(acc.flush(), Some("tail".to_string()));
        assert_eq!(acc.flush(), None);
    }
}
