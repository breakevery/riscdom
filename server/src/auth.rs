//! The authentication hook, shaped as settled in `docs/control-plane-api.md` §3.
//!
//! This batch ships the trait and one implementation, [`NoAuth`]. The mechanism
//! (tokens, scopes, the permission intermediary) is settled later; the shape is
//! what an integrator codes against today.

use std::fmt;

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

/// The v0.9 default: every request is authorised as [`Actor::anonymous`].
///
/// The `Authorization` header is still read into [`ReqMeta`] (so a real hook sees
/// it); this implementation simply ignores it.
pub struct NoAuth;

impl Authn for NoAuth {
    fn authorise(&self, _meta: &ReqMeta) -> Result<Actor, AuthError> {
        Ok(Actor::anonymous())
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
    }

    #[test]
    fn auth_errors_map_to_the_documented_code_and_status() {
        assert_eq!(AuthError::Unauthorized.code(), "unauthorized");
        assert_eq!(AuthError::Unauthorized.status(), 401);
        assert_eq!(AuthError::Forbidden.code(), "forbidden");
        assert_eq!(AuthError::Forbidden.status(), 403);
    }
}
