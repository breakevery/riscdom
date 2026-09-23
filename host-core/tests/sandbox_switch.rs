//! v0.9 sandbox batch F2b-1 — the core switch.
//!
//! What is exercised here: `current_sandbox` is runtime state (a stored default is
//! what a restart starts from, not what is current); validation runs **before** the
//! running VM is touched, so a definition that cannot run changes nothing; one
//! switch at a time; and a switch that passes validation but cannot start leaves
//! the node **stopped**, not half-switched.
//!
//! No QEMU is run and nothing leaves the machine. The one test that reaches
//! `start` hands it a runnable stand-in (a copied `cmd.exe` / `/bin/echo`, the
//! same trick `tests/qemu_injection.rs` uses) — it is not QEMU, so the start
//! fails, which is exactly the state that test is about.
//!
//! The success path itself — a switch that boots a real guest and becomes
//! current — needs a real QEMU and a real kernel ELF: the same ticket the golden
//! path walks (`tests/golden_path.rs`, `--ignored`), and it is deliberately not
//! pretended at here.

use host_core::state::AppState;
use serde_json::json;
use std::path::{Path, PathBuf};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-sandbox-switch-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A state whose data directory this test owns, so the scan and `settings.json`
/// are both inside the fixture.
fn state(tag: &str) -> (AppState, PathBuf) {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    (state, data_dir)
}

/// Re-read the same data directory, which is what a restart sees.
fn restart(state: &AppState) -> AppState {
    AppState::with_data_dir(state.workspace_root_display(), state.data_dir()).expect("state")
}

/// A file that exists, for the checks that only ask whether something is there.
fn a_file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"not really a tool").expect("write");
    path
}

/// A runnable binary that is not the tool it pretends to be: `<path> --version`
/// exits 0, so validation passes and the failure lands in `start`.
fn a_runnable_binary(dir: &Path, name: &str) -> PathBuf {
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

/// The name the toolchain scan looks for, with this platform's suffix.
fn compiler_name() -> String {
    if cfg!(windows) {
        format!("{}.exe", agent::GCC_NAMES[0])
    } else {
        agent::GCC_NAMES[0].to_string()
    }
}

/// Write `settings.json` into the data directory, before a state reads it.
fn write_settings(data_dir: &Path, settings: serde_json::Value) {
    let path = data_dir.join("settings.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&settings).expect("serde"),
    )
    .expect("write settings");
}

/// The storage path of a definition that passes validation: everything pinned and
/// present.
fn workable(data_dir: &Path, name: &str) -> serde_json::Value {
    json!({
        "name": name,
        "qemu_exe": a_runnable_binary(data_dir, "stand-in-qemu").display().to_string(),
        "toolchain_path": a_file(data_dir, &compiler_name()).display().to_string(),
        "kernel": a_file(data_dir, "hello.elf").display().to_string(),
    })
}

// ----- the runtime current --------------------------------------------------

#[test]
fn nothing_is_current_until_a_switch_chooses_one() {
    let (state, data_dir) = state("current-default");
    // Fresh: no switch has run, and the fallback is the name the *default*
    // answers with — not the current one (F2b decision 1).
    assert_eq!(state.current_sandbox(), None);
    assert_eq!(
        state.sandbox_default_name(),
        host_core::DEFAULT_SANDBOX_NAME
    );

    // A stored default is a *default*: a restart starts from it, and it is still
    // not what is current.
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{ "name": "blink" }],
            "default_sandbox": "blink"
        }),
    );
    let restarted = restart(&state);
    assert_eq!(restarted.current_sandbox(), None);
    assert_eq!(restarted.sandbox_default_name(), "blink");
}

#[test]
fn a_switch_that_never_ran_leaves_nothing_current() {
    // The converse of the above, for the failure paths below: a switch that is
    // refused (whatever the reason) must not claim to have switched.
    let (state, _data_dir) = state("no-current");
    let _ = state.switch_sandbox("no-such-sandbox");
    assert_eq!(state.current_sandbox(), None);
}

// ----- validation happens before anything is stopped ------------------------

