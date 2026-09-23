//! End-to-end tests for the CLI's control commands: the real `riscdom` binary
//! against a real control plane, in local (embedded) mode.
//!
//! Nothing here needs a model, a VM or the network: the workspace is a fresh temp
//! directory, so the host answers what it answers on an empty one — a `503` for a
//! run (no model configured), a reserved `501` for `vm start`, `{"deleted":false}`
//! for an unknown snapshot — and those answers are what the tests assert on.
//!
//! stdin is a pipe here, so the confirmation tests exercise the path a script or
//! an AI takes: without `--yes`, refuse.

use riscdom_cli::args::{parse, Parsed};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-cli-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Run the CLI in local mode with a workspace of its own.
///
/// The model environment is cleared first: a real `DEEPSEEK_API_KEY` in the
/// developer's shell would turn `run` into a real model call, and these tests
/// stay hermetic (no network, no VM). With no key the host answers its documented
/// `503 unavailable`, and that is what the tests assert on.
fn run(tag: &str, args: &[&str]) -> Output {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_riscdom"));
    command
        .arg("--workspace")
        .arg(&workspace)
        .arg("--data-dir")
        .arg(&data_dir)
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("DEEPSEEK_BASE_URL")
        .env_remove("DEEPSEEK_MODEL");
    command.args(args);
    command.output().expect("the CLI runs")
}

/// Run with a chosen workspace, for a sequence of commands.
fn run_in(workspace: &PathBuf, data_dir: &PathBuf, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_riscdom"));
    command
        .arg("--workspace")
        .arg(workspace)
        .arg("--data-dir")
        .arg(data_dir)
        .env_remove("DEEPSEEK_API_KEY")
        .env_remove("DEEPSEEK_BASE_URL")
        .env_remove("DEEPSEEK_MODEL");
    command.args(args);
    command.output().expect("the CLI runs")
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("an exit code")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// The error object the CLI wrote to stderr.
///
/// The embedded control plane logs a connection error to the same stderr, but
/// only when the process exits with the `--follow` stream still open — its reader
/// socket is then closed mid-response. The CLI's own object is always the last
/// non-empty line, so that is what is parsed; the interleaving itself is a finding
/// for the report, not something to freeze into an assertion here.
fn error_body(output: &Output) -> serde_json::Value {
    let text = stderr(output);
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string();
    serde_json::from_str(&line).unwrap_or_else(|e| {
        panic!(
            "the last stderr line is not a JSON error body ({e}): {line:?} (all of stderr {text:?}, stdout {:?})",
            stdout(output)
        )
    })
}

fn json_stdout(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {:?} (stderr {:?})",
            stdout(output),
            stderr(output)
        )
    })
}

