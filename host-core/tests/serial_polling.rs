//! Stage 15c — serial text accumulation from the push stream.

use host_core::state::append_serial_chunk;
use std::sync::Mutex;

#[test]
fn appends_valid_utf8_and_returns_the_chunk() {
    let accum = Mutex::new(String::new());
    let first = append_serial_chunk(&accum, b"HELLO ");
    let second = append_serial_chunk(&accum, b"RISCV\n");
    assert_eq!(first, "HELLO ");
    assert_eq!(second, "RISCV\n");
    assert_eq!(*accum.lock().unwrap(), "HELLO RISCV\n");
}

#[test]
fn invalid_utf8_is_converted_lossily() {
    let accum = Mutex::new(String::new());
    let text = append_serial_chunk(&accum, &[0x41, 0xFF, 0x42]);
    assert!(text.starts_with('A'), "{text:?}");
    assert!(text.ends_with('B'), "{text:?}");
    assert!(
        text.contains('\u{FFFD}'),
        "expected a replacement char: {text:?}"
    );
    assert_eq!(*accum.lock().unwrap(), text);
}

#[test]
fn empty_chunk_is_a_no_op() {
    let accum = Mutex::new(String::new());
    assert_eq!(append_serial_chunk(&accum, b""), "");
    assert_eq!(*accum.lock().unwrap(), "");
}
