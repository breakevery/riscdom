//! Agent and instance identity (v0.8 batch B; instance half v1.0 M2a-1).
//!
//! Every audit event names the agent that caused it, and every sandbox instance
//! names itself. Both shapes are the same three parts:
//!
//! - **device** — which machine. It defaults to [`DEFAULT_DEVICE`] until the node
//!   layer names it ([`set_device`]); naming a node is the connection layer's work
//!   (roadmap §4), and until then a single machine is `local`;
//! - **pid** — which process, so two processes writing one chain cannot collide;
//! - **seq** — which identity *inside* that process, handed out by [`next_agent_id`]
//!   / [`next_instance_id`] from **one** process-wide counter, so two `AppState`s
//!   (or two instances) in one process never share an identity.
//!
//! Example: `local-12345-1`. The value is an audit field, not a secret: it names
//! an agent or an instance, and it is what lets one chain say "who made whom do
//! what" once several agents and instances write to it.
//!
//! **The parts are not a path syntax.** Nothing parses an identity back into
//! device/pid/seq during normal operation — the type is the identity. The last two
//! parts are the only ones that can never contain a `-` (a pid and a number), so a
//! caller that has to take an identity apart reads it from the **right**
//! (`rsplitn(3, '-')`: sequence, pid, device) and never from the left: a device
//! name may carry a `-`, and `splitn` from the left would cut it in half.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

/// The device part an identity carries until the node names itself.
pub const DEFAULT_DEVICE: &str = "local";

/// The device part of every identity this process mints.
///
/// Process-wide and settable, because a node's name is a *deployment* fact
/// (roadmap §4) and the ids are minted in many places. `None` means "nobody named
/// this node", which answers [`DEFAULT_DEVICE`]. The last caller wins, the same
/// shape as `host_core::paths::set_app_data_dir` — and for the same reason: this
/// is set once at startup, not negotiated.
static DEVICE: RwLock<Option<String>> = RwLock::new(None);

/// Which identity in this process; the first one is `seq` 1.
///
/// **One counter for both spaces.** Agents and instances are minted with the same
/// three-part shape, so a shared counter is what guarantees the two never mint the
/// same string: separate counters could hand `local-1-1` to an agent and to an
/// instance, and a reader of the chain could not tell which was meant.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// The device this process stamps into identities ([`DEFAULT_DEVICE`] until set).
pub fn device() -> String {
    DEVICE
        .read()
        .ok()
        .and_then(|held| held.clone())
        .unwrap_or_else(|| DEFAULT_DEVICE.to_string())
}

/// Name this node (v1.0 M2a-1; the node layer sets this once).
///
/// Returns the device that was in force before the call, so a caller can put it
/// back. A blank name is **ignored** — it would mint identities that start with a
/// `-`, and every one of them would look like it had no device at all.
pub fn set_device(name: impl Into<String>) -> String {
    let previous = device();
    let name = name.into();
    if name.trim().is_empty() {
        return previous;
    }
    if let Ok(mut held) = DEVICE.write() {
        *held = Some(name);
    }
    previous
}

/// The next agent identity in this process: `<device>-<pid>-<seq>`.
///
/// Monotonic and process-wide: every call returns a value no other call (here or
/// in another process) returns.
pub fn next_agent_id() -> String {
    mint(&device(), std::process::id(), next_seq())
}

/// The next sandbox-instance identity in this process: `<device>-<pid>-<seq>`.
///
/// The same shape as [`next_agent_id`] and from the same counter, so an instance
/// id and an agent id are never the same string (v1.0 M2a-1).
pub fn next_instance_id() -> InstanceId {
    InstanceId(mint(&device(), std::process::id(), next_seq()))
}

/// One sandbox instance's identity.
///
/// A newtype for the same reason [`AgentId`](crate::dispatch::AgentId) is one: the
/// day an instance crosses a machine, the wire format is one decision in one
/// place. It is a distinct type from an agent id even though both are three-part
/// strings — the type is what keeps "who ran this" and "what ran this" apart.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InstanceId(String);

