//! Stage 13a — keyring backends.

use host_core::keyring::{user_for_provider, InMemoryKeyring, KeyringBackend, OsKeyring, SERVICE};

#[test]
fn in_memory_set_get_delete() {
    let kr = InMemoryKeyring::new();
    let user = user_for_provider("deepseek");

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
fn entries_are_scoped_by_provider() {
    let kr = InMemoryKeyring::new();
    let ds = user_for_provider("deepseek");
    let ol = user_for_provider("ollama");
    kr.set(SERVICE, &ds, "k1").unwrap();
    kr.set(SERVICE, &ol, "k2").unwrap();

    kr.delete(SERVICE, &ds).unwrap();
    assert_eq!(kr.get(SERVICE, &ds).unwrap(), None);
    assert_eq!(kr.get(SERVICE, &ol).unwrap(), Some("k2".to_string()));
}

#[test]
fn user_for_provider_shape() {
    assert_eq!(user_for_provider("deepseek"), "llm-api-key:deepseek");
    assert_eq!(user_for_provider("ollama"), "llm-api-key:ollama");
    assert_eq!(SERVICE, "com.breakevery.riscdom");
}

#[test]
fn os_keyring_constructs_without_panicking() {
    // Never write to the real OS keyring in tests; just prove it is usable.
    let kr = OsKeyring::new();
    let _: &dyn KeyringBackend = &kr;
}
