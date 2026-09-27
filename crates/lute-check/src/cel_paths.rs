//! Shared CEL state-path extraction (dsl §9.1/§9.4).
//!
//! Both the CEL-slot resolver (T4.3, [`crate::cel_resolve`]) and the
//! definite-assignment analysis (T4.4, [`crate::defassign`]) need to reconstruct
//! the dotted state paths (`scene.*`/`run.*`/`user.*`/`app.*`) an expression
//! *reads*. This module is the single AST walk they share: it collects the
//! **maximal** dotted `Select`/`Ident` chains (never the intermediate prefixes,
//! so `scene.player.hp` yields exactly `scene.player.hp`, not also `scene` /
//! `scene.player`) and classifies each as an ordinary [`PathRole::Read`] or a
//! guard [`PathRole::Guard`].
//!
//! A **guard** is a presence test that *tolerates* an unset path:
//! - `has(p)` — the CEL macro expands to a test-only `Select` (`select.test`).
//! - `isSet(p)` — a DSL global call whose sole argument is a static path.
//!
//! Per the cel-parser 0.10.1 carry-forward (T3.1/T4.3), per-node byte offsets are
//! unavailable on a successfully parsed AST, so the caller assigns spans from the
//! enclosing slot; this walk yields only the reconstructed path strings + roles.

use cel_parser::ast::{operators as op, CallExpr, EntryExpr, Expr};

/// State-tier roots that introduce a declared state-path read (dsl §9.1).
/// Tier-GENERAL: kept scalar-agnostic on purpose (0.3.0's relational tiers
/// reuse this same list, dsl 0.2.0 §5). `entry` (dsl 0.19.0 §5) is a
/// read-only root: its only declared shape is the reserved
/// [`is_reserved_entry_read`] path, so any other `entry.*` read is
/// `E-UNDECLARED` and every `entry.*` write is rejected. `prev` (dsl 0.23.0
/// §6) is read-only too: `prev.run.<path>` mirrors each declared `run.<path>`.
/// So is `clock` (dsl 0.24.0 §1): the declared clock's derived paths. And
/// `occasion` (dsl 0.26.0 §5): a kind beat's `occasion.target`.
/// `season` (dsl 0.27.0 §5): a declared season's tier `season.<name>.*`.
/// The list itself is the reserved-names table's ([`lute_manifest::reserved`]).
pub(crate) const STATE_ROOTS: &[&str] = lute_manifest::reserved::STATE_ROOTS;

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

/// dsl 0.27.0 §3: the family a `{{<path>[occasion.target]}}` interpolation
/// reads — `user.bond` for `user.bond[occasion.target]` — or `None` for any
/// other text. The family is a bare dotted state path and the index is
/// exactly the kind beat's member: the one computed read `{{…}}` admits.
pub fn occasion_indexed_family(raw: &str) -> Option<&str> {
    let family = raw
        .strip_suffix(']')?
        .strip_suffix(crate::beats::OCCASION_TARGET)?
        .strip_suffix('[')?;
    (is_state_path(family) && family.split('.').all(crate::check::is_cel_ident)).then_some(family)
}

/// How a state path appears in an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathRole {
    /// An ordinary value read (subject to definite-assignment, dsl §9.4).
    Read,
    /// A presence test (`has(p)`/`isSet(p)`) in a **dominating** position (top
    /// level or a conjunct of `&&`): it proves the path for the guarded body.
    Guard,
    /// A presence test in a **non-dominating** position (under `||`/`!`/`?:`):
    /// it proves nothing for the guarded body (dsl §9.4). The path is still
    /// surfaced so read-site declaration checks are unaffected; the reads it
    /// short-circuits over carry it in [`PathUse::local`] instead.
    WeakGuard,
}

/// One reconstructed state path plus how it was used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathUse {
    pub path: String,
    pub role: PathRole,
    /// dsl 0.24.0 (T1-7b): the paths an enclosing short-circuit proves present
    /// at THIS read — `isSet(p) && …p…`, `isSet(p) ? …p… : …`,
    /// `!isSet(p) || …p…`, `!isSet(p) ? … : …p…`, at any depth. Local to the
    /// subexpression it sits in: it never proves the guarded body (that is
    /// [`PathRole::Guard`]'s job). Empty when no guard encloses the read.
    pub local: Vec<String>,
}

