//! The control plane the CLI's end-to-end tests talk to (v1.0 batch DT / M8-4a).
//!
//! The CLI is a **client** and starts nothing, so these tests run a real
//! `riscdom-server` and point the CLI at it with `--remote`. The binary is taken
//! from `RISCDOM_SERVER_BIN`, or found beside the test binaries
//! (`target/<profile>/`). **When it is not there the tests skip**, printing why: a
//! client's suite must not go red because a program that lives in another
//! repository is not built here (M8-4d is where the cross-repository integration
//! returns).

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The server binary, or `None` (the caller then skips).
pub fn server_bin() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RISCDOM_SERVER_BIN") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent()?;
    for profile in ["debug", "release"] {
        let candidate = root
            .join("target")
            .join(profile)
            .join(exe("riscdom-server"));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `true` when there is no server to drive — the test prints why and returns.
pub fn skip_if_no_server() -> bool {
    if server_bin().is_none() {
        eprintln!(
            "skipping: no `riscdom-server` binary (set RISCDOM_SERVER_BIN, or build it in this \
             workspace) — the CLI is a client and starts none of its own (v1.0 batch DT / M8-4a)"
        );
        return true;
    }
    false
}

/// A `riscdom-server` on a loopback port the OS picks, over `workspace` / `data_dir`.
pub struct Server {
    child: Child,
    /// `host:port`, as the CLI's `--remote` wants it.
    pub addr: String,
    /// `<data-dir>/token`, which the server writes before it binds.
    pub token_file: PathBuf,
}

impl Server {
    /// Start one, or `None` when the binary is missing.
    pub fn start(workspace: &Path, data_dir: &Path) -> Option<Server> {
        let bin = server_bin()?;
        let mut child = Command::new(bin)
            .arg("--bind")
            .arg("127.0.0.1:0")
            .arg("--workspace")
            .arg(workspace)
            .arg("--data-dir")
            .arg(data_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        // Drain the rest of the server's output on a thread of its own: dropping
        // the pipe would give it a broken pipe on the next line it prints.
        let mut reader = std::io::BufReader::new(stdout);
        let mut banner = String::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        while banner.is_empty() && Instant::now() < deadline {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if let Some((_, rest)) = line.split_once("listening on http://") {
                        banner = rest.trim().to_string();
                    }
                }
            }
        }
        let addr = if banner.is_empty() {
            let _ = child.kill();
            return None;
        } else {
            banner
        };
        std::thread::spawn(move || {
            let mut rest = String::new();
            while matches!(reader.read_line(&mut rest), Ok(n) if n > 0) {
                rest.clear();
            }
        });
        Some(Server {
            child,
            addr,
            token_file: data_dir.join("token"),
        })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
