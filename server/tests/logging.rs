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
use std::process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio};
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
/// port it picked is known.
///
/// Returns `(child, addr, stderr, stdout)`: **both** pipes are drained from the start —
/// stderr into a buffer the tests watch, stdout to EOF. The stdout half is what the CI
/// failure was about: the server writes three more lines after the banner, and a reader
/// that walks away turns those writes into EPIPE — which on Unix is SIGPIPE, and SIGPIPE
/// kills the process before it can log the line these tests wait for (v0.9 logging batch).
fn start(extra: &[&str]) -> (Child, SocketAddr, Drained, Drained) {
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
    let stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
    let (addr, stdout) = read_banner(stdout);
    let printed = drain_reader(stdout);
    (child, addr, collected, printed)
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

/// The address from `riscdom-server … listening on http://<addr>`, **plus the reader**.
///
/// Handing the reader back is the fix: the server prints three more lines after the banner
/// (`server/src/main.rs:81-83`), and a reader that goes away turns those writes into EPIPE —
/// which on Unix is SIGPIPE, which kills the process before it can log the
/// `connection from … ended` line these tests wait for. The caller drains what is left
/// (v0.9 logging batch).
fn read_banner(mut stdout: BufReader<ChildStdout>) -> (SocketAddr, BufReader<ChildStdout>) {
    let mut line = String::new();
    loop {
        line.clear();
        let read = stdout.read_line(&mut line).expect("the banner line");
        assert!(read > 0, "the server never printed its banner");
        if let Some(rest) = line.split("listening on http://").nth(1) {
            return (rest.trim().parse().expect("the bound address"), stdout);
        }
    }
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

/// How a child process left, in words a CI log can be read from.
///
/// `exited with code N` and `killed by signal N` are very different findings, and the CI
/// failure this batch is chasing showed the child **gone** with nothing else to go on — so
/// the next failure has to say which of the two it was (v0.9 logging batch).
fn exit_status(child: &mut Child) -> String {
    match child.try_wait() {
        Ok(None) => "still running".to_string(),
        Ok(Some(status)) => describe_status(&status),
        Err(e) => format!("could not be queried: {e}"),
    }
}

fn describe_status(status: &ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exited with code {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    "exited without a code".to_string()
}

/// [`diagnosis`] plus the child's own state.
///
/// A failure that only describes the *reader* cannot tell "the server never logged" from
/// "the server was already gone", which is exactly the distinction the last CI failure
/// needed.
fn diagnosis_with_child(drained: &Drained, child: &mut Child, stdout: &Drained) -> String {
    format!(
        "{}\nchild: {}\nserver stdout: {} line(s), reader {}",
        diagnosis(drained),
        exit_status(child),
        stdout.text().lines().count(),
        if stdout.alive() {
            "still reading"
        } else {
            "already stopped (EOF)"
        }
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
    let (mut child, addr, drained, _stdout) = start(&[]);
    close_abruptly(addr);
    let seen = wait_for_line(&drained, "ended", Duration::from_millis(700));
    let text = text_of(&drained);
    let child_state = exit_status(&mut child);
    stop(child);
    assert!(
        !seen,
        "the default must be silent (child: {child_state}), got: {text}"
    );
    assert!(!text.contains("connection from"), "{text}");
}

#[test]
fn the_connection_line_appears_at_info() {
    let (mut child, addr, drained, stdout) = start(&["--log-level", "info"]);
    close_abruptly(addr);
    let seen = wait_for_line(&drained, "ended", Duration::from_secs(5));
    let text = text_of(&drained);
    let diagnosis = diagnosis_with_child(&drained, &mut child, &stdout);
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
    let (mut child, addr, drained, _stdout) = start(&["--log-level", "error"]);
    close_abruptly(addr);
    let seen = wait_for_line(&drained, "ended", Duration::from_millis(700));
    let text = text_of(&drained);
    let child_state = exit_status(&mut child);
    stop(child);
    assert!(
        !seen,
        "error must not log connections (child: {child_state}), got: {text}"
    );
}

/// The other half of the CI bug: a child whose stdout pipe is **kept** being read is not
/// killed by its own later writes.
///
/// Reading only the first line and walking away is what turned the server's three closing
/// `println!`s into a fatal write on Unix — the shape this test pins, and the reason a
/// successful child is what it asserts (a SIGPIPE death is not `success()`).
#[test]
fn a_pipe_that_is_read_to_the_end_does_not_kill_its_writer() {
    let mut child = if cfg!(windows) {
        Command::new("cmd")
            .args([
                "/c",
                "(echo first & echo second & echo third & echo fourth)",
            ])
            .stdout(Stdio::piped())
            .spawn()
    } else {
        Command::new("sh")
            .args(["-c", "echo first; echo second; echo third; echo fourth"])
            .stdout(Stdio::piped())
            .spawn()
    }
    .expect("spawn a process that writes more than one line");

    let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
    let mut first = String::new();
    stdout.read_line(&mut first).expect("the first line");
    assert!(first.contains("first"), "{first:?}");

    // Keep reading, exactly as `start` now does after the banner.
    let drained = drain_reader(stdout);
    assert!(
        wait_for_line(&drained, "fourth", Duration::from_secs(5)),
        "{}",
        diagnosis(&drained)
    );
    let status = child.wait().expect("wait");
    assert!(
        status.success(),
        "the writer must survive its own writes: {status:?} (a signal is not success)"
    );
}

/// The exit-status reporting is itself testable — which matters, because it is only ever
/// *used* on a machine where something has already gone wrong.
#[test]
fn a_child_that_has_left_says_how() {
    let mut child = if cfg!(windows) {
        Command::new("cmd").args(["/c", "exit", "1"]).spawn()
    } else {
        Command::new("sh").args(["-c", "exit 1"]).spawn()
    }
    .expect("spawn a process that leaves at once");
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(1));
    assert_eq!(exit_status(&mut child), "exited with code 1");
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