/// `true` when `path`'s leading segment is a state tier (`scene`/`run`/…).
pub(crate) fn is_state_path(path: &str) -> bool {
    path.split('.')
        .next()
        .is_some_and(|root| STATE_ROOTS.contains(&root))
}

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
        ty: EnginePathType::Enum(crate::match_check::QUEST_STATES),
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

/// The engine-owned paths each document's schema folds from the construct
/// that declares them (their type is that construct's): readable where the
/// construct is in scope, never declared in `state:` (`E-STATE-DECL`) and
/// never `::set`.
pub const FOLDED_ENGINE_PATHS: &[EnginePath] = &[
    EnginePath {
        shape: "scene.choices.<branch>",
        ty: EnginePathType::Folded,
        tier: "scene",
        meaning: "the choice a `<branch>` / `<hub>` took: one of its choice ids, `unset` before",
    },
    EnginePath {
        shape: "scene.visited.<hub>.<choice>",
        ty: EnginePathType::Folded,
        tier: "scene",
        meaning: "that `<hub>` choice was ever taken (bool)",
    },
    EnginePath {
        shape: "occasion.target",
        ty: EnginePathType::Folded,
        tier: "occasion",
        meaning: "in a beat of a targeted occasion: the member the answered raise is for",
    },
    EnginePath {
        shape: "occasion.payload.<field>",
        ty: EnginePathType::Folded,
        tier: "occasion",
        meaning: "in a beat of an occasion with a `payload:`: that field of the answered raise",
    },
    EnginePath {
        shape: "clock.<field>",
        ty: EnginePathType::Folded,
        tier: "run",
        meaning: "derived from the declared `clock:` (day, slot, weekday, index, ended …)",
    },
    EnginePath {
        shape: "prev.<path>",
        ty: EnginePathType::Folded,
        tier: "run",
        meaning: "the previous run's (or season window's) value of `<path>`",
    },
];

/// The [`RESERVED_PATHS`] row `path` has, if any.
pub fn reserved_path(path: &str) -> Option<&'static EnginePath> {
    RESERVED_PATHS.iter().find(|r| r.matches(path))
}

/// `true` for a RESERVED quest path ([`RESERVED_PATHS`]):
/// `quest.<id>.state` / `.activatedAt` / `.failedBy`, or
/// `quest.<id>.objectives.<oid>.done` / `.failed`.
pub(crate) fn is_reserved_quest_path(path: &str) -> bool {
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

/// `true` when the engine owns `path`: a [`RESERVED_PATHS`] path, or one in
/// a namespace `state:` may not declare (`scene.choices.*`,
/// `scene.visited.*`, `occasion.*`, `clock.*`, `entry.*`, `prev.*` — the
/// shapes of [`FOLDED_ENGINE_PATHS`]).
pub fn is_engine_owned_path(path: &str) -> bool {
    reserved_path(path).is_some() || crate::meta::engine_namespace(path).is_some()
}

/// `true` for the user-tier `entry.<id>.everRead` flag (dsl 0.22.0 §7) — the
/// one reserved entry flag a new run does not reset.
pub fn is_entry_ever_read(path: &str) -> bool {
    reserved_entry_id(path).is_some() && path.ends_with(".everRead")
}

/// `true` for any path rooted at the read-only `entry` tier (dsl 0.19.0 §5:
/// "writing an `entry.*` path is rejected, as writing a `quest.*` path is").
pub(crate) fn is_entry_path(path: &str) -> bool {
    path.split('.').next() == Some("entry")
}

/// `true` specifically for the `quest.<id>.objectives.<oid>.done` reserved
/// shape (5 segments, segment 2 == `objectives`, segment 4 == `done`) — the
/// sub-case of [`is_reserved_quest_path`] that `check_quest` (dsl 0.2.0 §6.4,
/// `crate::match_check`) seeds with a `bool` decl carrying `default: false`.
/// Distinguished from `quest.<id>.state` (no default — a `<match>` over it
/// must cover `unset`, dsl 0.2.0 §5.2) so a caller that needs to mirror
/// `check_quest`'s synthetic decl (definite-assignment defaulting) can treat
/// the two reserved shapes differently without re-deriving the segment shape.
pub(crate) fn is_reserved_quest_objective_done(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "objectives", _, "done"]
    )
}

