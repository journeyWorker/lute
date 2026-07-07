//! Portable expression AST (`expr`) for CEL slots (IR addendum A7).
//!
//! The compiled artifact must be self-contained for engines that ship NO CEL
//! parser: every CEL slot on a record (a `<match>` arm `test`, a `<choice>`
//! `when`, a `::set` `value`) carries a parallel **`expr` AST** that a plain
//! JSON walker can evaluate. The raw CEL string field stays alongside it for
//! debug/provenance.
//!
//! [`lower_expr`] parses a raw CEL fragment via `lute_cel::parse_slot` — exactly
//! like `lute_check::match_check::parse_expr` — performing the DSL token
//! substitution (`$`->`_`, `@`->space) and panic-guarding malformed CEL, then
//! walks the `cel_parser::ast::Expr` tree into an [`ExprNode`]. Anything outside
//! the closed Lute-CEL profile (dsl §8.4) — a `null`/bytes literal, a map/struct/
//! comprehension, an unknown call — lowers to `None`; a `None` child poisons its
//! parent, so a partially-unlowerable slot omits `expr` entirely rather than
//! emitting a half-tree.
//!
//! ## Serialized shape (byte-stability contract)
//! [`ExprNode`] is a serde **`untagged`** enum: each variant serializes as its
//! bare struct body (no discriminant), producing exactly these JSON shapes —
//! field declaration order = serialized order:
//! - literal   → `{"lit": <number|bool|string>}` (all numbers are f64/double)
//! - path      → `{"path": "user.level"}`
//! - unary     → `{"op": "!"|"-", "l": <node>}`
//! - binary    → `{"op": "<sym>", "l": <node>, "r": <node>}` where `<sym>` ∈
//!   `&& || == != < <= > >= + - * / in`
//! - ternary   → `{"cond": <node>, "then": <node>, "else": <node>}`
//! - list      → `{"list": [<node>, ...]}`
//! - `isSet(p)`→ `{"isSet": "<path>"}`
//! - `has(p)`  → `{"has": "<path>"}`

use cel_parser::ast::{CallExpr, Expr};
use cel_parser::reference::Val;
use lute_cel::CelArena;
use serde::Serialize;

/// One node of the portable expression AST (dsl §8.4 profile). See the module
/// docs for the exact serialized JSON shape of each variant.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ExprNode {
    /// Scalar literal: `{"lit": <number|bool|string>}`.
    Lit { lit: LitVal },
    /// Static state/subject path: `{"path": "a.b.c"}`.
    Path { path: String },
    /// Unary operator (`!`/`-`): `{"op": "<sym>", "l": <node>}`.
    Unary {
        op: &'static str,
        l: Box<ExprNode>,
    },
    /// Binary operator: `{"op": "<sym>", "l": <node>, "r": <node>}`.
    Binary {
        op: &'static str,
        l: Box<ExprNode>,
        r: Box<ExprNode>,
    },
    /// Ternary conditional: `{"cond": <node>, "then": <node>, "else": <node>}`.
    Cond {
        cond: Box<ExprNode>,
        then: Box<ExprNode>,
        #[serde(rename = "else")]
        otherwise: Box<ExprNode>,
    },
    /// List literal: `{"list": [<node>, ...]}`.
    List { list: Vec<ExprNode> },
    /// `isSet(path)` extension: `{"isSet": "<path>"}`.
    IsSet {
        #[serde(rename = "isSet")]
        is_set: String,
    },
    /// `has(path)` macro: `{"has": "<path>"}`.
    Has { has: String },
}

/// A scalar literal value. Serialized untagged, so it emits a bare JSON number,
/// bool, or string as the value of the `lit` field. All numeric CEL literals
/// (`Int`/`UInt`/`Double`) collapse to an f64 double.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum LitVal {
    /// Numeric literal (always f64).
    Num(f64),
    /// Boolean literal.
    Bool(bool),
    /// String literal.
    Str(String),
}

/// Parse a raw CEL fragment and lower it to a portable [`ExprNode`].
///
/// Mirrors `lute_check::match_check::parse_expr`: a fresh [`CelArena`] +
/// `lute_cel::parse_slot` (DSL token substitution + malformed-CEL panic guard).
/// Returns `None` for empty/malformed input or any construct outside the
/// Lute-CEL profile — the raw string field still carries the CEL for debug.
pub fn lower_expr(raw: &str) -> Option<ExprNode> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = CelArena::default();
    let handle = lute_cel::parse_slot(&mut arena, raw, 0).ok()?;
    let root = arena.get(handle)?;
    lower(&root.expr)
}

