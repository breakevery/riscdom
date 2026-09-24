//! Stage 25a — local settings file (`settings.json`) and toolchain persistence.

use host_core::settings::{ExecutorSpecSettings, LocalSettings, SETTINGS_VERSION};
use host_core::state::AppState;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-settings-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_missing_file_loads_defaults() {
    let path = unique_dir("default").join("settings.json");
    let settings = LocalSettings::load(&path);
    assert_eq!(settings, LocalSettings::default());
    assert_eq!(settings.version, SETTINGS_VERSION);
    assert_eq!(settings.toolchain_path, None);
    assert_eq!(settings.zig_path, None);
    assert_eq!(settings.rust_sysroot, None);
}

#[test]
fn save_then_load_round_trips() {
    let path = unique_dir("roundtrip").join("nested").join("settings.json");
    let settings = LocalSettings {
        version: SETTINGS_VERSION,
        qemu_path: None,
        toolchain_path: Some(
            r"C:\tools\riscv64-unknown-elf\bin\riscv64-unknown-elf-gcc.exe".into(),
        ),
        zig_path: Some(r"C:\tools\zig\zig.exe".into()),
        rust_sysroot: Some(r"C:\tools\rust-std-riscv64gc-unknown-none-elf".into()),
        preflight: None,
        theme: None,
        language: None,
        alert_on_audit_failure: false,
        default_sandbox: None,
        sandboxes: Vec::new(),
        executors: Vec::new(),
    };
    settings.save(&path).expect("save");
    assert!(path.is_file(), "{path:?}");
    assert_eq!(LocalSettings::load(&path), settings);
}

