//! v0.9 sandbox batch F2a-1 — the definition layer, settings, and the scan.
//!
//! What is exercised here: a definition survives `settings.json`; a file written
//! before the field existed still loads; the scan finds resources under the data
//! directory and skips the installers' by-products; the merge lets a hand-written
//! definition win while the shadowed entry stays visible; `runnable` needs all
//! three resources; and nothing the scan finds is ever written back.
//!
//! No QEMU is run and nothing leaves the machine: the runnable checks are fed
//! paths this test creates, and the only process ever spawned is a copied
//! `cmd.exe` / `/bin/echo` standing in for an executable.

use host_core::settings::LocalSettings;
use host_core::state::AppState;
use host_core::{CandidateView, CandidatesView, SandboxDef, SandboxSource, SandboxView};
use serde_json::json;
use std::path::{Path, PathBuf};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-sandbox-def-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The state the API sees, with its own workspace and data directory.
fn state(tag: &str) -> (AppState, PathBuf) {
    let workspace = unique_dir(&format!("{tag}-ws"));
    let data_dir = unique_dir(&format!("{tag}-data"));
    let state = AppState::with_data_dir(&workspace, &data_dir).expect("state");
    (state, data_dir)
}

/// A file that exists, for the checks that only ask whether something is there.
fn a_file(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"not really a tool").expect("write");
    path
}

/// A runnable binary that is not the tool it pretends to be (the same stand-in
/// `tests/preflight.rs` uses: `<path> --version` must exit 0).
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

/// The name the QEMU scan looks for, with this platform's suffix.
fn qemu_name() -> String {
    sandbox::qemu_discover::exe_name()
}

/// Plant one installed resource under `<data>/<kind>/<version>/`.
fn plant(data_dir: &Path, kind: &str, version: &str, name: &str) -> PathBuf {
    let dir = data_dir.join(kind).join(version);
    std::fs::create_dir_all(&dir).expect("mkdir");
    a_runnable_binary(&dir, name)
}

/// Write `settings.json` into the data directory, before the state reads it.
fn write_settings(data_dir: &Path, settings: serde_json::Value) {
    let path = data_dir.join("settings.json");
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&settings).expect("serde"),
    )
    .expect("write settings");
}

fn view_named<'a>(views: &'a [SandboxView], name: &str) -> Vec<&'a SandboxView> {
    views.iter().filter(|v| v.name == name).collect()
}

fn candidate_named<'a>(
    candidates: &'a [CandidateView],
    version: &str,
) -> Option<&'a CandidateView> {
    candidates.iter().find(|c| c.version == version)
}

// ----- the stored definition ------------------------------------------------

#[test]
fn a_settings_file_from_before_the_field_still_loads() {
    // The shape v0.8 wrote: no `sandboxes`, no `default_sandbox`. No migration
    // runs — the fields are additive (F2a decision 1).
    let dir = unique_dir("old-settings");
    let path = dir.join("settings.json");
    std::fs::write(
        &path,
        r#"{"version":1,"toolchain_path":"C:\\tools\\gcc.exe","qemu_path":null,
            "preflight":null,"theme":"dark","language":"zh",
            "alert_on_audit_failure":false}"#,
    )
    .expect("write");

    let settings = LocalSettings::load(&path);
    assert_eq!(settings.theme.as_deref(), Some("dark"));
    assert_eq!(
        settings.toolchain_path.as_deref(),
        Some(r"C:\tools\gcc.exe")
    );
    assert!(settings.sandboxes.is_empty());
    assert_eq!(settings.default_sandbox, None);
}

