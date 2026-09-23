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
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Run one parsed command line, writing to `out` on success and `err` on
/// failure. Returns the process exit code, because that is the CLI's contract.
///
/// Taking the streams as arguments is what lets the unit tests drive every
/// branch without spawning a process; `main` passes stdout and stderr. The one
/// exception is `--follow`, whose reader prints from a thread of its own and
/// therefore writes to stdout directly.
pub fn run(parsed: Parsed, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match parsed {
        Parsed::Help => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        Parsed::Version => {
            let _ = writeln!(out, "riscdom {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Parsed::Command(args) => args,
    };

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

/// `run --follow`: subscribe first, then run, printing the stream as it arrives.
///
/// The stream has to be **read** while the run is going, or the server's bounded
/// channel would drop what it missed (a lagging subscriber is told it lagged and
/// loses those frames). One thread cannot do both — the run request blocks until
/// the run ends — so the reader gets a thread of its own and the request stays in
/// this one.
fn follow(args: &Args, session: &Session, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut stream = match session.open_stream("/v0/events") {
        Ok(stream) => stream,
        Err(error) => return report(args, &error, err),
    };
    // The first frame is `hello`: reading it proves the stream is live *before*
    // the run starts, so nothing after this point can be missed.
    match stream.next_frame() {
        Ok(Some(frame)) if frame.kind().as_deref() == Some("hello") => {}
        Ok(_) => {
            return report(
                args,
                &Error::local("the event stream did not greet us".to_string()),
                err,
            )
        }
        Err(error) => return report(args, &error, err),
    }

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
