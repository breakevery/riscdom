//! Environment capability preflight (v0.4 batch 3).
//!
//! Version-number rules were deliberately **not** implemented: this repository
//! records no QEMU × GCC compatibility matrix, and writing one would be guesswork
//! (see `PROJECT_CONSTITUTION.md` §10, v0.4 item 5). What is implemented instead is
//! the honest alternative — run the real pair, on the real paths, and report what
//! actually happened. That covers the failures users really hit: a toolchain on a
//! long or space-bearing path, a QEMU that cannot be spawned, a combination that
//! compiles but never boots.
//!
//! The four steps are fail-fast (a later step needs the earlier ones), and the
//! result is cached in `settings.json` against the configuration fingerprint, so
//! it is reused until the configuration changes.
//!
//! Nothing here touches the audit chain: this is an environment check, not a run.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// The banner the preflight guest prints. Seeing it means the guest really ran.
pub const BANNER: &str = "PRECHECK OK";

/// How long a booted guest gets to print the banner.
pub const BANNER_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the serial buffer is polled while waiting for the banner.
pub const BANNER_POLL: Duration = Duration::from_millis(50);

/// How long the compile step may take before the guard gives up on it.
///
/// A compiler that has not answered in this long is not going to (a damaged
/// binary, a wrapper waiting on input, a path the OS is choking on), and the
/// preflight must not hang the panel behind it (v0.4 batch 3-followup).
pub const COMPILE_TIMEOUT: Duration = Duration::from_secs(30);

/// Options for one preflight run.
///
/// The compile timeout is a parameter rather than a constant read inside the
/// runner, so the guard's timeout path can be exercised without waiting 30 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreflightOptions {
    pub compile_timeout: Duration,
}

impl Default for PreflightOptions {
    fn default() -> Self {
        Self {
            compile_timeout: COMPILE_TIMEOUT,
        }
    }
}

/// The guest: four lines, one banner, nothing else to go wrong.
pub const GUEST_SRC: &str = r#"/* Preflight guest (v0.4): print a banner and spin. */
#define UART0 0x10000000UL
int main(void) { volatile unsigned char *u = (volatile unsigned char *)UART0;
    const char *s = "PRECHECK OK\n"; while (*s) *u = (unsigned char)(*s++); for (;;) {} }
"#;

/// Step 1: the configured compiler runs and reports a version.
pub const STEP_GCC_RUNS: &str = "gcc_runs";
/// Step 2: it compiles the preflight guest on the real paths.
pub const STEP_GCC_COMPILES: &str = "gcc_compiles";
/// Step 3: the configured QEMU runs and reports a version.
pub const STEP_QEMU_RUNS: &str = "qemu_runs";
/// Step 4: it boots that guest and the banner arrives.
pub const STEP_GUEST_BOOTS: &str = "guest_boots";

/// Every step, in order.
pub const STEPS: [&str; 4] = [
    STEP_GCC_RUNS,
    STEP_GCC_COMPILES,
    STEP_QEMU_RUNS,
    STEP_GUEST_BOOTS,
];

/// What to suggest when the compiler cannot be run at all.
pub const SUGGEST_GCC_RUNS: &str =
    "装一个 RISC-V 裸机 GCC（或在「设置 → 工具链」里重新指定路径），再点「重新预检」。";
/// What to suggest when it runs but the build fails.
pub const SUGGEST_GCC_COMPILES: &str =
    "看上面的编译器输出：路径过长/含特殊字符、缺 -march 支持、或二进制损毁都可能。可一键下载一份工具链，或手动换路径。";
/// What to suggest when QEMU cannot be run.
///
/// Platform-neutral on purpose (v0.7 batch A): the app runs on Windows / macOS / Linux
/// now, so the suggestion names the download page and each platform's route instead of
/// assuming `winget`.
pub const SUGGEST_QEMU_RUNS: &str =
    "装 QEMU（官方下载页 https://www.qemu.org/download/：Windows 可用 winget，macOS 可用 brew，Linux 用发行版包），或在「设置 → 工具链」里指定 qemu-system-riscv64 的完整路径。";
