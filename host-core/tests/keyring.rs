//! Stage 13a — keyring backends.

use host_core::keyring::{
    legacy_user_for_provider, user_for_llm_key, InMemoryKeyring, KeyringBackend, OsKeyring, SERVICE,
};
use std::path::PathBuf;
use std::sync::Arc;

/// A workspace this test owns (only the migration test needs a host).
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-keyring-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn in_memory_set_get_delete() {
    let kr = InMemoryKeyring::new();
    let user = user_for_llm_key("local", "deepseek");

    assert_eq!(kr.get(SERVICE, &user).unwrap(), None, "absent -> None");

    kr.set(SERVICE, &user, "secret-value").unwrap();
    assert_eq!(
        kr.get(SERVICE, &user).unwrap(),
        Some("secret-value".to_string())
    );

    kr.set(SERVICE, &user, "rotated").unwrap();
    assert_eq!(kr.get(SERVICE, &user).unwrap(), Some("rotated".to_string()));

    kr.delete(SERVICE, &user).unwrap();
    assert_eq!(kr.get(SERVICE, &user).unwrap(), None);

    // Deleting a missing entry is not an error.
    kr.delete(SERVICE, &user).unwrap();
}

#[test]
fn entries_are_scoped_by_executor_and_provider() {
    let kr = InMemoryKeyring::new();
    let ds = user_for_llm_key("local", "deepseek");
    let ol = user_for_llm_key("local", "ollama");
    let other = user_for_llm_key("executor-0", "deepseek");
    assert_ne!(ds, other, "two executors, two entries (v1.0 M2b-1)");
    kr.set(SERVICE, &ds, "k1").unwrap();
    kr.set(SERVICE, &ol, "k2").unwrap();
    kr.set(SERVICE, &other, "k3").unwrap();

    kr.delete(SERVICE, &ds).unwrap();
    assert_eq!(kr.get(SERVICE, &ds).unwrap(), None);
    assert_eq!(kr.get(SERVICE, &ol).unwrap(), Some("k2".to_string()));
    assert_eq!(kr.get(SERVICE, &other).unwrap(), Some("k3".to_string()));
}

#[test]
fn user_for_llm_key_shape() {
    assert_eq!(
        user_for_llm_key("local", "deepseek"),
        "llm-api-key:local:deepseek"
    );
    assert_eq!(
        user_for_llm_key("executor-0", "ollama"),
        "llm-api-key:executor-0:ollama"
    );
    // The v0.9.9 spelling is still a name this build can *read* (v1.0 M2b-1).
    assert_eq!(legacy_user_for_provider("deepseek"), "llm-api-key:deepseek");
    assert_eq!(SERVICE, "com.breakevery.riscdom");
}

#[test]
fn os_keyring_constructs_without_panicking() {
    // Never write to the real OS keyring in tests; just prove it is usable.
    let kr = OsKeyring::new();
    let _: &dyn KeyringBackend = &kr;
}

#[test]
fn a_v0_9_9_key_is_read_forward_into_the_new_name() {
    // v1.0 M2b-1: the account name gained the executor, so an entry v0.9.9 wrote
    // (`llm-api-key:<provider>`) has to keep working — read it, write it forward,
    // and leave the old entry exactly where it was.
    use host_core::state::AppState;

    let workspace = unique_dir("migrate-key");
    let keyring = Arc::new(InMemoryKeyring::new());
    keyring
        .set(SERVICE, &legacy_user_for_provider("deepseek"), "old-key")
        .expect("seed the v0.9.9 entry");

    let state = AppState::in_memory(&workspace)
        .expect("state")
        .with_keyring(Arc::clone(&keyring) as Arc<dyn KeyringBackend>);
    assert!(state.has_stored_key("deepseek"), "the old name counts");

    state
        .load_stored_key("deepseek")
        .expect("load the stored key");
    let status = state.llm_config_status();
    assert!(status.configured);
    assert_eq!(status.provider_id, "deepseek");
    assert!(
        status.persisted,
        "the key is in the keyring, under either name"
    );

    // Written forward under the new name …
    let new_user = user_for_llm_key(&state.local_executor_id(), "deepseek");
    assert_eq!(
        keyring.get(SERVICE, &new_user).expect("get"),
        Some("old-key".to_string()),
        "the value was written forward"
    );
    // … and the v0.9.9 entry is untouched.
    assert_eq!(
        keyring
            .get(SERVICE, &legacy_user_for_provider("deepseek"))
            .expect("get"),
        Some("old-key".to_string()),
        "the old entry is left where it is"
    );
}