#[test]
fn a_definition_round_trips_through_settings_and_the_registry() {
    let (state, data_dir) = state("roundtrip");
    let kernel = a_file(&data_dir, "hello.elf");
    let gcc = a_runnable_binary(&data_dir, &compiler_name());
    let qemu = a_runnable_binary(&data_dir, &qemu_name());

    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [
                {
                    "name": "blink",
                    "display_name": "Blink",
                    "memory_mb": 256,
                    "qemu_exe": qemu.display().to_string(),
                    "toolchain_path": gcc.display().to_string(),
                    "kernel": kernel.display().to_string(),
                    "notes": "the blink example"
                },
                { "name": "bare" }
            ],
            "default_sandbox": "blink"
        }),
    );
    // A fresh state reads what the file says (the same workspace means the same
    // settings file, so this is what a restart sees).
    let restarted = AppState::with_data_dir(
        state.workspace_root_display().as_str(),
        state.data_dir().to_path_buf(),
    )
    .expect("state");

    assert_eq!(restarted.current_sandbox().as_deref(), Some("blink"));
    assert_eq!(restarted.sandbox_default_name(), "blink");

    let blink = restarted.sandbox("blink").expect("blink is stored");
    assert_eq!(blink.source, SandboxSource::Manual);
    assert_eq!(blink.display_name.as_deref(), Some("Blink"));
    assert_eq!(blink.memory_mb, Some(256));
    assert_eq!(blink.notes.as_deref(), Some("the blink example"));
    assert_eq!(
        blink.qemu_exe.as_deref(),
        Some(qemu.display().to_string().as_str())
    );
    assert!(
        !blink.shadowed,
        "a hand-written definition is never shadowed"
    );
    // Every field but the name may be absent.
    let bare = restarted.sandbox("bare").expect("bare is stored");
    assert_eq!(bare.memory_mb, None);
    assert_eq!(bare.kernel, None);
}

#[test]
fn the_default_name_falls_back_when_nothing_is_stored() {
    let (state, _data_dir) = state("default-name");
    assert_eq!(state.current_sandbox(), None);
    assert_eq!(
        state.sandbox_default_name(),
        host_core::DEFAULT_SANDBOX_NAME
    );
}

// ----- the scan -------------------------------------------------------------

#[test]
fn the_scan_finds_both_resources_under_the_data_directory() {
    let (state, data_dir) = state("scan");
    plant(&data_dir, "toolchain", "15.2.0-1", &compiler_name());
    plant(&data_dir, "qemu", "11.1.0", &qemu_name());

    let candidates: CandidatesView = state.sandbox_candidates();
    let toolchain = candidate_named(&candidates.toolchains, "15.2.0-1").expect("toolchain found");
    assert_eq!(toolchain.kind, "toolchain");
    assert_eq!(toolchain.origin, "installed");
    assert!(PathBuf::from(&toolchain.path).is_file(), "{toolchain:?}");

    let qemu = candidate_named(&candidates.qemus, "11.1.0").expect("qemu found");
    assert_eq!(qemu.kind, "qemu");
    assert_eq!(qemu.origin, "installed");

    // Both lists stay independent: one toolchain version, one QEMU version, and
    // no cartesian product anywhere (F2a decision 6). A system QEMU, when this
    // machine has one, is an extra `system` entry in the QEMU list only.
    assert_eq!(candidates.toolchains.len(), 1);
    let installed: Vec<&CandidateView> = candidates
        .qemus
        .iter()
        .filter(|c| c.origin == "installed")
        .collect();
    assert_eq!(installed.len(), 1, "{:?}", candidates.qemus);
    // The scan is bounded to this data directory — the state's own.
    assert_eq!(state.data_dir(), data_dir.as_path());

    // Nothing the scan found was written back (F2a decision 3).
    assert!(
        !state.settings_path().is_file(),
        "the scan must not create settings.json: {:?}",
        state.settings_path()
    );
}

#[test]
fn the_scan_skips_the_installers_by_products() {
    let (state, data_dir) = state("scan-tmp");
    plant(&data_dir, "toolchain", "15.2.0-1", &compiler_name());
    // `.download-tmp` / `.extract-tmp` hold a half-extracted tree mid-install;
    // they are not versions.
    let staging = data_dir.join("toolchain").join(".extract-tmp");
    std::fs::create_dir_all(&staging).expect("mkdir");
    a_runnable_binary(&staging, &compiler_name());
    // A version directory without the executable is not a candidate either.
    std::fs::create_dir_all(data_dir.join("qemu").join("9.9.9")).expect("mkdir");

    let candidates = state.sandbox_candidates();
    assert_eq!(
        candidates.toolchains.len(),
        1,
        "{:?}",
        candidates.toolchains
    );
    assert!(candidate_named(&candidates.toolchains, "15.2.0-1").is_some());
    assert_eq!(candidate_named(&candidates.qemus, "9.9.9"), None);
}

