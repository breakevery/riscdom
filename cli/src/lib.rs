//! riscdom — the command-line control-plane client.
//!
//! The CLI is a **client of the control plane**, not a second way into the
//! kernel: every command goes through HTTP, and the local mode simply starts the
//! control plane inside this process, on a loopback port the OS picks. That is
//! what keeps `riscdom` and `riscdom --remote host:port` the same code path (the
//! decision is in `docs/decisions.md` §8).
//!
//! ```text
//! riscdom [options] <command> [args]
//! ```
//!
//! Exit codes (the map is `client::exit_code_for` and the table is in
//! `cli/README.md`): `0` success, `1` a local failure (no connection, no token,
//! no workspace), `2` a usage error, a refused confirmation or a `400`, `3` the
//! control plane refused or failed, `4` authentication failed.

pub mod args;
pub mod client;
pub mod render;
pub mod sse;

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use args::{Args, Command, Parsed, USAGE};
pub use client::{exit_code_for, Client, Embedded, Error, Reply, Session};
pub use sse::{Frame, SseStream};

/// How long `--follow` waits for the stream to catch up after the run's own
/// request came back. The run is over by then; this is only the tail of the
/// stream, and waiting forever for a frame that may never come is worse than
/// cutting it.
const FOLLOW_GRACE: Duration = Duration::from_millis(500);

/// How long `--wait` sleeps between polls of the reader's flag. The waiting is
/// the point of the flag, so there is no deadline; what ends it is the event that
/// says the work is over (or the stream ending).
const WAIT_POLL: Duration = Duration::from_millis(5);

