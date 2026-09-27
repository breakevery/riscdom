//! v1.0 M2a-1 — the instance table.
//!
//! What is exercised here: a node starts with **one** instance (its own — the slot
//! a switch and a plain run act on, so nothing that existed before the instance
//! model had to change meaning); registering an instance gives it its own VM slot,
//! its own serial buffers and its own snapshot directory; **deriving an instance
//! does not change what the node is running**; an instance can be stopped and
//! forgotten while the node's own stays; and the snapshot walk still finds what the
//! older per-agent layout wrote.
//!
//! **No QEMU is run.** `spawn_instance`'s success path needs a real guest — the same
//! ticket the golden path walks (`tests/golden_path.rs`, `--ignored`) — so what is
//! pinned here is the table half (`register_instance`), the refusal path, and the
//! fact that a spawn which cannot start leaves no trace in the table. The
//! post-registration failure path (`start` refuses a runnable stand-in) is
//! `tests/sandbox_switch.rs`'s for the switch, and is deliberately not pretended at
//! for spawn.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice};
use agent::InstanceId;
use host_core::events::RecordingEventSink;
use host_core::state::AppState;
use host_core::{EventSink, HostError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The sink an instance act's own event lands in (v1.0 gap 2/N).
fn event_sink() -> Arc<dyn EventSink> {
    Arc::new(RecordingEventSink::new())
}

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-instances-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A state whose data directory and workspace this test owns.
fn state(tag: &str) -> AppState {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    AppState::with_data_dir(&workspace, &data_dir).expect("state")
}

/// A runnable binary that is not the tool it pretends to be, so a definition
/// passes the host's checks and fails later.
fn stand_in(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    #[cfg(target_os = "windows")]
    {
        std::fs::copy(r"C:\Windows\System32\cmd.exe", &path).expect("copy cmd.exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::copy("/bin/echo", &path).expect("copy echo");
    }
    path
}

/// An LLM that answers once with a final message, so a run can be driven
/// hermetically (the same shape `task_sandbox.rs` uses).
fn scripted_llm(text: &str) -> Box<dyn agent::LlmClient> {
    Box::new(MockLlm::new(vec![ChatResponse {
        id: Some("resp-final".into()),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }]))
}

/// Seed `<data_dir>/settings.json` with one definition the host's checks accept
/// without a real QEMU or GCC: a runnable stand-in and a file.
fn seed_runnable_definition(data_dir: &Path, name: &str) {
    let compiler = data_dir.join("stand-in-gcc");
    std::fs::write(&compiler, b"not really a compiler").expect("compiler");
    std::fs::write(
        data_dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "sandboxes": [{
                "name": name,
                "qemu_exe": stand_in(data_dir, "stand-in-qemu").display().to_string(),
                "toolchain_path": compiler.display().to_string(),
            }],
            "default_sandbox": name,
        }))
        .expect("json"),
    )
    .expect("settings");
}

#[test]
fn a_node_starts_with_exactly_its_own_instance() {
    let app = state("own");
    let own = app.own_instance_id().clone();

    assert_eq!(app.instance_ids().len(), 1, "one instance to begin with");
    assert_eq!(app.current_instance_id(), own, "its own is what it runs");
    assert!(app.instance_ids().contains(&own));
    assert_eq!(
        app.instance_definition(&own).as_deref(),
        Some(""),
        "the node's own instance has no definition until a switch names one"
    );

    // The alias the pre-M2a paths use is the current instance's slot, not a second
    // slot beside it.
    let handle = app.instance(&own).expect("the handle");
    assert!(Arc::ptr_eq(&app.vm_slot(), &handle.vm_slot));
    assert!(Arc::ptr_eq(&app.vm_slot(), &app.current_instance().vm_slot));
}

#[test]
fn a_derived_instance_has_its_own_slot_buffers_and_snapshot_directory() {
    let app = state("derive");
    let own = app.instance(&app.own_instance_id().clone()).expect("own");

    let first = app.register_instance("blink");
    let second = app.register_instance("blink");

    assert_ne!(first.id, second.id, "each instance mints its own identity");
    assert_eq!(app.instance_ids().len(), 3, "own + two derived");
    assert_eq!(app.instance_definition(&first.id).as_deref(), Some("blink"));

    // Separate state, which is what "one running thing" means (v1.0 M2a-1): two
    // guests must not share a slot, a serial stream or a snapshot name.
    assert!(!Arc::ptr_eq(&first.vm_slot, &second.vm_slot));
    assert!(!Arc::ptr_eq(&first.serial_senders, &second.serial_senders));
    assert!(!Arc::ptr_eq(&first.serial_accum, &second.serial_accum));
    assert!(!Arc::ptr_eq(
        &first.vm_started_at_ms,
        &second.vm_started_at_ms
    ));
    assert!(!Arc::ptr_eq(&first.vm_slot, &own.vm_slot));

    // Each instance writes its own serial text, and reads back only its own.
    first.serial_accum.lock().unwrap().push_str("first");
    second.serial_accum.lock().unwrap().push_str("second");
    assert_eq!(
        app.serial_buffer(),
        "",
        "the node reads its current instance"
    );

    // Snapshot directories are per instance, under the same root.
    let root = app.snapshot_root();
    for instance in [&own, &first, &second] {
        assert!(instance.snapshot_dir.starts_with(&root), "{instance:?}");
    }
    assert_ne!(first.snapshot_dir, second.snapshot_dir);
    assert_ne!(first.snapshot_dir, own.snapshot_dir);
    assert!(
        app.snapshot_dir()
            .ends_with(app.current_instance_id().as_str()),
        "the node's own snapshot directory is the current instance's"
    );
}

