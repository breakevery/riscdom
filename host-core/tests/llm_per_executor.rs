//! v1.0 M2b-3a — one LLM configuration per executor, and the wildcard session list.
//!
//! The batch's claim is that an executor is a **name** the whole LLM path is keyed
//! by: the configuration in memory, the entry in `settings.json`, the keyring
//! account and the status the UI reads. These tests hold the two executors apart and
//! check that neither can see or clobber the other's.

use host_core::keyring::{user_for_llm_key, InMemoryKeyring, KeyringBackend, SERVICE};
use host_core::settings::{is_reserved_executor_label, ExecutorSpecSettings, LocalSettings};
use host_core::state::{AppState, LlmConfigInput};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The executor every test here configures — never the node's own name.
const WORKER: &str = "worker-a";
/// A second worker, for the "clearing one leaves the other" case.
const OTHER: &str = "worker-b";

fn unique_ws(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-exec-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write a `settings.json` into a workspace, the way a person's file lands there.
fn write_settings(ws: &Path, settings: &LocalSettings) {
    let dir = ws.join(".riscdom");
    std::fs::create_dir_all(&dir).unwrap();
    settings
        .save(&dir.join("settings.json"))
        .expect("save settings");
}

/// Save a key for `executor`, remembered.
fn save_for(state: &AppState, executor: &str, model: &str) {
    state
        .set_llm_config_with_for(
            executor,
            Some("deepseek".into()),
            format!("{executor}-key-not-real"),
            "http://127.0.0.1:9/v1".into(),
            model.into(),
            Some(true),
        )
        .expect("save for the executor");
}

#[test]
fn two_executors_keep_their_own_configuration() {
    let state = AppState::in_memory(unique_ws("own")).expect("state");
    assert!(!state.llm_config_status().configured);

    save_for(&state, WORKER, "worker-model");

    let worker = state.llm_config_status_for(WORKER);
    assert!(worker.configured);
    assert_eq!(worker.model, "worker-model");
    assert_eq!(worker.base_url, "http://127.0.0.1:9/v1");
    assert!(worker.persisted, "the key is in the keyring");
    assert!(worker.config_persisted, "the non-secret half is on disk");

    // The node's own reading is untouched by a worker's save.
    assert!(!state.llm_config_status().configured);
    assert!(!state.has_stored_key("deepseek"));
    assert!(state.has_stored_key_for(WORKER, "deepseek"));

    // Neither status carries the key.
    let json = serde_json::to_string(&worker).unwrap();
    assert!(!json.contains("key-not-real"), "key leaked: {json}");
}

#[test]
fn the_keyring_account_names_the_executor() {
    let keyring = Arc::new(InMemoryKeyring::new());
    let state = AppState::in_memory(unique_ws("account"))
        .expect("state")
        .with_keyring(keyring.clone());

    save_for(&state, WORKER, "worker-model");

    let worker_user = user_for_llm_key(WORKER, "deepseek");
    let local_user = user_for_llm_key(&state.local_executor_id(), "deepseek");
    assert_ne!(worker_user, local_user);
    assert!(
        keyring.get(SERVICE, &worker_user).expect("get").is_some(),
        "the key is filed under the worker's own account"
    );
    assert!(
        keyring.get(SERVICE, &local_user).expect("get").is_none(),
        "and never under the node's"
    );
}

#[test]
fn clearing_one_executor_leaves_the_other() {
    let state = AppState::in_memory(unique_ws("clear")).expect("state");
    save_for(&state, WORKER, "worker-model");
    save_for(&state, OTHER, "other-model");

    state.clear_llm_config_for(WORKER);

    assert!(!state.llm_config_status_for(WORKER).configured);
    assert!(!state.has_stored_key_for(WORKER, "deepseek"));
    let other = state.llm_config_status_for(OTHER);
    assert!(other.configured);
    assert_eq!(other.model, "other-model");
    assert!(state.has_stored_key_for(OTHER, "deepseek"));

    // The node's own clear is its own: a worker survives it.
    save_for(&state, WORKER, "worker-model");
    state.clear_llm_config();
    assert!(state.llm_config_status_for(WORKER).configured);
}

#[test]
fn readiness_follows_the_executor() {
    let state = AppState::in_memory(unique_ws("ready")).expect("state");
    assert!(!state.llm_readiness().ready);
    assert_eq!(state.llm_readiness().reason.as_deref(), Some("no_config"));

    save_for(&state, WORKER, "worker-model");

    assert!(state.llm_readiness_for(WORKER).ready);
    assert!(
        !state.llm_readiness().ready,
        "the node is still unconfigured"
    );
    assert!(!state.llm_readiness_for(OTHER).ready);
}

#[test]
fn a_named_executor_loads_its_own_persisted_entry() {
    let keyring = Arc::new(InMemoryKeyring::new());
    let ws = unique_ws("load");
    let first = AppState::in_memory(&ws)
        .expect("state")
        .with_keyring(keyring.clone());
    save_for(&first, WORKER, "worker-model");

    // "Restart": a fresh state sharing the same keyring and settings file.
    let second = AppState::in_memory(&ws)
        .expect("state")
        .with_keyring(keyring);
    assert!(!second.llm_config_status_for(WORKER).configured);
    assert!(second.has_stored_key_for(WORKER, "deepseek"));

    second
        .load_stored_key_for(WORKER, "deepseek")
        .expect("load the worker's key");
    let worker = second.llm_config_status_for(WORKER);
    assert!(worker.configured);
    // The worker's own endpoint and model come back, not a preset's and not the
    // node's.
    assert_eq!(worker.model, "worker-model");
    assert_eq!(worker.base_url, "http://127.0.0.1:9/v1");

    // The node itself never had a key, so its own load still fails.
    assert_eq!(
        second.load_stored_key("deepseek").unwrap_err(),
        "no_stored_key"
    );
}

#[test]
fn the_wildcard_list_covers_every_executor() {
    let state = AppState::in_memory(unique_ws("all")).expect("state");
    let local = state.local_executor_id();
    let mine = state.create_session("mine", &local).expect("local session");
    let theirs = state
        .create_session("theirs", WORKER)
        .expect("worker session");

    // One executor's list is that executor's.
    assert_eq!(state.list_sessions(10, &local).expect("local").len(), 1);
    assert_eq!(state.list_sessions(10, WORKER).expect("worker").len(), 1);

    // The wildcard is every session in one list, newest first.
    let all = state.list_all_sessions(10).expect("all");
    assert_eq!(all.len(), 2);
    let ids: Vec<&str> = all.iter().map(|meta| meta.id.as_str()).collect();
    assert!(ids.contains(&mine.as_str()) && ids.contains(&theirs.as_str()));
    assert!(
        all[0].updated_at_ms >= all[1].updated_at_ms,
        "newest first: {:?}",
        all.iter().map(|m| m.updated_at_ms).collect::<Vec<_>>()
    );

    // The limit counts rows, not rows per executor.
    assert_eq!(state.list_all_sessions(1).expect("one").len(), 1);
    // Owner labels travel with the rows, so a merged list can say whose each is.
    assert!(all
        .iter()
        .any(|meta| meta.executor_id.as_deref() == Some(WORKER)));
    assert!(all
        .iter()
        .any(|meta| meta.executor_id.as_deref() == Some(local.as_str())));
}

#[test]
fn a_label_the_node_already_answers_to_is_not_registered() {
    let ws = unique_ws("reserved");
    let settings = LocalSettings {
        executors: vec![
            ExecutorSpecSettings {
                label: "local".into(),
                program: "worker.exe".into(),
                args: Vec::new(),
            },
            ExecutorSpecSettings {
                label: "*".into(),
                program: "worker.exe".into(),
                args: Vec::new(),
            },
            ExecutorSpecSettings {
                label: WORKER.into(),
                program: "worker.exe".into(),
                args: Vec::new(),
            },
        ],
        ..LocalSettings::default()
    };
    write_settings(&ws, &settings);

    assert!(is_reserved_executor_label("local"));
    assert!(is_reserved_executor_label("*"));

    let state = AppState::in_memory(&ws).expect("state");
    assert_eq!(
        state.executors(),
        vec![WORKER.to_string()],
        "only the label nobody else answers to is a target"
    );

    // The skip is visible: one audit event per refused label, not a silent drop.
    let events = state
        .list_events(
            50,
            host_core::EventFilter {
                action_prefix: Some("host.executor".into()),
                ..Default::default()
            },
        )
        .expect("events");
    assert_eq!(
        events.len(),
        2,
        "{:?}",
        events.iter().map(|e| e.action.clone()).collect::<Vec<_>>()
    );
    assert!(events.iter().all(|e| e.action == "host.executor.reserved"));

    // The settings file itself is left exactly as the person wrote it.
    let loaded = LocalSettings::load(&ws.join(".riscdom").join("settings.json"));
    assert_eq!(
        loaded.executors.len(),
        3,
        "the file is not rewritten or refused"
    );
}

#[test]
fn a_named_executor_is_configured_in_memory_without_touching_the_file() {
    let ws = unique_ws("memory");
    let state = AppState::in_memory(&ws).expect("state");
    state.set_llm_config_for(
        WORKER,
        LlmConfigInput {
            provider_id: "deepseek".into(),
            api_key: "in-memory-only".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            model: "worker-model".into(),
        },
    );
    assert!(state.llm_config_status_for(WORKER).configured);
    assert!(
        !state.llm_config_status_for(WORKER).config_persisted,
        "the memory seam writes no settings file"
    );
    assert!(!ws.join(".riscdom").join("settings.json").exists());
}
