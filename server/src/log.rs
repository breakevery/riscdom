//! The runtime log lines this library emits, and the switch that turns them on.
//!
//! **Off by default.** The control plane also runs *inside* another program — the
//! CLI's local mode embeds it — where stderr belongs to the caller: a
//! per-connection line there lands in the middle of the caller's own error output
//! (and in JSON mode, in the middle of its error *object*). The switch is
//! [`crate::ServerConfig::with_log_level`]; `riscdom-server` spells it
//! `--log-level <off|error|info>`.
//!
//! What is *not* behind this switch: `main.rs`'s start-up banner, its usage text
//! and its fatal errors. Those are the `riscdom-server` **binary's** own console
//! interface — what the operator asked to see — and the embedded CLI never runs
//! `main.rs` at all.

/// How much this library logs.
///
/// Ordered: [`LogLevel::Error`] covers failures, [`LogLevel::Info`] adds the
/// per-connection chatter, and [`LogLevel::Off`] — the default — writes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LogLevel {
    /// The default: no runtime lines at all.
    #[default]
    Off,
    /// Failures: a failed accept, a download or a preflight that ended in an error.
    Error,
    /// The above, plus one line per connection that ended badly.
    Info,
}

impl LogLevel {
    /// `off` / `error` / `info`, case-insensitively.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "error" | "errors" => Ok(Self::Error),
            "info" => Ok(Self::Info),
            other => Err(format!(
                "unknown log level {other:?}; use off, error or info"
            )),
        }
    }

    /// The name to show for this level.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Error => "error",
            Self::Info => "info",
        }
    }

    /// Is a line at `level` worth writing, given this setting?
    pub fn allows(self, level: Self) -> bool {
        self >= level
    }
}

/// Write one runtime line, when `enabled` allows a line of `level`.
///
/// The only place this library writes to stderr, so the prefix and the switch are
/// decided once.
pub fn line(enabled: LogLevel, level: LogLevel, message: impl std::fmt::Display) {
    if enabled.allows(level) {
        eprintln!("riscdom-server: {message}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_parse_from_their_names() {
        assert_eq!(LogLevel::parse("off"), Ok(LogLevel::Off));
        assert_eq!(LogLevel::parse("  ERROR "), Ok(LogLevel::Error));
        assert_eq!(LogLevel::parse("info"), Ok(LogLevel::Info));
        for junk in ["", "verbose", "debug", "3"] {
            assert!(LogLevel::parse(junk).is_err(), "{junk:?}");
        }
    }

    #[test]
    fn the_names_round_trip() {
        for level in [LogLevel::Off, LogLevel::Error, LogLevel::Info] {
            assert_eq!(LogLevel::parse(level.as_str()), Ok(level));
        }
    }

    #[test]
    fn off_writes_nothing_and_info_writes_everything() {
        assert!(!LogLevel::Off.allows(LogLevel::Error));
        assert!(!LogLevel::Off.allows(LogLevel::Info));
        assert!(LogLevel::Error.allows(LogLevel::Error));
        assert!(!LogLevel::Error.allows(LogLevel::Info));
        assert!(LogLevel::Info.allows(LogLevel::Error));
        assert!(LogLevel::Info.allows(LogLevel::Info));
        // The default is the quiet one.
        assert_eq!(LogLevel::default(), LogLevel::Off);
    }
}
