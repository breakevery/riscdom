//! The authentication hook, shaped as settled in `docs/control-plane-api.md` §3.
//!
//! This batch ships the trait and one implementation, [`NoAuth`]. The mechanism
//! (tokens, scopes, the permission intermediary) is settled later; the shape is
//! what an integrator codes against today.

use std::fmt;
use subtle::ConstantTimeEq;

/// What the control plane knows about a request before it acts.
///
/// The bearer token travels here and nowhere else: [`ReqMeta`]'s `Debug` redacts
/// it, so a stray `{:?}` can never write it to a log.
pub struct ReqMeta {
    /// The HTTP method (`GET`, `POST`, ...).
    pub method: String,
    /// The request path, without the query string.
    pub path: String,
    /// The bearer token from `Authorization`, when one was sent.
    pub token: Option<String>,
    /// The capability this endpoint requires, read from the route table.
    ///
    /// The hook sees it so a hook *may* reason about it, but the check itself is
    /// the server's: after `authorise` returns, the request path asks the [`Actor`]
    /// whether it [`allows`](Actor::allows) this capability and answers `403` if
    /// it does not. A route cannot express "no capability": the route table's
    /// column is a [`Capability`], not an `Option`.
    pub capability: Option<Capability>,
}

impl fmt::Debug for ReqMeta {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReqMeta")
            .field("method", &self.method)
            .field("path", &self.path)
            .field(
                "token",
                &self.token.as_ref().map(|_| "<redacted>").unwrap_or("none"),
            )
            .field("capability", &self.capability)
            .finish()
    }
}

/// What a request is allowed to act as.
///
/// `agent_id` maps 1:1 onto the audit chain's `agent_id`, which is how the chain
/// tells "a human did it" apart from "a supervisor AI did it".
///
/// `capabilities` is what the actor may actually do: the server checks the
/// capability each route declares against this set before the handler runs, so
/// an actor with an empty set can reach nothing. v0.9 has exactly two shapes —
/// [`Actor::owner`] (the token holder: everything) and an empty set — and the set
/// is here so finer-grained actors need no new plumbing later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The identity every audit row this request writes carries.
    pub agent_id: String,
    /// The narrative kind, for the audit reader.
    pub kind: ActorKind,
    /// What this actor may do.
    pub capabilities: std::collections::BTreeSet<Capability>,
}

impl Actor {
    /// No identity was proven and nothing is allowed.
    pub fn anonymous() -> Self {
        Self {
            agent_id: "anonymous".to_string(),
            kind: ActorKind::Anonymous,
            capabilities: std::collections::BTreeSet::new(),
        }
    }

    /// The v0.9 holder: every capability.
    pub fn owner() -> Self {
        Self::named_owner("owner")
    }

    /// [`Actor::owner`] with an identity of its own.
    pub fn named_owner(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            kind: ActorKind::Human,
            capabilities: Capability::ALL.iter().copied().collect(),
        }
    }

    /// May this actor do `capability`?
    pub fn allows(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// One thing an actor may do.
///
/// The vocabulary of the API document's §5 tables. Every route declares one, and
/// because the declaration is a typed column of the route table an undeclared
/// route is not something that can be written down — the server cannot start
/// serving a path whose capability nobody named.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Capability {
    AgentRun,
    AuditExport,
    AuditRead,
    EventsSubscribe,
    HealthRead,
    LlmConfigure,
    LlmRead,
    PreflightRead,
    PreflightRun,
    QemuConfigure,
    QemuRead,
    RunsControl,
    RunsRead,
    SandboxAssemble,
    SandboxRead,
    SandboxSwitch,
    SerialExport,
    SerialRead,
    SessionRead,
    SessionWrite,
    SettingsRead,
    SettingsWrite,
    SnapshotRead,
    SnapshotWrite,
    StatusRead,
    ToolchainConfigure,
    ToolchainInstall,
    ToolchainRead,
    VmControl,
    VmRead,
    WorkspaceRead,
    WorkspaceWrite,
}

