//! `rooms.json`: membership and the rules a room carries (v1.0 M4c).
//!
//! [connection.md §5](../../docs/connection.md) freezes this. A **room is a membership list
//! plus its rules**, and its landing place is `rooms.json`: one versioned JSON file whose
//! `rooms[]` each carry a `name`, a `members[]` of **`node_id`s**, and a `rules` object with
//! §5.2's three entries.
//!
//! **A member is a `node_id` and nothing else.** A room names *who*; `peers.json` says what
//! a node *is*. That is why an entry here has no key: [§5.1](../../docs/connection.md) keeps
//! the two files with **two authors** — a room's `members[]` is what the local deployer says
//! about who is in it, while a `peers.json` entry's `rooms[]` is what that node *claims
//! about itself*. Neither overwrites the other.
//!
//! **Membership is configuration.** v1.0 has no join protocol ([§5.2](../../docs/connection.md)):
//! the members are what the deployer wrote, and there is no wire request that adds one. A
//! dynamic membership protocol would be a new mechanism with its own authority question —
//! [decisions §33](../../docs/decisions.md)'s territory — so it is not invented here.
//!
//! The three rules, and what is enforced where:
//!
//! | Rule | Shape | Enforced |
//! |---|---|---|
//! | `rate` | `{messages, window_seconds}`, **per member** | [`RateCounters::admit`], in memory; over budget is [`Category::Refused`] |
//! | `mention` | `"members"` or `"nobody"`, defaulting to **`"nobody"`** | [`Room::allows_mention_from`] |
//! | `require_signature` | **only `true`** in v1.0 | at **load**: a `false` is refused, because §3 already makes a signature universal on the peer path |

use crate::error::Category;
use crate::versioned::{self, Versioned, VersionedError, VersionedLoad};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// The file name, inside the node's data directory.
pub const ROOMS_FILE: &str = "rooms.json";

/// A room's message budget: `messages` per `window_seconds`, **per member**.
///
/// Per member rather than per room, because a room-wide budget would let one member starve
/// the others ([§5.2](../../docs/connection.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateRule {
    /// How many messages one member may send inside one window.
    pub messages: u32,
    /// The window, in seconds.
    pub window_seconds: u32,
}

impl RateRule {
    /// Is `used` still inside the budget?
    pub fn allows(&self, used: u32) -> bool {
        used < self.messages
    }

    /// The window in milliseconds.
    pub fn window_ms(&self) -> i64 {
        i64::from(self.window_seconds) * 1_000
    }
}

/// Who may `@` whom inside a room ([§5.2](../../docs/connection.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mention {
    /// A member may `@` another member of this room.
    Members,
    /// Nobody may `@` anybody: the **default**, because default deny is the project's habit.
    #[default]
    Nobody,
}

impl Mention {
    /// The wire word.
    pub fn as_str(self) -> &'static str {
        match self {
            Mention::Members => "members",
            Mention::Nobody => "nobody",
        }
    }

    /// Does this setting allow a member-to-member mention?
    pub fn allows(self) -> bool {
        matches!(self, Mention::Members)
    }
}

/// The three rules a room carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomRules {
    /// The message budget, per member.
    pub rate: RateRule,
    /// Defaults to [`Mention::Nobody`] when the file does not say.
    #[serde(default)]
    pub mention: Mention,
    /// Always `true` after a successful load: a `false` is refused by
    /// [`RoomsFile`]'s own check, because §3 makes a signature universal on the peer path
    /// and a room setting must not be able to lower that floor.
    pub require_signature: bool,
}

impl RoomRules {
    /// The rule a room must state to be loadable.
    pub fn new(rate: RateRule) -> Self {
        Self {
            rate,
            mention: Mention::Nobody,
            require_signature: true,
        }
    }

    /// Does this room require a signature?
    ///
    /// `true` for every room that loaded: the field is kept because
    /// [roadmap §4](../../docs/roadmap-v1.0.md) names the rule, and because a version that
    /// wanted to allow unsigned traffic inside a room would have to say so here — in a
    /// decision, not by quietly setting a flag.
    pub fn requires_signature(&self) -> bool {
        self.require_signature
    }
}

/// One room: a name, its members and its rules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Room {
    pub name: String,
    /// The members, as **`node_id`s** — the device names `peers.json` also keys on.
    #[serde(default)]
    pub members: Vec<String>,
    pub rules: RoomRules,
}

impl Room {
    /// Is this node a member?
    pub fn has_member(&self, node_id: &str) -> bool {
        self.members.iter().any(|member| member == node_id)
    }

    /// How many members.
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Nobody is in it?
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Does the room allow member-to-member mentions at all?
    pub fn allows_mention(&self) -> bool {
        self.rules.mention.allows()
    }

    /// May **this** node `@` another member here?
    ///
    /// Membership first, then the rule: a node that is not a member is refused by
    /// membership, not by `mention` ([§5.2](../../docs/connection.md)).
    pub fn allows_mention_from(&self, node_id: &str) -> bool {
        self.has_member(node_id) && self.allows_mention()
    }
}

