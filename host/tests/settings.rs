//! Stage 25a — local settings file (`settings.json`) and toolchain persistence.

use host::settings::{LocalSettings, SETTINGS_VERSION};
use host::state::AppState;
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
    };
    settings.save(&path).expect("save");
    assert!(path.is_file(), "{path:?}");
    assert_eq!(LocalSettings::load(&path), settings);
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