impl InstanceId {
    /// An identity whose shape this call does not check (the parts are not a path
    /// syntax — see the module docs).
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The identity as it appears on the wire and in the audit chain.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The next value of the process-wide counter.
fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed) + 1
}

/// The one place an identity's text is built.
fn mint(device: &str, pid: u32, seq: u64) -> String {
    format!("{device}-{pid}-{seq}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Tests that read or write the process-wide device take this, so the one that
    /// changes it cannot race the ones that assert on it. Unit tests of this crate
    /// run in their own process, so nothing outside this module is affected.
    static DEVICE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn ids_are_unique_and_shaped_as_documented() {
        let _guard = DEVICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let a = next_agent_id();
        let b = next_agent_id();
        assert_ne!(a, b, "two ids in one process must differ");
        for id in [&a, &b] {
            // From the right: the sequence and the pid can never contain a `-`, so
            // the remainder is the device, whatever a deployer named it.
            let parts: Vec<&str> = id.rsplitn(3, '-').collect();
            assert_eq!(parts.len(), 3, "device-pid-seq: {id}");
            assert!(parts[0].parse::<u64>().is_ok(), "seq: {id}");
            assert_eq!(parts[1], std::process::id().to_string());
            assert_eq!(parts[2], device());
        }
    }

    #[test]
    fn agents_and_instances_never_mint_the_same_identity() {
        let _guard = DEVICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // One counter serves both spaces, so the strings cannot collide.
        let agent = next_agent_id();
        let instance = next_instance_id();
        assert_ne!(agent, instance.as_str());
        assert_ne!(instance.as_str(), "local-0-0");
        assert!(instance.as_str().starts_with(&device()));
        assert_eq!(instance.to_string(), instance.as_str());
    }

    #[test]
    fn the_device_is_settable_and_an_empty_name_is_ignored() {
        let _guard = DEVICE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(device(), DEFAULT_DEVICE, "nothing named this node yet");

        // A blank name changes nothing: it would mint ids starting with `-`.
        let before = set_device("   ");
        assert_eq!(before, DEFAULT_DEVICE);
        assert_eq!(device(), DEFAULT_DEVICE);

        let before = set_device("lab-node");
        assert_eq!(
            before, DEFAULT_DEVICE,
            "the setter reports what was in force"
        );
        assert_eq!(device(), "lab-node");
        assert!(
            next_instance_id().as_str().starts_with("lab-node-"),
            "the new device is what identities use"
        );

        // A device name may contain the separator: the identity is one string, and
        // only the right-hand two parts are machine-made.
        set_device("rack-7.node-a");
        let id = next_instance_id();
        let parts: Vec<&str> = id.as_str().rsplitn(3, '-').collect();
        assert_eq!(parts.len(), 3, "{id}");
        assert_eq!(parts[2], "rack-7.node-a");
        assert_eq!(parts[1], std::process::id().to_string());
        assert!(parts[0].parse::<u64>().is_ok(), "{id}");

        let restored = set_device(before.clone());
        assert_eq!(
            restored, "rack-7.node-a",
            "the setter reports what was in force"
        );
        assert_eq!(device(), before, "the test put the device back");
    }

    #[test]
    fn an_instance_id_carries_a_value_nothing_parses() {
        let id = InstanceId::new("instance-made-by-hand");
        assert_eq!(id.as_str(), "instance-made-by-hand");
        assert_eq!(id, InstanceId::new("instance-made-by-hand"));
        assert_ne!(id, InstanceId::new("another"));
        // It survives the wire, because a task will name one (v1.0 M2a-3).
        let json = serde_json::to_string(&id).expect("serialises");
        assert_eq!(json, r#""instance-made-by-hand""#);
        assert_eq!(
            serde_json::from_str::<InstanceId>(&json).expect("parses"),
            id
        );
    }
}
