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

use cel_parser::ast::{EntryExpr, Expr};

/// State-tier roots that introduce a declared state-path read (dsl §9.1).
/// Tier-GENERAL: kept scalar-agnostic on purpose (0.3.0's relational tiers
/// reuse this same list, dsl 0.2.0 §5).
pub(crate) const STATE_ROOTS: &[&str] = &["scene", "run", "user", "app", "quest"];

/// How a state path appears in an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathRole {
    /// An ordinary value read (subject to definite-assignment, dsl §9.4).
    Read,
    /// A presence test (`has(p)`/`isSet(p)`) in a **dominating** position (top
    /// level or a conjunct of `&&`): it proves the path for the guarded body.
    Guard,
    /// A presence test in a **non-dominating** position (under `||`/`!`): it
    /// proves nothing (dsl §9.4). The path is still surfaced so read-site
    /// declaration checks are unaffected, but definite-assignment ignores it.
    WeakGuard,
}

/// One reconstructed state path plus how it was used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathUse {
    pub path: String,
    pub role: PathRole,
}

/// `true` when `path`'s leading segment is a state tier (`scene`/`run`/…).
pub(crate) fn is_state_path(path: &str) -> bool {
    path.split('.')
        .next()
        .is_some_and(|root| STATE_ROOTS.contains(&root))
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
    let mut out = Vec::new();
    walk(expr, true, &mut out);
    out
}

fn push_path(out: &mut Vec<PathUse>, path: String, role: PathRole) {
    if is_state_path(&path) {
        out.push(PathUse { path, role });
    }
}

fn walk(expr: &Expr, dominating: bool, out: &mut Vec<PathUse>) {
    match expr {
        Expr::Ident(name) => push_path(out, name.clone(), PathRole::Read),
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
                push_path(out, path, role);
            } else {
                // Chain bottoms out in a non-ident (e.g. `f(x).field`,
                // `xs[0].field`): not a static state path, but its operand may
                // still contain reads.
                walk(&sel.operand.expr, false, out);
            }
        }
        Expr::Call(call) => {
            // `isSet(p)` — a DSL presence guard whose single arg is a static path.
            if call.target.is_none()
                && call.func_name.eq_ignore_ascii_case("isSet")
                && call.args.len() == 1
            {
                if let Some(path) = select_path(&call.args[0].expr) {
                    if is_state_path(&path) {
                        let role = if dominating {
                            PathRole::Guard
                        } else {
                            PathRole::WeakGuard
                        };
                        push_path(out, path, role);
                        return;
                    }
                }
            }
            // Boolean structure controls dominance: `&&` preserves it for both
            // args; `||` and `!` (and any other call/operand) drop it.
            let child_dom = dominating && call.target.is_none() && call.func_name == "_&&_";
            if let Some(target) = &call.target {
                walk(&target.expr, false, out);
            }
            for arg in &call.args {
                walk(&arg.expr, child_dom, out);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                walk(&el.expr, false, out);
            }
        }
        Expr::Map(map) => {
            for entry in &map.entries {
                walk_entry(&entry.expr, out);
            }
        }
        Expr::Struct(st) => {
            for entry in &st.entries {
                walk_entry(&entry.expr, out);
            }
        }
        Expr::Comprehension(c) => {
            walk(&c.iter_range.expr, false, out);
            walk(&c.accu_init.expr, false, out);
            walk(&c.loop_cond.expr, false, out);
            walk(&c.loop_step.expr, false, out);
            walk(&c.result.expr, false, out);
        }
        Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn walk_entry(entry: &EntryExpr, out: &mut Vec<PathUse>) {
    match entry {
        EntryExpr::MapEntry(m) => {
            walk(&m.key.expr, false, out);
            walk(&m.value.expr, false, out);
        }
        EntryExpr::StructField(f) => walk(&f.value.expr, false, out),
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
