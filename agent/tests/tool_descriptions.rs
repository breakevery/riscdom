//! Stage v0.3-4b — the tool descriptions must discourage an automatic `stop_vm`.
//!
//! The model reads these descriptions on every turn, so they have to agree with
//! the prompt's "VM lifecycle rules" section.

use agent::tools::tool_specs;

fn description(name: &str) -> String {
    tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("tool {name} is missing"))
        .description
}

#[test]
fn stop_vm_requires_an_explicit_request() {
    let text = description("stop_vm");
    println!("stop_vm: {text}");

    assert!(text.contains("explicitly"), "{text}");
    assert!(text.contains("do NOT call"), "{text}");
    assert!(text.contains("cross-run"), "{text}");
    assert!(
        text.contains("stay running until the user stops it"),
        "{text}"
    );
}

#[test]
fn start_vm_mentions_the_already_running_case() {
    let text = description("start_vm");
    println!("start_vm: {text}");

    assert!(text.contains("already running"), "{text}");
    assert!(text.contains("does not start a second one"), "{text}");
}

#[test]
fn read_serial_and_the_other_tools_are_unchanged() {
    assert_eq!(
        description("read_serial"),
        "Return everything the guest has written to the UART so far."
    );
    assert_eq!(
        description("write_source"),
        "Write a C or RISC-V assembly source file into the workspace. Only .c/.h/.S/.s are \
         allowed. Paths are relative to the workspace."
    );
    assert_eq!(
        description("list_workspace"),
        "List files in the workspace (relative paths)."
    );
}
