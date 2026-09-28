//! The error model's categories, in one place (v1.0 M4a).
//!
//! [error-model.md](../../docs/error-model.md) §3 names five categories, and every layer
//! that refuses something maps its own refusal onto one of them: the verifier
//! ([`crate::sign::VerifyError`]) and the transport ([`crate::transport::TransportError`])
//! both answer with a [`Category`], so a caller branches on the same word whichever layer
//! said no.
//!
//! All five are defined here so the mapping is total and stable — but **no layer of this
//! crate can produce all five**. Verification reaches `Refused`, `Invalid` and `Network`;
//! the transport reaches `Network` and `Invalid`. `Crashed` is a dead process and
//! `Partial` is a batch that half finished, and neither is something one frame can be.

/// The error model's categories ([error-model.md](../../docs/error-model.md) §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// The transport failed: nothing was reached, or a read or write ran out of time.
    Network,
    /// Something was reached and it said no — an unknown peer, a replay, a policy.
    Refused,
    /// A process died.
    Crashed,
    /// A batch completed only partly.
    Partial,
    /// The input is wrong: a broken signature, a version we do not speak, a frame that
    /// does not parse.
    Invalid,
}

impl Category {
    /// The word the error model and the wire use.
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Network => "network",
            Category::Refused => "refused",
            Category::Crashed => "crashed",
            Category::Partial => "partial",
            Category::Invalid => "invalid",
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
