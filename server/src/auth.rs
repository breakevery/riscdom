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
    /// The identity the caller claims for itself, from `X-RiscDom-Agent` (v1.0 gap 2/N).
    ///
    /// Optional, and absent means "the token's own identity" — which is what every
    /// release before this one assumed. An **AI supervisor** sends it so the rows its
    /// acts leave name it rather than the node: without it M's work is attributed to
    /// `operator` at best, and to `host` at worst.
    pub caller: Option<String>,
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
            .field("caller", &self.caller)
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

    /// The token holder, acting **as the caller that named itself**, when one did.
    ///
    /// `X-RiscDom-Agent` (v1.0 gap 2/N) turns the same credential into a named actor: the
    /// identity travels into every row the request writes, and the kind becomes
    /// [`ActorKind::Supervisor`] so the audit narrative says *an AI dispatcher did this*
    /// rather than leaving it to be guessed from an id. The **capabilities are unchanged**
    /// — a caller names itself, it does not widen what the token may do.
    pub fn named_caller(caller: Option<&str>, default_id: impl Into<String>) -> Self {
        match caller {
            Some(id) => Self {
                agent_id: id.to_string(),
                kind: ActorKind::Supervisor,
                capabilities: Capability::ALL.iter().copied().collect(),
            },
            None => Self::named_owner(default_id),
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
    /// Reading the chain **on another node** (v1.0 M2a-1): the remote half of
    /// [`Capability::AuditRead`], kept a separate name because "I may read this
    /// node's history" and "I may read the network's" are different powers.
    AuditReadRemote,
    EventsSubscribe,
    HealthRead,
    LlmConfigure,
    LlmRead,
    PreflightRead,
    PreflightRun,
    QemuConfigure,
    QemuRead,
    /// Deciding a pending sandbox request (v1.0 M2a-1): the act that was
    /// `sandbox.read` plus the request's own implied capability (decisions §36),
    /// now named.
    RequestApprove,
    RunsControl,
    RunsRead,
    SandboxAssemble,
    /// Deriving an instance from a definition (v1.0 M2a-1, roadmap §3).
    SandboxInstantiate,
    /// Deriving an instance **on another node** (v1.0 M2a-1).
    SandboxInstantiateRemote,
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
    /// Dispatching a task (v1.0 M2a-1). It sits beside [`Capability::AgentRun`]
    /// rather than replacing it: running on *this* node and handing work to a
    /// dispatcher are different acts (decisions §1 — mechanism, not policy).
    TaskDispatch,
    /// Dispatching a task **to another node** (v1.0 M2a-1).
    TaskDispatchRemote,
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
        Capability::AuditReadRemote,
        Capability::EventsSubscribe,
        Capability::HealthRead,
        Capability::LlmConfigure,
        Capability::LlmRead,
        Capability::PreflightRead,
        Capability::PreflightRun,
        Capability::QemuConfigure,
        Capability::QemuRead,
        Capability::RequestApprove,
        Capability::RunsControl,
        Capability::RunsRead,
        Capability::SandboxAssemble,
        Capability::SandboxInstantiate,
        Capability::SandboxInstantiateRemote,
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
        Capability::TaskDispatch,
        Capability::TaskDispatchRemote,
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
            Capability::AuditReadRemote => "audit.read.remote",
            Capability::EventsSubscribe => "events.subscribe",
            Capability::HealthRead => "health.read",
            Capability::LlmConfigure => "llm.configure",
            Capability::LlmRead => "llm.read",
            Capability::PreflightRead => "preflight.read",
            Capability::PreflightRun => "preflight.run",
            Capability::QemuConfigure => "qemu.configure",
            Capability::QemuRead => "qemu.read",
            Capability::RequestApprove => "request.approve",
            Capability::RunsControl => "runs.control",
            Capability::RunsRead => "runs.read",
            Capability::SandboxAssemble => "sandbox.assemble",
            Capability::SandboxInstantiate => "sandbox.instantiate",
            Capability::SandboxInstantiateRemote => "sandbox.instantiate.remote",
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
            Capability::TaskDispatch => "task.dispatch",
            Capability::TaskDispatchRemote => "task.dispatch.remote",
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
    fn authorise(&self, meta: &ReqMeta) -> Result<Actor, AuthError> {
        // Explicitly the holder of every capability: `--no-auth` means "run
        // everything", not "run the anonymous actor", so the endpoints behave
        // exactly as they do with a token. A caller that named itself is still named
        // (v1.0 gap 2/N): `--no-auth` decides what is *allowed*, not who is asking.
        Ok(Actor::named_caller(meta.caller.as_deref(), "owner"))
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
            // `X-RiscDom-Agent` names the caller without changing that power
            // (v1.0 gap 2/N).
            Ok(Actor::named_caller(meta.caller.as_deref(), "operator"))
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
            caller: None,
            capability: Some(Capability::HealthRead),
        }
    }

    fn meta_as(token: Option<&str>, caller: &str) -> ReqMeta {
        ReqMeta {
            caller: Some(caller.to_string()),
            ..meta(token)
        }
    }

    #[test]
    fn a_caller_that_names_itself_becomes_a_supervisor() {
        // v1.0 gap 2/N: `X-RiscDom-Agent` names the actor without widening its power.
        let token = TokenAuth::new("the-token");
        let named = token
            .authorise(&meta_as(Some("the-token"), "m-1"))
            .expect("allowed");
        assert_eq!(named.agent_id, "m-1");
        assert_eq!(named.kind, ActorKind::Supervisor);
        assert_eq!(named.capabilities.len(), Capability::ALL.len());

        // …and without it the identity is the token's own, exactly as before.
        let anonymous_caller = token.authorise(&meta(Some("the-token"))).expect("allowed");
        assert_eq!(anonymous_caller.agent_id, "operator");
        assert_eq!(anonymous_caller.kind, ActorKind::Human);

        // `--no-auth` decides what is allowed, not who is asking.
        let named = NoAuth
            .authorise(&meta_as(Some("anything"), "m-2"))
            .expect("allowed");
        assert_eq!(named.agent_id, "m-2");
        assert_eq!(named.kind, ActorKind::Supervisor);
        assert_eq!(named.capabilities.len(), Capability::ALL.len());
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
        // The vocabulary grows; the *contract* is which names exist, never how many
        // (v1.0 M2a-1: the count used to be pinned here, and adding a capability is
        // an expected act — a guard that has to be edited for the expected case is a
        // guard that hides the unexpected one).
        const MUST_HAVE: &[&str] = &[
            // The v0.9 vocabulary's load-bearing names: if one of these disappears,
            // `ALL` was rewritten rather than extended.
            "agent.run",
            "audit.read",
            "events.subscribe",
            "sandbox.read",
            "sandbox.switch",
            "settings.write",
            "vm.control",
            "workspace.write",
            // The v1.0 M2a-1 additions (instance model): three local, three remote.
            "sandbox.instantiate",
            "task.dispatch",
            "request.approve",
            "sandbox.instantiate.remote",
            "task.dispatch.remote",
            "audit.read.remote",
        ];
        assert!(
            Capability::ALL.len() >= 38,
            "the vocabulary the document lists (32) plus M2a-1's six: {}",
            Capability::ALL.len()
        );
        for name in MUST_HAVE {
            assert!(
                Capability::ALL.iter().any(|c| c.as_str() == *name),
                "{name} is missing from ALL"
            );
        }
        let mut names: Vec<&str> = Capability::ALL.iter().map(Capability::as_str).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "every name is used once: {names:?}");
        assert!(names.iter().all(|name| name.contains('.')), "{names:?}");
        // A `.remote` name is always the local name plus the suffix, so the two
        // halves of one power cannot drift apart.
        for name in names.iter().filter(|name| name.ends_with(".remote")) {
            let local = name.trim_end_matches(".remote");
            assert!(
                Capability::ALL.iter().any(|c| c.as_str() == local),
                "{name} has no local half ({local})"
            );
        }
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
