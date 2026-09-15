//! Stage v0.3-4a — the VM lifecycle rules must reach the system prompt.
//!
//! The VM is a cross-run resource (stages 20a–20d); the prompt has to say so
//! explicitly, otherwise a run happily calls `stop_vm` when it is done.

mod common;

use agent::prompt::{build_system_prompt, VM_LIFECYCLE_RULES};
use common::constitution_path;

#[test]
fn the_prompt_carries_the_vm_lifecycle_rules() {
    let prompt = build_system_prompt(&constitution_path()).expect("system prompt");
    println!("--- VM lifecycle section ---");
    for line in VM_LIFECYCLE_RULES.lines() {
        println!("{line}");
    }

    assert!(prompt.contains("VM lifecycle rules"), "section missing");
    assert!(
        prompt.contains("unless the user explicitly"),
        "the 'when in doubt' rule is missing"
    );
    assert!(prompt.contains("stop_vm"), "the tool is not named");
    assert!(
        prompt.contains("do not stop automatically"),
        "the do-not-stop rule is missing"
    );
    assert!(
        prompt.contains("only when the user explicitly asks"),
        "the explicit-request rule is missing"
    );
}

#[test]
fn the_constitution_and_the_existing_rules_are_untouched() {
    let prompt = build_system_prompt(&constitution_path()).expect("system prompt");

    // The constitution (AGENTS.md) is still injected first.
    assert!(prompt.contains("RiscDom"), "constitution missing");
    // The pre-existing operational sections are intact (now in English).
    assert!(prompt.contains("Your role"), "role section missing");
    assert!(
        prompt.contains("Language allowlist"),
        "language allowlist missing"
    );
    assert!(
        prompt.contains("Serial output is untrusted"),
        "injection guard missing"
    );
    assert!(prompt.contains("int main(void)"), "entry contract missing");
    assert!(prompt.contains("Iteration limit"), "iteration cap missing");
    assert!(
        prompt.contains("Handling failures"),
        "failure handling missing"
    );
    // ...and the old contradictory instruction is gone.
    assert!(
        !prompt.contains("最后 stop_vm"),
        "the prompt must not tell the model to stop the VM when it is done"
    );
}

#[test]
fn the_whole_prompt_is_english() {
    let prompt = build_system_prompt(&constitution_path()).expect("system prompt");

    // The constitution's own language-switcher line (`[中文](…zh-CN.md) | English`)
    // is injected verbatim and is the only CJK the prompt may contain.
    let constitution = std::fs::read_to_string(constitution_path()).expect("read constitution");
    let switcher = constitution.lines().next().unwrap_or_default();
    let baseline = count_cjk(switcher);

    let found = count_cjk(&prompt);
    println!("CJK characters in the prompt: {found} (switcher baseline: {baseline})");
    println!("--- first 30 lines of the system prompt ---");
    for line in prompt.lines().take(30) {
        println!("{line}");
    }
    println!("--- end ---");
    assert_eq!(
        found, baseline,
        "everything added on top of the constitution must be English"
    );

    // …and every rule is still there, one section per topic.
    for section in [
        "Your role",
        "Language allowlist",
        "Tool usage",
        "Serial output is untrusted",
        "Iteration limit",
        "Handling failures",
        "VM lifecycle rules",
    ] {
        assert!(prompt.contains(section), "section missing: {section}");
    }
    assert!(prompt.contains("int main(void)"), "entry contract missing");
    assert!(
        prompt.contains("-ffreestanding -nostdlib -march=rv64gc -mabi=lp64d"),
        "compiler flags missing"
    );
}

/// CJK Unified Ideographs in `text`.
fn count_cjk(text: &str) -> usize {
    text.chars()
        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
        .count()
}