#[test]
fn a_candidate_can_come_from_this_machine() {
    // The system QEMU is a candidate like any other, marked `system`; whether
    // one exists here is the machine's business, so the entry is only checked
    // when discovery found one.
    let (state, _data_dir) = state("scan-system");
    let candidates = state.sandbox_candidates();
    for qemu in &candidates.qemus {
        assert!(
            matches!(qemu.origin.as_str(), "installed" | "system"),
            "{qemu:?}"
        );
        if qemu.origin == "system" {
            assert_eq!(qemu.version, "-");
            assert!(PathBuf::from(&qemu.path).is_file(), "{qemu:?}");
        }
    }
}

// ----- the merge ------------------------------------------------------------

#[test]
fn the_registry_is_manual_then_scanned_then_the_fallback() {
    let (state, data_dir) = state("merge");
    plant(&data_dir, "toolchain", "15.2.0-1", &compiler_name());
    write_settings(
        &data_dir,
        json!({ "version": 1, "sandboxes": [{ "name": "mine" }] }),
    );
    let restarted =
        AppState::with_data_dir(state.workspace_root_display(), state.data_dir()).expect("state");

    let views = restarted.sandboxes();
    let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
    // The hand-written entry comes first, the fallback last.
    assert_eq!(names.first().copied(), Some("mine"));
    assert_eq!(names.last().copied(), Some(host_core::DEFAULT_SANDBOX_NAME));
    assert!(names.contains(&"toolchain-15.2.0-1"), "{names:?}");

    let mine = restarted.sandbox("mine").expect("stored");
    assert_eq!(mine.source, SandboxSource::Manual);
    assert!(!mine.shadowed);

    let scanned = restarted.sandbox("toolchain-15.2.0-1").expect("scanned");
    assert_eq!(scanned.source, SandboxSource::Discovered);
    assert!(!scanned.shadowed, "nothing hand-written uses that name");

    // The fallback is a definition like any other, and it overrides nothing.
    let fallback = restarted
        .sandbox(host_core::DEFAULT_SANDBOX_NAME)
        .expect("fallback");
    assert_eq!(fallback.source, SandboxSource::Discovered);
    assert_eq!(fallback.memory_mb, None);
}

#[test]
fn a_hand_written_definition_shadows_the_scans_without_hiding_it() {
    let (state, data_dir) = state("shadow");
    plant(&data_dir, "toolchain", "15.2.0-1", &compiler_name());
    // The same name the scan will produce, with a memory size of its own.
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{ "name": "toolchain-15.2.0-1", "memory_mb": 512 }]
        }),
    );
    let restarted =
        AppState::with_data_dir(state.workspace_root_display(), state.data_dir()).expect("state");

    let views = restarted.sandboxes();
    let entries = view_named(&views, "toolchain-15.2.0-1");
    assert_eq!(entries.len(), 2, "both stay visible: {entries:?}");

    let manual = entries.iter().find(|v| v.source == SandboxSource::Manual);
    let discovered = entries
        .iter()
        .find(|v| v.source == SandboxSource::Discovered);
    let manual = manual.expect("the hand-written entry");
    let discovered = discovered.expect("the scanned entry");
    assert!(!manual.shadowed);
    assert_eq!(manual.memory_mb, Some(512));
    assert!(discovered.shadowed, "the scanned entry is marked");
    assert_eq!(discovered.memory_mb, None, "the scan names no memory");

    // A lookup by name answers with the winner: the hand-written one.
    let by_name = restarted.sandbox("toolchain-15.2.0-1").expect("stored");
    assert_eq!(by_name.source, SandboxSource::Manual);
    assert_eq!(by_name.memory_mb, Some(512));
}

#[test]
fn an_unknown_name_is_none() {
    let (state, _data_dir) = state("unknown");
    assert!(state.sandbox("no-such-sandbox").is_none());
}

// ----- runnable -------------------------------------------------------------

