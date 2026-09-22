//! `worker` — the executor binary **and** the supervisor half that drives a fleet
//! of them (v0.8 main deliverable 2/2).
//!
//! - the **binary** target (`src/main.rs`) is the executor: one `Task` in on
//!   stdin, one `TaskOutcome` out on stdout;
//! - this **library** target is the supervisor side: [`supervisor`] turns a list of
//!   tasks into dispatches over several executor processes and reports what came
//!   back.
//!
//! It is a library rather than a second binary so the runnable demo
//! (`examples/dispatch.rs`) and the integration tests share one implementation
//! instead of two copies of the same loop.

pub mod supervisor;
