//! v0.8 follow-up A2 — the preflight directory is per agent.
//!
//! Two processes may share one workspace; before this change both compiled the
//! preflight guest and booted their preflight VM into
//! `<workspace>/.riscdom/preflight`, at the same paths, at the same time. New
//! artifacts go to `<workspace>/.riscdom/preflight/<agent_id>/`, and a guest an
//! older version left in the shared root is still readable.
//!
//! These tests touch the filesystem only: no compiler, no QEMU, no guest. The
//! full preflight (which does boot a guest) keeps its own tests in
//! `tests/preflight.rs`.

use host::preflight::GUEST_SRC;
use host::state::AppState;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-preflight-files-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Two instances over **one** workspace, each with its own data directory.
fn two_agents_on_one_workspace(tag: &str) -> (AppState, AppState, PathBuf) {
    let workspace = unique_dir(tag);
    let first =
        AppState::with_data_dir(workspace.clone(), workspace.join("data-a")).expect("state a");
    let second =
        AppState::with_data_dir(workspace.clone(), workspace.join("data-b")).expect("state b");
    (first, second, workspace)
}

#[test]
fn two_agents_write_their_own_preflight_guest() {
    let (first, second, workspace) = two_agents_on_one_workspace("isolation");
    assert_ne!(
        first.agent_id(),
        second.agent_id(),
        "two agents, two identities"
    );

    // The layout: <workspace>/.riscdom/preflight/<agent_id>/, under a shared root.
    let root = workspace.join(".riscdom").join("preflight");
    assert_eq!(first.preflight_root(), root);
    assert_eq!(second.preflight_root(), root);
    assert_eq!(first.preflight_dir(), root.join(first.agent_id()));
    assert_eq!(second.preflight_dir(), root.join(second.agent_id()));
    assert_ne!(first.preflight_dir(), second.preflight_dir());

    let (first_src, first_elf) = first.write_preflight_guest().expect("write a");
    let (second_src, _second_elf) = second.write_preflight_guest().expect("write b");
    assert_eq!(first_src, first.preflight_dir().join("guest.c"));
    assert_eq!(first_elf, first.preflight_dir().join("guest.elf"));
    assert_eq!(second_src, second.preflight_dir().join("guest.c"));
    assert_ne!(first_src, second_src, "each agent owns its own guest.c");

    // The same name in two agents' directories is two files: mark one, touch the
    // other, and the first is untouched.
    std::fs::write(&first_src, "// first agent's guest").expect("mark a");
    let (second_src_again, _) = second.write_preflight_guest().expect("rewrite b");
    assert_eq!(second_src_again, second_src);
    assert_eq!(
        std::fs::read_to_string(&first_src).expect("read a"),
        "// first agent's guest",
        "the second agent's write must not touch the first agent's guest"
    );
    // ...and the second agent's own file holds the real source.
    assert_eq!(
        std::fs::read_to_string(&second_src).expect("read b"),
        GUEST_SRC
    );
}

#[test]
fn a_guest_left_in_the_shared_root_is_still_readable() {
    let (state, _other, _workspace) = two_agents_on_one_workspace("compat");
    let root = state.preflight_root();
    let own = state.preflight_dir();

    // Nothing anywhere yet.
    assert_eq!(state.find_preflight_guest(), None);

    // An older version's guest, in the shared root only.
    std::fs::create_dir_all(&root).expect("root");
    let legacy = root.join("guest.elf");
    std::fs::write(&legacy, b"older guest").expect("plant");
    assert_eq!(
        state.find_preflight_guest(),
        Some(legacy.clone()),
        "a pre-A2 guest stays usable"
    );

    // This agent's own build exists now: it wins over the shared root.
    let (_, own_elf) = state.write_preflight_guest().expect("write");
    std::fs::write(&own_elf, b"this agent's guest").expect("plant own");
    assert_eq!(
        state.find_preflight_guest(),
        Some(own_elf.clone()),
        "the per-agent guest wins on a collision"
    );
    assert!(own_elf.starts_with(&own) && own != root);
}
