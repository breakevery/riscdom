//! Stage 26 — the **real** OS keyring.
//!
//! Ignored by default: it writes a temporary entry into the actual OS credential
//! store. Run it by hand:
//!
//! ```text
//! cargo test -p host --test keyring_os -- --ignored --nocapture
//! ```
//!
//! Unit tests keep using `InMemoryKeyring`; only this file touches the OS store.
#![cfg(target_os = "windows")]

use host::keyring::{KeyringBackend, OsKeyring};

const SERVICE: &str = "com.breakevery.riscdom.test";
const USER: &str = "probe";
const TARGET: &str = "com.breakevery.riscdom.test";

fn cmdkey(args: &[&str]) -> String {
    let out = std::process::Command::new("cmdkey")
        .args(args)
        .output()
        .expect("run cmdkey");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    format!("{stdout}{stderr}")
}

#[test]
#[ignore = "writes a real Windows Credential Manager entry; run with --ignored"]
fn os_keyring_persists_to_credential_manager() {
    let backend = OsKeyring::new();

    // Start clean (a leftover from an aborted run must not mask the result).
    let _ = backend.delete(SERVICE, USER);

    backend
        .set(SERVICE, USER, "test-value")
        .expect("set must succeed once a native backend is enabled");

    // 1. The OS store returns what we wrote.
    assert_eq!(
        backend.get(SERVICE, USER).expect("get").as_deref(),
        Some("test-value")
    );

    // 2. Windows itself can see the credential.
    //    NB: `cmdkey /list:<filter>` echoes the filter in a header line even when
    //    nothing matches, and keyring's windows-native target is
    //    `<user>.<service>`. So count occurrences of the full target name: an
    //    existing entry appears twice (echoed header + entry block), an absent
    //    one once (echoed header only). Locale-independent.
    let os_target = format!("{USER}.{TARGET}");
    let listed = cmdkey(&[&format!("/list:{os_target}")]);
    println!("--- cmdkey /list:{os_target} (after set) ---\n{listed}");
    assert!(
        listed.matches(&os_target).count() >= 2,
        "cmdkey did not show the credential for {os_target}; full listing for diagnosis:\n{}",
        cmdkey(&["/list"])
    );

    // 3. Overwriting the same target works (no manual delete required).
    backend
        .set(SERVICE, USER, "second-value")
        .expect("overwrite");
    assert_eq!(
        backend.get(SERVICE, USER).expect("get").as_deref(),
        Some("second-value")
    );

    // 4. Deleting removes it from the OS store and from Windows.
    backend.delete(SERVICE, USER).expect("delete");
    assert_eq!(backend.get(SERVICE, USER).expect("get"), None);
    // Belt and braces: also drop the concrete OS target name (`<user>.<service>`).
    let _ = std::process::Command::new("cmdkey")
        .args([&format!("/delete:{USER}.{TARGET}")])
        .output();

    let after = cmdkey(&[&format!("/list:{os_target}")]);
    println!("--- after cleanup ---\n{after}");
    assert_eq!(
        after.matches(&os_target).count(),
        1,
        "credentials left behind (expected only the echoed header):\n{}",
        cmdkey(&["/list"])
    );
}
