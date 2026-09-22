//! v0.5 batches 3–4 — the golden path, steps 3 to 7, end to end (ignored by default).
//!
//! One test walks the main line the v0.5 milestone is about, in order:
//!
//! 3. run an agent task (mock LLM, real QEMU guest);
//! 4. save a snapshot of the running guest;
//! 5. export that run's audit interval;
//! 6. roll back — restore the snapshot, which is a run of its own;
//! 7. change one fingerprint field and run again.
//!
//! Manual run:
//!
//! ```text
//! cargo test -p host-core --test golden_path -- --ignored --nocapture
//! ```
//!
//! Requires QEMU and a RISC-V bare-metal GCC. The run is driven by `MockLlm`, so
//! no API key and no network are involved; steps 1 and 2 (install, configure) are
//! the manual checklist's job, not this test's.
//!
//! The export is verified the way an outsider would: the file is written into an
//! empty database **by itself** — it is self-contained since batch 4, so no prefix and
//! no other artefact is needed — and the checker is run over that. `audit-verify` is a
//! binary of the `audit` crate, and a `host-core` test cannot name it with
//! `CARGO_BIN_EXE_*`; when this checkout has built it (the gate does) the real binary
//! is used, and when it has not, the test runs the exact library calls the binary
//! wraps and says so.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use audit::{verify_chain, AuditStore, ChainStatus, ACTION_RUN_END, ACTION_RUN_START};
use host_core::events::{RecordingEventSink, EV_AGENT_FINAL};
use host_core::state::{AppState, LlmConfigInput};
use host_core::EventSink;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-golden-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool_response(id: &str, name: &str, args: serde_json::Value) -> ChatResponse {
    ChatResponse {
        id: Some(format!("resp-{id}")),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: id.to_string(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: name.to_string(),
                        arguments: args.to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            finish_reason: Some("tool_calls".into()),
        }],
        usage: None,
    }
}

fn final_response(text: &str) -> ChatResponse {
    ChatResponse {
        id: Some("resp-final".into()),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }
}

/// Step 3's task: write, compile and boot the guest, then read its serial.
fn boot_script() -> Vec<ChatResponse> {
    vec![
        tool_response(
            "c1",
            "write_source",
            serde_json::json!({ "path": "hello.c", "content": HELLO_C }),
        ),
        tool_response(
            "c2",
            "compile",
            serde_json::json!({ "source_path": "hello.c", "output_elf": "hello.elf" }),
        ),
        tool_response(
            "c3",
            "start_vm",
            serde_json::json!({ "elf_path": "hello.elf" }),
        ),
        tool_response("c4", "read_serial", serde_json::json!({})),
        final_response("booted"),
    ]
}

/// Step 7's task: the guest is already up after the rollback, so just look at it.
fn inspect_script() -> Vec<ChatResponse> {
    vec![
        tool_response("c1", "read_serial", serde_json::json!({})),
        final_response("again"),
    ]
}

fn lines(path: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(path)
        .expect("export file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
        .collect()
}

/// Write the exported lines into a fresh database, in order — nothing else.
fn import_export(export: &Path, db: &Path) -> usize {
    let _ = AuditStore::open(db).expect("fresh store"); // schema (table + triggers)
    let exported = lines(export);
    {
        let conn = Connection::open(db).expect("raw connection");
        for line in &exported {
            conn.execute(
                "INSERT INTO audit_events \
                 (id, timestamp_ms, actor, action, detail_json, prev_hash, hash) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    line["id"].as_i64().unwrap(),
                    line["timestamp_ms"].as_i64().unwrap(),
                    line["actor"].as_str().unwrap(),
                    line["action"].as_str().unwrap(),
                    serde_json::to_string(&line["detail"]).unwrap(),
                    line["prev_hash"].as_str().unwrap(),
                    line["hash"].as_str().unwrap(),
                ],
            )
            .expect("import row");
        }
    }
    exported.len()
}

/// The checker binary this checkout built, if any.
fn audit_verify_bin() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .to_path_buf();
    ["audit-verify.exe", "audit-verify"]
        .iter()
        .map(|name| root.join("target").join("debug").join(name))
        .find(|p| p.is_file())
}

