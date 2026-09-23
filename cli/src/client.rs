//! Talking to the control plane: the embedded server, the HTTP client, the
//! token, and the exit-code map.
//!
//! Both modes are one path. Without `--remote` the CLI starts the control plane
//! **inside this process** on a loopback port the OS picks (`127.0.0.1:0`), then
//! speaks HTTP to it exactly as it would to a remote one — the CLI never calls
//! `AppState` directly.

use crate::args::Args;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Success (a 2xx answer).
pub const EXIT_OK: u8 = 0;
/// A local failure: no connection, no token, no workspace, no runtime.
pub const EXIT_LOCAL: u8 = 1;
/// A usage error, or the control plane rejected the request (`400`).
pub const EXIT_USAGE: u8 = 2;
/// The control plane refused or failed (`404` / `405` / `409` / `5xx`).
pub const EXIT_REMOTE: u8 = 3;
/// Authentication failed (`401` / `403`).
pub const EXIT_AUTH: u8 = 4;

/// How long one request may take. The read-only commands answer immediately; a
/// long-running control (`run`) will need its own budget.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Map an HTTP status onto the CLI's exit code.
pub fn exit_code_for(status: u16) -> u8 {
    match status {
        200..=299 => EXIT_OK,
        400 => EXIT_USAGE,
        401 | 403 => EXIT_AUTH,
        _ => EXIT_REMOTE,
    }
}

/// One failure on the way to an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The exit code this failure maps to.
    pub code: u8,
    /// The human-readable message.
    pub message: String,
    /// The control plane's error object, when the failure came from it.
    pub body: Option<String>,
}

impl Error {
    pub fn local(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_LOCAL,
            message: message.into(),
            body: None,
        }
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_AUTH,
            message: message.into(),
            body: None,
        }
    }

    /// A failure that came back as a response: keep its status-derived code and
    /// its body, which is already the documented error object.
    pub fn from_reply(reply: Reply) -> Self {
        Self {
            code: exit_code_for(reply.status),
            message: error_message(&reply),
            body: if reply.body.trim().is_empty() {
                None
            } else {
                Some(reply.body)
            },
        }
    }

    /// The single line a human reads.
    pub fn human(&self) -> String {
        self.message.clone()
    }

    /// The object a script reads: the control plane's own, or a `cli_error`
    /// wrapper when the failure never reached it.
    pub fn json(&self) -> String {
        match &self.body {
            Some(body) => body.trim().to_string(),
            None => serde_json::json!({
                "code": "cli_error",
                "message": self.message,
                "retryable": false,
                "cause": null,
            })
            .to_string(),
        }
    }
}

/// Pull `code: message` out of an error body, falling back to the whole body.
fn error_message(reply: &Reply) -> String {
    match &reply.json {
        Some(value) => {
            let code = value.get("code").and_then(|v| v.as_str());
            let message = value.get("message").and_then(|v| v.as_str());
            let cause = value.get("cause").and_then(|v| v.as_str());
            match (code, message) {
                (Some(code), Some(message)) => match cause {
                    Some(cause) => format!("{code}: {message} (cause: {cause})"),
                    None => format!("{code}: {message}"),
                },
                _ => reply.body.trim().to_string(),
            }
        }
        None => format!("HTTP {}: {}", reply.status, reply.body.trim()),
    }
}

/// One answer from the control plane.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    /// The parsed body, when it was JSON.
    pub json: Option<serde_json::Value>,
}

impl Reply {
    pub fn is_success(&self) -> bool {
        (200..=299).contains(&self.status)
    }
}

/// The HTTP side: one base URL, one credential, one blocking client.
pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(base: String, token: Option<String>) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| Error::local(format!("cannot build the HTTP client: {e}")))?;
        Ok(Self { base, token, http })
    }

    /// `GET <path>`, returning the body whether or not the status is a success.
    pub fn get(&self, path: &str) -> Result<Reply, Error> {
        let url = format!("{}{path}", self.base);
        let mut request = self.http.get(&url).header("Accept", "application/json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|e| Error::local(format!("cannot reach {url}: {e}")))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|e| Error::local(format!("cannot read the answer from {url}: {e}")))?;
        let json = serde_json::from_str(&body).ok();
        Ok(Reply { status, body, json })
    }
}

/// The control plane running inside this process, on its own loopback port.
pub struct Embedded {
    addr: SocketAddr,
    token: String,
    running: server::Running,
}