#[test]
fn deriving_an_instance_does_not_change_what_the_node_runs() {
    let app = state("current");
    let own = app.own_instance_id().clone();
    let own_slot = app.vm_slot();

    let derived = app.register_instance("scratch");
    assert_eq!(
        app.current_instance_id(),
        own,
        "registering is not adopting (v1.0 M2a-1)"
    );
    assert!(
        Arc::ptr_eq(&app.vm_slot(), &own_slot),
        "same slot as before"
    );
    assert_ne!(app.current_instance_id(), derived.id);

    // A spawn that cannot start changes nothing either: the definition is looked
    // up (and missing), so no instance is registered and the current pointer —
    // which `spawn_instance` never touches — is where it was.
    let refused = app.spawn_instance("no-such-definition", None, event_sink());
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(app.instance_ids().len(), 2, "the derived one, and the own");
    assert_eq!(app.current_instance_id(), own);
    assert!(Arc::ptr_eq(&app.vm_slot(), &own_slot));
}

#[test]
fn stopping_an_instance_forgets_it_and_keeps_the_nodes_own() {
    let app = state("stop");
    let own = app.own_instance_id().clone();
    let derived = app.register_instance("scratch");

    app.stop_instance(&derived.id, None, event_sink())
        .expect("stop");
    assert!(
        app.instance(&derived.id).is_none(),
        "a stopped instance leaves the table"
    );
    assert_eq!(app.instance_ids(), vec![own.clone()]);

    // The node's own instance is not removable: it is the slot a switch and a run
    // act on, so stopping it empties the slot and keeps the entry.
    app.stop_instance(&own, None, event_sink())
        .expect("stop the node's own");
    assert!(app.instance(&own).is_some(), "the own instance stays");
    assert!(!app.vm_is_running());

    let unknown = InstanceId::new("not-an-instance");
    assert!(
        app.stop_instance(&unknown, None, event_sink()).is_err(),
        "no such instance"
    );
}

#[test]
fn the_snapshot_walk_still_reads_the_older_per_agent_layout() {
    let app = state("snapshots");

    // What v0.8 wrote: one directory per *agent*, under the shared root.
    let legacy = app.snapshot_root().join(app.agent_id());
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    let legacy_file = legacy.join("old.mig");
    std::fs::write(&legacy_file, b"not really a snapshot").expect("write");

    let listed = app.list_snapshots().expect("list");
    assert!(
        listed.iter().any(|s| s.name == "old"),
        "the older layout is still listed: {listed:?}"
    );

    // And it can still be deleted through the host, which walks the same chain.
    assert!(app.delete_snapshot("old").expect("delete"));
    assert!(!legacy_file.is_file(), "the legacy file is gone");

    // A hand-written file at the shared root (the layout before that) is found too.
    let root_file = app.snapshot_root().join("ancient.mig");
    std::fs::write(&root_file, b"x").expect("write root");
    assert!(
        app.list_snapshots()
            .expect("list")
            .iter()
            .any(|s| s.name == "ancient"),
        "the oldest layout is still listed"
    );
}

#[test]
fn a_derive_that_cannot_start_leaves_no_trace() {
    // A definition the checks accept and the start refuses: a runnable stand-in
    // where QEMU should be, a file where the compiler should be, and a kernel in
    // the workspace so nothing refuses before the VM does. What matters is that the
    // instance the derive registered does **not** survive the failure — a table that
    // keeps a VM that never ran is a table that lies (v1.0 M2a-1's rule, reached
    // here through the derive path the endpoint uses).
    let workspace = unique_dir("derive-fail");
    std::fs::write(workspace.join("hello.elf"), b"not really an ELF").expect("kernel");
    let data_dir = unique_dir("derive-fail-data");

    let qemu = stand_in(&data_dir, "stand-in-qemu");
    let compiler = data_dir.join("stand-in-gcc");
    std::fs::write(&compiler, b"not really a compiler").expect("compiler");
    std::fs::write(
        data_dir.join("settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": 1,
            "sandboxes": [{
                "name": "blink",
                "qemu_exe": qemu.display().to_string(),
                "toolchain_path": compiler.display().to_string(),
            }],
        }))
        .expect("json"),
    )
    .expect("settings");

    let app = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    let before = app.instance_ids();
    let refused = app.spawn_instance("blink", None, event_sink());
    assert!(refused.is_err(), "a stand-in is not a QEMU: {refused:?}");
    assert_eq!(app.instance_ids(), before, "the table is where it was");
    assert!(!app.vm_is_running(), "nothing is running");
}