impl Capability {
    /// Every capability: what [`Actor::owner`] holds. A new variant must be added
    /// here too, which a test enforces.
    pub const ALL: &'static [Capability] = &[
        Capability::AgentRun,
        Capability::AuditExport,
        Capability::AuditRead,
        Capability::EventsSubscribe,
        Capability::HealthRead,
        Capability::LlmConfigure,
        Capability::LlmRead,
        Capability::PreflightRead,
        Capability::PreflightRun,
        Capability::QemuConfigure,
        Capability::QemuRead,
        Capability::RunsControl,
        Capability::RunsRead,
        Capability::SandboxAssemble,
        Capability::SandboxRead,
        Capability::SandboxSwitch,
        Capability::SerialExport,
        Capability::SerialRead,
        Capability::SessionRead,
        Capability::SessionWrite,
        Capability::SettingsRead,
        Capability::SettingsWrite,
        Capability::SnapshotRead,
        Capability::SnapshotWrite,
        Capability::StatusRead,
        Capability::ToolchainConfigure,
        Capability::ToolchainInstall,
        Capability::ToolchainRead,
        Capability::VmControl,
        Capability::VmRead,
        Capability::WorkspaceRead,
        Capability::WorkspaceWrite,
    ];

    /// The name the route table, the API document and the error body use.
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::AgentRun => "agent.run",
            Capability::AuditExport => "audit.export",
            Capability::AuditRead => "audit.read",
            Capability::EventsSubscribe => "events.subscribe",
            Capability::HealthRead => "health.read",
            Capability::LlmConfigure => "llm.configure",
            Capability::LlmRead => "llm.read",
            Capability::PreflightRead => "preflight.read",
            Capability::PreflightRun => "preflight.run",
            Capability::QemuConfigure => "qemu.configure",
            Capability::QemuRead => "qemu.read",
            Capability::RunsControl => "runs.control",
            Capability::RunsRead => "runs.read",
            Capability::SandboxAssemble => "sandbox.assemble",
            Capability::SandboxRead => "sandbox.read",
            Capability::SandboxSwitch => "sandbox.switch",
            Capability::SerialExport => "serial.export",
            Capability::SerialRead => "serial.read",
            Capability::SessionRead => "session.read",
            Capability::SessionWrite => "session.write",
            Capability::SettingsRead => "settings.read",
            Capability::SettingsWrite => "settings.write",
            Capability::SnapshotRead => "snapshot.read",
            Capability::SnapshotWrite => "snapshot.write",
            Capability::StatusRead => "status.read",
            Capability::ToolchainConfigure => "toolchain.configure",
            Capability::ToolchainInstall => "toolchain.install",
            Capability::ToolchainRead => "toolchain.read",
            Capability::VmControl => "vm.control",
            Capability::VmRead => "vm.read",
            Capability::WorkspaceRead => "workspace.read",
            Capability::WorkspaceWrite => "workspace.write",
        }
    }
}

impl fmt::Debug for Capability {
    /// The canonical name, so a log line reads `capability: Some(audit.read)`
    /// rather than `Some(AuditRead)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The narrative kind of an actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Human,
    Supervisor,
    Executor,
    /// No identity was proven (the [`NoAuth`] default).
    Anonymous,
}

/// Why a request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    /// No token, or the token was refused.
    Unauthorized,
    /// Authenticated, but not allowed to do this.
    Forbidden,
}

impl AuthError {
    /// The `code` field of the error model (`docs/control-plane-api.md` §4).
    pub fn code(&self) -> &'static str {
        match self {
            AuthError::Unauthorized => "unauthorized",
            AuthError::Forbidden => "forbidden",
        }
    }

    /// The HTTP status the code maps to.
    pub fn status(&self) -> u16 {
        match self {
            AuthError::Unauthorized => 401,
            AuthError::Forbidden => 403,
        }
    }

    /// The human-readable `message` (never contains a secret).
    pub fn message(&self) -> &'static str {
        match self {
            AuthError::Unauthorized => "missing or invalid bearer token",
            AuthError::Forbidden => "the actor is not allowed to do this",
        }
    }
}

/// Resolve a request to the actor that made it, or refuse.
///
/// Fail closed: a control plane with no [`Authn`] installed refuses every
/// request. [`NoAuth`] is an *installed* hook that authorises everyone as
/// anonymous — the v0.9 default, not an absence of auth.
pub trait Authn: Send + Sync {
    fn authorise(&self, meta: &ReqMeta) -> Result<Actor, AuthError>;
}

/// The opt-out: every request is authorised as [`Actor::anonymous`].
///
/// **Not the default any more** (v0.9). The control endpoints include destructive
/// ones, so the server installs [`TokenAuth`] unless it is started with
/// `--no-auth` — which prints a warning, because it means anyone who can reach
/// the socket can delete a session or stop the VM.
///
/// The `Authorization` header is still read into [`ReqMeta`] (so a real hook sees
/// it) and the route's capability is still named there; this implementation
/// simply ignores both. Capability enforcement is the permission intermediary's
/// job, a later batch.
pub struct NoAuth;

