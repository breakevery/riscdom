//! The library's runtime logging: off by default, opt-in through `--log-level`.
//!
//! These lines go to the **process's** stderr, so this drives the `riscdom-server`
//! binary and reads what it wrote, rather than trying to capture `eprintln!` from
//! inside the test process.
//!
//! Nothing here needs the network (loopback only), a model or QEMU: the one thing
//! that has to happen is a connection that ends badly, which is what the
//! `connection … ended` line reports.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-server-log-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// Start `riscdom-server` on an ephemeral port and read its banner back, so the
/// port it picked is known. stderr is drained from the start into a buffer.
fn start(extra: &[&str]) -> (Child, SocketAddr, Arc<Mutex<String>>) {
    let workspace = unique_dir("ws");
    let data_dir = unique_dir("data");
    let mut child = Command::new(env!("CARGO_BIN_EXE_riscdom-server"))
        .arg("--bind")
        .arg("127.0.0.1:0")
        .arg("--workspace")
        .arg(&workspace)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("--no-auth")
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");

    let stderr = child.stderr.take().expect("stderr is piped");
    let collected = drain(stderr);
    let addr = read_banner(child.stdout.take().expect("stdout is piped"));
    (child, addr, collected)
}

/// Read stderr line by line into a shared buffer, so a test can watch it grow.
fn drain(pipe: ChildStderr) -> Arc<Mutex<String>> {
    let buffer = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&buffer);
    std::thread::spawn(move || {
        for line in BufReader::new(pipe).lines() {
            let Ok(line) = line else { break };
            if let Ok(mut guard) = sink.lock() {
                guard.push_str(&line);
                guard.push('\n');
            }
        }
    });
    buffer
}

/// The address from `riscdom-server … listening on http://<addr>`.
fn read_banner(stdout: impl std::io::Read) -> SocketAddr {
    for line in BufReader::new(stdout).lines() {
        let line = line.expect("the banner line");
        if let Some(rest) = line.split("listening on http://").nth(1) {
            return rest.trim().parse().expect("the bound address");
        }
    }
    panic!("the server never printed its banner");
}

/// Open a connection, start a request and vanish: the server reads an incomplete
/// message, which is the error it logs when logging is on.
fn close_abruptly(addr: SocketAddr) {
    let mut stream = TcpStream::connect(addr).expect("the server accepts");
    stream
        .write_all(b"POST /v0/health HTTP/1.1\r\nHost: x\r\n")
        .expect("write half a request");
    let _ = stream.flush();
    std::thread::sleep(Duration::from_millis(50));
    drop(stream);
}

/// Wait for `needle` to appear in the drained stderr.
fn wait_for_line(buffer: &Arc<Mutex<String>>, needle: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(guard) = buffer.lock() {
            if guard.contains(needle) {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn text_of(buffer: &Arc<Mutex<String>>) -> String {
    buffer.lock().map(|guard| guard.clone()).unwrap_or_default()
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn the_connection_line_is_off_by_default() {
    // The default matters: the CLI embeds this server, and its stderr is the CLI's
    // own error channel (in `--json`, the error object itself).
    let (child, addr, stderr) = start(&[]);
    close_abruptly(addr);
    let seen = wait_for_line(&stderr, "ended", Duration::from_millis(700));
    let text = text_of(&stderr);
    stop(child);
    assert!(!seen, "the default must be silent, got: {text}");
    assert!(!text.contains("connection from"), "{text}");
}

#[test]
fn the_connection_line_appears_at_info() {
    let (child, addr, stderr) = start(&["--log-level", "info"]);
    close_abruptly(addr);
    let seen = wait_for_line(&stderr, "ended", Duration::from_secs(5));
    let text = text_of(&stderr);
    stop(child);
    assert!(seen, "info must log the connection, got: {text}");
    assert!(text.contains("riscdom-server: connection from"), "{text}");
    // The peer and the error only: never a credential.
    assert!(!text.contains("Authorization"), "{text}");
}

#[test]
fn error_keeps_the_per_connection_line_quiet() {
    // `error` is for failures, not for the per-connection chatter: an operator who
    // asks for errors does not want one line per closed socket.
    let (child, addr, stderr) = start(&["--log-level", "error"]);
    close_abruptly(addr);
    let seen = wait_for_line(&stderr, "ended", Duration::from_millis(700));
    let text = text_of(&stderr);
    stop(child);
    assert!(!seen, "error must not log connections, got: {text}");
}

#[test]
fn a_bad_log_level_is_a_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_riscdom-server"))
        .args(["--log-level", "verbose"])
        .output()
        .expect("the server runs");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown log level"), "{stderr}");
    // A usage error prints the usage on stderr, next to the message.
    assert!(stderr.contains("--log-level <off|error|info>"), "{stderr}");
}