impl Embedded {
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// The token the embedded server requires, so the client can present it.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Stop accepting connections. The process is about to exit anyway; this is
    /// what makes the intent explicit.
    pub fn abort(&self) {
        self.running.abort();
    }
}

/// A session: how to reach the control plane, and the credential to use.
pub struct Session {
    client: Client,
    /// `Some` in local mode: the embedded server must outlive the request.
    embedded: Option<Embedded>,
}

impl Session {
    /// Open a session: remote when `--remote` is given, embedded otherwise.
    pub fn open(args: &Args) -> Result<Self, Error> {
        match &args.remote {
            Some(remote) => {
                let base = remote_base_url(remote);
                let token = remote_token(args)?;
                Ok(Self {
                    client: Client::new(base, token)?,
                    embedded: None,
                })
            }
            None => {
                let embedded = start_embedded(&args.workspace, args.data_dir.as_deref())?;
                let token = embedded.token().to_string();
                let client = Client::new(embedded.base_url(), Some(token))?;
                Ok(Self {
                    client,
                    embedded: Some(embedded),
                })
            }
        }
    }

    pub fn get(&self, path: &str) -> Result<Reply, Error> {
        self.client.get(path)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(embedded) = &self.embedded {
            embedded.abort();
        }
    }
}

/// `--remote host:port` → `http://host:port`. A pasted scheme is tolerated (and
/// replaced with plain HTTP, which is what this build speaks).
fn remote_base_url(remote: &str) -> String {
    match remote.split_once("://") {
        Some((_, authority)) => format!("http://{authority}"),
        None => format!("http://{remote}"),
    }
}

/// The credential for a remote control plane.
///
/// `--token-file` first (only a path lands on the command line), then
/// `RISCDOM_TOKEN` (not on the command line either), then `--token` — which does
/// land in the shell history, so the CLI says so.
fn remote_token(args: &Args) -> Result<Option<String>, Error> {
    if let Some(path) = &args.token_file {
        return read_token_file(path).map(Some);
    }
    if let Ok(token) = std::env::var("RISCDOM_TOKEN") {
        let token = token.trim().to_string();
        if token.is_empty() {
            return Err(Error::auth("RISCDOM_TOKEN is set but empty"));
        }
        return Ok(Some(token));
    }
    if let Some(token) = &args.token {
        eprintln!(
            "riscdom: warning: --token puts the bearer token in the shell history and in `ps`; \
             prefer --token-file or RISCDOM_TOKEN"
        );
        return Ok(Some(token.clone()));
    }
    Ok(None)
}

/// Read a token file: one value, surrounding whitespace ignored.
///
/// The CLI keeps its own reader rather than reusing `server::token::load_or_create`,
/// which generates a token when the file is missing — a client must not.
fn read_token_file(path: &Path) -> Result<String, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        Error::auth(format!(
            "cannot read the token file {}: {e}",
            path.display()
        ))
    })?;
    let token = text.trim().to_string();
    if token.is_empty() {
        return Err(Error::auth(format!(
            "the token file {} is empty",
            path.display()
        )));
    }
    Ok(token)
}