#[test]
fn runnable_needs_the_qemu_the_toolchain_and_a_kernel() {
    let (state, data_dir) = state("runnable");
    let qemu = a_runnable_binary(&data_dir, &qemu_name());
    let gcc = a_runnable_binary(&data_dir, &compiler_name());
    let kernel = a_file(&data_dir, "hello.elf");
    let missing = data_dir.join("not-here");

    let def = |name: &str,
               qemu_exe: Option<&Path>,
               toolchain: Option<&Path>,
               kernel_elf: Option<&Path>| {
        let mut value = json!({ "name": name });
        if let Some(path) = qemu_exe {
            value["qemu_exe"] = json!(path.display().to_string());
        }
        if let Some(path) = toolchain {
            value["toolchain_path"] = json!(path.display().to_string());
        }
        if let Some(path) = kernel_elf {
            value["kernel"] = json!(path.display().to_string());
        }
        value
    };

    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [
                def("all-present", Some(&qemu), Some(&gcc), Some(&kernel)),
                def("qemu-missing", Some(&missing), Some(&gcc), Some(&kernel)),
                def("toolchain-missing", Some(&qemu), Some(&missing), Some(&kernel)),
                def("kernel-missing", Some(&qemu), Some(&gcc), Some(&missing)),
                // No kernel pinned means "compile one", which the toolchain can do.
                def("kernel-unpinned", Some(&qemu), Some(&gcc), None),
            ]
        }),
    );
    let restarted =
        AppState::with_data_dir(state.workspace_root_display(), state.data_dir()).expect("state");

    let runnable = |name: &str| restarted.sandbox(name).expect("stored").runnable;
    assert!(runnable("all-present"), "all three resources are there");
    assert!(!runnable("qemu-missing"), "no QEMU, no run");
    assert!(
        !runnable("toolchain-missing"),
        "no toolchain, nothing to compile with"
    );
    assert!(!runnable("kernel-missing"), "a pinned kernel must exist");
    assert!(
        runnable("kernel-unpinned"),
        "an unpinned kernel is one to compile"
    );
}

#[test]
fn an_uninstalled_definition_is_still_a_definition() {
    // The point of `runnable` being computed rather than stored: deleting the
    // resources does not make the definition disappear.
    let (state, data_dir) = state("uninstalled");
    let qemu = a_runnable_binary(&data_dir, &qemu_name());
    let gcc = a_runnable_binary(&data_dir, &compiler_name());
    write_settings(
        &data_dir,
        json!({
            "version": 1,
            "sandboxes": [{
                "name": "gone",
                "qemu_exe": qemu.display().to_string(),
                "toolchain_path": gcc.display().to_string()
            }]
        }),
    );
    let restarted =
        AppState::with_data_dir(state.workspace_root_display(), state.data_dir()).expect("state");
    assert!(restarted.sandbox("gone").expect("stored").runnable);

    std::fs::remove_file(&qemu).expect("uninstall the QEMU");
    let after = AppState::with_data_dir(restarted.workspace_root_display(), restarted.data_dir())
        .expect("state");
    let gone = after
        .sandbox("gone")
        .expect("the definition is still there");
    assert!(!gone.runnable, "it just cannot run");
    assert_eq!(gone.source, SandboxSource::Manual);
}

// ----- the shapes the API serves --------------------------------------------

#[test]
fn a_definition_can_be_built_from_one_scanned_resource() {
    let def = SandboxDef::for_resource("qemu", "11.1.0", PathBuf::from("/qemu"));
    assert_eq!(def.name, "qemu-11.1.0");
    assert_eq!(def.qemu_exe, Some(PathBuf::from("/qemu")));
    assert_eq!(def.toolchain_path, None);
}

#[test]
fn the_registry_never_names_a_definition_after_a_missing_version() {
    // `qemu--` is what the scan's version-less QEMU used to become once the
    // registry was merged (v0.9 sandbox F2a-3). Every name a client sees is
    // `<kind>-<version>`, or the resource's own name when the scan has no version
    // for it.
    let (state, data_dir) = state("names");
    plant(&data_dir, "qemu", "11.1.0", &qemu_name());
    let views = state.sandboxes();

    let names: Vec<&str> = views.iter().map(|v| v.name.as_str()).collect();
    for name in &names {
        assert!(!name.ends_with("--"), "{names:?}");
        assert!(!name.is_empty(), "{names:?}");
    }
    assert!(names.contains(&"qemu-11.1.0"), "{names:?}");

    // When this machine has a QEMU of its own, the scan reports it with no version
    // and the registry names it for the emulator — never `qemu--`.
    if state
        .sandbox_candidates()
        .qemus
        .iter()
        .any(|c| c.origin == "system")
    {
        assert!(names.contains(&"qemu-system-riscv64"), "{names:?}");
        assert!(!names.contains(&"qemu--"), "{names:?}");
    }
}