#[test]
#[ignore = "walks a real QEMU guest; run with --ignored"]
fn golden_path_steps_three_to_seven() {
    let ws = unique_dir("run");
    let state = AppState::in_memory(&ws).expect("state");
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn EventSink>)
        .expect("serial forwarder");
    state.set_llm_config(LlmConfigInput {
        provider_id: "deepseek".into(),
        api_key: "placeholder-not-used".into(),
        base_url: "https://api.deepseek.com".into(),
        model: "deepseek-chat".into(),
    });

    // ---- step 3: run an agent task -----------------------------------------
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(boot_script())));
    let first = state
        .run_agent(sink.clone() as Arc<dyn EventSink>, "boot hello world")
        .expect("first run");
    assert_eq!(first.kind, "final", "{first:?}");
    assert_eq!(sink.count(EV_AGENT_FINAL), 1);
    assert!(
        sink.serial_text().contains("HELLO RISCV"),
        "the guest must have booted: {:?}",
        sink.serial_text()
    );
    assert!(
        state.vm_is_running(),
        "step 4 needs the host to own a running VM"
    );

    let task_a = state.list_runs(20).expect("runs");
    assert_eq!(task_a.len(), 1, "one task, one run");
    let run_a = task_a[0].clone();
    assert_eq!(run_a.status, "ok");
    assert!(run_a.ended_at_ms.is_some(), "step 3's run is closed");
    assert_eq!(
        run_a.resumed_from_snapshot, None,
        "a task run is not a restore"
    );
    println!("run A: {} ({})", run_a.run_id, run_a.fingerprint_short);

    // ---- step 4: save a snapshot -------------------------------------------
    let bytes = state.save_snapshot_real("golden-snap").expect("save");
    assert!(bytes > 0, "the .mig snapshot must not be empty");
    let listed = state.list_snapshots().expect("list snapshots");
    assert!(
        listed.iter().any(|s| s.name == "golden-snap"),
        "the snapshot must be listable: {listed:?}"
    );

    // ---- step 5: export this run's audit record ----------------------------
    std::fs::create_dir_all(ws.join("exports")).expect("export dir");
    let written = state
        .export_run_audit(&run_a.run_id, "exports/run_a.jsonl".into())
        .expect("export the run's record");
    let export = ws.join("exports").join("run_a.jsonl");
    let exported = lines(&export);
    assert_eq!(written, exported.len());
    assert!(
        exported.iter().any(|l| l["action"] == ACTION_RUN_START),
        "the record contains the run's opening marker"
    );
    assert_eq!(exported.last().unwrap()["action"], ACTION_RUN_END);
    assert_eq!(exported.last().unwrap()["detail"]["run_id"], run_a.run_id);

    // An outsider's check: the file goes into an empty database **alone** — no prefix
    // carried over from this one — and the checker judges what it finds (v0.5 batch 4).
    let start_seq = state
        .audit
        .lock()
        .expect("audit")
        .get_run(&run_a.run_id)
        .expect("get run")
        .expect("the run is indexed")
        .start_seq;
    assert_eq!(
        exported[0]["prev_hash"],
        audit::GENESIS_PREV_HASH,
        "the file starts at the chain's first event"
    );
    assert!(
        exported[0]["id"].as_i64().unwrap() < start_seq,
        "the run does not open the chain, so the file carries more than the run"
    );

    let db = ws.join("verify.db");
    let imported = import_export(&export, &db);
    assert_eq!(imported, exported.len());

    let verdict = match audit_verify_bin() {
        Some(bin) => {
            // 1. The file alone: the chain verdict needs nothing this process holds.
            let out = Command::new(&bin)
                .arg(&db)
                .output()
                .expect("run audit-verify");
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            println!("audit-verify ({}) -> {}", bin.display(), out.status);
            println!("{stdout}");
            assert_eq!(
                out.status.code(),
                Some(0),
                "stdout: {stdout}\nstderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(stdout.contains("Intact"), "stdout: {stdout}");

            // 2. The run is recoverable from it: the derived index rebuilds, and
            //    `--runs` then agrees. (The rebuild is `audit-rebuild`'s job, not
            //    something the exported file has to carry.)
            AuditStore::open(&db)
                .expect("store")
                .rebuild_run_index()
                .expect("rebuild the imported index");
            let out = Command::new(&bin)
                .arg(&db)
                .arg("--runs")
                .output()
                .expect("run audit-verify --runs");
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            println!("audit-verify --runs -> {}", out.status);
            println!("{stdout}");
            assert_eq!(out.status.code(), Some(0), "stdout: {stdout}");
            assert!(stdout.contains("Intact"), "stdout: {stdout}");
            assert!(
                stdout.contains("RunIndex { findings: 0 }"),
                "stdout: {stdout}"
            );
            stdout
        }
        None => {
            println!(
                "audit-verify binary not found under target/debug; \
                 running the library calls it wraps"
            );
            let store = AuditStore::open(&db).expect("store");
            assert!(matches!(
                verify_chain(&store).expect("verify"),
                ChainStatus::Intact { .. }
            ));
            let mut store = AuditStore::open(&db).expect("store");
            store.rebuild_run_index().expect("rebuild");
            assert!(
                store.check_run_index().expect("check").is_empty(),
                "the imported index must agree with the imported chain"
            );
            "Intact (library path)".to_string()
        }
    };
    assert!(verdict.contains("Intact"), "verdict: {verdict}");

    // ---- step 6: roll back (the restore is a run of its own) ---------------
    state
        .resume_from_snapshot_real("golden-snap")
        .expect("restore the snapshot");
    assert!(state.vm_is_running(), "the restored VM must be in the slot");

    let after_restore = state.list_runs(20).expect("runs");
    assert_eq!(after_restore.len(), 2, "a restore is its own run");
    let restored = after_restore
        .iter()
        .find(|r| r.run_id != run_a.run_id)
        .expect("the restore run");
    assert_eq!(
        restored.resumed_from_snapshot.as_deref(),
        Some("golden-snap"),
        "the restore run names the snapshot it came from"
    );
    assert_eq!(
        restored.parent_run_id.as_deref(),
        Some(run_a.run_id.as_str()),
        "the restore links to the run that produced the snapshot"
    );
    assert_eq!(restored.status, "ok");
    println!(
        "restore run: {} (from {})",
        restored.run_id,
        restored.resumed_from_snapshot.as_deref().unwrap_or("?")
    );

    // ---- step 7: change one fingerprint field and run again ----------------
    state.set_llm_config(LlmConfigInput {
        provider_id: "deepseek".into(),
        api_key: "placeholder-not-used".into(),
        base_url: "https://api.deepseek.com".into(),
        model: "deepseek-reasoner".into(), // the one changed field
    });
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(inspect_script())));
    let second = state
        .run_agent(sink as Arc<dyn EventSink>, "look at it again")
        .expect("second run");
    assert_eq!(second.kind, "final", "{second:?}");

    let all = state.list_runs(20).expect("runs");
    assert_eq!(all.len(), 3, "two tasks plus the restore");
    let run_b = all
        .iter()
        .find(|r| r.run_id != run_a.run_id && r.run_id != restored.run_id)
        .expect("the second task run");
    assert_eq!(run_b.status, "ok");
    assert!(run_b.ended_at_ms.is_some(), "the second run is closed");
    assert_eq!(
        run_b.resumed_from_snapshot, None,
        "the second task started from scratch, not from the snapshot"
    );

    // The two runs pair up and differ in the field that was changed.
    println!("run B: {} ({})", run_b.run_id, run_b.fingerprint_short);
    assert_ne!(
        run_a.fingerprint, run_b.fingerprint,
        "changing the model must change the fingerprint"
    );
    assert_ne!(run_a.fingerprint_short, run_b.fingerprint_short);
    assert_eq!(run_a.fingerprint.len(), 64);
    assert_eq!(run_b.fingerprint.len(), 64);

    // "Side by side": both runs are in the list with the digests the panel shows.
    let (short_a, short_b) = (&run_a.fingerprint_short, &run_b.fingerprint_short);
    assert_ne!(
        short_a, short_b,
        "the panel must show two different digests"
    );
    assert_eq!(short_a.len(), 16, "the panel shows 16 hex characters");
    assert_eq!(short_b.len(), 16, "the panel shows 16 hex characters");

    // The whole walk leaves the chain intact and the index in step.
    let status = state.audit_status().expect("audit status");
    println!("chain: {:?}", status.chain);
    assert!(
        matches!(status.chain, host_core::ChainStatusView::Intact { .. }),
        "chain not intact: {:?}",
        status.chain
    );
    assert!(
        state
            .audit
            .lock()
            .unwrap()
            .check_run_index()
            .expect("check")
            .is_empty(),
        "the derived index must agree with the chain"
    );
}