/// Start the control plane inside this process, on a loopback port the OS picks.
///
/// Public because it is the whole of the local mode, and because the integration
/// tests need a real control plane to talk to. The token is the host's own
/// (`<data-dir>/token`, generated on first use by the same code the standalone
/// server runs), installed as `TokenAuth`; the client then presents it, so the
/// local path exercises the authenticated one.
pub fn start_embedded(workspace: &Path, data_dir: Option<&Path>) -> Result<Embedded, Error> {
    let state = match data_dir {
        Some(dir) => host_core::AppState::with_data_dir(workspace.to_path_buf(), dir.to_path_buf()),
        None => host_core::AppState::new(workspace.to_path_buf()),
    }
    .map_err(|e| Error::local(format!("cannot open the workspace state: {e}")))?;
    let state = Arc::new(state);

    let token_file = server::token::load_or_create(state.data_dir()).map_err(|e| {
        Error::local(format!(
            "cannot use the token in {}: {e}",
            state.data_dir().display()
        ))
    })?;
    let token = token_file.token().to_string();

    let authn: Arc<dyn server::Authn> = Arc::new(server::TokenAuth::new(token.clone()));
    let bind: SocketAddr = "127.0.0.1:0"
        .parse()
        .expect("a loopback literal is an address");
    // No heartbeat: the CLI does not subscribe to the stream.
    let config = server::ServerConfig::new(bind)
        .with_heartbeat(None)
        .with_authn(authn);
    let http = server::Server::new(Arc::clone(&state), config);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .map_err(|e| Error::local(format!("cannot start the async runtime: {e}")))?;
    let running = runtime
        .block_on(http.start())
        .map_err(|e| Error::local(format!("cannot bind a loopback port: {e}")))?;
    let addr = running.local_addr();
    // The accept loop is a task in that runtime, so the runtime has to keep being
    // polled: park it on a thread of its own (the process exit ends it).
    std::thread::spawn(move || runtime.block_on(std::future::pending::<()>()));

    Ok(Embedded {
        addr,
        token,
        running,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{parse, Parsed};
    use std::io::Write;

    #[test]
    fn the_exit_code_map_follows_the_documented_table() {
        for status in [200, 201, 204, 299] {
            assert_eq!(exit_code_for(status), EXIT_OK, "{status}");
        }
        assert_eq!(exit_code_for(400), EXIT_USAGE);
        assert_eq!(exit_code_for(401), EXIT_AUTH);
        assert_eq!(exit_code_for(403), EXIT_AUTH);
        for status in [404, 405, 409, 500, 501, 503] {
            assert_eq!(exit_code_for(status), EXIT_REMOTE, "{status}");
        }
    }

    #[test]
    fn a_server_error_keeps_its_body_and_its_code() {
        let reply = Reply {
            status: 403,
            body: r#"{"code":"forbidden","message":"the actor may not audit.read","retryable":false,"cause":"capability"}"#
                .to_string(),
            json: Some(serde_json::json!({
                "code": "forbidden",
                "message": "the actor may not audit.read",
                "retryable": false,
                "cause": "capability",
            })),
        };
        let error = Error::from_reply(reply);
        assert_eq!(error.code, EXIT_AUTH);
        assert_eq!(
            error.human(),
            "forbidden: the actor may not audit.read (cause: capability)"
        );
        // The JSON mode hands the control plane's object back unchanged.
        assert!(error.json().contains(r#""code":"forbidden""#));
    }

    #[test]
    fn a_local_failure_is_wrapped_in_the_error_shape() {
        let error = Error::local("cannot reach http://127.0.0.1:1/v0/health");
        assert_eq!(error.code, EXIT_LOCAL);
        let json = error.json();
        assert!(json.contains(r#""code":"cli_error""#), "{json}");
        assert!(json.contains(r#""retryable":false"#), "{json}");
    }

    #[test]
    fn a_token_file_is_read_and_trimmed() {
        let dir = std::env::temp_dir().join(format!("riscdom-cli-token-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("token");
        let mut file = std::fs::File::create(&path).expect("token file");
        writeln!(file, "  a-token-value  ").expect("write");
        assert_eq!(read_token_file(&path).expect("reads"), "a-token-value");

        std::fs::write(&path, "   \n").expect("write");
        let error = read_token_file(&path).expect_err("empty");
        assert_eq!(error.code, EXIT_AUTH);
        assert!(error.human().contains("is empty"), "{}", error.human());

        let missing = dir.join("nope");
        assert_eq!(
            read_token_file(&missing).expect_err("missing").code,
            EXIT_AUTH
        );
    }

    #[test]
    fn the_token_sources_are_tried_in_order() {
        let parsed = parse(
            [
                "--remote",
                "127.0.0.1:7821",
                "--token",
                "from-flag",
                "health",
            ]
            .iter()
            .map(|a| a.to_string())
            .collect(),
        )
        .expect("parses");
        let args = match parsed {
            Parsed::Command(args) => args,
            other => panic!("not a command: {other:?}"),
        };
        // `--token` is the last resort, and it still works.
        assert_eq!(
            remote_token(&args).expect("a token").as_deref(),
            Some("from-flag")
        );

        // Without any of the three, there is simply no credential to present.
        let parsed = parse(vec!["health".to_string()]).expect("parses");
        let args = match parsed {
            Parsed::Command(args) => args,
            other => panic!("not a command: {other:?}"),
        };
        assert_eq!(remote_token(&args).expect("no token"), None);
    }

    #[test]
    fn the_remote_url_is_built_from_host_and_port() {
        assert_eq!(remote_base_url("127.0.0.1:7821"), "http://127.0.0.1:7821");
        assert_eq!(
            remote_base_url("box.example:9000"),
            "http://box.example:9000"
        );
    }
}