/// `true` specifically for the `quest.<id>.activatedAt` reserved shape (3
/// segments, segment 2 == `activatedAt`) — the sub-case of
/// [`is_reserved_quest_path`] that `check_quest` seeds with a
/// [`lute_manifest::types::Type::NarrativeTime`] decl (dsl 0.8.0 §5): the
/// quest-instance activation instant, engine-populated at the
/// `unset → active` transition.
///
/// Needed as its own predicate at three sites where the three reserved shapes
/// diverge: the narrative-time classification in [`crate::temporal`] (a
/// FOREIGN quest's anchor is readable but never folded into this document's
/// schema, so `resolve_type` alone cannot see it), the definite-assignment
/// exemption in [`crate::defassign`], and the `<match>` domain synthesis in
/// [`crate::match_check`] (narrative time is opaque — `Domain::Infinite`, not
/// the lifecycle enum `quest.<id>.state` carries).
pub(crate) fn is_reserved_quest_activated_at(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "activatedAt"]
    )
}

/// `true` specifically for `quest.<id>.state` (3 segments, segment 2 ==
/// `state`, non-empty id) — the sub-case of [`is_reserved_quest_path`] that is
/// the quest LIFECYCLE ENUM. 0.21.1 T1-1: it is always assigned
/// (`unset | active | complete | failed` — the engine writes `unset` for every
/// known quest before activation), so it is never maybe-unset, `'unset'` is a
/// member rather than a misspelled sentinel, and `isSet()` on it is always
/// true. Every checker site that reasons about unset-ness asks this predicate.
pub(crate) fn is_reserved_quest_state(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", id, "state"] if !id.is_empty()
    )
}

/// dsl 0.24.0 §2: the members of `quest.<id>.failedBy` — why the quest
/// failed: its own `fail`, a required objective's `by` / `until` deadline, a
/// required subquest that failed (`subquest`, dsl 0.28.0), a cascade from
/// its parent, or a sibling completing a `complete="any"` parent
/// (`superseded`); `unset` while it has not failed.
pub(crate) const QUEST_FAILED_BY: &[&str] = &[
    "unset",
    "fail",
    "by",
    "until",
    "subquest",
    "cascade",
    "superseded",
];

/// `true` specifically for `quest.<id>.failedBy` (dsl 0.24.0 §2) — the
/// sub-case of [`is_reserved_quest_path`] that is the failure-reason enum
/// ([`QUEST_FAILED_BY`]). Like `quest.<id>.state` it is always assigned (the
/// engine stores `unset` until the quest fails) and never folded into a
/// document's schema: every quest's, local or foreign, is admitted by shape
/// and typed here.
pub(crate) fn is_reserved_quest_failed_by(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", id, "failedBy"] if !id.is_empty()
    )
}

/// `true` specifically for `quest.<id>.objectives.<oid>.failed` (dsl 0.24.0
/// §2) — the sub-case of [`is_reserved_quest_path`] that is an objective's
/// failure flag: a `bool` defaulting to `false`, always assigned, never
/// folded into a document's schema (admitted by shape, like a foreign
/// quest's `done`).
pub(crate) fn is_reserved_quest_objective_failed(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<&str>>().as_slice(),
        ["quest", _, "objectives", _, "failed"]
    )
}

/// `E-PATH-IDENT`: a `-` in a CEL-facing name — a state-path segment, a `defs`
/// name, or a def parameter name (dsl §8.4, §4.4 `CelIdent`). CEL parses `-` as
/// subtraction, so these positions forbid it; `Ident` positions (directive/attr/
/// speaker/choice/branch/hub/asset ids) keep permitting it.
pub const E_PATH_IDENT: &str = "E-PATH-IDENT";

/// `true` when any segment of a dotted state path AFTER the leading tier contains
/// `-` (dsl §8.4). The tier keyword (`scene`/`run`/`user`/`app`) is fixed and
/// never carries `-`, so only the `CelIdent` segments matter.
pub(crate) fn state_path_has_hyphen(path: &str) -> bool {
    path.split('.').skip(1).any(|seg| seg.contains('-'))
}

