//! Talking to the control plane: the embedded server, the HTTP client, the
//! token, the confirmation prompt, and the exit-code map.
//!
//! Both modes are one path. Without `--remote` the CLI starts the control plane
//! **inside this process** on a loopback port the OS picks (`127.0.0.1:0`), then
//! speaks HTTP to it exactly as it would to a remote one — the CLI never calls
//! `AppState` directly.

use crate::args::Args;
use crate::args::Command;
use crate::sse::SseStream;
use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Success (a 2xx answer).
pub const EXIT_OK: u8 = 0;
/// A local failure: no connection, no token, no workspace, no runtime.
pub const EXIT_LOCAL: u8 = 1;
/// A usage error, a refused confirmation, or the control plane rejecting the
/// request (`400`).
pub const EXIT_USAGE: u8 = 2;
/// The control plane refused or failed (`404` / `405` / `409` / `5xx`).
pub const EXIT_REMOTE: u8 = 3;
/// Authentication failed (`401` / `403`).
pub const EXIT_AUTH: u8 = 4;

/// How long one read-only request may take. Those answer immediately.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a control request may take. `run` drives a whole agent turn — model
/// calls, tools, a guest — so this is a budget, not a hint.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// How long a connection may take to establish. The streams have no total
/// timeout, because an event stream is open by design.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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

    /// A refusal that never reached the control plane (a declined confirmation,
    /// a missing `--yes`): a usage error, because the command as written was not
    /// one the operator stood behind.
    pub fn refused(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_USAGE,
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

    /// `true` when the endpoint answered with no body at all (`204`).
    pub fn is_empty(&self) -> bool {
        self.body.trim().is_empty()
    }
}

/// The HTTP side: one base URL, one token, one blocking client.
///
/// Cloning is cheap and shares the connection pool, which is how `--follow` puts
/// the request on another thread while this one reads the stream.
#[derive(Clone)]
pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::blocking::Client,
}

impl Client {
    pub fn new(
        base: String,
        token: Option<String>,
        timeout: Option<Duration>,
    ) -> Result<Self, Error> {
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(timeout)
            .build()
            .map_err(|e| Error::local(format!("cannot build the HTTP client: {e}")))?;
        Ok(Self { base, token, http })
    }

    /// `GET <path>`, returning the body whether or not the status is a success.
    pub fn get(&self, path: &str) -> Result<Reply, Error> {
        let url = self.url(path);
        self.send(url.clone(), self.http.get(&url))
    }

    /// `POST <path>` with a JSON body (or none).
    pub fn post(&self, path: &str, body: Option<&serde_json::Value>) -> Result<Reply, Error> {
        let url = self.url(path);
        let mut request = self.http.post(&url);
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }
        self.send(url, request)
    }

    /// `POST <path>` with bytes, answering **bytes** (v0.9 project in/out).
    ///
    /// The one request on this surface whose body is not JSON and whose answer is
    /// not JSON: an archive goes out and an archive comes back. A failure still
    /// answers the error model, so a non-2xx body is parsed as JSON and reported the
    /// way every other refusal is.
    pub fn post_bytes(
        &self,
        path: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<Vec<u8>, Error> {
        let url = self.url(path);
        let mut request = self.http.post(&url).header("Content-Type", content_type);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .body(body)
            .send()
            .map_err(|e| Error::local(format!("cannot reach {url}: {e}")))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .map_err(|e| Error::local(format!("cannot read the answer from {url}: {e}")))?;
        if !(200..=299).contains(&status) {
            let body = String::from_utf8_lossy(&bytes).to_string();
            let json = serde_json::from_str(&body).ok();
            return Err(Error::from_reply(Reply { status, body, json }));
        }
        Ok(bytes.to_vec())
    }

    /// Open an event stream (`GET /v0/events`).
    ///
    /// The response is handed back before its body ends: the caller reads frames.
    pub fn open_stream(&self, path: &str) -> Result<SseStream, Error> {
        let url = self.url(path);
        let mut request = self.http.get(&url).header("Accept", "text/event-stream");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|e| Error::local(format!("cannot open {url}: {e}")))?;
        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = response.text().unwrap_or_default();
            let json = serde_json::from_str(&body).ok();
            return Err(Error::from_reply(Reply { status, body, json }));
        }
        Ok(SseStream::new(response))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn send(
        &self,
        url: String,
        request: reqwest::blocking::RequestBuilder,
    ) -> Result<Reply, Error> {
        let mut request = request.header("Accept", "application/json");
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

/// A session: how to reach the control plane, and the token to use.
///
/// Three clients, because the timeout is not one number: a read answers now, a
/// control may take half an hour, and a stream must never time out at all.
pub struct Session {
    read: Client,
    control: Client,
    stream: Client,
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
                    read: Client::new(base.clone(), token.clone(), Some(READ_TIMEOUT))?,
                    control: Client::new(base.clone(), token.clone(), Some(CONTROL_TIMEOUT))?,
                    stream: Client::new(base, token, None)?,
                    embedded: None,
                })
            }
            None => {
                let embedded = start_embedded(&args.workspace, args.data_dir.as_deref())?;
                let base = embedded.base_url();
                let token = Some(embedded.token().to_string());
                Ok(Self {
                    read: Client::new(base.clone(), token.clone(), Some(READ_TIMEOUT))?,
                    control: Client::new(base.clone(), token.clone(), Some(CONTROL_TIMEOUT))?,
                    stream: Client::new(base, token, None)?,
                    embedded: Some(embedded),
                })
            }
        }
    }

    pub fn get(&self, path: &str) -> Result<Reply, Error> {
        self.read.get(path)
    }

    /// A control request. Cloned into a thread by `--follow`.
    pub fn post(&self, path: &str, body: Option<&serde_json::Value>) -> Result<Reply, Error> {
        self.control.post(path, body)
    }

    /// A control request whose body and answer are bytes (project in/out).
    pub fn post_bytes(
        &self,
        path: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<Vec<u8>, Error> {
        self.control.post_bytes(path, body, content_type)
    }

    pub fn open_stream(&self, path: &str) -> Result<SseStream, Error> {
        self.stream.open_stream(path)
    }

    /// A control client whose requests can be moved to another thread.
    pub fn control_client(&self) -> Client {
        self.control.clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Some(embedded) = &self.embedded {
            embedded.abort();
        }
    }
}