#[test]
fn an_unknown_name_is_refused_with_sandbox_not_found() {
    let (state, _data_dir) = state("unknown");
    let err = state
        .switch_sandbox("no-such-sandbox")
        .expect_err("there is no such definition");
    assert!(err.to_string().starts_with("sandbox_not_found:"), "{err}");
    assert_eq!(state.current_sandbox(), None);
    // The switch slot is not left claimed by a refusal that never got that far.
    assert!(state.begin_sandbox_switch("any").is_ok());
    state.finish_sandbox_switch();
}

#[test]
fn a_qemu_that_is_not_there_refuses_the_switch() {
    let (state, data_dir) = state("qemu-missing");
    let missing = data_dir.join("no-qemu-here");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{
                "name": "broken-qemu",
                "qemu_exe": missing.display().to_string(),
                "toolchain_path": a_file(&data_dir, &compiler_name()).display().to_string(),
                "kernel": a_file(&data_dir, "hello.elf").display().to_string()
            }]
        }),
    );
    let restarted = restart(&state);
    let before = restarted.audit_status().expect("status").count;

    let err = restarted
        .switch_sandbox("broken-qemu")
        .expect_err("a QEMU that is not a file cannot be switched to");
    assert!(
        err.to_string().starts_with("sandbox_qemu_missing:"),
        "{err}"
    );

    // Nothing was touched: no VM, no audit row, nothing current, no slot held.
    assert!(!restarted.vm_status().running);
    assert_eq!(restarted.audit_status().expect("status").count, before);
    assert_eq!(restarted.current_sandbox(), None);
    assert!(restarted.begin_sandbox_switch("any").is_ok());
    restarted.finish_sandbox_switch();
}

#[test]
fn a_toolchain_that_is_not_there_refuses_the_switch() {
    let (state, data_dir) = state("toolchain-missing");
    let missing = data_dir.join("no-gcc-here");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{
                "name": "broken-toolchain",
                "qemu_exe": a_runnable_binary(&data_dir, "stand-in-qemu").display().to_string(),
                "toolchain_path": missing.display().to_string(),
                "kernel": a_file(&data_dir, "hello.elf").display().to_string()
            }]
        }),
    );
    let restarted = restart(&state);
    let before = restarted.audit_status().expect("status").count;

    let err = restarted
        .switch_sandbox("broken-toolchain")
        .expect_err("a toolchain that is not a file cannot compile anything");
    assert!(
        err.to_string().starts_with("sandbox_toolchain_missing:"),
        "{err}"
    );

    assert!(!restarted.vm_status().running);
    assert_eq!(restarted.audit_status().expect("status").count, before);
    assert_eq!(restarted.current_sandbox(), None);
}

#[test]
fn a_kernel_that_is_not_there_refuses_the_switch() {
    let (state, data_dir) = state("kernel-missing");
    let missing = data_dir.join("no-kernel.elf");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{
                "name": "broken-kernel",
                "qemu_exe": a_runnable_binary(&data_dir, "stand-in-qemu").display().to_string(),
                "toolchain_path": a_file(&data_dir, &compiler_name()).display().to_string(),
                "kernel": missing.display().to_string()
            }]
        }),
    );
    let restarted = restart(&state);
    let before = restarted.audit_status().expect("status").count;

    let err = restarted
        .switch_sandbox("broken-kernel")
        .expect_err("a kernel that is not a file cannot be booted");
    assert!(
        err.to_string().starts_with("sandbox_kernel_missing:"),
        "{err}"
    );

    assert!(!restarted.vm_status().running);
    assert_eq!(restarted.audit_status().expect("status").count, before);
    assert_eq!(restarted.current_sandbox(), None);
}