/// Outcome of synthesizing a `<match>` arm's executable expr (IR A13).
#[derive(Debug)]
pub(crate) enum ArmExpr {
    /// The synthesized arm expr, or `None` when nothing lowers (a `test`-only
    /// guard outside the CEL profile, or a malformed/empty pattern). The arm
    /// carries no `expr` in that case — identical to the C1 behavior.
    Lowered(Option<ExprNode>),
    /// A `<when is="unset">` alternative whose `<match>` subject is NOT a bare
    /// path (a compound expr): the arm cannot be lowered to an executable expr
    /// (A13 rule 5). The caller MUST emit a compile diagnostic and leave the
    /// arm's `expr` as `None` — never silently drop the arm.
    UnsetOnCompoundSubject,
}

/// Synthesize a `<match>` arm's executable `expr` (IR A13) by inlining the
/// match `subject` into the arm's `is` literal-pattern and/or `test` guard, so
/// the runtime never parses/evaluates `subject` (it stays a debug-only string).
///
/// - `is` is the `<when is="…">` value (trimmed), or `None` when absent.
/// - `test_raw` is the `test` guard CEL — empty when there is no `test` attr.
/// - `subject_raw` is the `<match on="…">` subject CEL (a state path).
///
/// Mirrors `lute_check::match_check::analyze_is_pattern`: the `is` value is
/// split on `'|'`, each alternative trimmed (empties dropped), classified as
/// `true`/`false` (bool), a number literal (f64), `unset`, or an enum/string
/// member, lowered to a comparison against the inlined subject, then the
/// alternatives are left-folded with `||`. With a `test` guard the `is` expr is
/// `&&`-joined to `lower_expr(test_raw)`. Reuses [`lower_expr`] for the subject
/// and the guard — no hand-rolled CEL parsing.
pub(crate) fn synth_arm_expr(is: Option<&str>, test_raw: &str, subject_raw: &str) -> ArmExpr {
    // Rule 2: no `is` → the arm expr is exactly the lowered `test` guard; the
    // subject is NOT inlined for a pure `test` guard (empty guard → `None`).
    let Some(is_raw) = is else {
        return ArmExpr::Lowered(lower_expr(test_raw));
    };
    // The subject inlined into every `is` comparison (A13). `None` when the
    // subject itself is outside the CEL profile — then no comparison lowers.
    let subject = lower_expr(subject_raw);
    let is_expr = match synth_is_expr(is_raw, subject.as_ref()) {
        IsSynth::Unlowerable => return ArmExpr::UnsetOnCompoundSubject,
        IsSynth::Expr(node) => node,
    };
    if test_raw.trim().is_empty() {
        // Rule 4: `is` only.
        return ArmExpr::Lowered(is_expr);
    }
    // Rule 3: `is` + `test` → `<is-expr> && <test-expr>`. Either side failing to
    // lower poisons the arm expr (a well-formed arm never hits this).
    let joined = match (is_expr, lower_expr(test_raw)) {
        (Some(l), Some(r)) => Some(ExprNode::Binary {
            op: "&&",
            l: Box::new(l),
            r: Box::new(r),
        }),
        _ => None,
    };
    ArmExpr::Lowered(joined)
}

/// Result of lowering the `is` literal-pattern alone (before any `test` join).
enum IsSynth {
    /// The folded `is` expr, or `None` when the subject is out of profile so no
    /// comparison could be built.
    Expr(Option<ExprNode>),
    /// An `unset` alternative whose subject is not a bare path (A13 rule 5).
    Unlowerable,
}

