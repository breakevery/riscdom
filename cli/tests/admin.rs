//! End-to-end tests for the CLI's export and configuration commands: the real
//! `riscdom` binary against a real control plane, in local (embedded) mode.
//!
//! Everything here stays inside a temp workspace and a temp data directory, and
//! the model environment is cleared (C3's lesson: a real `DEEPSEEK_API_KEY` in the
//! developer's shell would turn a command into a real network call). No QEMU, no
//! downloads, no model.
//!
//! One command is deliberately **not** driven here: `preflight run`. It compiles a
//! guest and boots QEMU, which this batch is not allowed to do; its argument
//! parsing is covered by the unit tests, its `--wait` terminal test by
//! `lib::tests`, and `--wait` itself end to end by `toolchain download --wait`
//! (the one async path that can finish without touching the network).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-admin-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// One command, with a workspace and a data dir of its own.
fn run(tag: &str, args: &[&str]) -> Output {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    run_in(&workspace, &data_dir, args)
}

/// One command, with the workspace and data dir a test needs.
fn run_in(workspace: &Path, data_dir: &Path, args: &[&str]) -> Output {
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

/// The JSON object the CLI wrote to stderr, ignoring any log line the embedded
/// server may have interleaved in front of it.
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
            "the last stderr line is not a JSON error body ({e}): {line:?} (all of stderr {text:?})"
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

/// A data dir whose toolchain directory already holds a compiler.
///
/// `download_and_install` is **idempotent**: an install directory that already
/// contains a compiler emits `done` and returns, so the download path can be
/// driven end to end without a single network call. The file is a real
/// executable — a copy of this very CLI, whose `--version` exits `0` — because the
/// host runs it to check that the toolchain works.
fn data_dir_with_a_toolchain(tag: &str) -> PathBuf {
    let data_dir = unique_dir(tag);
    // `dest_root.join(spec.version)`, and the version is the pinned xPack release.
    let install = data_dir.join("toolchain").join("15.2.0-1").join("bin");
    std::fs::create_dir_all(&install).expect("install dir");
    std::fs::copy(
        env!("CARGO_BIN_EXE_riscdom"),
        install.join("riscv64-unknown-elf-gcc.exe"),
    )
    .expect("a compiler-shaped executable");
    data_dir
}

#[test]
fn the_exports_write_into_the_workspace_and_say_how_much() {
    let workspace = unique_dir("export-ws");
    let data_dir = unique_dir("export-data");

    // Records something first: an export of an untouched chain is a valid, empty
    // file, and the interesting case is the one with events in it.
    let activity = run_in(&workspace, &data_dir, &["theme", "set", "dark"]);
    assert_eq!(exit_code(&activity), 0, "stderr: {}", stderr(&activity));

    // The default path is a name the *server* resolves against the workspace.
    let output = run_in(&workspace, &data_dir, &["export", "audit-jsonl"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    let line = stdout(&output);
    assert!(line.starts_with("exported 1 event to "), "{line}");
    assert!(line.trim_end().ends_with("audit.jsonl"), "{line}");
    let written = workspace.join("audit.jsonl");
    assert!(written.is_file(), "the export is not there: {written:?}");
    let text = std::fs::read_to_string(&written).expect("read the export");
    assert_eq!(text.lines().count(), 1, "one event, one line: {text}");

    // `--out` is passed through as the endpoint's `path`, and the number that
    // comes back is the number of events written (the field is `events_exported`).
    let output = run_in(
        &workspace,
        &data_dir,
        &["--json", "export", "audit-jsonl", "--out", "sub.jsonl"],
    );
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    let sub = workspace.join("sub.jsonl");
    let sub_lines = std::fs::read_to_string(&sub)
        .expect("read the export")
        .lines()
        .count();
    let body = json_stdout(&output);
    assert_eq!(body["events_exported"], sub_lines as i64, "{body}");
    assert!(body.get("bytes_written").is_none(), "{body}");

    // The serial log: nothing has been captured, so it is an empty file.
    let output = run_in(&workspace, &data_dir, &["export", "serial-log"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "wrote 0 bytes to serial.log");
    assert!(workspace.join("serial.log").is_file());

    // An export does not create directories: the write is the host's, and it
    // fails where the directory is missing.
    let output = run_in(
        &workspace,
        &data_dir,
        &[
            "--json",
            "export",
            "audit-jsonl",
            "--out",
            "missing/audit.jsonl",
        ],
    );
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(error_body(&output)["code"], "internal");

    // An unknown run has no interval to export: the endpoint checks the run
    // first and names it in the answer.
    let output = run_in(
        &workspace,
        &data_dir,
        &["--json", "export", "run-audit", "nope"],
    );
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "not_found", "{body}");
    assert_eq!(body["cause"], "run_id", "{body}");
}

#[test]
fn an_export_outside_the_workspace_is_a_bad_request() {
    let workspace = unique_dir("escape-ws");
    let data_dir = unique_dir("escape-data");
    let output = run_in(
        &workspace,
        &data_dir,
        &[
            "--json",
            "export",
            "audit-jsonl",
            "--out",
            "../escaped.jsonl",
        ],
    );
    // The path is the *caller's* parameter, so the workspace policy answers the
    // documented `400 bad_request` with `cause: "path"` — not a `403`, which this
    // control plane reserves for authentication and authorisation.
    assert_eq!(exit_code(&output), 2, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "bad_request", "{body}");
    assert_eq!(body["cause"], "path", "{body}");
    assert!(!workspace
        .parent()
        .expect("parent")
        .join("escaped.jsonl")
        .exists());
}

#[test]
fn the_theme_and_the_language_are_written_or_refused() {
    let workspace = unique_dir("theme-ws");
    let data_dir = unique_dir("theme-data");

    for args in [["theme", "set", "dark"], ["language", "set", "zh"]] {
        let output = run_in(&workspace, &data_dir, &args);
        assert_eq!(exit_code(&output), 0, "{args:?}: {}", stderr(&output));
        assert_eq!(stdout(&output).trim(), "ok");
    }

    // The vocabularies are the server's: a value outside them is a `400`.
    for (args, expected) in [
        (vec!["--json", "theme", "set", "mauve"], "light"),
        (vec!["--json", "language", "set", "klingon"], "system"),
    ] {
        let output = run_in(&workspace, &data_dir, &args);
        assert_eq!(exit_code(&output), 2, "{args:?}: {}", stderr(&output));
        let body = error_body(&output);
        assert_eq!(body["code"], "bad_request", "{body}");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains(expected),
            "{body}"
        );
    }
}

#[test]
fn the_alert_and_the_two_path_clears() {
    let workspace = unique_dir("clears-ws");
    let data_dir = unique_dir("clears-data");

    let output = run_in(&workspace, &data_dir, &["audit", "alert", "set", "on"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "ok");

    // `on|off` is the CLI's spelling; anything else never leaves the CLI.
    let output = run_in(&workspace, &data_dir, &["audit", "alert", "set", "maybe"]);
    assert_eq!(exit_code(&output), 2, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("on or off"), "{}", stderr(&output));

    // The three clears all ask first, and `--yes` answers.
    for args in [["qemu", "clear"], ["toolchain", "clear"]] {
        let refused = run_in(&workspace, &data_dir, &args);
        assert_eq!(exit_code(&refused), 2, "{args:?}: {}", stderr(&refused));
        assert!(stderr(&refused).contains("confirmation"), "{args:?}");

        let cleared = run_in(&workspace, &data_dir, &[args[0], args[1], "--yes"]);
        assert_eq!(exit_code(&cleared), 0, "{args:?}: {}", stderr(&cleared));
        assert_eq!(stdout(&cleared).trim(), "ok");
    }
}

#[test]
fn the_two_path_setters_refuse_a_file_that_is_not_there() {
    let workspace = unique_dir("path-ws");
    let data_dir = unique_dir("path-data");
    let missing = format!("does-not-exist-{}", std::process::id());

    for args in [
        vec!["--json", "qemu", "path", &missing],
        vec!["--json", "toolchain", "path", &missing],
    ] {
        let output = run_in(&workspace, &data_dir, &args);
        // The host checks that the file exists and runs; both failures are the
        // documented `400 bad_request`.
        assert_eq!(exit_code(&output), 2, "{args:?}: {}", stderr(&output));
        assert_eq!(error_body(&output)["code"], "bad_request", "{args:?}");
    }
}

#[test]
fn llm_set_accepts_the_key_inline_and_from_a_file() {
    let workspace = unique_dir("llm-ws");
    let data_dir = unique_dir("llm-data");

    let output = run_in(
        &workspace,
        &data_dir,
        &[
            "--json",
            "llm",
            "set",
            "--api-key",
            "not-a-real-key",
            "--base-url",
            "https://api.deepseek.com",
            "--model",
            "deepseek-chat",
        ],
    );
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    // A `204`: nothing to print in JSON mode.
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    // The key came from the command line, so the CLI says where that lands.
    assert!(
        stderr(&output).contains("warning: --api-key"),
        "{}",
        stderr(&output)
    );
    // No `--remember`: nothing was written to the OS credential store.
    assert!(!stderr(&output).contains("keyring"), "{}", stderr(&output));

    // The same thing with the key in a file, which is not warned about.
    let key_file = workspace.join("key.txt");
    std::fs::write(&key_file, "  not-a-real-key  \n").expect("key file");
    let output = run_in(
        &workspace,
        &data_dir,
        &[
            "--json",
            "llm",
            "set",
            "--api-key-file",
            key_file.to_str().expect("utf-8 path"),
            "--base-url",
            "https://api.deepseek.com",
            "--model",
            "deepseek-chat",
            "--provider-id",
            "deepseek",
        ],
    );
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert!(
        !stderr(&output).contains("warning: --api-key"),
        "{}",
        stderr(&output)
    );

    // A key file that is not there is a local failure, and says so.
    let output = run_in(
        &workspace,
        &data_dir,
        &[
            "llm",
            "set",
            "--api-key-file",
            "no-such-key-file",
            "--base-url",
            "u",
            "--model",
            "m",
        ],
    );
    assert_eq!(exit_code(&output), 1, "stderr: {}", stderr(&output));
    assert!(stderr(&output).contains("API key"), "{}", stderr(&output));

    // And an incomplete command line never reaches the control plane.
    let output = run_in(&workspace, &data_dir, &["llm", "set", "--api-key", "k"]);
    assert_eq!(exit_code(&output), 2, "stderr: {}", stderr(&output));
    assert!(
        stderr(&output).contains("usage: riscdom"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn llm_load_key_reports_a_provider_with_no_stored_key() {
    let workspace = unique_dir("loadkey-ws");
    let data_dir = unique_dir("loadkey-data");
    let output = run_in(
        &workspace,
        &data_dir,
        &["--json", "llm", "load-key", "no-such-provider-12345"],
    );
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    assert_eq!(error_body(&output)["code"], "not_found");
}

#[test]
fn the_model_configuration_is_cleared_only_after_a_confirmation() {
    let workspace = unique_dir("llmclear-ws");
    let data_dir = unique_dir("llmclear-data");

    let refused = run_in(&workspace, &data_dir, &["llm", "clear"]);
    assert_eq!(exit_code(&refused), 2, "stderr: {}", stderr(&refused));
    let text = stderr(&refused);
    assert!(text.contains("confirmation"), "{text}");
    assert!(text.contains("--yes"), "{text}");
    assert!(stdout(&refused).is_empty(), "it wrote to stdout");

    let cleared = run_in(&workspace, &data_dir, &["llm", "clear", "--yes"]);
    assert_eq!(exit_code(&cleared), 0, "stderr: {}", stderr(&cleared));
    assert_eq!(stdout(&cleared).trim(), "ok");
}

#[test]
fn the_toolchain_download_acknowledges_and_waiting_watches_it_finish() {
    let workspace = unique_dir("download-ws");

    // Without `--wait`: the `202` acknowledgement, and the CLI is done.
    let data_dir = data_dir_with_a_toolchain("download-data");
    let output = run_in(&workspace, &data_dir, &["toolchain", "download"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "download started");
    let output = run_in(&workspace, &data_dir, &["--json", "toolchain", "download"]);
    // A second start while the first is still running is the documented `409`...
    // but the first one is already over here (nothing to download), so this
    // starts its own and acknowledges it again.
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    assert_eq!(json_stdout(&output)["state"], "started");

    // With `--wait`: the frames, the closing line, and exit 0.
    let data_dir = data_dir_with_a_toolchain("download-wait-data");
    let output = run_in(&workspace, &data_dir, &["toolchain", "download", "--wait"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("toolchain:download"), "{text}");
    assert!(text.trim_end().ends_with("download ok"), "{text}");
    // The library's runtime logging is off by default, which is what keeps an
    // embedded server from writing into the CLI's own stderr.
    assert!(stderr(&output).is_empty(), "stderr: {}", stderr(&output));

    // The same wait, in JSON: the envelopes verbatim, one per line, and no
    // closing line (the exit code is the summary there).
    let data_dir = data_dir_with_a_toolchain("download-wait-json-data");
    let output = run_in(
        &workspace,
        &data_dir,
        &["--json", "toolchain", "download", "--wait"],
    );
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "one frame: {lines:?}");
    let frame: serde_json::Value =
        serde_json::from_str(lines[0]).expect("each line is an envelope");
    assert_eq!(frame["kind"], "event", "{frame}");
    assert_eq!(frame["event"], "toolchain:download", "{frame}");
    assert_eq!(frame["payload"]["state"], "done", "{frame}");
}

#[test]
fn cancelling_when_nothing_is_running_is_a_conflict() {
    let output = run("cancel", &["--json", "toolchain", "cancel"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "conflict", "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("no toolchain download"),
        "{body}"
    );
}

#[test]
fn the_qemu_download_reports_the_unpinned_decision() {
    // No QEMU release is pinned (the project guides instead of downloading), so the
    // endpoint answers the documented `503 unavailable` with `cause: "qemu"` and the
    // guidance — on every platform. Nothing is downloaded, and nothing can be.
    let output = run("qemu-download", &["--json", "qemu", "download"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "unavailable", "{body}");
    assert_eq!(body["cause"], "qemu", "{body}");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("QEMU"), "{message}");
    assert!(output.stdout.is_empty(), "nothing on stdout");

    // `--wait` must not turn the refusal into a hang: the request is refused
    // before anything is subscribed to, and the CLI exits with the same code.
    let waited = run("qemu-download-wait", &["qemu", "download", "--wait"]);
    assert_eq!(exit_code(&waited), 3, "stderr: {}", stderr(&waited));
    assert!(
        stderr(&waited).contains("unavailable"),
        "{}",
        stderr(&waited)
    );

    // Nothing was installed, and the status says so.
    let status = run("qemu-status", &["qemu", "status"]);
    assert_eq!(exit_code(&status), 0, "stderr: {}", stderr(&status));
    assert_eq!(stdout(&status).trim(), "in_progress false\nlast_event  -");
}

#[test]
fn cancelling_a_qemu_download_that_is_not_running_is_a_conflict() {
    let output = run("qemu-cancel", &["--json", "qemu", "cancel"]);
    assert_eq!(exit_code(&output), 3, "stderr: {}", stderr(&output));
    let body = error_body(&output);
    assert_eq!(body["code"], "conflict", "{body}");
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("no QEMU download"),
        "{body}"
    );
}

#[test]
fn the_preflight_acknowledgement_answers() {
    // `preflight run` is not driven here (it compiles and boots a guest); its
    // `ack` half is safe: it records the escape hatch and answers with the view.
    let output = run("preflight-ack", &["preflight", "ack"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("checked"), "{text}");
    assert!(text.contains("gcc_runs"), "{text}");

    let output = run("preflight-ack-json", &["--json", "preflight", "ack"]);
    assert_eq!(exit_code(&output), 0, "stderr: {}", stderr(&output));
    let view = json_stdout(&output);
    assert!(view["rows"].is_array(), "{view}");
    assert_eq!(view["overridden"], true, "{view}");
}
