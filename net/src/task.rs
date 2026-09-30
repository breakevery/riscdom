//! The task frames two nodes hand each other (v1.0 M6-1a).
//!
//! [roadmap §5](../../docs/roadmap-v1.0.md) makes "hand a task to a node by name" one of the five things
//! the kernel provides, and §14.6 says how two Ms talk: **the cross-device protocol, with no protocol of
//! their own**. So a task crossing a machine is an ordinary §3 frame addressed to the peer's node id —
//! the same mechanism §6.7's probes, §33's reports and a closed segment (v1.0 M5-3c-2) already travel as.
//!
//! Two bodies live here: the **task** one node hands out, and the **reply** the peer sends back. Both
//! carry scalars and opaque JSON only, because `net` does not depend on `agent`: `Task` and `TaskOutcome`
//! are spelled out in `host-core`, which owns the mapping (and the `target` a node-scoped task carries is
//! that layer's decision too — this module only carries the string).
//!
//! **What is deliberately absent.** No retry, no queue, no scheduling, no dependency graph: §5's red line
//! is that those are policy and belong to the caller. A frame goes out once; the answer, or the absence
//! of one, is the caller's to judge.

use crate::message::PROTOCOL_VERSION;
use serde_json::Value;

/// A task on its way to another node (v1.0 M6-1a).
///
/// The wire's view of `agent::Task`: its id, the executor it names, what to run, and the two optional
/// declarations (`sandbox`, `instance`) that decide where it runs. Every field travels; nothing is
/// inferred on the far side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskFrame {
    /// The task's identity, `task-<device>-<pid>-<seq>` — kept across the crossing so the far node's
    /// records and the near node's name the same task.
    pub task_id: String,
    /// The executor to run it on. What a **node-scoped** target is spelled like is the layer above
    /// (`host-core`'s, and M6-1b's); this module carries the string it is given.
    pub target: String,
    /// What to do.
    pub input: String,
    /// The sandbox definition the task wants, when it declares one.
    pub sandbox: Option<String>,
    /// The instance the task wants, when it declares one.
    pub instance: Option<String>,
}

/// The body a task travels as (v1.0 M6-1a).
///
/// An ordinary §3 frame's body, like §6.6's registration: the identity of the sender is the preamble's
/// `from`, so the body names nobody.
pub fn task_body(task: &TaskFrame) -> Value {
    serde_json::json!({
        "task": PROTOCOL_VERSION,
        "task_id": task.task_id,
        "target": task.target,
        "input": task.input,
        "sandbox": task.sandbox,
        "instance": task.instance,
    })
}

/// Is this body a task (v1.0 M6-1a)?
pub fn is_task(body: &Value) -> bool {
    body.get("task").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// Read a task out of a body, when the body is one.
///
/// The two optional declarations are read leniently (absent or `null` means "none"); everything else is
/// required, because a task that cannot be rebuilt is not a task this node should run.
pub fn task_from_body(body: &Value) -> Option<TaskFrame> {
    if !is_task(body) {
        return None;
    }
    Some(TaskFrame {
        task_id: body.get("task_id")?.as_str()?.to_string(),
        target: body.get("target")?.as_str()?.to_string(),
        input: body.get("input")?.as_str()?.to_string(),
        sandbox: body
            .get("sandbox")
            .and_then(Value::as_str)
            .map(str::to_string),
        instance: body
            .get("instance")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// A peer's answer to a task (v1.0 M6-1a).
///
/// A reply is **either** an outcome **or** an error, never both: the far node either ran the task or it
/// did not, and saying which is the whole point of the frame. `agent_id` names the executor that ran it
/// (the peer's own identity, which the caller could not have known), and the outcome travels as opaque
/// JSON because the type lives a layer above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskReply {
    /// The task this answers.
    pub task_id: String,
    /// The executor that ran it, when one did.
    pub agent_id: Option<String>,
    /// What it produced, when it ran.
    pub outcome: Option<Value>,
    /// Why it did not run, when it did not — an authorisation refusal or the executor's own failure.
    pub error: Option<String>,
}

/// The body a reply travels as (v1.0 M6-1a).
pub fn task_reply_body(reply: &TaskReply) -> Value {
    serde_json::json!({
        "task_reply": PROTOCOL_VERSION,
        "task_id": reply.task_id,
        "agent_id": reply.agent_id,
        "outcome": reply.outcome,
        "error": reply.error,
    })
}

/// Is this body a task reply (v1.0 M6-1a)?
pub fn is_task_reply(body: &Value) -> bool {
    body.get("task_reply").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// Read a reply out of a body, when the body is one.
///
/// `task_id` is required — a reply that cannot be matched to its task is a reply nobody can use.
pub fn task_reply_from_body(body: &Value) -> Option<TaskReply> {
    if !is_task_reply(body) {
        return None;
    }
    Some(TaskReply {
        task_id: body.get("task_id")?.as_str()?.to_string(),
        agent_id: body
            .get("agent_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        outcome: body
            .get("outcome")
            .filter(|value| !value.is_null())
            .cloned(),
        error: body
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_task() -> TaskFrame {
        TaskFrame {
            task_id: "task-dev-a-1-1".to_string(),
            target: "dev-b".to_string(),
            input: "hello".to_string(),
            sandbox: Some("blink".to_string()),
            instance: None,
        }
    }

    #[test]
    fn a_task_body_round_trips() {
        let task = a_task();
        assert_eq!(task_from_body(&task_body(&task)), Some(task.clone()));
        assert!(is_task(&task_body(&task)));
        // The optional halves are genuinely optional.
        let bare = TaskFrame {
            sandbox: None,
            instance: Some("dev-b-1-2".to_string()),
            ..a_task()
        };
        assert_eq!(task_from_body(&task_body(&bare)), Some(bare));
        assert_eq!(
            task_from_body(&serde_json::json!({ "task": 1, "task_id": "x" })),
            None,
            "a task without its input is not a task"
        );
        assert_eq!(task_from_body(&serde_json::json!({ "hello": 1 })), None);
    }

    #[test]
    fn a_reply_body_round_trips_either_way() {
        let ran = TaskReply {
            task_id: "task-dev-a-1-1".to_string(),
            agent_id: Some("dev-b".to_string()),
            outcome: Some(serde_json::json!({ "kind": "final", "content": "done" })),
            error: None,
        };
        assert_eq!(task_reply_from_body(&task_reply_body(&ran)), Some(ran));

        let refused = TaskReply {
            task_id: "task-dev-a-1-1".to_string(),
            agent_id: None,
            outcome: None,
            error: Some("the peer did not authorise dispatch".to_string()),
        };
        assert_eq!(
            task_reply_from_body(&task_reply_body(&refused)),
            Some(refused)
        );
        assert_eq!(
            task_reply_from_body(&serde_json::json!({ "task_reply": 1 })),
            None,
            "a reply without a task id matches nothing"
        );
    }

    #[test]
    fn the_two_frames_are_not_each_other() {
        let task = a_task();
        assert!(!is_task_reply(&task_body(&task)));
        let reply = TaskReply {
            task_id: task.task_id.clone(),
            agent_id: None,
            outcome: None,
            error: None,
        };
        assert!(!is_task(&task_reply_body(&reply)));
        // And neither is any other body this crate knows.
        assert!(!is_task(&crate::relay::digest_body(None, 0)));
        assert!(!is_task_reply(&crate::relay::heartbeat_body()));
        assert!(!is_task(&crate::liveness::probe_body()));
        assert!(!is_task_reply(&crate::suppression::takeover_body(
            "c", "a", 1
        )));
    }
}
