//! Stage 19a — the migration TCP relay.

use sandbox::relay::MigrationRelay;
use sandbox::SandboxError;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

fn unique_path(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("riscdom-relay-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// 1 MiB of deterministic pseudo-random bytes.
fn sample(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push((state >> 24) as u8);
    }
    out
}

#[test]
fn receive_to_file_writes_every_byte() {
    let data = sample(1024 * 1024);
    let path = unique_path("recv.bin");
    let relay = MigrationRelay::bind_local().expect("bind");
    let addr = relay.addr();

    let payload = data.clone();
    let sender = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).expect("connect");
        stream.write_all(&payload).expect("write");
        let _ = stream.shutdown(Shutdown::Write);
    });

    let written = relay.receive_to_file(&path).expect("receive");
    sender.join().expect("join");

    assert_eq!(written, data.len() as u64);
    assert_eq!(std::fs::read(&path).expect("read back"), data);
}

#[test]
fn send_file_delivers_the_whole_file() {
    let data = sample(512 * 1024);
    let path = unique_path("send.bin");
    std::fs::write(&path, &data).expect("write source");

    let relay = MigrationRelay::bind_local().expect("bind");
    let addr = relay.addr();

    let receiver = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(addr).expect("connect");
        let mut got = Vec::new();
        stream.read_to_end(&mut got).expect("read");
        got
    });

    let sent = relay.send_file(&path).expect("send");
    let got = receiver.join().expect("join");

    assert_eq!(sent, data.len() as u64);
    assert_eq!(got, data);
}

#[test]
fn receive_times_out_when_nobody_connects() {
    let relay = MigrationRelay::bind_local_with_timeout(Duration::from_millis(300)).expect("bind");
    let path = unique_path("never.bin");

    let err = relay.receive_to_file(&path).expect_err("must time out");
    assert!(
        matches!(err, SandboxError::RelayTimeout(_)),
        "expected RelayTimeout, got {err:?}"
    );
}

#[test]
fn send_file_fails_fast_for_a_missing_path() {
    let relay = MigrationRelay::bind_local_with_timeout(Duration::from_millis(300)).expect("bind");
    let missing = unique_path("does-not-exist.bin");

    let err = relay.send_file(&missing).expect_err("must fail");
    assert!(
        matches!(err, SandboxError::Io(_)),
        "expected Io error, got {err:?}"
    );
}

#[test]
fn relay_address_is_loopback() {
    let relay = MigrationRelay::bind_local().expect("bind");
    assert!(relay.addr().ip().is_loopback());
    assert_ne!(relay.addr().port(), 0);
}