#[test]
fn run_reports_the_hosts_refusal_when_no_model_is_configured() {
    // The control plane answers the documented `503 unavailable`, and the CLI
    // maps it to exit 3 with the error object on stderr.
    let output = run("run", &["--json", "run", "say hi"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "unavailable", "{body}");
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));

    // Human mode says the same thing in one line.
    let output = run("run-human", &["run", "say hi"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("unavailable"),
        "{}",
        stderr(&output)
    );
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn run_follow_terminates_instead_of_waiting_for_a_run_that_never_starts() {
    // `--follow` subscribes first and runs second. With no model the run fails
    // immediately and there are no agent events at all — the point here is that
    // the command *ends* (the reader thread is not allowed to hold it open).
    let output = run("follow", &["--json", "run", "say hi", "--follow"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "unavailable", "{body}");
}

#[test]
fn follow_is_refused_on_anything_but_run() {
    for args in [
        vec!["health", "--follow"],
        vec!["--follow", "status"],
        vec!["--follow", "vm", "stop"],
    ] {
        let output = run("follow-usage", &args);
        assert_eq!(exit_code(&output), 2, "{args:?}: {}", stderr(&output));
        assert!(
            stderr(&output).contains("--follow"),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_destructive_command_refuses_without_yes_when_stdin_is_a_pipe() {
    for args in [
        vec!["vm", "stop"],
        vec!["snapshots", "delete", "nope"],
        vec!["snapshots", "resume", "nope"],
        vec!["sessions", "delete", "s-1"],
        vec!["sessions", "clear-all"],
        // A switch stops the running VM and refuses while a run is in flight, so
        // it is in the same family (v0.9 sandbox F2b-2).
        vec!["sandboxes", "switch", "blink"],
    ] {
        let output = run("confirm", &args);
        assert_eq!(exit_code(&output), 2, "{args:?}: {}", stderr(&output));
        let text = stderr(&output);
        assert!(text.contains("confirmation"), "{args:?}: {text}");
        assert!(text.contains("--yes"), "{args:?}: {text}");
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
    }
}

#[test]
fn a_switch_to_an_unknown_sandbox_is_the_control_planes_404() {
    // `--yes` gets past the confirmation; the answer is the control plane's, and
    // the CLI keeps its status-derived exit code (3 — refused or failed).
    let output = run(
        "switch-missing",
        &["--json", "sandboxes", "switch", "no-such-sandbox", "--yes"],
    );
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "not_found", "{body}");
    assert_eq!(body["cause"], "name", "{body}");
}

#[test]
fn vm_stop_with_yes_succeeds_even_with_no_vm() {
    // The host's stop is idempotent: no VM is not an error.
    let output = run("vm-stop", &["vm", "stop", "--yes"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "ok");
}

#[test]
fn vm_start_is_reserved_and_says_so() {
    let output = run("vm-start", &["--json", "vm", "start"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "not_implemented", "{body}");
}

#[test]
fn deleting_an_unknown_snapshot_reports_that_nothing_was_deleted() {
    let output = run(
        "snapshot-delete",
        &["--json", "snapshots", "delete", "nope", "--yes"],
    );
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(json_stdout(&output)["deleted"], false);

    let output = run(
        "snapshot-delete-human",
        &["snapshots", "delete", "nope", "--yes"],
    );
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "nothing to delete");
}

#[test]
fn the_session_lifecycle_works_end_to_end() {
    let workspace = unique_dir("sessions-ws");
    let data_dir = unique_dir("sessions-data");

    let created = run_in(
        &workspace,
        &data_dir,
        &["--json", "sessions", "create", "work"],
    );
    assert_eq!(exit_code(&created), 0, "stderr: {}", stderr(&created));
    let session_id = json_stdout(&created)["session_id"]
        .as_str()
        .expect("a session id")
        .to_string();

    // Renaming answers `204`: nothing to print, success all the same.
    let renamed = run_in(
        &workspace,
        &data_dir,
        &["sessions", "rename", &session_id, "later"],
    );
    assert_eq!(exit_code(&renamed), 0, "stderr: {}", stderr(&renamed));
    assert_eq!(stdout(&renamed).trim(), "ok");

    // Opening it shows the title the rename set.
    let opened = run_in(
        &workspace,
        &data_dir,
        &["--json", "sessions", "open", &session_id],
    );
    assert_eq!(exit_code(&opened), 0, "stderr: {}", stderr(&opened));
    let detail = json_stdout(&opened);
    assert_eq!(detail["meta"]["title"], "later", "{detail}");
    assert!(detail["messages"].is_array(), "{detail}");

    // Deleting it needs the confirmation.
    let refused = run_in(&workspace, &data_dir, &["sessions", "delete", &session_id]);
    assert_eq!(exit_code(&refused), 2, "stderr: {}", stderr(&refused));

    let deleted = run_in(
        &workspace,
        &data_dir,
        &["sessions", "delete", &session_id, "--yes"],
    );
    assert_eq!(exit_code(&deleted), 0, "stderr: {}", stderr(&deleted));

    // And the list is empty again.
    let listed = run_in(
        &workspace,
        &data_dir,
        &["--json", "sessions", "open", &session_id],
    );
    assert_ne!(exit_code(&listed), 0, "the session should be gone");

    let cleared = run_in(&workspace, &data_dir, &["sessions", "clear-all", "--yes"]);
    assert_eq!(exit_code(&cleared), 0, "stderr: {}", stderr(&cleared));
}

#[test]
fn abandon_stale_is_idempotent_and_asks_nothing() {
    let output = run("abandon", &["--json", "runs", "abandon-stale"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(json_stdout(&output)["abandoned"], serde_json::json!([]));

    let output = run("abandon-human", &["runs", "abandon-stale"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "no stale runs");
}

#[test]
fn a_control_command_with_a_missing_argument_is_a_usage_error() {
    for args in [
        vec!["run"],
        vec!["snapshots", "save"],
        vec!["sessions", "rename", "s-1"],
    ] {
        let output = run("usage", &args);
        assert_eq!(exit_code(&output), 2, "{args:?}: {}", stderr(&output));
        assert!(
            stderr(&output).contains("usage: riscdom"),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn the_parser_agrees_with_the_binary_about_the_new_commands() {
    // A sanity check that the library view and the binary share one parser: the
    // commands the tests drive above are the ones `parse` produces.
    for (words, expected_path, expected_method) in [
        (vec!["run", "hi"], "/v0/agent/run", "POST"),
        (vec!["vm", "stop"], "/v0/vm/stop", "POST"),
        (vec!["vm", "start"], "/v0/vm/start", "POST"),
        (vec!["snapshots", "save", "x"], "/v0/snapshots/save", "POST"),
        (
            vec!["snapshots", "resume", "x"],
            "/v0/snapshots/resume",
            "POST",
        ),
        (
            vec!["snapshots", "delete", "x"],
            "/v0/snapshots/delete",
            "POST",
        ),
        (
            vec!["sessions", "create", "t"],
            "/v0/sessions/create",
            "POST",
        ),
        (vec!["sessions", "open", "s"], "/v0/sessions/open", "POST"),
        (
            vec!["sessions", "delete", "s"],
            "/v0/sessions/delete",
            "POST",
        ),
        (
            vec!["sessions", "rename", "s", "t"],
            "/v0/sessions/rename",
            "POST",
        ),
        (vec!["sessions", "clear-all"], "/v0/sessions/clear", "POST"),
        (
            vec!["runs", "abandon-stale"],
            "/v0/runs/abandon-stale",
            "POST",
        ),
    ] {
        let parsed = parse(words.iter().map(|w| w.to_string()).collect()).expect("parses");
        let args = match parsed {
            Parsed::Command(args) => *args,
            other => panic!("{words:?} is not a command: {other:?}"),
        };
        assert_eq!(args.command.request_path(), expected_path, "{words:?}");
        assert_eq!(args.command.method(), expected_method, "{words:?}");
    }
}