/// What to suggest when the guest never printed its banner.
pub const SUGGEST_GUEST_BOOTS: &str =
    "这对「工具链 × QEMU」能编译但跑不起来。可以换一个已知可用的组合（例如 QEMU 11.1.0 + xPack 15.2.0），或先「仍要继续」自行确认。";

/// One step's outcome, as the UI renders it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreflightRow {
    pub step: String,
    /// `ok` / `failed` / `not_run`.
    pub state: String,
    pub detail: Option<String>,
}

/// The cached result: bound to the configuration fingerprint it was produced for,
/// so a configuration change invalidates it by construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreflightCache {
    pub fingerprint: String,
    pub ok: bool,
    /// Id of the step that failed (absent when `ok`).
    #[serde(default)]
    pub failed_step: Option<String>,
    /// The raw detail of the failure (compiler output, process error), for the
    /// operator to read.
    #[serde(default)]
    pub detail: Option<String>,
    /// What to do about it.
    #[serde(default)]
    pub suggestion: Option<String>,
    pub checked_at_ms: i64,
    /// The user chose "continue anyway" for this configuration (the escape hatch).
    #[serde(default)]
    pub overridden: bool,
}

impl PreflightCache {
    /// Rows for the UI: steps before the failure passed, the failing step failed,
    /// later steps were never reached (the run is fail-fast).
    pub fn rows(&self) -> Vec<PreflightRow> {
        let failed_at = self
            .failed_step
            .as_deref()
            .and_then(|f| STEPS.iter().position(|s| *s == f));
        STEPS
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let state = match (self.ok, failed_at) {
                    (true, _) => "ok",
                    (false, Some(at)) if index < at => "ok",
                    (false, Some(at)) if index == at => "failed",
                    // Either after the failure, or an unknown step id: nothing to
                    // claim, so nothing is claimed.
                    (false, _) => "not_run",
                };
                PreflightRow {
                    step: (*step).to_string(),
                    state: state.to_string(),
                    detail: if failed_at == Some(index) {
                        self.detail.clone()
                    } else {
                        None
                    },
                }
            })
            .collect()
    }
}

/// The preflight result handed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct PreflightView {
    /// Did this call run the checks, or reuse the cache.
    pub ran: bool,
    /// The configuration fingerprint the result belongs to.
    pub fingerprint: String,
    /// Whether a result exists at all for that fingerprint.
    pub checked: bool,
    pub ok: bool,
    pub rows: Vec<PreflightRow>,
    pub failed_step: Option<String>,
    pub detail: Option<String>,
    pub suggestion: Option<String>,
    pub checked_at_ms: Option<i64>,
    /// The user explicitly chose to continue with this configuration.
    pub overridden: bool,
}

impl PreflightView {
    /// Nothing checked yet for this configuration.
    pub fn unchecked(fingerprint: &str) -> Self {
        Self {
            ran: false,
            fingerprint: fingerprint.to_string(),
            checked: false,
            ok: false,
            rows: STEPS
                .iter()
                .map(|step| PreflightRow {
                    step: (*step).to_string(),
                    state: "not_run".to_string(),
                    detail: None,
                })
                .collect(),
            failed_step: None,
            detail: None,
            suggestion: None,
            checked_at_ms: None,
            overridden: false,
        }
    }

    /// A view over an existing cache entry.
    pub fn from_cache(cache: &PreflightCache, ran: bool) -> Self {
        Self {
            ran,
            fingerprint: cache.fingerprint.clone(),
            checked: true,
            ok: cache.ok,
            rows: cache.rows(),
            failed_step: cache.failed_step.clone(),
            detail: cache.detail.clone(),
            suggestion: cache.suggestion.clone(),
            checked_at_ms: Some(cache.checked_at_ms),
            overridden: cache.overridden,
        }
    }

    /// Should the UI offer the "continue anyway" escape hatch: the check failed,
    /// and the user has not already accepted this configuration.
    pub fn needs_override(&self) -> bool {
        self.checked && !self.ok && !self.overridden
    }
}