/// Ask before doing something that destroys state.
///
/// `--yes` answers up front. Otherwise: on a terminal, ask and read the answer;
/// **not** on a terminal (a script, an AI, a pipe) there is nobody to ask, so the
/// command is refused — silence is not consent.
pub fn confirm(prompt: &str, yes: bool) -> Result<(), Error> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() {
        return Err(Error::refused(format!(
            "refusing without confirmation: {prompt} (stdin is not a terminal; pass --yes)"
        )));
    }
    eprint!("{prompt} [y/N] ");
    let mut answer = String::new();
    let read = std::io::stdin().read_line(&mut answer);
    let answer = answer.trim();
    match read {
        Ok(_) if answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes") => Ok(()),
        _ => Err(Error::refused(format!("not confirmed: {prompt}"))),
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

/// The token for a remote control plane.
///
/// `--token-file` first (only a path lands on the command line), then
/// `RISCDOM_TOKEN` (not on the command line either), then `--token` — which does
/// land in the shell history, so the CLI says so.
fn remote_token(args: &Args) -> Result<Option<String>, Error> {
    if let Some(path) = &args.token_file {
        return read_token_file(path).map(Some);
    }
    if let Ok(value) = std::env::var("RISCDOM_TOKEN") {
        let value = value.trim().to_string();
        if value.is_empty() {
            return Err(Error::auth("RISCDOM_TOKEN is set but empty"));
        }
        return Ok(Some(value));
    }
    if let Some(value) = &args.token {
        eprintln!(
            "riscdom: warning: --token puts the token in the shell history and in `ps`; \
             prefer --token-file or RISCDOM_TOKEN"
        );
        return Ok(Some(value.clone()));
    }
    Ok(None)
}

/// Read a token file: one value, surrounding whitespace ignored.
///
/// The CLI keeps its own reader rather than reusing `server::token::load_or_create`,
/// which generates a token when the file is missing — a client must not.
fn read_token_file(path: &Path) -> Result<String, Error> {
    read_one_value(path).map_err(Error::auth)
}

/// The one value a small file holds, trimmed. `Err` is the message to show.
fn read_one_value(path: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value = text.trim().to_string();
    if value.is_empty() {
        return Err(format!("{} is empty", path.display()));
    }
    Ok(value)
}

/// Resolve `llm set`'s key: read `--api-key-file`, or warn about `--api-key`.
///
/// A key on the command line lands in the shell history and in `ps`, exactly like
/// `--token`, so the file is the shape to prefer; the flag still works, with the
/// same warning `--token` prints.
///
/// The read lives here rather than in `args` so that `Command::body` stays a pure
/// function of the command line.
pub fn resolve_key_file(command: &mut Command) -> Result<(), Error> {
    let Command::LlmSet {
        api_key,
        api_key_file,
        ..
    } = command
    else {
        return Ok(());
    };
    match (api_key.is_some(), api_key_file.take()) {
        // Already inline: the command line is where the key came from.
        (true, _) => {
            eprintln!(
                "riscdom: warning: --api-key puts the key in the shell history and in `ps`; \
                 prefer --api-key-file"
            );
            Ok(())
        }
        (false, Some(path)) => {
            let key = read_one_value(&path)
                .map_err(|e| Error::local(format!("cannot read the API key: {e}")))?;
            *api_key = Some(key);
            Ok(())
        }
        // `parse` refuses this before it can be reached.
        (false, None) => Err(Error::refused(
            "llm set needs --api-key <key> or --api-key-file <path>".to_string(),
        )),
    }
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

    let file = server::token::load_or_create(state.data_dir()).map_err(|e| {
        Error::local(format!(
            "cannot use the token in {}: {e}",
            state.data_dir().display()
        ))
    })?;
    let token = file.token().to_string();

    let authn: Arc<dyn server::Authn> = Arc::new(server::TokenAuth::new(token.clone()));
    let bind: SocketAddr = "127.0.0.1:0"
        .parse()
        .expect("a loopback literal is an address");
    // No heartbeat: the CLI subscribes only for `--follow`, and a frame nobody
    // needs is not worth a thread in every short-lived invocation.
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
    fn a_501_from_a_reserved_endpoint_is_a_remote_failure_with_its_message() {
        let reply = Reply {
            status: 501,
            body: r#"{"code":"not_implemented","message":"starting a VM is reserved","retryable":false,"cause":null}"#
                .to_string(),
            json: Some(serde_json::json!({
                "code": "not_implemented",
                "message": "starting a VM is reserved",
                "retryable": false,
                "cause": null,
            })),
        };
        let error = Error::from_reply(reply);
        assert_eq!(error.code, EXIT_REMOTE);
        assert!(
            error.human().contains("not_implemented"),
            "{}",
            error.human()
        );
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
    fn a_refused_confirmation_is_a_usage_error() {
        let error = Error::refused("not confirmed: Delete snapshot \"a\"?");
        assert_eq!(error.code, EXIT_USAGE);
        assert!(error.human().contains("not confirmed"));
    }

    #[test]
    fn a_token_file_is_read_and_trimmed() {
        let dir = std::env::temp_dir().join(format!("riscdom-cli-cred-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("token");
        let mut file = std::fs::File::create(&path).expect("token file");
        writeln!(file, "  a-value  ").expect("write");
        assert_eq!(read_token_file(&path).expect("reads"), "a-value");

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
            Parsed::Command(args) => *args,
            other => panic!("not a command: {other:?}"),
        };
        // `--token` is the last resort, and it still works.
        assert_eq!(
            remote_token(&args).expect("a token").as_deref(),
            Some("from-flag")
        );

        // Without any of the three, there is simply nothing to present.
        let parsed = parse(vec!["health".to_string()]).expect("parses");
        let args = match parsed {
            Parsed::Command(args) => *args,
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

    #[test]
    fn yes_answers_the_prompt_without_reading_stdin() {
        // The unit tests run with a stdin that may or may not be a terminal;
        // `--yes` must not depend on which.
        assert!(confirm("Delete everything?", true).is_ok());
    }

    #[test]
    fn the_api_key_file_is_read_into_the_command() {
        let dir = std::env::temp_dir().join(format!(
            "riscdom-cli-key-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("key");
        std::fs::write(&path, "  sk-from-file\n").expect("write");

        let mut command = Command::LlmSet {
            api_key: None,
            api_key_file: Some(path),
            base_url: "u".into(),
            model: "m".into(),
            provider_id: None,
            remember: false,
        };
        resolve_key_file(&mut command).expect("reads the file");
        match command {
            Command::LlmSet {
                api_key,
                api_key_file,
                ..
            } => {
                assert_eq!(api_key.as_deref(), Some("sk-from-file"));
                assert!(api_key_file.is_none(), "the file is consumed");
            }
            other => panic!("not an llm set: {other:?}"),
        }

        // A file that cannot be read is a local failure, in the CLI's own words.
        let mut missing = Command::LlmSet {
            api_key: None,
            api_key_file: Some(dir.join("nope")),
            base_url: "u".into(),
            model: "m".into(),
            provider_id: None,
            remember: false,
        };
        let error = resolve_key_file(&mut missing).expect_err("missing file");
        assert_eq!(error.code, EXIT_LOCAL);
        assert!(error.human().contains("API key"), "{}", error.human());

        // A key already on the command line is left where it is (and warned
        // about by the caller's stderr).
        let mut inline = Command::LlmSet {
            api_key: Some("sk-inline".into()),
            api_key_file: None,
            base_url: "u".into(),
            model: "m".into(),
            provider_id: None,
            remember: false,
        };
        resolve_key_file(&mut inline).expect("nothing to read");
        assert!(matches!(
            inline,
            Command::LlmSet {
                api_key: Some(_),
                ..
            }
        ));

        // Any other command is left alone.
        let mut health = Command::Health;
        resolve_key_file(&mut health).expect("untouched");
        assert_eq!(health, Command::Health);
    }
}
