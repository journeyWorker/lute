//! Engine-owned CEL state paths (dsl §9.1).

/// `true` for any path rooted at the read-only `prev` mirror (dsl 0.23.0 §6).
pub fn is_prev_path(path: &str) -> bool {
    path.split('.').next() == Some("prev")
}

/// The `prev.run.<path>` mirror of a `run.<path>` (dsl 0.23.0 §6).
pub fn prev_run_path(run_path: &str) -> Option<String> {
    run_path
        .strip_prefix("run.")
        .filter(|rest| !rest.is_empty())
        .map(|rest| format!("prev.run.{rest}"))
}

/// The `quest.<id>.state` lifecycle members, in IR domain order (0.21.1 T1-1;
/// `lute-compile` emits the same list — the folded enum plus `unset`).
pub const QUEST_STATES: &[&str] = &["active", "complete", "failed", "unset"];

/// The value an engine-owned path holds (see [`RESERVED_PATHS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnginePathType {
    /// One of these members.
    Enum(&'static [&'static str]),
    /// `true` / `false`, `false` until the engine sets it.
    Bool,
    /// A narrative-time instant (dsl 0.8.0 §5).
    NarrativeTime,
    /// Typed by the construct that declares it (a `<branch>`'s choice ids,
    /// an occasion's `payload:`, the declared `clock:` …) — folded into each
    /// document's schema, not implicit everywhere.
    Folded,
}

/// One engine-owned path shape: content reads it, never declares it in
/// `state:` and never `::set`s it. `shape` is dotted; a `<name>` segment
/// stands for any id. `tier` is the lifetime it follows (`quest` = the
/// quest's own `tier`).
#[derive(Clone, Copy, Debug)]
pub struct EnginePath {
    pub shape: &'static str,
    pub ty: EnginePathType,
    pub tier: &'static str,
    pub meaning: &'static str,
}

impl EnginePath {
    /// Does `path` have this shape? A `<name>` segment matches any segment.
    pub fn matches(&self, path: &str) -> bool {
        let mut want = self.shape.split('.');
        let mut got = path.split('.');
        loop {
            match (want.next(), got.next()) {
                (None, None) => return true,
                (Some(w), Some(_)) if w.starts_with('<') => {}
                (Some(w), Some(g)) if w == g => {}
                _ => return false,
            }
        }
    }
}

/// The RESERVED paths (dsl 0.2.0 §5.2, 0.8.0 §5, 0.19.0 §5, 0.22.0 §7,
/// 0.24.0 §2): engine-populated, implicitly declared for every quest and
/// entry — local or foreign — and readable from any CEL slot in any
/// document kind. Content MUST NOT `::set` them (`E-QUEST-RESERVED-WRITE`)
/// nor declare them (`E-QUEST-RESERVED-DECL`); `check-project` resolves the
/// ids (`W-QUEST-REF-UNKNOWN`, `W-ENTRY-REF-UNKNOWN`). The one table the
/// shape predicates below and `lute context` read.
pub const RESERVED_PATHS: &[EnginePath] = &[
    EnginePath {
        shape: "quest.<quest>.state",
        ty: EnginePathType::Enum(QUEST_STATES),
        tier: "quest",
        meaning: "the quest's lifecycle; `unset` until it activates (always assigned)",
    },
    EnginePath {
        shape: "quest.<quest>.failedBy",
        ty: EnginePathType::Enum(QUEST_FAILED_BY),
        tier: "quest",
        meaning: "why the quest failed: its `fail`, a required objective's `by` / `until`, a \
                  required `subquest` that failed, a `cascade` from its parent, or `superseded` \
                  by a sibling; `unset` while it has not failed",
    },
    EnginePath {
        shape: "quest.<quest>.activatedAt",
        ty: EnginePathType::NarrativeTime,
        tier: "quest",
        meaning: "the moment the quest activated",
    },
    EnginePath {
        shape: "quest.<quest>.objectives.<objective>.done",
        ty: EnginePathType::Bool,
        tier: "quest",
        meaning: "the objective is done",
    },
    EnginePath {
        shape: "quest.<quest>.objectives.<objective>.failed",
        ty: EnginePathType::Bool,
        tier: "quest",
        meaning: "the objective failed (its `by` / `until` deadline passed first)",
    },
    EnginePath {
        shape: "entry.<entry>.read",
        ty: EnginePathType::Bool,
        tier: "run",
        meaning: "the entry was read this run",
    },
    EnginePath {
        shape: "entry.<entry>.everRead",
        ty: EnginePathType::Bool,
        tier: "user",
        meaning: "the entry was ever read (a new run does not reset it)",
    },
];

