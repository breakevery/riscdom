//! The `riscdom-server` command line.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

/// What `--help` prints.
pub const USAGE: &str = "\
usage: riscdom-server [--bind <addr>] [--workspace <dir>] [--data-dir <dir>]
                      [--heartbeat-ms <n>] [--auth | --no-auth]

  --bind          address to listen on (default 127.0.0.1:7821, or $RISCDOM_BIND)
  --workspace     the workspace this host owns (default: the current directory);
                  its audit chain lives at <workspace>/.riscdom/audit.db
  --data-dir      where settings, sessions and the token live
                  (default: this platform's host data dir)
  --heartbeat-ms  SSE heartbeat period, 0 disables it (default 15000)
  --auth          require the bearer token in <data-dir>/token (the default)
  --no-auth       do not require a token; prints a warning, for local debugging
";

/// Whether the endpoints ask for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// The default: the request must present the token from `<data-dir>/token`.
    Token,
    /// `--no-auth`: anyone who can reach the socket may use every endpoint.
    None,
}

/// The parsed command line.
#[derive(Debug)]
pub struct Cli {
    pub bind: SocketAddr,
    pub workspace: PathBuf,
    pub data_dir: Option<PathBuf>,
    pub heartbeat: Option<Duration>,
    pub auth: AuthMode,
}

/// The default bind, when neither the flag nor the environment says otherwise.
pub const DEFAULT_BIND: &str = "127.0.0.1:7821";

/// The default heartbeat period.
pub const DEFAULT_HEARTBEAT_MS: u64 = 15_000;

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

impl Cli {
    /// Parse the arguments after the program name.
    ///
    /// `--auth` and `--no-auth` both set the mode; the last one wins.
    pub fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut bind_raw =
            std::env::var("RISCDOM_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
        let mut workspace: Option<PathBuf> = None;
        let mut data_dir: Option<PathBuf> = None;
        let mut heartbeat_ms = DEFAULT_HEARTBEAT_MS;
        let mut auth = AuthMode::Token;

        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--bind" => bind_raw = next_value(&mut args, &flag)?,
                "--workspace" => workspace = Some(PathBuf::from(next_value(&mut args, &flag)?)),
                "--data-dir" => data_dir = Some(PathBuf::from(next_value(&mut args, &flag)?)),
                "--heartbeat-ms" => {
                    let raw = next_value(&mut args, &flag)?;
                    heartbeat_ms = raw
                        .parse()
                        .map_err(|e| format!("--heartbeat-ms {raw:?} is not a number: {e}"))?;
                }
                "--auth" => auth = AuthMode::Token,
                "--no-auth" => auth = AuthMode::None,
                other => return Err(format!("unknown argument {other:?}")),
            }
        }

        let bind = bind_raw
            .parse()
            .map_err(|e| format!("--bind {bind_raw:?} is not an address: {e}"))?;
        let heartbeat = if heartbeat_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(heartbeat_ms))
        };
        Ok(Self {
            bind,
            workspace: workspace.unwrap_or_else(|| PathBuf::from(".")),
            data_dir,
            heartbeat,
            auth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::parse(args.iter().map(|a| a.to_string()).collect()).expect("parses")
    }

    #[test]
    fn auth_is_on_unless_it_is_turned_off() {
        assert_eq!(parse(&[]).auth, AuthMode::Token);
        assert_eq!(parse(&["--auth"]).auth, AuthMode::Token);
        assert_eq!(parse(&["--no-auth"]).auth, AuthMode::None);
        // Last one wins, so a wrapper can add the flag without a fight.
        assert_eq!(parse(&["--no-auth", "--auth"]).auth, AuthMode::Token);
        assert_eq!(parse(&["--auth", "--no-auth"]).auth, AuthMode::None);
    }

    #[test]
    fn the_other_flags_still_work() {
        let cli = parse(&[
            "--bind",
            "127.0.0.1:9999",
            "--workspace",
            "/tmp/ws",
            "--data-dir",
            "/tmp/data",
            "--heartbeat-ms",
            "0",
        ]);
        assert_eq!(cli.bind.port(), 9999);
        assert_eq!(cli.workspace, PathBuf::from("/tmp/ws"));
        assert_eq!(cli.data_dir, Some(PathBuf::from("/tmp/data")));
        assert_eq!(cli.heartbeat, None);
        assert_eq!(parse(&[]).heartbeat, Some(Duration::from_millis(15_000)));
    }

    #[test]
    fn a_usage_error_is_reported_not_guessed() {
        for args in [
            vec!["--bind"],
            vec!["--nope"],
            vec!["--heartbeat-ms", "soon"],
            vec!["--bind", "not-an-address"],
        ] {
            assert!(
                Cli::parse(args.iter().map(|a| a.to_string()).collect()).is_err(),
                "{args:?} should not parse"
            );
        }
    }
}
