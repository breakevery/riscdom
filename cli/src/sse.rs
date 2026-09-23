//! Reading the event stream (`GET /v0/events`).
//!
//! The wire format is the one `docs/control-plane-events.md` settles: `id:` and
//! `data:` lines, a blank line between frames, and comment lines (`: keep-alive`)
//! that carry no frame. There is deliberately no `event:` field, so a reader only
//! has to look at `data:`.
//!
//! `--follow` is why this exists: the CLI has to be *reading* the stream while the
//! run it started is still going, or the server's bounded channel would drop what
//! it missed (a lagging subscriber is told it lagged and loses those frames; the
//! `Last-Event-ID` handshake is how a client repairs that, and repairing mid-run
//! is not this batch).

use crate::client::Error;
use std::io::{BufRead, BufReader, Read};

/// One frame from the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The `id:` line, when the frame has one (`hello` frames do not).
    pub id: Option<String>,
    /// The `data:` payload, exactly as it arrived.
    pub data: String,
}

impl Frame {
    /// The frame's payload as JSON, when it parses.
    pub fn json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.data).ok()
    }

    /// The `event` field of an event envelope, when this is one.
    pub fn event(&self) -> Option<String> {
        self.json()?.get("event")?.as_str().map(str::to_string)
    }

    /// The `kind` field of an envelope (`hello` / `event` / `gap`).
    pub fn kind(&self) -> Option<String> {
        self.json()?.get("kind")?.as_str().map(str::to_string)
    }

    /// A frame built by hand, for the rendering and parsing tests that must not
    /// need a live connection.
    #[cfg(test)]
    pub fn for_test(id: Option<String>, data: &str) -> Self {
        Self {
            id,
            data: data.to_string(),
        }
    }
}

/// A live event stream, read frame by frame.
pub struct SseStream {
    reader: BufReader<reqwest::blocking::Response>,
}

impl SseStream {
    pub fn new(response: reqwest::blocking::Response) -> Self {
        Self {
            reader: BufReader::new(response),
        }
    }

    /// The next frame, or `None` at end of stream.
    ///
    /// Comment lines are skipped, so a heartbeat does not look like an empty
    /// frame; several `data:` lines in one frame are joined with newlines, which
    /// is what the SSE format says a reader should do.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, Error> {
        let mut id: Option<String> = None;
        let mut data: Vec<String> = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            let read = self
                .reader
                .read_line(&mut line)
                .map_err(|e| Error::local(format!("the event stream broke: {e}")))?;
            if read == 0 {
                // End of stream. A frame that was started but not terminated is
                // still a frame; an empty one is just the end.
                return Ok(if data.is_empty() {
                    None
                } else {
                    Some(Frame {
                        id,
                        data: data.join("\n"),
                    })
                });
            }
            let text = line.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                if data.is_empty() && id.is_none() {
                    continue; // a keep-alive or a stray blank line
                }
                return Ok(Some(Frame {
                    id,
                    data: data.join("\n"),
                }));
            }
            if let Some(value) = text.strip_prefix("id: ") {
                id = Some(value.to_string());
            } else if let Some(value) = text.strip_prefix("data: ") {
                data.push(value.to_string());
            }
            // `:` comment lines and unknown fields are ignored on purpose.
        }
    }
}

/// Read the whole body of a stream (tests, and a `--follow` that is already over).
pub fn read_to_string(response: &mut reqwest::blocking::Response) -> Result<String, Error> {
    let mut text = String::new();
    response
        .read_to_string(&mut text)
        .map_err(|e| Error::local(format!("the event stream broke: {e}")))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Parse a capture with the same rules, without a live connection.
    fn frames(text: &str) -> Vec<Frame> {
        let mut reader = BufReader::new(Cursor::new(text.as_bytes().to_vec()));
        let mut out = Vec::new();
        let mut id: Option<String> = None;
        let mut data: Vec<String> = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).expect("read") == 0 {
                if !data.is_empty() {
                    out.push(Frame {
                        id,
                        data: data.join("\n"),
                    });
                }
                return out;
            }
            let text = line.trim_end_matches(['\n', '\r']);
            if text.is_empty() {
                if data.is_empty() && id.is_none() {
                    continue;
                }
                out.push(Frame {
                    id: id.take(),
                    data: data.join("\n"),
                });
                data.clear();
            } else if let Some(value) = text.strip_prefix("id: ") {
                id = Some(value.to_string());
            } else if let Some(value) = text.strip_prefix("data: ") {
                data.push(value.to_string());
            }
        }
    }

    #[test]
    fn a_capture_parses_into_frames_and_heartbeats_are_skipped() {
        let text = concat!(
            "id: 1-0\n",
            "data: {\"version\":1,\"kind\":\"hello\",\"event\":null}\n",
            "\n",
            ": keep-alive\n",
            "\n",
            "id: 1-1\n",
            "data: {\"version\":1,\"kind\":\"event\",\"event\":\"agent:final\",\"payload\":{\"kind\":\"final\"}}\n",
            "\n",
        );
        let parsed = frames(text);
        assert_eq!(parsed.len(), 2, "{parsed:?}");
        assert_eq!(parsed[0].id.as_deref(), Some("1-0"));
        assert_eq!(parsed[0].kind().as_deref(), Some("hello"));
        assert_eq!(parsed[1].event().as_deref(), Some("agent:final"));
        assert_eq!(parsed[1].json().expect("json")["payload"]["kind"], "final");
    }

    #[test]
    fn a_frame_without_an_id_is_still_a_frame() {
        let parsed = frames("data: {\"kind\":\"hello\"}\n\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, None);
        assert_eq!(parsed[0].kind().as_deref(), Some("hello"));
    }

    #[test]
    fn several_data_lines_belong_to_one_frame() {
        let parsed = frames("id: 2-1\ndata: one\ndata: two\n\n");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].data, "one\ntwo");
        // Not JSON, so no envelope fields, and no panic.
        assert_eq!(parsed[0].kind(), None);
        assert_eq!(parsed[0].event(), None);
    }
}