/// The one rule for a quest id and an objective id (`what` names which): each
/// is ONE `CelIdent` segment of the reserved `quest.<id>.state` /
/// `quest.<id>.objectives.<oid>.done` paths (`[A-Za-z_][A-Za-z0-9_]*`).
/// `None` when `id` is one; else why not, as an `E-PATH-IDENT` message.
/// A `.` would split the id into more path segments, so no read of the
/// quest's paths could ever name it.
pub(crate) fn quest_id_fault(what: &str, id: &str) -> Option<String> {
    if id.is_empty() || crate::check::is_cel_ident(id) {
        return None;
    }
    let suggestion: String = id
        .split(['.', '-'])
        .enumerate()
        .map(|(i, seg)| {
            let mut cs = seg.chars();
            match cs.next() {
                Some(c) if i > 0 => c.to_ascii_uppercase().to_string() + cs.as_str(),
                Some(c) => c.to_string() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect();
    Some(if id.contains('.') {
        format!(
            "{what} id `{id}` has a `.`; the id must be one name, so `quest.{id}.state` would \
             be read as more path segments and never name it — write `{suggestion}`"
        )
    } else if id.contains('-') {
        format!("{what} id `{id}` has a `-`; CEL-facing names forbid `-` — write `{suggestion}`")
    } else {
        format!("{what} id `{id}` is not a name (`[A-Za-z_][A-Za-z0-9_]*`)")
    })
}

/// Collect every maximal state-path use in `expr` (recursing into all
/// sub-expressions: call args, list/map/struct elements, comprehensions).
pub(crate) fn collect_path_uses(expr: &Expr) -> Vec<PathUse> {
    let mut walk = Walk::default();
    walk.expr(expr, true);
    walk.out
}

/// The path-use walk: `local` is the stack of paths the enclosing
/// short-circuits prove at the current position ([`PathUse::local`]).
#[derive(Default)]
struct Walk {
    out: Vec<PathUse>,
    local: Vec<String>,
}

impl Walk {
    fn push(&mut self, path: String, role: PathRole) {
        if is_state_path(&path) {
            let local = if role == PathRole::Read {
                self.local.clone()
            } else {
                Vec::new()
            };
            self.out.push(PathUse { path, role, local });
        }
    }

    /// Walk `expr` with `proved` in the local proof set for its duration only.
    fn under(&mut self, expr: &Expr, dominating: bool, proved: Vec<String>) {
        let depth = self.local.len();
        self.local.extend(proved);
        self.expr(expr, dominating);
        self.local.truncate(depth);
    }

    fn expr(&mut self, expr: &Expr, dominating: bool) {
        match expr {
            Expr::Ident(name) => self.push(name.clone(), PathRole::Read),
            Expr::Select(sel) => {
                // A test-only Select is the `has(p)` macro (dsl §9.4 guard).
                let role = if sel.test {
                    if dominating {
                        PathRole::Guard
                    } else {
                        PathRole::WeakGuard
                    }
                } else {
                    PathRole::Read
                };
                if let Some(path) = select_path(expr) {
                    self.push(path, role);
                } else {
                    // Chain bottoms out in a non-ident (e.g. `f(x).field`,
                    // `xs[0].field`): not a static state path, but its operand
                    // may still contain reads.
                    self.expr(&sel.operand.expr, false);
                }
            }
            Expr::Call(call) => {
                // `isSet(p)` — a DSL presence guard whose single arg is a static path.
                if let Some(path) = is_set_arg(call) {
                    let role = if dominating {
                        PathRole::Guard
                    } else {
                        PathRole::WeakGuard
                    };
                    self.push(path, role);
                    return;
                }
                // Boolean structure controls dominance: `&&` preserves it for
                // both args; `||`, `!`, `?:` (and any other call/operand) drop
                // it. Short-circuit order controls LOCAL proof (dsl 0.24.0):
                // the right operand of `&&` runs only when the left is true,
                // of `||` only when it is false, and a conditional's branches
                // only when its condition is true / false.
                if call.target.is_none() {
                    match (call.func_name.as_str(), call.args.as_slice()) {
                        (op::LOGICAL_AND, [a, b]) => {
                            self.expr(&a.expr, dominating);
                            self.under(&b.expr, dominating, proved_if(&a.expr, true));
                            return;
                        }
                        (op::LOGICAL_OR, [a, b]) => {
                            self.expr(&a.expr, false);
                            self.under(&b.expr, false, proved_if(&a.expr, false));
                            return;
                        }
                        (op::CONDITIONAL, [c, then, other]) => {
                            self.expr(&c.expr, false);
                            self.under(&then.expr, false, proved_if(&c.expr, true));
                            self.under(&other.expr, false, proved_if(&c.expr, false));
                            return;
                        }
                        _ => {}
                    }
                }
                if let Some(target) = &call.target {
                    self.expr(&target.expr, false);
                }
                for arg in &call.args {
                    self.expr(&arg.expr, false);
                }
            }
            Expr::List(list) => {
                for el in &list.elements {
                    self.expr(&el.expr, false);
                }
            }
            Expr::Map(map) => {
                for entry in &map.entries {
                    self.entry(&entry.expr);
                }
            }
            Expr::Struct(st) => {
                for entry in &st.entries {
                    self.entry(&entry.expr);
                }
            }
            Expr::Comprehension(c) => {
                self.expr(&c.iter_range.expr, false);
                self.expr(&c.accu_init.expr, false);
                self.expr(&c.loop_cond.expr, false);
                self.expr(&c.loop_step.expr, false);
                self.expr(&c.result.expr, false);
            }
            Expr::Literal(_) | Expr::Unspecified => {}
        }
    }

    fn entry(&mut self, entry: &EntryExpr) {
        match entry {
            EntryExpr::MapEntry(m) => {
                self.expr(&m.key.expr, false);
                self.expr(&m.value.expr, false);
            }
            EntryExpr::StructField(f) => self.expr(&f.value.expr, false),
        }
    }
}

/// The static state path of an `isSet(p)` call, or `None`.
fn is_set_arg(call: &CallExpr) -> Option<String> {
    if call.target.is_some()
        || !call.func_name.eq_ignore_ascii_case("isSet")
        || call.args.len() != 1
    {
        return None;
    }
    select_path(&call.args[0].expr).filter(|p| is_state_path(p))
}

/// The state paths provably set whenever `expr` evaluates to `outcome`:
/// `isSet(p)`/`has(p)` proves `p` when true, `!e` flips the outcome, a true
/// `a && b` (a false `a || b`) proves what either side does, and a false
/// `a && b` (a true `a || b`) only what both sides do. Anything else proves
/// nothing.
pub(crate) fn proved_if(expr: &Expr, outcome: bool) -> Vec<String> {
    match expr {
        Expr::Select(sel) if sel.test => match select_path(expr) {
            Some(p) if outcome && is_state_path(&p) => vec![p],
            _ => Vec::new(),
        },
        Expr::Call(call) => {
            if let Some(p) = is_set_arg(call) {
                return if outcome { vec![p] } else { Vec::new() };
            }
            if call.target.is_some() {
                return Vec::new();
            }
            let both = |a: &Expr, b: &Expr, o: bool| -> (Vec<String>, Vec<String>) {
                (proved_if(a, o), proved_if(b, o))
            };
            match (call.func_name.as_str(), call.args.as_slice()) {
                (op::LOGICAL_NOT, [a]) => proved_if(&a.expr, !outcome),
                (op::LOGICAL_AND, [a, b]) | (op::LOGICAL_OR, [a, b]) => {
                    let (mut l, r) = both(&a.expr, &b.expr, outcome);
                    // `&&` true / `||` false: every operand took `outcome`.
                    if (call.func_name == op::LOGICAL_AND) == outcome {
                        l.extend(r);
                    } else {
                        l.retain(|p| r.contains(p));
                    }
                    l
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}

/// Reconstruct the dotted path of a pure `Ident`/`Select` chain (`a.b.c`).
/// Returns `None` if the chain bottoms out in anything but a bare `Ident`.
pub(crate) fn select_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Select(sel) => {
            let base = select_path(&sel.operand.expr)?;
            Some(format!("{base}.{}", sel.field))
        }
        _ => None,
    }
}

/// The nearest DECLARED state path to `path` within `max_dist` edits (dsl
/// 0.5.0 §2.2): `None` when nothing declared is close enough, `path` itself
/// is already declared (distance 0 is excluded — no self-suggestion), or the
/// schema declares nothing. Ties broken by `BTreeMap` key order (stable,
/// deterministic).
pub(crate) fn nearest_declared_path<'s>(
    path: &str,
    schema: &'s crate::meta::StateSchema,
    max_dist: usize,
) -> Option<&'s str> {
    lute_manifest::suggest::nearest(path, schema.decls.keys().map(|k| k.as_str()), max_dist)
}

/// The message for a read of an undeclared path under an engine namespace —
/// `quest.<id>.*`, `entry.<id>.*`, `scene.choices` / `scene.visited`, or
/// `occasion.*` — built from [`RESERVED_PATHS`] and [`FOLDED_ENGINE_PATHS`]:
/// what that namespace holds, and the nearest shape filled with the path's
/// own ids (`quest.q1.status` → `quest.q1.state`, `entry.e.everread` →
/// `entry.e.everRead`). `None` for any other path (`clock.*`, `prev.*` and
/// `season.*` are judged by their own declarations). `declared` filters a
/// `scene.*` suggestion: only this scene's branches and hubs are readable.
/// `occasion.payload.*` is the caller's (it names the occasion's `payload:`).
pub(crate) fn engine_path_hint(path: &str, declared: &dyn Fn(&str) -> bool) -> Option<String> {
    use lute_manifest::suggest::nearest;
    let segs: Vec<&str> = path.split('.').collect();
    let root = segs[0];
    let rows: Vec<&EnginePath> = RESERVED_PATHS
        .iter()
        .chain(FOLDED_ENGINE_PATHS)
        .filter(|r| r.shape.split('.').next() == Some(root))
        .collect();
    match root {
        "quest" | "entry" => {}
        "scene" => {
            let sub = segs.get(1).copied().unwrap_or_default();
            let subs = ["choices", "visited"];
            if !subs.contains(&sub) && nearest(sub, subs, 2).is_none() {
                return None;
            }
        }
        "occasion" if path == crate::beats::OCCASION_TARGET => {
            return Some(format!(
                "`{path}` has a value only in a beat or entry that targets a kind \
                 (`target=\"kind:<kind>\"`) or runs once for each member of one \
                 (`for=\"kind:<kind>\"`), and nothing here does — read `{path}` there, or read \
                 something this condition has"
            ));
        }
        "occasion" => {}
        _ => return None,
    }
    // Each shape with its `<name>` segments filled from the path's own.
    let filled: Vec<String> = rows
        .iter()
        .map(|r| {
            r.shape
                .split('.')
                .enumerate()
                .map(|(i, w)| match segs.get(i) {
                    Some(got) if w.starts_with('<') => *got,
                    _ => w,
                })
                .collect::<Vec<_>>()
                .join(".")
        })
        .collect();
    let usable = |c: &&str| !c.contains('<') && (root != "scene" || declared(c));
    let whole = nearest(path, filled.iter().map(String::as_str).filter(usable), 2);
    // Else the last segment against the leaves of the shapes that agree on
    // everything before it (`objectives.o.complete` → `done`).
    let leaf = || {
        let (prefix, leaf) = path.rsplit_once('.')?;
        let leaves = filled
            .iter()
            .filter_map(|c| c.rsplit_once('.'))
            .filter(|(p, _)| *p == prefix)
            .map(|(_, l)| l);
        let near = nearest(leaf, leaves, 2)?;
        Some(format!("{prefix}.{near}"))
    };
    let suggestion = whole
        .map(str::to_string)
        .or_else(leaf)
        .filter(|s| usable(&s.as_str()))
        .map_or_else(String::new, |s| format!(" — did you mean `{s}`?"));
    let listing = |rows: &[&EnginePath]| {
        rows.iter()
            .map(|r| format!("`{}`", r.shape))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Some(match root {
        "quest" | "entry" => {
            let head = format!("{root}.<{root}>.");
            let keys: Vec<String> = rows
                .iter()
                .map(|r| format!("`{}`", r.shape.trim_start_matches(head.as_str())))
                .collect();
            format!(
                "`{path}` is not a path the engine keeps{suggestion}: every `{root}.<id>` has {} \
                 — the engine sets them, so `state:` does not declare them",
                keys.join(", ")
            )
        }
        "scene" => format!(
            "`{path}` is not a choice of this scene{suggestion}: the engine keeps {} for this \
             scene's own `<branch>` / `<hub>` ids, and `scene.*` ends with its scene — to read a \
             pick another scene made, record it with `into=\"run.…\"` on that choice",
            listing(&rows)
        ),
        _ => format!(
            "`{path}` is not a path the engine keeps{suggestion}: `occasion.*` has {} while a beat \
             answers its occasion",
            listing(&rows)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shape with every `<name>` segment filled in.
    fn instance(shape: &str) -> String {
        shape
            .split('.')
            .map(|s| if s.starts_with('<') { "x" } else { s })
            .collect::<Vec<_>>()
            .join(".")
    }

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

    /// Every folded engine shape is one `state:` refuses to declare, so the
    /// table `lute context` prints cannot name a path the checker lets an
    /// author own.
    #[test]
    fn every_folded_engine_shape_is_refused_as_a_declaration() {
        for p in FOLDED_ENGINE_PATHS.iter().chain(RESERVED_PATHS) {
            let path = instance(p.shape);
            assert!(is_engine_owned_path(&path), "{path}");
        }
        assert!(!is_engine_owned_path("run.day"));
        assert!(!is_engine_owned_path("quest.q.notes"));
    }
}