/// Lower a `<when is="…">` literal pattern (dsl §7.3.1) against the inlined
/// `subject`: split on `'|'`, classify each trimmed alternative
/// (`true`/`false` → bool, number → f64, `unset` → `!isSet(path)`, else →
/// string), build a comparison per alternative, then left-fold with `||`.
/// Mirrors `lute_check::match_check::analyze_is_pattern`.
fn synth_is_expr(is_raw: &str, subject: Option<&ExprNode>) -> IsSynth {
    let mut nodes: Vec<ExprNode> = Vec::new();
    for lit in is_raw.split('|') {
        let lit = lit.trim();
        if lit.is_empty() {
            continue;
        }
        let node = match lit {
            "true" => subject_eq(subject, LitVal::Bool(true)),
            "false" => subject_eq(subject, LitVal::Bool(false)),
            "unset" => match subject {
                // `unset` → `!isSet(<subject-path>)`; requires a bare path.
                Some(ExprNode::Path { path }) => Some(ExprNode::Unary {
                    op: "!",
                    l: Box::new(ExprNode::IsSet {
                        is_set: path.clone(),
                    }),
                }),
                _ => return IsSynth::Unlowerable,
            },
            // `is_number_literal` already proved the `f64` parse succeeds.
            _ if is_number_literal(lit) => {
                subject_eq(subject, LitVal::Num(lit.parse().expect("number literal")))
            }
            _ => subject_eq(subject, LitVal::Str(lit.to_string())),
        };
        match node {
            Some(n) => nodes.push(n),
            // Subject out of profile → no comparison lowers, so neither does the
            // whole `is` pattern.
            None => return IsSynth::Expr(None),
        }
    }
    let mut it = nodes.into_iter();
    let Some(first) = it.next() else {
        // Every alternative was empty (a stray `|` or blank `is`).
        return IsSynth::Expr(None);
    };
    let folded = it.fold(first, |acc, n| ExprNode::Binary {
        op: "||",
        l: Box::new(acc),
        r: Box::new(n),
    });
    IsSynth::Expr(Some(folded))
}

/// `<subject> == <lit>` with the subject node inlined; `None` when the subject
/// is out of the CEL profile (it never lowered to an [`ExprNode`]).
fn subject_eq(subject: Option<&ExprNode>, lit: LitVal) -> Option<ExprNode> {
    Some(ExprNode::Binary {
        op: "==",
        l: Box::new(subject?.clone()),
        r: Box::new(ExprNode::Lit { lit }),
    })
}

/// True when `lit` is a decimal `Number` literal (dsl §7.3.1) rather than an
/// enum-member ident — mirrors `lute_check::match_check::is_number_literal`:
/// a leading digit/sign/dot plus a successful `f64` parse.
fn is_number_literal(lit: &str) -> bool {
    let head = lit.strip_prefix(['+', '-']).unwrap_or(lit);
    matches!(head.bytes().next(), Some(b'0'..=b'9' | b'.')) && lit.parse::<f64>().is_ok()
}

/// Walk one `cel_parser::ast::Expr` into an [`ExprNode`]; `None` on any
/// out-of-profile node (or any child that fails to lower).
fn lower(expr: &Expr) -> Option<ExprNode> {
    match expr {
        Expr::Literal(v) => lower_literal(v),
        Expr::Ident(name) => Some(ExprNode::Path { path: name.clone() }),
        Expr::Select(sel) => {
            // `has(p)` expands to a test-only Select; a plain Select is a path.
            let path = select_path(expr)?;
            if sel.test {
                Some(ExprNode::Has { has: path })
            } else {
                Some(ExprNode::Path { path })
            }
        }
        Expr::List(list) => {
            let mut items = Vec::with_capacity(list.elements.len());
            for el in &list.elements {
                items.push(lower(&el.expr)?);
            }
            Some(ExprNode::List { list: items })
        }
        Expr::Call(c) if c.target.is_none() => lower_call(c),
        _ => None,
    }
}

/// Lower a receiverless `Call`: a synthetic operator (`cel_parser` lowers every
/// operator to a fixed `func_name`), the ternary, or the `isSet(<path>)`
/// extension. Anything else → `None`.
fn lower_call(c: &CallExpr) -> Option<ExprNode> {
    use cel_parser::ast::operators as op;
    let name = c.func_name.as_str();

    // Binary operators (exactly two operands under `l`/`r`).
    if let Some(sym) = binary_symbol(name) {
        if c.args.len() != 2 {
            return None;
        }
        let l = Box::new(lower(&c.args[0].expr)?);
        let r = Box::new(lower(&c.args[1].expr)?);
        return Some(ExprNode::Binary { op: sym, l, r });
    }

    if name == op::LOGICAL_NOT && c.args.len() == 1 {
        return Some(ExprNode::Unary {
            op: "!",
            l: Box::new(lower(&c.args[0].expr)?),
        });
    }
    if name == op::NEGATE && c.args.len() == 1 {
        return Some(ExprNode::Unary {
            op: "-",
            l: Box::new(lower(&c.args[0].expr)?),
        });
    }
    if name == op::CONDITIONAL && c.args.len() == 3 {
        return Some(ExprNode::Cond {
            cond: Box::new(lower(&c.args[0].expr)?),
            then: Box::new(lower(&c.args[1].expr)?),
            otherwise: Box::new(lower(&c.args[2].expr)?),
        });
    }
    // `isSet(<static path>)` (mirrors `cel_resolve::is_profile_isset_call`).
    if name.eq_ignore_ascii_case("isSet") && c.args.len() == 1 {
        return Some(ExprNode::IsSet {
            is_set: select_path(&c.args[0].expr)?,
        });
    }
    None
}

