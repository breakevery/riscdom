//! The library's runtime logging: off by default, opt-in through `--log-level`.
//!
//! These lines go to the **process's** stderr, so this drives the `riscdom-server`
//! binary and reads what it wrote, rather than trying to capture `eprintln!` from
//! inside the test process.
//!
//! Nothing here needs the network (loopback only), a model or QEMU: the one thing
//! that has to happen is a connection that ends badly, which is what the
//! `connection … ended` line reports.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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

/// What the drain thread has seen so far.
///
/// The buffer alone was not enough to explain a failure: when the CI run went red, the
/// message said the line never arrived, and nothing said whether the reader was still
/// reading, had stopped, or had quietly thrown something away. These three fields answer
/// that.
struct Drained {
    /// Every line the reader accepted, in order.
    text: Arc<Mutex<String>>,
    /// Lines the reader could not accept: not UTF-8, or a read that failed.
    bad_lines: Arc<AtomicUsize>,
    /// `true` while the draining thread is still reading (it goes `false` at EOF).
    alive: Arc<AtomicBool>,
}

impl Drained {
    fn text(&self) -> String {
        self.text
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn bad_lines(&self) -> usize {
        self.bad_lines.load(Ordering::Relaxed)
    }

    fn alive(&self) -> bool {
        self.alive.load(Ordering::Relaxed)
    }
}

/// Start `riscdom-server` on an ephemeral port and read its banner back, so the
/// port it picked is known. stderr is drained from the start into a buffer.
fn start(extra: &[&str]) -> (Child, SocketAddr, Drained) {
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
fn drain(pipe: ChildStderr) -> Drained {
    drain_reader(BufReader::new(pipe))
}

/// The same, for any reader: what makes the loop testable without a child process.
///
/// **One bad line must not end the reading.** This loop used to be
/// `for line in reader.lines() { let Ok(line) = line else { break }; … }`, so the first
/// line the reader could not accept ended the thread and threw away everything after it
/// — including the `connection from … ended` line these tests wait for (v0.9 logging
/// batch). Now a bad line is counted and the reading continues.
fn drain_reader(mut reader: impl BufRead + Send + 'static) -> Drained {
    let text = Arc::new(Mutex::new(String::new()));
    let bad_lines = Arc::new(AtomicUsize::new(0));
    let alive = Arc::new(AtomicBool::new(true));

    let sink = Arc::clone(&text);
    let bad = Arc::clone(&bad_lines);
    let running = Arc::clone(&alive);
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                // EOF: the server exited, so there is nothing left to read.
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(mut guard) = sink.lock() {
                        guard.push_str(&line);
                        if !line.ends_with('\n') {
                            guard.push('\n');
                        }
                    }
                }
                // Not UTF-8. `read_line` has already consumed those bytes, so continuing
                // makes progress: the next call reads the following line.
                Err(e) if e.kind() == ErrorKind::InvalidData => {
                    bad.fetch_add(1, Ordering::Relaxed);
                }
                // A signal interrupted the read: retry rather than count it.
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                // Anything else is counted and the reading carries on. A closed pipe
                // arrives as `Ok(0)`, so this is not how the server's exit is seen.
                Err(_) => {
                    bad.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        running.store(false, Ordering::Relaxed);
    });

    Drained {
        text,
        bad_lines,
        alive,
    }
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
///
/// The timeout is unchanged and the poll stays: this batch fixed the reader and the
/// diagnosis, not the waiting (v0.9 logging batch).
fn wait_for_line(drained: &Drained, needle: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if drained.text().contains(needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn text_of(drained: &Drained) -> String {
    drained.text()
}

/// What a failing test should say: everything the reader saw, and what it made of it.
///
/// Written for the next CI failure rather than for a passing run: the reader's own state
/// (`alive` / `bad_lines`) is what distinguishes "the line never arrived" from "the reader
/// had stopped before it did".
fn diagnosis(drained: &Drained) -> String {
    let text = drained.text();
    format!(
        "{} line(s) captured, {} line(s) unreadable, reader {}:\n--- captured stderr ---\n{}---",
        text.lines().count(),
        drained.bad_lines(),
        if drained.alive() {
            "still reading"
        } else {
            "already stopped (EOF)"
        },
        text
    )
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn the_connection_line_is_off_by_default() {
    // The default matters: the CLI embeds this server, and its stderr is the CLI's
    // own error channel (in `--json`, the error object itself).
    let (child, addr, drained) = start(&[]);
    close_abruptly(addr);
    let seen = wait_for_line(&drained, "ended", Duration::from_millis(700));
    let text = text_of(&drained);
    stop(child);
    assert!(!seen, "the default must be silent, got: {text}");
    assert!(!text.contains("connection from"), "{text}");
}

#[test]
fn the_connection_line_appears_at_info() {
    let (child, addr, drained) = start(&["--log-level", "info"]);
    close_abruptly(addr);
    let seen = wait_for_line(&drained, "ended", Duration::from_secs(5));
    let text = text_of(&drained);
    let diagnosis = diagnosis(&drained);
    stop(child);
    assert!(seen, "info must log the connection: {diagnosis}");
    assert!(text.contains("riscdom-server: connection from"), "{text}");
    // The peer and the error only: never a credential.
    assert!(!text.contains("Authorization"), "{text}");
}

#[test]
fn error_keeps_the_per_connection_line_quiet() {
    // `error` is for failures, not for the per-connection chatter: an operator who
    // asks for errors does not want one line per closed socket.
    let (child, addr, drained) = start(&["--log-level", "error"]);
    close_abruptly(addr);
    let seen = wait_for_line(&drained, "ended", Duration::from_millis(700));
    let text = text_of(&drained);
    stop(child);
    assert!(!seen, "error must not log connections, got: {text}");
}

/// A stream the reader can finish on its own: the reader half of the CI bug, without a
/// child process.
#[test]
fn the_reader_catches_every_line_of_a_normal_stream() {
    let drained = drain_reader(std::io::Cursor::new(b"first\nsecond\nthird\n".to_vec()));
    wait_for_line(&drained, "third", Duration::from_secs(2));
    assert_eq!(drained.text(), "first\nsecond\nthird\n");
    assert_eq!(drained.bad_lines(), 0);
    assert!(!drained.alive(), "the reader stops at EOF");
}

/// The bug this batch fixes: a line that is not text used to end the reading, so every
/// later line was lost. It must cost exactly one counted line and nothing else.
#[test]
fn the_reader_keeps_going_after_a_line_that_is_not_text() {
    let mut bytes = b"first\n".to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe, b'\n']);
    bytes.extend_from_slice(b"the line the test is waiting for\n");
    let drained = drain_reader(std::io::Cursor::new(bytes));

    assert!(
        wait_for_line(
            &drained,
            "the line the test is waiting for",
            Duration::from_secs(2)
        ),
        "{}",
        diagnosis(&drained)
    );
    assert!(drained.text().contains("first"), "{}", diagnosis(&drained));
    assert_eq!(drained.bad_lines(), 1, "{}", diagnosis(&drained));
    assert!(
        !drained.alive(),
        "the reader stops at EOF, not on the bad line"
    );
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