#[test]
fn a_definition_with_no_kernel_and_no_elf_in_the_workspace_refuses() {
    // `kernel: None` means "boot what the workspace has"; a workspace with no
    // `.elf` at all has nothing to boot, which is its own reason.
    let (state, data_dir) = state("no-elf");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{
                "name": "unpinned",
                "qemu_exe": a_runnable_binary(&data_dir, "stand-in-qemu").display().to_string(),
                "toolchain_path": a_file(&data_dir, &compiler_name()).display().to_string()
            }]
        }),
    );
    let restarted = restart(&state);
    let err = restarted
        .switch_sandbox("unpinned")
        .expect_err("there is no ELF to boot");
    assert!(
        err.to_string().starts_with("sandbox_kernel_missing:"),
        "{err}"
    );
    assert_eq!(restarted.current_sandbox(), None);
}

// ----- one switch at a time --------------------------------------------------

#[test]
fn a_second_switch_is_refused_while_one_is_in_progress() {
    let (state, _data_dir) = state("busy");
    state
        .begin_sandbox_switch("blink")
        .expect("the first claim");

    // Through the primitive…
    let err = state
        .begin_sandbox_switch("blink")
        .expect_err("a second claim must be refused");
    assert!(err.to_string().contains("already in progress"), "{err}");

    // …and through the switch itself. The name is valid (the fallback always
    // exists), so the refusal is about the slot and nothing else.
    let err = state
        .switch_sandbox(host_core::DEFAULT_SANDBOX_NAME)
        .expect_err("a switch during a switch must be refused");
    assert!(err.to_string().contains("already in progress"), "{err}");

    // Releasing it lets the next caller in; cancelling an idle slot is an error,
    // the same shape as the download slots' `cancel`.
    state.finish_sandbox_switch();
    assert!(state.begin_sandbox_switch("blink").is_ok());
    state.cancel_sandbox_switch().expect("cancel");
    assert!(
        state.cancel_sandbox_switch().is_err(),
        "nothing is in progress"
    );
    assert!(!state.run_in_flight());
}

// ----- a run in flight ------------------------------------------------------

#[test]
fn no_run_is_in_flight_on_a_fresh_node() {
    // The predicate a switch consults. A run is only in flight inside `run_agent`,
    // which these tests never enter (it needs an LLM, a toolchain and a QEMU).
    let (state, _data_dir) = state("no-run");
    assert!(!state.run_in_flight());
}

// ----- passed validation, failed to start: stopped, not half-switched --------

#[test]
fn a_switch_that_cannot_start_leaves_the_node_stopped() {
    // The stand-in answers `--version`, so every check passes; it is not QEMU, so
    // `start` fails — and the node must be *stopped* rather than half-switched
    // (F2b decision 2). `Drop` kills whatever the failed handle spawned.
    let (state, data_dir) = state("failed-start");
    write_settings(
        &data_dir,
        json!({ "version": 1, "sandboxes": [workable(&data_dir, "not-qemu")] }),
    );
    let restarted = restart(&state);

    let err = restarted
        .switch_sandbox("not-qemu")
        .expect_err("the stand-in is not a QEMU");
    assert!(err.to_string().contains("failed after 3 attempts"), "{err}");

    // Stopped, not half-switched — and what is current did not change.
    assert!(
        !restarted.vm_status().running,
        "no VM survives a failed switch"
    );
    assert_eq!(restarted.current_sandbox(), None);
    // The slot is free again, so the next attempt is a fresh one.
    assert!(restarted.begin_sandbox_switch("not-qemu").is_ok());
    restarted.finish_sandbox_switch();
}

// ----- what a switch does NOT change ----------------------------------------

#[test]
fn a_switch_never_writes_the_configuration() {
    // F2b decision 1: the switch changes what is running, not what is configured.
    // (The failure path is the one a test can walk without a real QEMU.)
    let (state, data_dir) = state("settings-untouched");
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [workable(&data_dir, "not-qemu")],
            "default_sandbox": "not-qemu"
        }),
    );
    let restarted = restart(&state);
    let text = std::fs::read_to_string(restarted.settings_path()).expect("settings.json");

    let _ = restarted.switch_sandbox("not-qemu");

    assert_eq!(
        std::fs::read_to_string(restarted.settings_path()).expect("settings.json"),
        text,
        "settings.json must be byte-identical after a switch"
    );
    assert_eq!(restarted.sandbox_default_name(), "not-qemu");
}