/// Map a `cel_parser` synthetic operator `func_name` to its binary symbol, or
/// `None` when it is not an in-profile binary operator. `%` (modulo), the
/// optional operators, and index are deliberately excluded (dsl §8.4).
fn binary_symbol(name: &str) -> Option<&'static str> {
    use cel_parser::ast::operators as op;
    let sym = if name == op::EQUALS {
        "=="
    } else if name == op::NOT_EQUALS {
        "!="
    } else if name == op::LESS {
        "<"
    } else if name == op::LESS_EQUALS {
        "<="
    } else if name == op::GREATER {
        ">"
    } else if name == op::GREATER_EQUALS {
        ">="
    } else if name == op::ADD {
        "+"
    } else if name == op::SUBSTRACT {
        "-"
    } else if name == op::MULTIPLY {
        "*"
    } else if name == op::DIVIDE {
        "/"
    } else if name == op::LOGICAL_AND {
        "&&"
    } else if name == op::LOGICAL_OR {
        "||"
    } else if name == op::IN {
        "in"
    } else {
        return None;
    };
    Some(sym)
}

/// Lower a scalar literal. Every numeric literal (`Int`/`UInt`/`Double`)
/// collapses to an f64 double. `Null` and `Bytes` are outside the slot profile
/// and lower to `None`.
fn lower_literal(v: &Val) -> Option<ExprNode> {
    let lit = match v {
        Val::Int(i) => LitVal::Num(*i as f64),
        Val::UInt(u) => LitVal::Num(*u as f64),
        Val::Double(d) => LitVal::Num(*d),
        Val::String(s) => LitVal::Str(s.clone()),
        Val::Boolean(b) => LitVal::Bool(*b),
        Val::Bytes(_) | Val::Null => return None,
    };
    Some(ExprNode::Lit { lit })
}

