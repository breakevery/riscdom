//! Minimal QMP (QEMU Machine Protocol) client.
//!
//! Speaks the tiny subset of QMP the sandbox needs:
//!
//! 1. connect to the endpoint and consume the `{"QMP": ...}` greeting,
//! 2. negotiate `qmp_capabilities`,
//! 3. send commands (`stop`, `cont`, `quit`) and read the matching reply.
//!
//! Asynchronous QMP *events* are skipped while waiting for a reply.
//!
//! Platform note: only the TCP endpoint is implemented for the MVP. Unix
//! sockets are rejected with [`SandboxError::Unsupported`].

use crate::error::SandboxError;
use crate::platform::QmpEndpoint;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::Duration;

/// How long to wait for a QMP reply.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// A low-level QMP connection.
pub struct QmpClient {
    reader: BufReader<TcpStream>,
}

impl QmpClient {
    /// Connect, read the greeting, and negotiate capabilities.
    pub fn connect(endpoint: &QmpEndpoint, timeout: Duration) -> Result<Self, SandboxError> {
        let stream = match endpoint {
            QmpEndpoint::Tcp { host, port } => {
                crate::platform::connect_with_retry(&format!("{host}:{port}"), timeout)?
            }
            #[cfg(unix)]
            QmpEndpoint::UnixSocket { .. } => {
                return Err(SandboxError::Unsupported(
                    "QMP over Unix socket is not implemented in the MVP; use Tcp".into(),
                ));
            }
        };

        stream
            .set_read_timeout(Some(READ_TIMEOUT))
            .map_err(|e| SandboxError::Qmp(e.to_string()))?;

        let mut reader = BufReader::new(stream);
        let mut greeting = String::new();
        reader
            .read_line(&mut greeting)
            .map_err(|e| SandboxError::Qmp(format!("reading greeting: {e}")))?;
        if !greeting.contains("QMP") {
            return Err(SandboxError::Qmp(format!(
                "unexpected QMP greeting: {}",
                greeting.trim()
            )));
        }

        let mut client = Self { reader };
        client.send_raw(&serde_json::json!({ "execute": "qmp_capabilities" }))?;
        client.read_response()?;
        Ok(client)
    }

    /// Send a command with no arguments.
    pub fn execute(&mut self, command: &str) -> Result<serde_json::Value, SandboxError> {
        self.send_raw(&serde_json::json!({ "execute": command }))?;
        self.read_response()
    }

    /// Send a command with arguments.
    pub fn execute_with_args(
        &mut self,
        command: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, SandboxError> {
        self.send_raw(&serde_json::json!({ "execute": command, "arguments": arguments }))?;
        self.read_response()
    }

    /// Pause the guest (`stop`).
    pub fn stop(&mut self) -> Result<(), SandboxError> {
        self.execute("stop")?;
        Ok(())
    }

    /// Resume the guest (`cont`).
    pub fn cont(&mut self) -> Result<(), SandboxError> {
        self.execute("cont")?;
        Ok(())
    }

    /// Ask QEMU to quit. The connection usually closes; replies are optional.
    pub fn quit(&mut self) -> Result<(), SandboxError> {
        self.send_raw(&serde_json::json!({ "execute": "quit" }))?;
        // Best effort: QEMU may drop the connection without a reply.
        let _ = self.read_response();
        Ok(())
    }

    fn send_raw(&mut self, value: &serde_json::Value) -> Result<(), SandboxError> {
        let mut line =
            serde_json::to_string(value).map_err(|e| SandboxError::Qmp(e.to_string()))?;
        line.push('\n');
        let stream = self.reader.get_mut();
        stream
            .write_all(line.as_bytes())
            .map_err(|e| SandboxError::Qmp(e.to_string()))?;
        stream
            .flush()
            .map_err(|e| SandboxError::Qmp(e.to_string()))?;
        Ok(())
    }

    /// Read lines until a reply (`return` / `error`) arrives, skipping events.
    fn read_response(&mut self) -> Result<serde_json::Value, SandboxError> {
        loop {
            let mut line = String::new();
            let n = self
                .reader
                .read_line(&mut line)
                .map_err(|e| SandboxError::Qmp(e.to_string()))?;
            if n == 0 {
                return Err(SandboxError::Qmp("connection closed".into()));
            }
            let value: serde_json::Value = match serde_json::from_str(line.trim()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if value.get("event").is_some() {
                continue;
            }
            if let Some(err) = value.get("error") {
                return Err(SandboxError::Qmp(err.to_string()));
            }
            return Ok(value);
        }
    }
}