/// The [`RESERVED_PATHS`] row `path` has, if any.
pub fn reserved_path(path: &str) -> Option<&'static EnginePath> {
    RESERVED_PATHS.iter().find(|r| r.matches(path))
}

/// `true` for a RESERVED quest path ([`RESERVED_PATHS`]):
/// `quest.<id>.state` / `.activatedAt` / `.failedBy`, or
/// `quest.<id>.objectives.<oid>.done` / `.failed`.
pub fn is_reserved_quest_path(path: &str) -> bool {
    reserved_path(path).is_some_and(|r| r.shape.starts_with("quest."))
}

/// `true` for a RESERVED lore flag ([`RESERVED_PATHS`]): `entry.<id>.read`
/// (run-tier) or `entry.<id>.everRead` (user-tier), both engine-written
/// `bool`s defaulting to `false`, readable whether or not THIS document
/// declares the `<entry>`.
pub fn is_reserved_entry_read(path: &str) -> bool {
    reserved_entry_id(path).is_some()
}

/// The `<id>` of a reserved entry flag path ([`is_reserved_entry_read`]:
/// `entry.<id>.read` or `entry.<id>.everRead`), or `None` for any other shape.
pub fn reserved_entry_id(path: &str) -> Option<&str> {
    reserved_path(path)
        .filter(|r| r.shape.starts_with("entry."))
        .and_then(|_| path.split('.').nth(1))
}

/// `true` specifically for the `quest.<id>.objectives.<oid>.done` reserved
/// shape (5 segments, segment 2 == `objectives`, segment 4 == `done`) — the
/// sub-case of [`is_reserved_quest_path`] that `check_quest` (dsl 0.2.0 §6.4,
/// `crate::match_check`) seeds with a `bool` decl carrying `default: false`.
/// Distinguished from `quest.<id>.state` (no default — a `<match>` over it
/// must cover `unset`, dsl 0.2.0 §5.2) so a caller that needs to mirror
/// `check_quest`'s synthetic decl (definite-assignment defaulting) can treat
/// the two reserved shapes differently without re-deriving the segment shape.
pub fn is_reserved_quest_objective_done(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "objectives", _, "done"]
    )
}

/// dsl 0.24.0 §2: the members of `quest.<id>.failedBy` — why the quest
/// failed: its own `fail`, a required objective's `by` / `until` deadline, a
/// required subquest that failed (`subquest`, dsl 0.28.0), a cascade from
/// its parent, or a sibling completing a `complete="any"` parent
/// (`superseded`); `unset` while it has not failed.
pub const QUEST_FAILED_BY: &[&str] = &[
    "unset",
    "fail",
    "by",
    "until",
    "subquest",
    "cascade",
    "superseded",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_shapes_match_exactly_their_segments() {
        for path in [
            "quest.q.state",
            "quest.q.failedBy",
            "quest.q.activatedAt",
            "quest.q.objectives.o.done",
            "quest.q.objectives.o.failed",
        ] {
            assert!(is_reserved_quest_path(path), "{path}");
        }
        for path in [
            "quest.q",
            "quest.q.notes",
            "quest.q.state.x",
            "quest.q.objectives.o",
            "quest.q.objectives.o.done.x",
            "run.q.state",
        ] {
            assert!(!is_reserved_quest_path(path), "{path}");
        }
        assert_eq!(reserved_entry_id("entry.lamp.read"), Some("lamp"));
        assert_eq!(reserved_entry_id("entry.lamp.everRead"), Some("lamp"));
        assert_eq!(reserved_entry_id("entry.lamp.read.x"), None);
        assert_eq!(reserved_entry_id("entry.lamp.seen"), None);
        assert_eq!(reserved_entry_id("quest.lamp.state"), None);
    }
}