/// Reconstruct the dotted path of a pure `Ident`/`Select` chain (`a.b.c`) —
/// mirrors `lute_check::cel_paths::select_path`. `None` if the chain bottoms out
/// in anything but a bare `Ident`.
fn select_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Select(sel) => {
            let base = select_path(&sel.operand.expr)?;
            Some(format!("{base}.{}", sel.field))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lowered(raw: &str) -> serde_json::Value {
        serde_json::to_value(lower_expr(raw).expect("expected a lowered expr")).unwrap()
    }

    #[test]
    fn binary_relational_with_parens() {
        // Parens are transparent in the CEL AST.
        assert_eq!(
            lowered("user.level >= (1)"),
            json!({"op": ">=", "l": {"path": "user.level"}, "r": {"lit": 1.0}})
        );
    }

    #[test]
    fn subject_equality_string() {
        // `$` is token-substituted to `_`.
        assert_eq!(
            lowered("$ == 'gold'"),
            json!({"op": "==", "l": {"path": "_"}, "r": {"lit": "gold"}})
        );
    }

    #[test]
    fn has_macro() {
        assert_eq!(lowered("has(scene.x)"), json!({"has": "scene.x"}));
    }

    #[test]
    fn subject_in_list() {
        assert_eq!(
            lowered("$ in ['a','b']"),
            json!({
                "op": "in",
                "l": {"path": "_"},
                "r": {"list": [{"lit": "a"}, {"lit": "b"}]}
            })
        );
    }

    #[test]
    fn not_isset() {
        assert_eq!(
            lowered("!isSet(scene.x)"),
            json!({"op": "!", "l": {"isSet": "scene.x"}})
        );
    }

    #[test]
    fn logical_and_paths() {
        assert_eq!(
            lowered("a && b"),
            json!({"op": "&&", "l": {"path": "a"}, "r": {"path": "b"}})
        );
    }

    #[test]
    fn malformed_is_none() {
        assert!(lower_expr("1 +").is_none());
    }

    #[test]
    fn empty_is_none() {
        assert!(lower_expr("").is_none());
        assert!(lower_expr("   ").is_none());
    }

    #[test]
    fn bool_and_numeric_literals() {
        assert_eq!(lowered("true"), json!({"lit": true}));
        assert_eq!(lowered("false"), json!({"lit": false}));
        // Integer literal collapses to an f64 double.
        assert_eq!(lowered("42"), json!({"lit": 42.0}));
    }

    #[test]
    fn negate_and_conditional() {
        assert_eq!(
            lowered("-run.hp"),
            json!({"op": "-", "l": {"path": "run.hp"}})
        );
        assert_eq!(
            lowered("$ ? 1 : 2"),
            json!({"cond": {"path": "_"}, "then": {"lit": 1.0}, "else": {"lit": 2.0}})
        );
    }

    #[test]
    fn isset_direct_and_null_out_of_profile() {
        assert_eq!(lowered("isSet(scene.x)"), json!({"isSet": "scene.x"}));
        // A bare `null` literal is out of profile → not lowerable.
        assert!(lower_expr("null").is_none());
    }

    #[test]
    fn out_of_profile_call_is_none() {
        // `size(x)` is not in the closed profile.
        assert!(lower_expr("size(run.items)").is_none());
    }

    /// Serialize the lowered arm expr; panics on any non-`Lowered(Some(_))`
    /// outcome so a missing/`None`/error case surfaces as a clear failure.
    fn arm_json(is: Option<&str>, test_raw: &str, subject_raw: &str) -> serde_json::Value {
        match synth_arm_expr(is, test_raw, subject_raw) {
            ArmExpr::Lowered(Some(node)) => serde_json::to_value(node).unwrap(),
            other => panic!("expected a lowered arm expr, got {other:?}"),
        }
    }

    #[test]
    fn is_string_member_inlines_subject_equality() {
        assert_eq!(
            arm_json(Some("gold"), "", "scene.serve.debut.rank"),
            json!({"op": "==", "l": {"path": "scene.serve.debut.rank"}, "r": {"lit": "gold"}})
        );
    }

    #[test]
    fn is_alternation_folds_with_or() {
        assert_eq!(
            arm_json(Some("silver|bronze"), "", "scene.serve.debut.rank"),
            json!({
                "op": "||",
                "l": {"op": "==", "l": {"path": "scene.serve.debut.rank"}, "r": {"lit": "silver"}},
                "r": {"op": "==", "l": {"path": "scene.serve.debut.rank"}, "r": {"lit": "bronze"}}
            })
        );
    }

    #[test]
    fn is_true_lowers_to_bool_equality() {
        assert_eq!(
            arm_json(Some("true"), "", "scene.flags.curious"),
            json!({"op": "==", "l": {"path": "scene.flags.curious"}, "r": {"lit": true}})
        );
    }

    #[test]
    fn is_number_lowers_to_f64_equality() {
        assert_eq!(
            arm_json(Some("3"), "", "run.rank"),
            json!({"op": "==", "l": {"path": "run.rank"}, "r": {"lit": 3.0}})
        );
    }

    #[test]
    fn is_unset_on_path_lowers_to_not_isset() {
        assert_eq!(
            arm_json(Some("unset"), "", "scene.choices.barConvo"),
            json!({"op": "!", "l": {"isSet": "scene.choices.barConvo"}})
        );
    }

    #[test]
    fn is_and_test_join_with_and() {
        assert_eq!(
            arm_json(Some("gold"), "run.coins > 0", "scene.serve.debut.rank"),
            json!({
                "op": "&&",
                "l": {"op": "==", "l": {"path": "scene.serve.debut.rank"}, "r": {"lit": "gold"}},
                "r": {"op": ">", "l": {"path": "run.coins"}, "r": {"lit": 0.0}}
            })
        );
    }

    #[test]
    fn test_only_arm_does_not_inline_subject() {
        // No `is`: the arm expr is exactly the lowered guard (subject not inlined).
        assert_eq!(
            arm_json(None, "run.coins > 0", "scene.serve.debut.rank"),
            json!({"op": ">", "l": {"path": "run.coins"}, "r": {"lit": 0.0}})
        );
    }

    #[test]
    fn is_unset_on_compound_subject_is_unlowerable() {
        // A non-path subject cannot back an `unset` isSet(path) — signal the
        // caller to emit a compile diagnostic; the arm's expr stays `None`.
        assert!(matches!(
            synth_arm_expr(Some("unset"), "", "scene.a == scene.b"),
            ArmExpr::UnsetOnCompoundSubject
        ));
    }
}