impl Authn for NoAuth {
    fn authorise(&self, _meta: &ReqMeta) -> Result<Actor, AuthError> {
        // Explicitly the holder of every capability: `--no-auth` means "run
        // everything", not "run the anonymous actor", so the endpoints behave
        // exactly as they do with a token.
        Ok(Actor::owner())
    }
}

/// The default: the request must present the token from `<data-dir>/token`.
///
/// The comparison is constant time (`subtle`), so a wrong token cannot be
/// recovered byte by byte from the time it takes to be refused. The only thing
/// that leaks is the token's length, which is fixed for a generated one.
pub struct TokenAuth {
    token: String,
}

impl TokenAuth {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl Authn for TokenAuth {
    fn authorise(&self, meta: &ReqMeta) -> Result<Actor, AuthError> {
        let presented = meta.token.as_deref().unwrap_or_default();
        if presented.as_bytes().ct_eq(self.token.as_bytes()).into() {
            // One token, full power (v0.9). The capability check itself lives in
            // the request path, so a finer-grained hook needs no new plumbing.
            Ok(Actor::named_owner("operator"))
        } else {
            Err(AuthError::Unauthorized)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(token: Option<&str>) -> ReqMeta {
        ReqMeta {
            method: "GET".into(),
            path: "/v0/health".into(),
            token: token.map(str::to_string),
            capability: Some(Capability::HealthRead),
        }
    }

    #[test]
    fn no_auth_hands_out_the_owner() {
        let actor = NoAuth.authorise(&meta(Some("anything"))).expect("allowed");
        assert_eq!(actor.agent_id, "owner");
        assert_eq!(actor.kind, ActorKind::Human);
        for capability in Capability::ALL {
            assert!(actor.allows(*capability), "{capability}");
        }
        assert!(NoAuth.authorise(&meta(None)).is_ok());
    }

    #[test]
    fn an_anonymous_actor_may_do_nothing() {
        let actor = Actor::anonymous();
        assert_eq!(actor.capabilities.len(), 0);
        assert!(!actor.allows(Capability::AuditRead));
        assert!(!actor.allows(Capability::HealthRead));
    }

    #[test]
    fn the_capability_vocabulary_is_well_formed() {
        assert_eq!(
            Capability::ALL.len(),
            32,
            "the vocabulary the document lists"
        );
        let mut names: Vec<&str> = Capability::ALL.iter().map(Capability::as_str).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "every name is used once: {names:?}");
        assert!(names.iter().all(|name| name.contains('.')), "{names:?}");
        // The owner holds the whole vocabulary, by construction and by check.
        let owner = Actor::owner();
        assert_eq!(owner.capabilities.len(), Capability::ALL.len());
    }

    #[test]
    fn req_meta_debug_redacts_the_token() {
        let shown = format!("{:?}", meta(Some("super-secret")));
        assert!(!shown.contains("super-secret"), "token leaked: {shown}");
        assert!(shown.contains("redacted"));
        assert!(format!("{:?}", meta(None)).contains("none"));
        // The capability is not a secret: it is fine to log.
        assert!(shown.contains("health.read"));
    }

    #[test]
    fn auth_errors_map_to_the_documented_code_and_status() {
        assert_eq!(AuthError::Unauthorized.code(), "unauthorized");
        assert_eq!(AuthError::Unauthorized.status(), 401);
        assert_eq!(AuthError::Forbidden.code(), "forbidden");
        assert_eq!(AuthError::Forbidden.status(), 403);
    }

    #[test]
    fn token_auth_accepts_only_the_token_it_holds() {
        let auth = TokenAuth::new("a-64-char-token");
        let actor = auth
            .authorise(&meta(Some("a-64-char-token")))
            .expect("allowed");
        assert_eq!(actor.agent_id, "operator");
        assert_eq!(actor.kind, ActorKind::Human);
        assert_eq!(actor.capabilities.len(), Capability::ALL.len());

        for wrong in ["", "a-64-char-toke", "a-64-char-tokenn", "A-64-CHAR-TOKEN"] {
            let refused = auth.authorise(&meta(Some(wrong)));
            assert_eq!(refused, Err(AuthError::Unauthorized), "accepted {wrong:?}");
        }
        assert_eq!(
            auth.authorise(&meta(None)),
            Err(AuthError::Unauthorized),
            "a missing header is refused too"
        );
    }
}
