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
pub(crate) const STATE_ROOTS: &[&str] = &["scene", "run", "user", "app"];

/// How a state path appears in an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathRole {
    /// An ordinary value read: subject to `E-MAYBE-UNSET` when unproven.
    Read,
    /// A presence guard (`has(p)`/`isSet(p)`): tolerates unset, and *proves* `p`
    /// within the scope it guards.
    Guard,
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

/// Collect every maximal state-path use in `expr` (recursing into all
/// sub-expressions: call args, list/map/struct elements, comprehensions).
pub(crate) fn collect_path_uses(expr: &Expr) -> Vec<PathUse> {
    let mut out = Vec::new();
    walk(expr, &mut out);
    out
}

fn push_path(out: &mut Vec<PathUse>, path: String, role: PathRole) {
    if is_state_path(&path) {
        out.push(PathUse { path, role });
    }
}

fn walk(expr: &Expr, out: &mut Vec<PathUse>) {
    match expr {
        Expr::Ident(name) => push_path(out, name.clone(), PathRole::Read),
        Expr::Select(sel) => {
            // A test-only Select is the `has(p)` macro expansion (dsl §9.4 guard).
            let role = if sel.test {
                PathRole::Guard
            } else {
                PathRole::Read
            };
            if let Some(path) = select_path(expr) {
                push_path(out, path, role);
            } else {
                // Chain bottoms out in a non-ident (e.g. `f(x).field`,
                // `xs[0].field`): not a static state path, but its operand may
                // still contain reads.
                walk(&sel.operand.expr, out);
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
                        push_path(out, path, PathRole::Guard);
                        return;
                    }
                }
            }
            if let Some(target) = &call.target {
                walk(&target.expr, out);
            }
            for arg in &call.args {
                walk(&arg.expr, out);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                walk(&el.expr, out);
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
            walk(&c.iter_range.expr, out);
            walk(&c.accu_init.expr, out);
            walk(&c.loop_cond.expr, out);
            walk(&c.loop_step.expr, out);
            walk(&c.result.expr, out);
        }
        Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn walk_entry(entry: &EntryExpr, out: &mut Vec<PathUse>) {
    match entry {
        EntryExpr::MapEntry(m) => {
            walk(&m.key.expr, out);
            walk(&m.value.expr, out);
        }
        EntryExpr::StructField(f) => walk(&f.value.expr, out),
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