/// The file: a version and a list of rooms.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomsFile {
    /// Always [`RoomsFile::SCHEMA_VERSION`] when this build writes it, and first on the wire.
    pub schema_version: u32,
    #[serde(default)]
    pub rooms: Vec<Room>,
}

/// Why a room file could not be used.
#[derive(Debug)]
pub enum RoomsError {
    /// The file could not be versioned, read or parsed.
    Versioned(VersionedError),
    /// A room is not usable — a nameless room, a duplicate name, a rate that is not a
    /// positive count, or (the one worth naming) a `require_signature: false`.
    Room(String),
}

impl std::fmt::Display for RoomsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoomsError::Versioned(e) => write!(f, "{e}"),
            RoomsError::Room(why) => write!(f, "the room file is not usable: {why}"),
        }
    }
}

impl std::error::Error for RoomsError {}

impl From<VersionedError> for RoomsError {
    fn from(error: VersionedError) -> Self {
        RoomsError::Versioned(error)
    }
}

impl From<std::io::Error> for RoomsError {
    fn from(error: std::io::Error) -> Self {
        RoomsError::Versioned(VersionedError::Io(error))
    }
}

impl RoomsFile {
    /// The version this build writes.
    pub const SCHEMA_VERSION: u32 = 1;

    /// No rooms at all: the node is in nothing, and adopts nothing.
    pub fn empty() -> Self {
        Self {
            schema_version: Self::SCHEMA_VERSION,
            rooms: Vec::new(),
        }
    }

    /// Read `<data_dir>/rooms.json`. Never creates anything.
    pub fn load_in(data_dir: &Path) -> Result<VersionedLoad<Self>, RoomsError> {
        Self::load(&data_dir.join(ROOMS_FILE))
    }

    /// Read a room file.
    pub fn load(path: &Path) -> Result<VersionedLoad<Self>, RoomsError> {
        versioned::load(path).map_err(RoomsError::from)
    }

    /// Write a room file (configuration, so it may be replaced).
    pub fn save_in(data_dir: &Path, file: &Self) -> Result<PathBuf, RoomsError> {
        let path = data_dir.join(ROOMS_FILE);
        std::fs::create_dir_all(data_dir)?;
        versioned::save(&path, file)?;
        Ok(path)
    }

    /// One room, by name.
    pub fn room(&self, name: &str) -> Option<&Room> {
        self.rooms.iter().find(|room| room.name == name)
    }

    /// Is anyone in any room?
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// How many rooms the file defines.
    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    /// **"Configured for a room"**, as [§5.3](../../docs/connection.md) defines it: the file
    /// names the room **and** its `members[]` lists this node.
    pub fn is_configured_for(&self, this_node_id: &str, room: &str) -> bool {
        self.room(room)
            .map(|room| room.has_member(this_node_id))
            .unwrap_or(false)
    }

    /// The room names this node is a member of.
    pub fn room_names_for(&self, this_node_id: &str) -> BTreeSet<String> {
        self.rooms
            .iter()
            .filter(|room| room.has_member(this_node_id))
            .map(|room| room.name.clone())
            .collect()
    }

    /// The rooms this node is a member of.
    pub fn rooms_for<'a>(&'a self, this_node_id: &'a str) -> impl Iterator<Item = &'a Room> {
        self.rooms
            .iter()
            .filter(move |room| room.has_member(this_node_id))
    }
}

impl Versioned for RoomsFile {
    const SCHEMA_VERSION: u32 = RoomsFile::SCHEMA_VERSION;

    fn from_value(value: serde_json::Value, _from: u32) -> Result<Self, VersionedError> {
        let file: RoomsFile = serde_json::from_value(value)
            .map_err(|e| VersionedError::Shape(format!("the room file is malformed: {e}")))?;
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for room in &file.rooms {
            validate_room(room)?;
            if !names.insert(room.name.as_str()) {
                return Err(VersionedError::Shape(format!(
                    "two rooms are called {:?}",
                    room.name
                )));
            }
        }
        Ok(file)
    }
}

/// The checks a room must pass, in the order a reader would ask them.
fn validate_room(room: &Room) -> Result<(), VersionedError> {
    if room.name.trim().is_empty() {
        return Err(VersionedError::Shape("a room has an empty name".into()));
    }
    for member in &room.members {
        if member.trim().is_empty() {
            return Err(VersionedError::Shape(format!(
                "room {:?} lists an empty member",
                room.name
            )));
        }
    }
    if !room.rules.require_signature {
        return Err(VersionedError::Shape(format!(
            "room {:?} sets require_signature: false; §3 makes a signature universal on the \
             peer path, so no room may lower that floor",
            room.name
        )));
    }
    if room.rules.rate.messages == 0 {
        return Err(VersionedError::Shape(format!(
            "room {:?} has a rate of 0 messages, which is not a budget",
            room.name
        )));
    }
    if room.rules.rate.window_seconds == 0 {
        return Err(VersionedError::Shape(format!(
            "room {:?} has a rate window of 0 seconds",
            room.name
        )));
    }
    Ok(())
}

