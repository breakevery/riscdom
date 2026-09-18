//! v0.4 batch 3 — the environment capability preflight.
//!
//! The preflight answers "does this配置 actually run" by doing it: run the
//! compiler, compile a tiny guest on the real paths, run QEMU, boot it. These
//! tests use the real toolchain where they must, and a runnable-but-wrong binary
//! where they need a failure.

use host::events::RecordingEventSink;
use host::preflight::{PreflightView, STEP_GCC_COMPILES, STEP_GCC_RUNS, STEP_GUEST_BOOTS};
use host::state::{AppState, LlmConfigInput};
use host::EventSink;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-preflight-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn state(tag: &str) -> AppState {
    AppState::in_memory(unique_dir(tag)).expect("state")
}

/// A runnable binary that is not the tool it pretends to be.
fn fake_binary(dir: &Path, name: &str) -> PathBuf {
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

#[test]
fn a_working_environment_passes_every_step() {
    let state = state("ok");
    let sink = Arc::new(RecordingEventSink::new());
    let view = state
        .ensure_preflight(true, Some(sink.clone() as Arc<dyn EventSink>))
        .expect("preflight");

    assert!(view.ran, "force must run the checks");
    assert!(view.ok, "steps: {:?}", view.rows);
    assert!(view.rows.iter().all(|r| r.state == "ok"), "{:?}", view.rows);
    assert!(view.checked && !view.needs_override());
    assert!(view.checked_at_ms.is_some());

    // Progress arrives step by step, plus a final line.
    let progress = sink
        .events()
        .iter()
        .filter(|(event, _)| event == "preflight:progress")
        .count();
    assert!(
        progress >= 5,
        "running+result per step and a done: {progress}"
    );

    // The result is cached in settings.json, next to the paths it belongs to.
    let text = std::fs::read_to_string(state.settings_path()).expect("settings.json");
    assert!(text.contains("\"preflight\""), "{text}");
    assert!(text.contains("\"ok\": true"), "{text}");

    // A preflight is an environment check, not a run: no run markers appear.
    let events = state
        .audit
        .lock()
        .expect("audit")
        .list(audit::EventFilter::default(), 500)
        .expect("list");
    assert!(
        events.iter().all(|e| !e.event.action.starts_with("run.")),
        "the preflight must not write run markers"
    );
}

#[test]
fn a_toolchain_that_cannot_compile_fails_at_the_compile_step() {
    let dir = unique_dir("badgcc");
    let fake = fake_binary(&dir, "not-a-compiler.exe");
    let state = AppState::in_memory(&dir).expect("state");
    state
        .set_toolchain_path(&fake.display().to_string())
        .expect("the fake is runnable, so it is accepted");

    let view = state.ensure_preflight(true, None).expect("preflight");
    assert!(!view.ok);
    assert_eq!(view.failed_step.as_deref(), Some(STEP_GCC_COMPILES));
    assert_eq!(
        view.rows[0].state, "ok",
        "`--version` answered: {:?}",
        view.rows
    );
    assert_eq!(view.rows[1].state, "failed");
    assert_eq!(view.rows[2].state, "not_run", "fail-fast");
    assert!(view.detail.is_some(), "the raw output must be reported");
    assert!(view.suggestion.is_some(), "and something to do about it");
    assert!(view.needs_override(), "the escape hatch is offered");
}

#[test]
fn a_qemu_that_cannot_boot_fails_at_the_boot_step() {
    let dir = unique_dir("badqemu");
    let fake = fake_binary(&dir, "not-qemu.exe");
    let state = AppState::in_memory(&dir).expect("state");
    state
        .set_qemu_path(&fake.display().to_string())
        .expect("the fake is runnable, so it is accepted");

    let view = state.ensure_preflight(true, None).expect("preflight");
    assert!(!view.ok);
    assert_eq!(view.failed_step.as_deref(), Some(STEP_GUEST_BOOTS));
    assert_eq!(
        view.rows.iter().filter(|r| r.state == "ok").count(),
        3,
        "compiler and QEMU ran; only the boot failed: {:?}",
        view.rows
    );
    let detail = view.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("QEMU") || detail.contains("串口"),
        "the failure must name the step's own problem: {detail}"
    );
}

#[test]
fn a_cached_result_is_reused_and_a_new_configuration_reruns() {
    let state = state("cache");
    let first = state.ensure_preflight(true, None).expect("first");
    assert!(first.ran && first.ok);

    let second = state.ensure_preflight(false, None).expect("second");
    assert!(!second.ran, "a matching fingerprint must reuse the cache");
    assert!(second.ok);
    assert_eq!(second.checked_at_ms, first.checked_at_ms);

    // A different model changes the configuration fingerprint.
    state.set_llm_config(LlmConfigInput {
        provider_id: "custom".into(),
        api_key: "***".into(),
        base_url: "https://example.invalid/v1".into(),
        model: "some-other-model".into(),
    });
    let third = state.ensure_preflight(false, None).expect("third");
    assert!(third.ran, "a changed fingerprint must re-run the checks");
    assert_ne!(third.checked_at_ms, first.checked_at_ms);
}

