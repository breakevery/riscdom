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
    /// *Declared, not enforced* in v0.9: the permission intermediary that decides
    /// whether an actor holds a capability is a later batch, so [`NoAuth`] ignores
    /// this. A hook that wants to enforce it already has what it needs.
    pub capability: Option<String>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The identity every audit row this request writes carries.
    pub agent_id: String,
    /// The narrative kind, for the audit reader.
    pub kind: ActorKind,
}

impl Actor {
    /// The actor [`NoAuth`] hands out: no identity was proven.
    pub fn anonymous() -> Self {
        Self {
            agent_id: "anonymous".to_string(),
            kind: ActorKind::Anonymous,
        }
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
        Ok(Actor::anonymous())
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
            Ok(Actor {
                // The identity an audit reader sees for a token holder. The
                // chain's own `agent_id` still names the host that wrote the row.
                agent_id: "operator".to_string(),
                kind: ActorKind::Human,
            })
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
            capability: Some("health.read".into()),
        }
    }

    #[test]
    fn no_auth_authorises_every_request_as_anonymous() {
        let actor = NoAuth.authorise(&meta(Some("anything"))).expect("allowed");
        assert_eq!(actor, Actor::anonymous());
        assert_eq!(actor.agent_id, "anonymous");
        assert_eq!(actor.kind, ActorKind::Anonymous);
        assert!(NoAuth.authorise(&meta(None)).is_ok());
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
        let ok = auth
            .authorise(&meta(Some("a-64-char-token")))
            .expect("allowed");
        assert_eq!(ok.agent_id, "operator");
        assert_eq!(ok.kind, ActorKind::Human);

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
