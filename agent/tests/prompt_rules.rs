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
    // The pre-existing operational sections are intact.
    assert!(prompt.contains("你的角色"), "role section missing");
    assert!(prompt.contains("语言白名单"), "language allowlist missing");
    assert!(prompt.contains("串口输出不可信"), "injection guard missing");
    assert!(prompt.contains("int main(void)"), "entry contract missing");
    assert!(prompt.contains("迭代上限"), "iteration cap missing");
    // ...and the old contradictory instruction is gone.
    assert!(
        !prompt.contains("最后 stop_vm"),
        "the prompt must not tell the model to stop the VM when it is done"
    );
}