/// Run one parsed command line, writing to `out` on success and `err` on
/// failure. Returns the process exit code, because that is the CLI's contract.
///
/// Taking the streams as arguments is what lets the unit tests drive every
/// branch without spawning a process; `main` passes stdout and stderr. The one
/// exception is `--follow`, whose reader prints from a thread of its own and
/// therefore writes to stdout directly.
pub fn run(parsed: Parsed, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut args = match parsed {
        Parsed::Help => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        Parsed::Version => {
            let _ = writeln!(out, "riscdom {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Parsed::Command(args) => *args,
    };

    // `--api-key-file` is read before anything else: the key is part of the
    // request, and the warning for `--api-key` belongs on the way in.
    if let Err(error) = client::resolve_key_file(&mut args.command) {
        return report(&args, &error, err);
    }

    // One session: remote, or the control plane embedded in this process.
    let session = match Session::open(&args) {
        Ok(session) => session,
        Err(error) => return report(&args, &error, err),
    };

    // Anything that destroys state asks first — unless `--yes` answered already.
    if let Some(prompt) = args.command.confirmation() {
        if let Err(error) = client::confirm(&prompt, args.yes) {
            return report(&args, &error, err);
        }
    }

    if args.follow {
        return follow(&args, &session, out, err);
    }
    if args.wait {
        return match waiting_for(&args.command) {
            Some(waiting) => wait(&args, &session, &waiting, out, err),
            // `parse` refuses `--wait` anywhere else, so this cannot be reached.
            None => report(
                &args,
                &Error::refused("--wait does not apply to this command"),
                err,
            ),
        };
    }

    let path = args.command.request_path();
    let body = args.command.body();
    let reply = if args.command.method() == "GET" {
        session.get(&path)
    } else {
        session.post(&path, body.as_ref())
    };
    let reply = match reply {
        Ok(reply) => reply,
        Err(error) => return report(&args, &error, err),
    };

    if !reply.is_success() {
        return report(&args, &Error::from_reply(reply), err);
    }
    if args.json {
        // Pass the control plane's JSON through untouched. A `204` has nothing to
        // pass, so JSON mode prints nothing at all.
        if !reply.is_empty() {
            let _ = writeln!(out, "{}", reply.body);
        }
    } else {
        let _ = writeln!(out, "{}", render::human(&args.command, &reply));
    }
    0
}

/// What a `--wait` invocation is waiting for.
struct Waiting {
    /// The event family to print as it arrives.
    event: &'static str,
    /// What the human mode calls the work in its closing line.
    label: &'static str,
    /// The terminal test: `Some(ok)` ends the wait.
    terminal: fn(&serde_json::Value) -> Option<bool>,
}

/// The wait a `--wait` command is waiting for; `None` when the flag does not
/// apply (`parse` refuses that case, so this is belt and braces).
fn waiting_for(command: &Command) -> Option<Waiting> {
    match command {
        Command::ToolchainDownload => Some(Waiting {
            event: "toolchain:download",
            label: "download",
            terminal: download_terminal,
        }),
        Command::QemuDownload => Some(Waiting {
            event: "qemu:download",
            label: "qemu download",
            terminal: download_terminal,
        }),
        Command::PreflightRun => Some(Waiting {
            event: "preflight:progress",
            label: "preflight",
            terminal: preflight_terminal,
        }),
        _ => None,
    }
}

/// A download is over when it says so.
///
/// One predicate for both assemblies: the toolchain and QEMU families carry the same
/// `state` vocabulary, which is the point of the shared shape (v0.9 sandbox F1).
///
/// `cancelled` is in the terminal set because the event exists: this CLI does not
/// cancel, so seeing it means something else did, and the download did not finish.
fn download_terminal(payload: &serde_json::Value) -> Option<bool> {
    match payload.get("state").and_then(serde_json::Value::as_str) {
        Some("done") => Some(true),
        Some("failed") | Some("cancelled") => Some(false),
        _ => None,
    }
}

/// `preflight:progress` has no end-of-run event: the steps are fail-fast, so the
/// run is over at the first `failed`, or at the last step's `ok`.
///
/// The step list comes from the host's own constant rather than a literal, so a
/// fifth check added later moves the end of the wait with it.
fn preflight_terminal(payload: &serde_json::Value) -> Option<bool> {
    let state = payload.get("state").and_then(serde_json::Value::as_str)?;
    if state == "failed" {
        return Some(false);
    }
    let step = payload.get("step").and_then(serde_json::Value::as_str)?;
    let last = host_core::preflight::STEPS.last()?;
    if step == *last && state == "ok" {
        Some(true)
    } else {
        None
    }
}

/// `--wait`: subscribe, start the work, print the family's frames, stop at the
/// end.
///
/// Same shape as `--follow` and for the same reason — the run request blocks, so
/// the stream needs a thread of its own — with one difference: the exit code is
/// the *work's* verdict, so `--wait` exits `3` when the download failed or the
/// preflight found a broken environment.
fn wait(
    args: &Args,
    session: &Session,
    waiting: &Waiting,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let mut stream = match subscribe(session) {
        Ok(stream) => stream,
        Err(error) => return report(args, &error, err),
    };

    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    // `EXIT_LOCAL` is "the stream ended before the work did": the reader sets it
    // only when it runs out of frames.
    let outcome = Arc::new(AtomicU8::new(client::EXIT_LOCAL));
    {
        let (stop, done, outcome) = (Arc::clone(&stop), Arc::clone(&done), Arc::clone(&outcome));
        let event = waiting.event;
        let terminal = waiting.terminal;
        let json = args.json;
        std::thread::spawn(move || {
            // The reader prints from its own thread, so it writes to stdout
            // directly rather than to the caller's stream.
            let stdout = std::io::stdout();
            while !stop.load(Ordering::Relaxed) {
                match stream.next_frame() {
                    Ok(Some(frame)) => {
                        if frame.event().as_deref() != Some(event) {
                            continue;
                        }
                        let payload = frame.json().and_then(|value| value.get("payload").cloned());
                        let line = if json {
                            frame.data.clone()
                        } else {
                            render::frame_line(&frame)
                        };
                        let mut handle = stdout.lock();
                        let _ = writeln!(handle, "{line}");
                        let _ = handle.flush();
                        if let Some(ok) = payload.as_ref().and_then(terminal) {
                            outcome.store(
                                if ok {
                                    client::EXIT_OK
                                } else {
                                    client::EXIT_REMOTE
                                },
                                Ordering::Relaxed,
                            );
                            done.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                    Ok(None) => {
                        done.store(true, Ordering::Relaxed);
                        break;
                    }
                    Err(_) => {
                        done.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
            done.store(true, Ordering::Relaxed);
        });
    }

    // The request that starts the work. An answer that is not a success is the
    // command's answer: there will be no events to wait for.
    let control = session.control_client();
    let path = args.command.request_path();
    let body = args.command.body();
    let reply = match control.post(&path, body.as_ref()) {
        Ok(reply) => reply,
        Err(error) => {
            stop.store(true, Ordering::Relaxed);
            return report(args, &error, err);
        }
    };
    if !reply.is_success() {
        stop.store(true, Ordering::Relaxed);
        return report(args, &Error::from_reply(reply), err);
    }

    // The request was accepted; the stream says when the work is over.
    while !done.load(Ordering::Relaxed) {
        std::thread::sleep(WAIT_POLL);
    }
    stop.store(true, Ordering::Relaxed);

    let code = outcome.load(Ordering::Relaxed);
    if code == client::EXIT_LOCAL {
        return report(
            args,
            &Error::local("the event stream ended before the work finished"),
            err,
        );
    }
    // The frames came straight from the reader thread; the closing line is what
    // says, in one word, what the exit code means.
    if !args.json {
        let _ = writeln!(
            out,
            "{} {}",
            waiting.label,
            if code == client::EXIT_OK {
                "ok"
            } else {
                "failed"
            }
        );
    }
    code
}

/// Open the event stream and eat the `hello` frame.
///
/// Reading the greeting is what proves the subscription is live *before* the
/// request that starts the work goes out, so nothing the work produces can be
/// missed.
fn subscribe(session: &Session) -> Result<SseStream, Error> {
    let mut stream = session.open_stream("/v0/events")?;
    match stream.next_frame()? {
        Some(frame) if frame.kind().as_deref() == Some("hello") => Ok(stream),
        _ => Err(Error::local("the event stream did not greet us")),
    }
}

/// `run --follow`: subscribe first, then run, printing the stream as it arrives.
///
/// The stream has to be **read** while the run is going, or the server's bounded
/// channel would drop what it missed (a lagging subscriber is told it lagged and
/// loses those frames). One thread cannot do both — the run request blocks until
/// the run ends — so the reader gets a thread of its own and the request stays in
/// this one.
fn follow(args: &Args, session: &Session, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut stream = match subscribe(session) {
        Ok(stream) => stream,
        Err(error) => return report(args, &error, err),
    };

    let stop = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let (stop, done) = (Arc::clone(&stop), Arc::clone(&done));
        let json = args.json;
        std::thread::spawn(move || {
            // The reader prints from its own thread, so it writes to stdout
            // directly rather than to the caller's stream.
            let stdout = std::io::stdout();
            while !stop.load(Ordering::Relaxed) {
                match stream.next_frame() {
                    Ok(Some(frame)) => {
                        let mut handle = stdout.lock();
                        let line = if json {
                            frame.data.clone()
                        } else {
                            render::frame_line(&frame)
                        };
                        let _ = writeln!(handle, "{line}");
                        let _ = handle.flush();
                        if frame.event().as_deref() == Some("agent:final") {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
            done.store(true, Ordering::Relaxed);
        })
    };

    // The request that starts the run. Its answer *is* the outcome, so it also
    // says when the stream has nothing left to say.
    let control = session.control_client();
    let body = args.command.body();
    let reply = control.post("/v0/agent/run", body.as_ref());

    // Give the reader the tail, but never wait forever for it.
    let deadline = Instant::now() + FOLLOW_GRACE;
    while !done.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    stop.store(true, Ordering::Relaxed);
    drop(reader); // detached: the process is about to end either way

    let reply = match reply {
        Ok(reply) => reply,
        Err(error) => return report(args, &error, err),
    };
    if !reply.is_success() {
        return report(args, &Error::from_reply(reply), err);
    }
    if args.json {
        if !reply.is_empty() {
            let _ = writeln!(out, "{}", reply.body);
        }
    } else {
        let _ = writeln!(out, "{}", render::human(&args.command, &reply));
    }
    0
}

/// Print a failure the way the mode asks for, and return its exit code.
fn report(args: &Args, error: &Error, err: &mut dyn Write) -> u8 {
    if args.json {
        // A server error body is already the documented shape; a local failure is
        // wrapped in it so a JSON consumer never has to parse prose.
        let _ = writeln!(err, "{}", error.json());
    } else {
        let _ = writeln!(err, "{}", error.human());
    }
    error.code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_download_is_over_at_done_or_failed() {
        for not_yet in ["started", "progress", "verifying", "extracting"] {
            assert_eq!(
                download_terminal(&serde_json::json!({ "state": not_yet })),
                None,
                "{not_yet}"
            );
        }
        assert_eq!(
            download_terminal(&serde_json::json!({ "state": "done", "install_path": "p" })),
            Some(true)
        );
        assert_eq!(
            download_terminal(&serde_json::json!({ "state": "failed", "reason": "no network" })),
            Some(false)
        );
        assert_eq!(
            download_terminal(&serde_json::json!({ "state": "cancelled" })),
            Some(false)
        );
        // A payload without a state says nothing about being over.
        assert_eq!(download_terminal(&serde_json::json!({})), None);
    }

    #[test]
    fn the_preflight_is_over_at_the_last_step_or_the_first_failure() {
        let last = host_core::preflight::STEPS.last().copied().expect("steps");
        assert_eq!(
            preflight_terminal(
                &serde_json::json!({ "step": "gcc_runs", "state": "running", "detail": null })
            ),
            None
        );
        assert_eq!(
            preflight_terminal(
                &serde_json::json!({ "step": "gcc_runs", "state": "ok", "detail": "gcc 13" })
            ),
            None,
            "an early step passing is not the end"
        );
        // Fail-fast: a failure ends the run wherever it happens.
        assert_eq!(
            preflight_terminal(
                &serde_json::json!({ "step": "gcc_compiles", "state": "failed", "detail": "x" })
            ),
            Some(false)
        );
        // The last step is the end, either way.
        assert_eq!(
            preflight_terminal(
                &serde_json::json!({ "step": last, "state": "ok", "detail": "banner" })
            ),
            Some(true)
        );
        assert_eq!(
            preflight_terminal(
                &serde_json::json!({ "step": last, "state": "failed", "detail": "no banner" })
            ),
            Some(false)
        );
    }

    #[test]
    fn a_wait_is_named_for_the_event_it_follows() {
        assert_eq!(
            waiting_for(&Command::ToolchainDownload)
                .expect("download")
                .event,
            "toolchain:download"
        );
        assert_eq!(
            waiting_for(&Command::QemuDownload)
                .expect("qemu download")
                .event,
            "qemu:download"
        );
        assert_eq!(
            waiting_for(&Command::PreflightRun)
                .expect("preflight")
                .event,
            "preflight:progress"
        );
        assert!(waiting_for(&Command::Health).is_none());
        assert!(waiting_for(&Command::PreflightAck).is_none());
        assert!(waiting_for(&Command::QemuCancel).is_none());
    }
}