/// Poll `seen` until it contains [`BANNER`], or give up after `timeout`.
///
/// Split out from the runner so the timeout path can be tested without a guest
/// that refuses to boot.
pub fn wait_for_banner<F>(timeout: Duration, mut seen: F) -> Result<(), String>
where
    F: FnMut() -> Vec<u8>,
{
    let deadline = Instant::now() + timeout;
    loop {
        let bytes = seen();
        if bytes.windows(BANNER.len()).any(|w| w == BANNER.as_bytes()) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let tail = &bytes[bytes.len().saturating_sub(120)..];
            return Err(format!(
                "等待串口出现 `{BANNER}` 超过 {} 秒；最近收到的串口内容：{:?}",
                timeout.as_secs(),
                String::from_utf8_lossy(tail)
            ));
        }
        std::thread::sleep(BANNER_POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_compile_budget_is_generous_but_bounded() {
        let options = PreflightOptions::default();
        assert_eq!(options.compile_timeout, COMPILE_TIMEOUT);
        assert!(
            options.compile_timeout >= Duration::from_secs(10)
                && options.compile_timeout <= Duration::from_secs(120),
            "a real compile needs room, a hang needs a ceiling: {:?}",
            options.compile_timeout
        );
    }

    #[test]
    fn the_guest_is_short_and_prints_the_banner() {
        let lines = GUEST_SRC.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(
            lines <= 4,
            "the preflight guest must stay tiny ({lines} lines)"
        );
        assert!(GUEST_SRC.contains("PRECHECK OK"));
        assert!(GUEST_SRC.contains("0x10000000"), "the UART address");
    }

    #[test]
    fn the_banner_wait_succeeds_as_soon_as_it_arrives() {
        let mut polls = 0;
        let seen = || {
            polls += 1;
            b"noise PRECHECK OK\n".to_vec()
        };
        assert!(wait_for_banner(Duration::from_millis(500), seen).is_ok());
        assert_eq!(polls, 1, "it must not keep polling after the banner");
    }

    #[test]
    fn the_banner_wait_times_out_with_the_last_bytes() {
        let started = Instant::now();
        let err = wait_for_banner(Duration::from_millis(150), || b"stuck".to_vec())
            .expect_err("a silent guest must time out");
        assert!(err.contains("PRECHECK OK"), "{err}");
        assert!(
            err.contains("stuck"),
            "the last bytes must be reported: {err}"
        );
        assert!(
            started.elapsed() >= Duration::from_millis(100),
            "it must actually wait"
        );
    }

    #[test]
    fn rows_mark_the_failure_and_leave_the_rest_unrun() {
        let cache = PreflightCache {
            fingerprint: "fp".into(),
            ok: false,
            failed_step: Some(STEP_GCC_COMPILES.into()),
            detail: Some("gcc: unknown option".into()),
            suggestion: Some(SUGGEST_GCC_COMPILES.into()),
            checked_at_ms: 1,
            overridden: false,
        };
        let rows = cache.rows();
        assert_eq!(rows[0].state, "ok", "step 1 passed");
        assert_eq!(rows[1].state, "failed");
        assert_eq!(rows[1].detail.as_deref(), Some("gcc: unknown option"));
        assert_eq!(rows[2].state, "not_run");
        assert_eq!(rows[3].state, "not_run");
    }

    #[test]
    fn a_passing_result_has_no_override_offer() {
        let cache = PreflightCache {
            fingerprint: "fp".into(),
            ok: true,
            failed_step: None,
            detail: None,
            suggestion: None,
            checked_at_ms: 1,
            overridden: false,
        };
        let view = PreflightView::from_cache(&cache, false);
        assert!(view.rows.iter().all(|r| r.state == "ok"));
        assert!(!view.needs_override());
        assert!(view.checked);
    }

    #[test]
    fn an_unchecked_view_has_four_unrun_rows() {
        let view = PreflightView::unchecked("fp");
        assert!(!view.checked);
        assert_eq!(view.rows.len(), 4);
        assert!(view.rows.iter().all(|r| r.state == "not_run"));
        assert!(!view.needs_override(), "nothing to bypass before a check");
    }
}