#[test]
fn changing_the_environment_forgets_the_cached_result() {
    let dir = unique_dir("invalidate");
    let state = AppState::in_memory(&dir).expect("state");
    let first = state.ensure_preflight(true, None).expect("first");
    assert!(first.checked);

    let fake = fake_binary(&dir, "another-gcc.exe");
    state
        .set_toolchain_path(&fake.display().to_string())
        .expect("set");

    // The cache no longer describes this configuration, so the status is "unchecked"
    // until something runs it again.
    let status = state.preflight_status();
    assert!(!status.checked, "{status:?}");
    assert!(status.rows.iter().all(|r| r.state == "not_run"));
}

#[test]
fn the_escape_hatch_is_recorded_and_stops_the_warning() {
    let dir = unique_dir("override");
    let fake = fake_binary(&dir, "not-a-compiler.exe");
    let state = AppState::in_memory(&dir).expect("state");
    state
        .set_toolchain_path(&fake.display().to_string())
        .expect("set");

    let view = state.ensure_preflight(true, None).expect("preflight");
    assert!(view.needs_override());

    let accepted = state.acknowledge_preflight().expect("acknowledge");
    assert!(accepted.overridden);
    assert!(!accepted.needs_override(), "the warning must stop");
    assert!(!accepted.ok, "accepting does not change the facts");

    let text = std::fs::read_to_string(state.settings_path()).expect("settings.json");
    assert!(text.contains("\"overridden\": true"), "{text}");

    let status = state.preflight_status();
    assert!(status.overridden && !status.needs_override());
}

/// A "compiler" that answers `--version` and hangs on every real invocation
/// (v0.4 batch 3-followup: the guard's timeout path).
#[cfg(target_os = "windows")]
fn write_slow_compiler(dir: &Path) -> PathBuf {
    let path = dir.join("slow-gcc.cmd");
    std::fs::write(
        &path,
        "@echo off\r\nif \"%~1\"==\"--version\" (echo slow-gcc 0.0.0 & exit /b 0)\r\nping -n 60 127.0.0.1 >nul\r\nexit /b 1\r\n",
    )
    .expect("write fake compiler");
    path
}

/// Are any of this process's compiler children still alive?
#[cfg(target_os = "windows")]
fn compiler_children_running(gcc: &Path) -> bool {
    let name = gcc
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let script = format!(
        "@(Get-CimInstance Win32_Process -Filter \"ParentProcessId={pid}\" | Where-Object {{ $_.Name -eq '{name}' -or $_.Name -eq 'cmd.exe' }}).Count",
        pid = std::process::id()
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(out) => {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<usize>()
                .unwrap_or(0)
                > 0
        }
        // No shell to ask: the assertion is best-effort by design.
        Err(_) => false,
    }
}

#[cfg(target_os = "windows")]
fn wait_until_no_compiler_children(gcc: &Path, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if !compiler_children_running(gcc) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

#[test]
#[cfg(target_os = "windows")]
fn a_compiler_that_overruns_is_stopped_and_reported() {
    let dir = unique_dir("slowgcc");
    let fake = write_slow_compiler(&dir);
    let state = AppState::in_memory(&dir).expect("state");
    state
        .set_toolchain_path(&fake.display().to_string())
        .expect("the fake answers --version, so it is accepted");

    let started = std::time::Instant::now();
    let view = state
        .ensure_preflight_with(
            true,
            None,
            host::preflight::PreflightOptions {
                compile_timeout: std::time::Duration::from_secs(3),
            },
        )
        .expect("preflight");
    let elapsed = started.elapsed();

    assert!(!view.ok);
    assert_eq!(view.failed_step.as_deref(), Some(STEP_GCC_COMPILES));
    let detail = view.detail.clone().unwrap_or_default();
    assert!(
        detail.contains("超过 3 秒"),
        "the report must name the timeout: {detail}"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "the guard must not wait for the compiler: {elapsed:?}"
    );

    // And it really stops it: no compiler child of ours is left running.
    assert!(
        wait_until_no_compiler_children(&fake, std::time::Duration::from_secs(10)),
        "the overrunning compiler must be stopped"
    );
}

#[test]
fn the_status_reports_unchecked_before_anything_ran() {
    let state = state("fresh");
    let view: PreflightView = state.preflight_status();
    assert!(!view.checked);
    assert!(!view.ran);
    assert_eq!(view.rows.len(), 4);
    assert!(view.rows.iter().all(|r| r.state == "not_run"));
    assert!(!view.needs_override(), "nothing to bypass yet");
    assert!(!view.fingerprint.is_empty());
    assert_eq!(
        STEP_GCC_RUNS, view.rows[0].step,
        "the first row is the compiler check"
    );
}
