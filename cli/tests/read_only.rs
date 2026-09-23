//! End-to-end tests for the CLI: the real `riscdom` binary, a real control plane.
//!
//! Nothing here touches QEMU or the network beyond loopback. The local mode starts
//! the control plane inside the CLI's own process, so a test only has to point
//! `--workspace` and `--data-dir` at temporary directories.
//!
//! The binary is driven the way `worker/tests/stdio.rs` drives its executor:
//! `CARGO_BIN_EXE_riscdom` is the path cargo built, and the test reads its streams.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

/// A directory no other test shares and nothing cleans up (temp).
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

/// Run the CLI with the local-mode flags a test needs, and no credential flags.
fn run(tag: &str, args: &[&str]) -> Output {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_riscdom"));
    command
        .arg("--workspace")
        .arg(&workspace)
        .arg("--data-dir")
        .arg(&data_dir);
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

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {:?} / stderr {:?}",
            stdout(output),
            stderr(output)
        )
    })
}

/// Write a token file and return its path.
fn token_file(dir: &Path, name: &str, token: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("{token}\n")).expect("token file");
    path
}

#[test]
fn help_prints_the_usage_and_exits_zero() {
    let output = run("help", &["--help"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("usage: riscdom"), "{text}");
    assert!(text.contains("--remote"), "{text}");
    assert!(text.contains("exit codes"), "{text}");
}

#[test]
fn version_prints_the_version_and_exits_zero() {
    let output = run("version", &["--version"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).starts_with("riscdom "),
        "{}",
        stdout(&output)
    );
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    for args in [
        vec!["nope"],
        vec![],
        vec!["runs"],
        vec!["audit", "nope"],
        vec!["runs", "get"],
    ] {
        let output = run("usage", &args);
        assert_eq!(exit_code(&output), 2, "{args:?}: {:?}", stderr(&output));
        assert!(
            stderr(&output).contains("usage: riscdom"),
            "{args:?}: {}",
            stderr(&output)
        );
        assert!(stdout(&output).is_empty(), "{args:?} wrote to stdout");
    }
}

#[test]
fn health_answers_in_json_locally() {
    let output = run("health-json", &["--json", "health"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let value = json(&output);
    assert_eq!(value["status"], "ok");
    assert!(value["version"].is_string(), "{value}");
    assert!(value["uptime_ms"].is_number(), "{value}");
}

#[test]
fn the_json_mode_passes_the_control_plane_through_and_the_human_mode_does_not() {
    let json_output = run("mode-json", &["--json", "status"]);
    assert_eq!(exit_code(&json_output), 0, "{}", stderr(&json_output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&json_output)).expect("JSON");
    // The documented fields, unchanged: the CLI adds nothing and drops nothing.
    for key in [
        "status",
        "version",
        "uptime_ms",
        "connections",
        "sse_subscribers",
        "agents",
        "agent_id",
    ] {
        assert!(value.get(key).is_some(), "{key} missing from {value}");
    }

    let human_output = run("mode-human", &["status"]);
    assert_eq!(exit_code(&human_output), 0, "{}", stderr(&human_output));
    let text = stdout(&human_output);
    assert!(!text.trim_start().starts_with('{'), "{text}");
    assert!(text.contains("agent_id"), "{text}");
    assert!(text.contains("sse_subscribers"), "{text}");
}

#[test]
fn the_read_only_commands_answer_and_agree_with_their_mode() {
    // Each of these must succeed against a fresh workspace, with no model, no VM
    // and no snapshot in it.
    for args in [
        vec!["--json", "health"],
        vec!["--json", "status"],
        vec!["--json", "agents"],
        vec!["--json", "runs", "list"],
        vec!["--json", "runs", "list", "--limit", "3"],
        vec!["--json", "audit", "status"],
        vec!["--json", "audit", "events"],
        vec!["--json", "audit", "events", "--limit", "5"],
        vec!["--json", "snapshots", "list"],
        vec!["--json", "sandboxes", "list"],
        vec!["--json", "sandboxes", "current"],
        vec!["--json", "sandboxes", "candidates"],
        vec!["--json", "sandboxes", "show", "default"],
    ] {
        let output = run("readonly", &args);
        assert_eq!(exit_code(&output), 0, "{args:?}: {}", stderr(&output));
        let _: serde_json::Value = json(&output);
    }

    // `agents` is the derived view in human mode; in JSON mode the rule is
    // passthrough, so it is the `/v0/status` document itself.
    let output = run("agents", &["--json", "agents"]);
    let value = json(&output);
    assert!(value["agents"].is_number(), "{value}");
    assert!(value["agent_id"].is_string(), "{value}");
    let output = run("agents-human", &["agents"]);
    let text = stdout(&output);
    assert!(text.starts_with("agents "), "{text}");
    assert!(text.contains("agent_id"), "{text}");
    assert!(!text.contains("connections"), "{text}");

    // `audit status` on a fresh workspace: an empty but intact chain.
    let output = run("audit", &["--json", "audit", "status"]);
    let value = json(&output);
    assert_eq!(value["chain"]["status"], "Intact", "{value}");
    assert_eq!(value["count"], 0, "{value}");

    // A human-mode empty list says so.
    let output = run("runs-human", &["runs", "list"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "no runs");

    // The sandbox registry: the built-in fallback is always there, and nothing is
    // stored on a fresh workspace.
    let output = run("sandboxes-json", &["--json", "sandboxes", "list"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let value = json(&output);
    assert!(value["current"].is_null(), "{value}");
    assert_eq!(value["default"], "default", "{value}");
    let rows = value["sandboxes"].as_array().expect("an array");
    assert!(!rows.is_empty(), "{value}");

    let output = run("sandboxes-human", &["sandboxes", "list"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("current"), "{text}");
    assert!(text.contains("default"), "{text}");
    assert!(text.contains("NAME"), "{text}");
    assert!(!text.trim_start().starts_with('{'), "{text}");

    // `show` on a name that is not there is the control plane's `404` (exit 3).
    let output = run(
        "sandboxes-missing",
        &["--json", "sandboxes", "show", "no-such-sandbox"],
    );
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));

    // `candidates` prints both families; the toolchain one is empty here because
    // the workspace and the data directory are both fresh.
    let output = run("sandboxes-candidates", &["sandboxes", "candidates"]);
    assert_eq!(exit_code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("TOOLCHAINS (0)"), "{text}");
    assert!(text.contains("QEMUS ("), "{text}");
}

#[test]
fn a_wrong_token_is_refused_with_four() {
    // A real control plane, started the way the CLI starts one, with a token we
    // know: the test needs to present a *different* one on purpose.
    let workspace = unique_dir("auth-ws");
    let data_dir = unique_dir("auth-data");
    let token = "cli-test-token-0123456789abcdef";
    token_file(&data_dir, "token", token);

    let server = riscdom_cli::client::start_embedded(&workspace, Some(&data_dir))
        .expect("an embedded control plane");
    let remote = server.base_url().trim_start_matches("http://").to_string();

    let wrong = unique_dir("auth-wrong");
    let wrong_token = token_file(&wrong, "token", "not-the-token");
    let output = Command::new(env!("CARGO_BIN_EXE_riscdom"))
        .args(["--json", "--remote", &remote, "--token-file"])
        .arg(&wrong_token)
        .arg("health")
        .output()
        .expect("the CLI runs");
    assert_eq!(exit_code(&output), 4, "stderr: {}", stderr(&output));
    // The failure body is the control plane's error object, on stderr.
    let body: serde_json::Value = serde_json::from_str(&stderr(&output)).expect("JSON error body");
    assert_eq!(body["code"], "unauthorized", "{body}");
    assert!(stdout(&output).is_empty(), "stdout: {}", stdout(&output));

    // The right token: the same request succeeds.
    let right = token_file(&wrong, "right-token", token);
    let output = Command::new(env!("CARGO_BIN_EXE_riscdom"))
        .args(["--json", "--remote", &remote, "--token-file"])
        .arg(&right)
        .arg("health")
        .output()
        .expect("the CLI runs");
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(json(&output)["status"], "ok");

    server.abort();
}

#[test]
fn a_remote_that_is_not_there_is_a_local_failure() {
    // Port 1 on loopback: nothing listens there, and nothing may be started.
    let output = run("refused", &["--json", "--remote", "127.0.0.1:1", "health"]);
    assert_eq!(exit_code(&output), 1, "stderr: {}", stderr(&output));
    let body: serde_json::Value = serde_json::from_str(&stderr(&output)).expect("JSON error body");
    assert_eq!(body["code"], "cli_error", "{body}");
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("cannot reach"));
}

#[test]
fn a_missing_token_file_is_an_authentication_failure() {
    let dir = unique_dir("no-token");
    let missing = dir.join("nope");
    let output = run(
        "no-token-run",
        &[
            "--json",
            "--remote",
            "127.0.0.1:1",
            "--token-file",
            &missing.display().to_string(),
            "health",
        ],
    );
    assert_eq!(exit_code(&output), 4, "stderr: {}", stderr(&output));
    let body: serde_json::Value = serde_json::from_str(&stderr(&output)).expect("JSON error body");
    assert_eq!(body["code"], "cli_error", "{body}");
    assert!(body["message"]
        .as_str()
        .unwrap_or_default()
        .contains("token"));
}
