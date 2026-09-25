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
/// So is `clock` (dsl 0.24.0 §1): the declared clock's derived paths.
pub(crate) const STATE_ROOTS: &[&str] =
    &["scene", "run", "user", "app", "quest", "entry", "prev", "clock"];

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

/// `true` for a RESERVED quest path (dsl 0.2.0 §5.2, dsl 0.8.0 §5, dsl
/// 0.24.0 §2): `quest.<id>.state` (3 segments, segment 2 == `state`),
/// `quest.<id>.activatedAt` (3 segments, segment 2 == `activatedAt`),
/// `quest.<id>.failedBy` (3 segments, segment 2 == `failedBy`), or
/// `quest.<id>.objectives.<oid>.done` / `.failed` (5 segments, segment 2 ==
/// `objectives`, segment 4 == `done` / `failed`). These are
/// engine-populated, implicitly-declared sub-namespaces of `quest.<id>.*` —
/// content MAY read them but MUST NOT `::set` them
/// (`E-QUEST-RESERVED-WRITE`) nor author-declare them
/// (`E-QUEST-RESERVED-DECL`).
pub(crate) fn is_reserved_quest_path(path: &str) -> bool {
    let segs: Vec<&str> = path.split('.').collect();
    matches!(
        segs.as_slice(),
        ["quest", _, "state" | "activatedAt" | "failedBy"]
            | ["quest", _, "objectives", _, "done" | "failed"]
    )
}

/// `true` for a RESERVED lore flag: `entry.<id>.read` (dsl 0.19.0 §5) or
/// `entry.<id>.everRead` (dsl 0.22.0 §7) — 3 segments, segment 0 ==
/// `entry`. Both are engine-written `bool`s (default `false`): `read` is
/// run-tier (reset by a new run), `everRead` user-tier (set on the first read
/// ever, never reset). Readable from any CEL slot in any document kind —
/// implicitly declared regardless of whether THIS document declares the
/// `<entry>` (the `quest.<id>.state` rule); content MUST NOT `::set` either
/// (`E-QUEST-RESERVED-WRITE`). `check-project` resolves the id
/// (`W-ENTRY-REF-UNKNOWN`).
pub fn is_reserved_entry_read(path: &str) -> bool {
    reserved_entry_id(path).is_some()
}

/// The `<id>` of a reserved entry flag path ([`is_reserved_entry_read`]:
/// `entry.<id>.read` or `entry.<id>.everRead`), or `None` for any other shape.
pub fn reserved_entry_id(path: &str) -> Option<&str> {
    let mut segs = path.split('.');
    match (segs.next(), segs.next(), segs.next(), segs.next()) {
        (Some("entry"), Some(id), Some("read" | "everRead"), None) => Some(id),
        _ => None,
    }
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
/// cascade from its parent, or a sibling completing a `complete="any"`
/// parent (`superseded`); `unset` while it has not failed.
pub(crate) const QUEST_FAILED_BY: &[&str] =
    &["unset", "fail", "by", "until", "cascade", "superseded"];

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