#[test]
fn executors_round_trip_and_a_half_specified_one_is_dropped() {
    let path = unique_dir("executors").join("settings.json");
    let settings = LocalSettings {
        executors: vec![
            ExecutorSpecSettings {
                label: "executor-0".into(),
                program: r"C:\tools\worker.exe".into(),
                args: vec!["--workspace".into(), "W".into()],
            },
            ExecutorSpecSettings {
                label: "executor-1".into(),
                program: "/usr/local/bin/worker".into(),
                args: Vec::new(),
            },
        ],
        ..LocalSettings::default()
    };
    settings.save(&path).expect("save");
    assert_eq!(LocalSettings::load(&path), settings);

    // A file written before the field existed still loads, with no fleet.
    let older = unique_dir("executors-old").join("settings.json");
    std::fs::write(&older, br#"{"version":1}"#).expect("write");
    assert!(LocalSettings::load(&older).executors.is_empty());

    // A label with nothing to run is not an executor: keeping it would make the
    // fleet listing promise a target that could never answer.
    let half = unique_dir("executors-half").join("settings.json");
    std::fs::write(
        &half,
        br#"{"version":1,"executors":[{"label":"ghost","program":"  "},{"label":"ok","program":"worker.exe"}]}"#,
    )
    .expect("write");
    let loaded = LocalSettings::load(&half);
    assert_eq!(loaded.executors.len(), 1);
    assert_eq!(loaded.executors[0].label, "ok");
}

#[test]
fn corrupt_json_loads_defaults_without_panicking() {
    let path = unique_dir("corrupt").join("settings.json");
    std::fs::write(&path, b"{ this is not json").expect("write");
    assert_eq!(LocalSettings::load(&path), LocalSettings::default());

    // A valid file with an unexpected shape (wrong type) is equally harmless.
    std::fs::write(&path, br#"{"version": "one", "toolchain_path": 7}"#).expect("write");
    assert_eq!(LocalSettings::load(&path), LocalSettings::default());
}

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn a_manual_toolchain_survives_a_restart() {
    // The same workspace means the same settings file, so a second `AppState`
    // simulates an application restart.
    let workspace = unique_dir("restart");
    let discovered = {
        let state = AppState::in_memory(&workspace).expect("state");
        state
            .probe_toolchain()
            .path
            .expect("a toolchain must be discoverable here")
    };

    {
        let state = AppState::in_memory(&workspace).expect("state");
        assert!(
            !state.settings_path().is_file(),
            "starts with no settings file"
        );
        state.set_toolchain_path(&discovered).expect("set");
        assert_eq!(state.probe_toolchain().source, "Manual");
        assert!(
            state.settings_path().is_file(),
            "settings.json must be written to {:?}",
            state.settings_path()
        );
    }

    // "restart": the manual path must come back from disk.
    let restarted = AppState::in_memory(&workspace).expect("state");
    let view = restarted.probe_toolchain();
    assert!(view.found, "{}", view.diagnostics);
    assert_eq!(view.source, "Manual");
    assert_eq!(view.path.as_deref(), Some(discovered.as_str()));

    // Clearing persists as well.
    restarted.clear_toolchain_path().expect("clear");
    let after_clear = AppState::in_memory(&workspace).expect("state");
    assert_ne!(after_clear.probe_toolchain().source, "Manual");
}

/// v0.9 F3a: the Zig pin is a second single value, persisted and restored exactly like
/// the GCC one, and it is independent of it (either can be manual on its own).
///
/// A machine without Zig prints a skip rather than failing — the gate runs every ignored
/// test with `--include-ignored`, so this one guards itself instead of carrying a marker
/// that only holds on a machine that has Zig.
/// v0.9 F3b-1: the Rust sysroot is a **directory**, and the host refuses one that does not
/// carry the target's libraries — which turns "the compile fails later with a rustc message"
/// into "this directory is not a sysroot".
#[test]
fn a_rust_sysroot_must_carry_the_targets_libraries() {
    let workspace = unique_dir("rust-sysroot");
    let target = agent::RustConfig::from_env().target;

    let good = workspace.join("rust-std-riscv");
    std::fs::create_dir_all(good.join("lib").join("rustlib").join(&target).join("lib"))
        .expect("mkdir");
    let hollow = workspace.join("not-a-sysroot");
    std::fs::create_dir_all(&hollow).expect("mkdir");

    {
        let state = AppState::in_memory(&workspace).expect("state");
        let err = state
            .set_rust_sysroot(&hollow.display().to_string())
            .expect_err("a directory without the target's libraries is not a sysroot");
        println!("{err}");

        state
            .set_rust_sysroot(&good.display().to_string())
            .expect("a sysroot-shaped directory is accepted");
        assert_eq!(state.rust_config().sysroot, Some(good.clone()));
    }

    // "Restart": the choice comes back from settings.json.
    let restarted = AppState::in_memory(&workspace).expect("state");
    assert_eq!(restarted.rust_config().sysroot, Some(good.clone()));

    restarted.clear_rust_sysroot().expect("clear");
    let cleared = AppState::in_memory(&workspace).expect("state");
    assert_eq!(cleared.rust_config().sysroot, None);
}

#[test]
fn a_manual_zig_path_survives_a_restart() {
    let Some(zig) = agent::ZigConfig::discover().ok() else {
        eprintln!("skip: a_manual_zig_path_survives_a_restart -- no Zig found (ENVIRONMENT.md)");
        return;
    };
    let shown = zig.display().to_string();

    let workspace = unique_dir("restart-zig");
    {
        let state = AppState::in_memory(&workspace).expect("state");
        state.set_zig_path(&shown).expect("set");
        assert_eq!(state.zig_config().source, agent::ToolchainSource::Manual);
    }

    // "restart": the manual path must come back from disk.
    let restarted = AppState::in_memory(&workspace).expect("state");
    assert_eq!(restarted.zig_config().zig, zig);
    assert_eq!(
        restarted.zig_config().source,
        agent::ToolchainSource::Manual
    );

    // Clearing persists as well.
    restarted.clear_zig_path().expect("clear");
    let after_clear = AppState::in_memory(&workspace).expect("state");
    assert_ne!(
        after_clear.zig_config().source,
        agent::ToolchainSource::Manual
    );
}

#[test]
fn the_audit_alert_defaults_on_and_round_trips_off() {
    let workspace = unique_dir("audit-alert");
    let state = AppState::in_memory(&workspace).expect("state");

    // v0.8: on by default, including before the user ever touches the setting.
    assert!(state.alert_on_audit_failure());
    assert!(state.audit_status().expect("status").alert_on_failure);

    state.set_alert_on_audit_failure(false).expect("set false");
    assert!(!state.alert_on_audit_failure());
    assert!(
        !state.audit_status().expect("status").alert_on_failure,
        "the status view carries the setting, so the tab can render the toggle"
    );

    // "Restart": the choice comes back from settings.json.
    let restarted = AppState::in_memory(&workspace).expect("state");
    assert!(!restarted.alert_on_audit_failure());
    let text = std::fs::read_to_string(restarted.settings_path()).expect("settings.json");
    assert!(text.contains("\"alert_on_audit_failure\": false"), "{text}");

    restarted
        .set_alert_on_audit_failure(true)
        .expect("set true");
    assert!(AppState::in_memory(&workspace)
        .expect("state")
        .alert_on_audit_failure());
}

#[test]
fn a_wrongly_typed_audit_alert_degrades_to_the_default() {
    // The field is a bool; a hand-edited file that says anything else must not be
    // able to make the interface shout (or whisper) by accident.
    let path = unique_dir("audit-alert-bad").join("settings.json");
    std::fs::write(&path, br#"{"version": 1, "alert_on_audit_failure": "yes"}"#).expect("write");
    assert!(LocalSettings::load(&path).alert_on_audit_failure);
}

#[test]
fn the_theme_preference_round_trips_and_rejects_nonsense() {
    let workspace = unique_dir("theme");
    let state = AppState::in_memory(&workspace).expect("state");

    // Unset means "follow the system".
    assert_eq!(state.theme(), "system");

    state.set_theme("light").expect("set light");
    assert_eq!(state.theme(), "light");
    // It is normalised, so a sloppy caller cannot store junk.
    state.set_theme("  DARK ").expect("set dark");
    assert_eq!(state.theme(), "dark");
    assert!(
        state.set_theme("neon").is_err(),
        "unknown themes are refused"
    );
    assert_eq!(state.theme(), "dark", "a refused value changes nothing");

    // "restart": the preference comes back from settings.json.
    let restarted = AppState::in_memory(&workspace).expect("state");
    assert_eq!(restarted.theme(), "dark");
    let text = std::fs::read_to_string(restarted.settings_path()).expect("settings.json");
    assert!(text.contains("\"theme\": \"dark\""), "{text}");

    restarted.set_theme("system").expect("set system");
    assert_eq!(
        AppState::in_memory(&workspace).expect("state").theme(),
        "system"
    );
}

#[test]
fn the_language_preference_round_trips_and_rejects_nonsense() {
    let workspace = unique_dir("language");
    let state = AppState::in_memory(&workspace).expect("state");

    // Unset means "follow the system", like the theme's default.
    assert_eq!(state.language(), "system");

    state.set_language("zh").expect("set zh");
    assert_eq!(state.language(), "zh");
    // It is normalised, so a sloppy caller cannot store junk.
    state.set_language("  EN ").expect("set en");
    assert_eq!(state.language(), "en");
    assert!(
        state.set_language("fr").is_err(),
        "unknown languages are refused"
    );
    assert_eq!(state.language(), "en", "a refused value changes nothing");

    // "restart": the preference comes back from settings.json.
    let restarted = AppState::in_memory(&workspace).expect("state");
    assert_eq!(restarted.language(), "en");
    let text = std::fs::read_to_string(restarted.settings_path()).expect("settings.json");
    assert!(text.contains("\"language\": \"en\""), "{text}");

    restarted.set_language("system").expect("set system");
    assert_eq!(
        AppState::in_memory(&workspace).expect("state").language(),
        "system"
    );
}