#[test]
fn a_run_that_names_an_instance_is_checked_before_the_environment() {
    // v1.0 M2a-3: the instance check is the first thing a run does, so both
    // refusals hold without a model, a QEMU or a GCC.
    let workspace = unique_dir("run-check-ws");
    let data_dir = unique_dir("run-check-data");
    seed_runnable_definition(&data_dir, "blink");
    let app = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    let sink = || Arc::new(RecordingEventSink::new()) as Arc<dyn EventSink>;

    // An id this node does not own: the caller's parameter.
    let unknown = InstanceId::new("local-0-999999");
    let err = app
        .run_agent_for(sink(), "hi", None, Some(&unknown))
        .expect_err("no such instance");
    assert!(matches!(err, HostError::InstanceNotFound(_)), "{err:?}");

    // An instance that exists, declared next to a different definition: a clash,
    // not a silent preference for one of the two.
    let scratch = app.register_instance("scratch");
    let err = app
        .run_agent_for(sink(), "hi", Some("blink"), Some(&scratch.id))
        .expect_err("not that definition");
    assert!(matches!(err, HostError::InstanceConflict(_)), "{err:?}");

    // A pair that belongs together gets past the check to the environment, which
    // has no model configured here — the refusal after it is the environment's.
    let blink = app.register_instance("blink");
    let err = app
        .run_agent_for(sink(), "hi", Some("blink"), Some(&blink.id))
        .expect_err("no model is configured");
    assert!(matches!(err, HostError::NotConfigured(_)), "{err:?}");
}

#[test]
fn the_run_start_event_records_the_definition_and_the_instance() {
    // A run that gets all the way through: a definition whose QEMU and compiler are
    // pinned files (the host's checks only ask whether they are there) and a model
    // that answers in one turn — so no guest is started and nothing leaves the
    // machine.
    let workspace = unique_dir("run-record-ws");
    let data_dir = unique_dir("run-record-data");
    seed_runnable_definition(&data_dir, "blink");
    let app = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    *app.llm_override.lock().unwrap() = Some(Arc::from(scripted_llm("done")));

    let instance = app.register_instance("blink");
    let view = app
        .run_agent_for(
            Arc::new(RecordingEventSink::new()) as Arc<dyn EventSink>,
            "hi",
            Some("blink"),
            Some(&instance.id),
        )
        .expect("a run with a stand-in toolchain and a scripted model");
    assert_eq!(view.kind, "final", "run failed: {:?}", view.reason);

    // What each `run.start` records: the definition it resolved to and the instance
    // it ran on (v1.0 M2a-3) — the link the chain could not make before.
    let run_starts = |app: &AppState| -> Vec<(String, String)> {
        app.list_events(200, None, None)
            .expect("events")
            .into_iter()
            .filter(|event| event.action == "run.start")
            .map(|event| {
                (
                    event.detail["sandbox"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    event.detail["instance"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect()
    };
    let after_first = run_starts(&app);
    assert_eq!(after_first.len(), 1, "one run so far");
    assert_eq!(after_first[0].0, "blink", "the definition it resolved to");
    assert_eq!(
        after_first[0].1,
        instance.id.as_str(),
        "the instance it ran on"
    );

    // A plain run records the node's **current** instance, so the chain answers
    // "which instance ran" even when nobody declared one. It needs its own scripted
    // model: a `MockLlm` hands out its responses in order, one turn each.
    *app.llm_override.lock().unwrap() = Some(Arc::from(scripted_llm("done")));
    let plain = app
        .run_agent_for(
            Arc::new(RecordingEventSink::new()) as Arc<dyn EventSink>,
            "hi",
            None,
            None,
        )
        .expect("a plain run");
    assert_eq!(plain.kind, "final", "run failed: {:?}", plain.reason);
    let after_second = run_starts(&app);
    assert_eq!(after_second.len(), 2, "two runs");
    assert!(
        after_second
            .iter()
            .any(|(_, recorded)| recorded == app.current_instance_id().as_str()),
        "the chain names the node's own instance: {after_second:?}"
    );
}
