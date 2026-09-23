//! v0.9 interface E2 — the executor's tool schema document must match the code.
//!
//! `docs/tool-schema-executor.md` carries the `tools[]` array an executor's model is
//! offered on every turn (`agent/src/agent.rs` fills the request's `tools` field with
//! `tools_json()`). Hand-written JSON drifts; this test is what makes the document fail
//! instead — in both directions, a changed tool and a typo in the document.
//!
//! The other half of E2 is the control plane's own tool schema
//! (`docs/tool-schema-control-plane.md`), checked from `server/src/routes.rs`'s tests,
//! where the route table lives.

use agent::tools::tools_json;
use serde_json::Value;

/// The document, embedded at compile time: a missing file is a build error, which is the
/// right failure for a document the interface promises to have.
const DOC: &str = include_str!("../../docs/tool-schema-executor.md");

/// The document's one fenced `json` block, parsed.
fn the_json_block() -> Value {
    let mut body = String::new();
    let mut inside = false;
    let mut blocks = 0usize;
    for line in DOC.lines() {
        if !inside && line.trim() == "```json" {
            inside = true;
            continue;
        }
        if inside && line.trim() == "```" {
            inside = false;
            blocks += 1;
            continue;
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    assert_eq!(
        blocks, 1,
        "the document must carry exactly one ```json block"
    );
    serde_json::from_str(&body).expect("the block is JSON")
}

#[test]
fn the_document_is_the_tool_json_the_model_receives() {
    let documented = the_json_block();
    let live = serde_json::to_value(tools_json()).expect("serde");
    assert_eq!(
        documented, live,
        "docs/tool-schema-executor.md no longer matches agent::tools::tools_json()"
    );
}

#[test]
fn the_document_names_every_tool_in_its_human_table_too() {
    let documented = the_json_block();
    let entries = documented.as_array().expect("an array of tools");
    // The catalogue is eight tools; the number is asserted here so that a ninth one
    // cannot land without this document being touched.
    assert_eq!(entries.len(), 8, "the executor's tool catalogue");
    for entry in entries {
        let name = entry["function"]["name"].as_str().expect("a name");
        assert!(
            DOC.contains(&format!("| `{name}` |")),
            "the human table does not list {name}"
        );
    }
}