/// One member's message count inside one window, for one room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Budget {
    used: u32,
    window_start_ms: i64,
}

/// The in-memory counters [`RateRule`] needs.
///
/// **Per member, per room**, and in memory: a budget is runtime state like the request
/// queue, not configuration. It is deliberately **not persisted** — a restart forgets every
/// count, and the cost of that is one window's worth of extra messages, which is smaller
/// than the cost of a file that could disagree with the rule it is counting against.
#[derive(Debug, Default)]
pub struct RateCounters {
    budgets: HashMap<(String, String), Budget>,
}

impl RateCounters {
    /// No member has sent anything.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many members are being counted.
    pub fn tracked(&self) -> usize {
        self.budgets.len()
    }

    /// How many messages this member has sent in the room's current window.
    pub fn used(&self, room: &str, member: &str) -> u32 {
        self.budgets
            .get(&(room.to_string(), member.to_string()))
            .map(|budget| budget.used)
            .unwrap_or(0)
    }

    /// Count one message from `member` in `room`, or refuse it.
    ///
    /// The window is a **fixed** one, starting at the member's first message inside it: when
    /// `now_ms` is past the window's end, the count starts again. Over budget is
    /// [`RateError::Exceeded`], whose category is [`Category::Refused`] — the error model
    /// already gives that word to "a policy the deployer set" ([error-model.md §4](../../docs/error-model.md)),
    /// so no new category appears.
    pub fn admit(
        &mut self,
        room: &str,
        member: &str,
        rule: &RateRule,
        now_ms: i64,
    ) -> Result<(), RateError> {
        let key = (room.to_string(), member.to_string());
        let budget = self.budgets.entry(key).or_insert(Budget {
            used: 0,
            window_start_ms: now_ms,
        });
        if now_ms - budget.window_start_ms >= rule.window_ms() {
            budget.used = 0;
            budget.window_start_ms = now_ms;
        }
        if !rule.allows(budget.used) {
            return Err(RateError {
                room: room.to_string(),
                member: member.to_string(),
                messages: rule.messages,
                window_seconds: rule.window_seconds,
            });
        }
        budget.used += 1;
        Ok(())
    }
}

/// A member that has used up its budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateError {
    pub room: String,
    pub member: String,
    pub messages: u32,
    pub window_seconds: u32,
}

impl RateError {
    /// [`Category::Refused`]: the sender was entitled to speak, and is being told no for a
    /// while. Nothing was reached *instead* of it, so it is not `network`.
    pub fn category(&self) -> Category {
        Category::Refused
    }
}

impl std::fmt::Display for RateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} has sent {} messages in {} within {}s",
            self.member, self.messages, self.room, self.window_seconds
        )
    }
}

impl std::error::Error for RateError {}

/// Room-file failures are the input being wrong, except an I/O failure, which is the
/// filesystem.
pub fn rooms_category(error: &RoomsError) -> Category {
    match error {
        RoomsError::Versioned(VersionedError::Io(_)) => Category::Network,
        _ => Category::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(messages: u32, window_seconds: u32, mention: Mention) -> RoomRules {
        RoomRules {
            rate: RateRule {
                messages,
                window_seconds,
            },
            mention,
            require_signature: true,
        }
    }

    #[test]
    fn a_member_budget_is_per_member_and_resets_with_the_window() {
        let mut counters = RateCounters::new();
        let rule = RateRule {
            messages: 2,
            window_seconds: 60,
        };
        let start = 1_700_000_000_000;

        assert!(counters.admit("lab", "dev-a", &rule, start).is_ok());
        assert!(counters.admit("lab", "dev-a", &rule, start + 1).is_ok());
        let refused = counters
            .admit("lab", "dev-a", &rule, start + 2)
            .expect_err("over budget");
        assert_eq!(refused.category(), Category::Refused);
        assert_eq!(counters.used("lab", "dev-a"), 2);

        // Another member has its own budget.
        assert!(counters.admit("lab", "dev-b", &rule, start + 2).is_ok());

        // Another room has its own.
        assert!(counters.admit("other", "dev-a", &rule, start + 2).is_ok());

        // The window moves on and the count starts again.
        assert!(counters
            .admit("lab", "dev-a", &rule, start + 60_000)
            .is_ok());
        assert_eq!(counters.used("lab", "dev-a"), 1);
        assert!(counters.tracked() > 1);
    }

    #[test]
    fn mention_defaults_to_nobody_and_follows_membership() {
        let room = Room {
            name: "lab".into(),
            members: vec!["dev-a".into()],
            rules: rules(10, 60, Mention::Nobody),
        };
        assert!(!room.allows_mention_from("dev-a"), "the default forbids");
        assert_eq!(Mention::default(), Mention::Nobody);

        let open = Room {
            rules: rules(10, 60, Mention::Members),
            ..room.clone()
        };
        assert!(open.allows_mention());
        assert!(open.allows_mention_from("dev-a"));
        // Membership, not the rule: somebody outside the room cannot `@` in it.
        assert!(!open.allows_mention_from("dev-b"));
    }
}
