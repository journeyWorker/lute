//! dsl 0.10.0 §3 (backlog #1, D-M): the `::set` right-hand side is typed
//! against the path it writes — `E-SET-TYPE`.
//!
//! [`crate::set_op`] owns the LEFT-hand side: write policy, declaredness and
//! the `AssignOp` op/type matrix. Its module doc states outright that it "does
//! NOT perform … RHS value-type compatibility" (`set_op.rs:27-29`). This
//! module is that missing half and nothing else.
//!
//! ## The tree
//!
//! The rules run over **`CelSlot.ast`** — the `cel_parser::ast::Expr` that
//! [`lute_cel::parse_slot`] fills for EVERY slot in a document, a `::set`'s
//! included (`lute-cel/src/fill.rs:53-54`), resolved through
//! [`lute_cel::CelArena::get`].
//!
//! It is **NOT** `lute-compile`'s `ExprNode` (`lute-compile/src/expr.rs:41`),
//! the crate's portable serialized expression AST. `ExprNode` carries no
//! `holds`/`count`/`now` node at all, and an unknown call there lowers to
//! `None` while a `None` child poisons its parent: implemented over it, rule 4
//! is unimplementable and rule 3 is lost for every expression containing a
//! fact query — two rules silently under-implemented with no failure to show
//! for it.
//!
//! ## Proof obligation, never a guess (D-M)
//!
//! An expression this module cannot decide is **accepted, with no
//! diagnostic**. A false `E-SET-TYPE` on a correct counter is strictly worse
//! than the silence §3 removes, because the author's only remedy would be to
//! stop declaring the type. The decidable set MAY grow in a later release; it
//! MUST never be widened by inference that can be wrong.

use cel_parser::ast::{operators as op, CallExpr, Expr, SelectExpr};
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::Set;

use crate::cel_paths::select_path;
use crate::cel_resolve::compatible;
use crate::ctx::ExpectedType;
use crate::meta::StateSchema;
use crate::set_op::resolve_type;

pub const E_SET_TYPE: &str = "E-SET-TYPE";

/// The outcome of deciding ONE expression's produced type (§3.3).
enum Decision {
    /// Decided: the expression produces this type.
    Ty(Type),
    /// Ill-typed on its own — rule 5 (a decidably non-`number` operand under
    /// `-`/`*`/`/`) or rule 6 (a `+` whose two decided sides disagree). The
    /// `String` describes the offending operand for the message.
    Ill(String),
    /// Outside the decidable set — accepted, no diagnostic.
    Undecidable,
}

/// dsl 0.10.0 §3: type one `::set`'s right-hand side against the declared type
/// of the path it writes. Returns an empty vec for everything §3 does not
/// decide.
pub(crate) fn check_set_type(
    set: &Set,
    arena: &CelArena,
    schema: &StateSchema,
) -> Vec<Diagnostic> {
    // §3.3 rule 8 plus its closing paragraph, in one gate. A WHOLE-slot `@ref`
    // already carries this position's expected type and a mismatch there is
    // the pre-existing `E-REF-TYPE` (`cel_resolve.rs:135-146`, driven by the
    // SAME `ExpectedType::Ty` the caller computes) — no construct grows a
    // second name for one fault. A `@ref` EMBEDDED in a compound expression is
    // undecidable. `$` is not a declared path either. And `parse_slot`
    // substitutes `@`->' ' and `$`->'_' before parsing
    // (`lute-cel/src/lib.rs:275-277`), so `slot.ast` cannot tell us which case
    // we are in: read the raw, and in all three cases say nothing.
    if !lute_cel::scan_refs(&set.expr.raw).is_empty() {
        return Vec::new();
    }
    // An undeclared target is `E-UNDECLARED`; `set_op::check_set` owns it.
    let Some(declared) = resolve_type(&set.path, schema) else {
        return Vec::new();
    };
    // §3.2: only the four author-declarable SCALAR types have a required type
    // under §3. `domain`/`providerRef`/`slotId`/`assetKind`/`enumFromOption`
    // are the five the author-`state:` guard falls THROUGH for (§14) and have
    // no row in §3.2's table; `list`/`record`/`map`/`narrativeTime` are
    // rejected at the declaration. A `::set` into any of them has no required
    // type and draws no `E-SET-TYPE`, whatever `E` produces.
    if !matches!(declared, Type::Bool | Type::Number | Type::Str | Type::Enum(_)) {
        return Vec::new();
    }
    // §3.1: `E-SET-TYPE` is SUPPRESSED wherever `E-SET-OP-TYPE` fires
    // (`set_op.rs:153`) — the target is wrong, and reporting the value against
    // a type the target does not have sends the author to the wrong end of the
    // line. With that suppression applied the table's two rows collapse: a
    // compound op survives only on a `number` target, whose required type is
    // `number` = `T`, and `=` requires `T`. The required type is ALWAYS `T`.
    if matches!(set.op.as_str(), "+=" | "-=" | "*=") && declared != &Type::Number {
        return Vec::new();
    }
    // A slot that did not parse is already `E-CEL-PARSE`'d once; never cascade.
    let Some(root) = set.expr.ast.clone().and_then(|h| arena.get(h)) else {
        return Vec::new();
    };
    match decide(&root.expr, schema) {
        Decision::Undecidable => Vec::new(),
        Decision::Ill(what) => vec![diag(
            format!(
                "`::set` writes an ill-typed expression into `{}`: {what} (dsl 0.10.0 §3)",
                set.path
            ),
            set.expr.span,
        )],
        Decision::Ty(produced) => {
            // §3.2's `enum` row: when `E` IS a string literal the member is in
            // hand, so it must be a declared one. Every other decided
            // right-hand side falls to `compatible` below, which admits the
            // whole string family — so an `enum`-typed READ satisfies a
            // required `string`, and an `enum: [a…]` -> `enum: [b…]` copy is
            // accepted because the checker cannot know which member the source
            // path holds at that point. That is D-M applied to its own table.
            if let (Type::Enum(members), Expr::Literal(Val::String(s))) = (declared, &root.expr) {
                if members.iter().any(|m| m == s.as_str()) {
                    return Vec::new();
                }
                let mut msg = format!(
                    "`::set` writes `\"{s}\"` into `{}`, which is not a member of its declared \
                     `enum: [{}]`",
                    set.path,
                    members.join(", ")
                );
                if let Some(sugg) = nearest_member(s, members) {
                    msg.push_str(&format!(" — did you mean `{sugg}`?"));
                }
                msg.push_str(" (dsl 0.10.0 §3)");
                return vec![diag(msg, set.expr.span)];
            }
            if compatible(&produced, &ExpectedType::Ty(declared.clone())) {
                return Vec::new();
            }
            vec![diag(
                format!(
                    "`::set` writes a `{}` into `{}`, declared `{}` (dsl 0.10.0 §3)",
                    scalar_name(&produced),
                    set.path,
                    scalar_name(declared)
                ),
                set.expr.span,
            )]
        }
    }
}

/// §3.3 rules 1–8 over one expression node. Total: every shape the rules do
/// not name is [`Decision::Undecidable`].
fn decide(expr: &Expr, schema: &StateSchema) -> Decision {
    match expr {
        // Rule 1: a literal.
        Expr::Literal(v) => match v {
            Val::Boolean(_) => Decision::Ty(Type::Bool),
            Val::Int(_) | Val::UInt(_) | Val::Double(_) => Decision::Ty(Type::Number),
            Val::String(_) => Decision::Ty(Type::Str),
            // CEL `null` is the DSL's `unset` sentinel (0.1 §11.2), not a
            // scalar; `Bytes` never appears in the closed Lute-CEL profile.
            Val::Null | Val::Bytes(_) => Decision::Undecidable,
        },
        // Rule 4's `has(p)`: a `Select` with `test: true`, NOT a path read.
        // Checked BEFORE rule 2, or `select_path` would swallow it.
        Expr::Select(SelectExpr { test: true, .. }) => Decision::Ty(Type::Bool),
        // Rule 2: a read of a declared state path. The substituted `$`
        // (`Ident("_")`) and any other bare ident resolve to nothing and fall
        // through to `Undecidable`.
        Expr::Ident(_) | Expr::Select(_) => {
            match select_path(expr).as_deref().and_then(|p| resolve_type(p, schema)) {
                Some(t) => Decision::Ty(t.clone()),
                None => Decision::Undecidable,
            }
        }
        Expr::Call(c) => decide_call(c, schema),
        // A list literal, a map/struct literal, a comprehension, an unset
        // node: §3.3's closing paragraph — undecidable, and it passes.
        _ => Decision::Undecidable,
    }
}

/// Rules 3–7. Operators are synthetic `Call`s in this AST, so they and the
/// profile functions dispatch through one `func_name` match.
fn decide_call(c: &CallExpr, schema: &StateSchema) -> Decision {
    match c.func_name.as_str() {
        // Rule 3: comparisons and logical operators produce `bool`.
        op::EQUALS
        | op::NOT_EQUALS
        | op::LESS
        | op::LESS_EQUALS
        | op::GREATER
        | op::GREATER_EQUALS
        | op::IN
        | op::LOGICAL_AND
        | op::LOGICAL_OR
        | op::LOGICAL_NOT => Decision::Ty(Type::Bool),
        // Rule 4: the fact-query / narrative-time profile calls. Matched by
        // EXACT name, as `cel_resolve::is_profile_fact_query` matches them.
        // `narrativeTime` is not an author-declarable state type, so a `now()`
        // right-hand side is `E-SET-TYPE` against every legal target — that is
        // how §3.2's `narrativeTime` clause is DERIVED from this closed rule
        // set rather than asserted beside it.
        "holds" => Decision::Ty(Type::Bool),
        "count" => Decision::Ty(Type::Number),
        "now" => Decision::Ty(Type::NarrativeTime),
        // Rule 5: `-` (binary and unary), `*` and `/` produce `number`, and
        // ANY operand whose type is decidable and is not `number` makes the
        // whole expression ill-typed, naming that operand.
        op::SUBSTRACT | op::MULTIPLY | op::DIVIDE | op::NEGATE => {
            for a in &c.args {
                match decide(&a.expr, schema) {
                    Decision::Ill(w) => return Decision::Ill(w),
                    Decision::Ty(t) if arith_rejects(&t) => {
                        return Decision::Ill(operand_desc(&a.expr, &t))
                    }
                    _ => {}
                }
            }
            Decision::Ty(Type::Number)
        }
        // Rule 6.
        op::ADD => decide_add(c, schema),
        // Rule 7: a conditional is the common type of its two branches when
        // both are decidable and equal; undecidable otherwise.
        op::CONDITIONAL => {
            let (Some(a), Some(b)) = (c.args.get(1), c.args.get(2)) else {
                return Decision::Undecidable;
            };
            match (decide(&a.expr, schema), decide(&b.expr, schema)) {
                (Decision::Ill(w), _) | (_, Decision::Ill(w)) => Decision::Ill(w),
                (Decision::Ty(x), Decision::Ty(y)) if x == y => Decision::Ty(x),
                _ => Decision::Undecidable,
            }
        }
        // Rule 4's `isSet(p)`: matched case-insensitively, exactly as
        // `cel_resolve::is_profile_isset_call` matches it, so the two passes
        // can never disagree about which calls are `isSet`.
        name if name.eq_ignore_ascii_case("isSet") => Decision::Ty(Type::Bool),
        _ => Decision::Undecidable,
    }
}

/// Rule 6: `+` is `number + number` or `string + string`. Two DECIDED sides
/// that disagree are ill-typed; anything else is undecidable.
fn decide_add(c: &CallExpr, schema: &StateSchema) -> Decision {
    let (Some(l), Some(r)) = (c.args.first(), c.args.get(1)) else {
        return Decision::Undecidable;
    };
    match (decide(&l.expr, schema), decide(&r.expr, schema)) {
        (Decision::Ill(w), _) | (_, Decision::Ill(w)) => Decision::Ill(w),
        (Decision::Ty(a), Decision::Ty(b)) => {
            if is_id_family(&a) || is_id_family(&b) {
                Decision::Undecidable
            } else if a == Type::Number && b == Type::Number {
                Decision::Ty(Type::Number)
            } else if is_string_family(&a) && is_string_family(&b) {
                Decision::Ty(Type::Str)
            } else {
                Decision::Ill(format!(
                    "a `{}` and a `{}` cannot be added",
                    scalar_name(&a),
                    scalar_name(&b)
                ))
            }
        }
        _ => Decision::Undecidable,
    }
}

/// Rule 5's operand test. `number` passes. The namespaced id family is treated
/// as UNDECIDABLE rather than ill-typed, mirroring `cel_resolve::compatible`'s
/// own `is_id_type` leniency so the checker's two type judgements can never
/// disagree. Everything else decidable and not a `number` is rejected.
fn arith_rejects(t: &Type) -> bool {
    !matches!(t, Type::Number) && !is_id_family(t)
}

/// The namespaced id types — value-level strings whose membership validity is
/// a separate concern (`cel_resolve.rs:748-753`).
fn is_id_family(t: &Type) -> bool {
    matches!(
        t,
        Type::ProviderRef(_) | Type::Domain(_) | Type::SlotId { .. } | Type::AssetKind(_)
    )
}

/// The mutually-compatible string family (`cel_resolve.rs:757-759`): an enum
/// value IS a string at the value level.
fn is_string_family(t: &Type) -> bool {
    matches!(t, Type::Str | Type::Enum(_) | Type::EnumFromOption(_))
}

/// Name the operand rule 5 rejects. A comparison is named by its authored
/// spelling — §3.3's own worked case wants the message to name the comparison,
/// not the whole write.
fn operand_desc(expr: &Expr, t: &Type) -> String {
    let what = match expr {
        Expr::Call(c) => match op_spelling(&c.func_name) {
            Some(sym) => format!("the `{sym}` comparison"),
            None => format!("the `{}(…)` call", c.func_name),
        },
        _ => match select_path(expr) {
            Some(p) => format!("`{p}`"),
            None => "an operand".to_string(),
        },
    };
    format!("{what} is a `{}`, not a `number`", scalar_name(t))
}

/// The authored spelling of a CEL synthetic operator name.
fn op_spelling(func_name: &str) -> Option<&'static str> {
    Some(match func_name {
        op::EQUALS => "==",
        op::NOT_EQUALS => "!=",
        op::LESS => "<",
        op::LESS_EQUALS => "<=",
        op::GREATER => ">",
        op::GREATER_EQUALS => ">=",
        op::IN => "in",
        op::LOGICAL_AND => "&&",
        op::LOGICAL_OR => "||",
        op::LOGICAL_NOT => "!",
        _ => return None,
    })
}

/// The nearest declared enum member to `got` within 2 edits — the same bound
/// and the same `min_by_key` tie-break `cel_paths::nearest_declared_path` uses
/// for `E-UNDECLARED`'s did-you-mean, so the two suggestions behave alike.
fn nearest_member<'m>(got: &str, members: &'m [String]) -> Option<&'m str> {
    lute_manifest::suggest::nearest(got, members.iter().map(|m| m.as_str()), 2)
}

/// The author-facing type name §3.2's table and §3.4's message use. Distinct
/// from `set_op::type_name`, which spells `Type::Str` as `str` for
/// `E-SET-OP-TYPE`; §3.4 quotes `string` verbatim and this diagnostic matches
/// the spec text rather than its neighbour.
fn scalar_name(t: &Type) -> &'static str {
    match t {
        Type::Bool => "bool",
        Type::Number => "number",
        Type::Str => "string",
        Type::Enum(_) | Type::EnumFromOption(_) => "enum",
        Type::NarrativeTime => "narrativeTime",
        Type::List(_) => "list",
        Type::Record(_) => "record",
        Type::Map { .. } => "map",
        Type::ProviderRef(_) => "providerRef",
        Type::Domain(_) => "domain",
        Type::SlotId { .. } => "slotId",
        Type::AssetKind(_) => "assetKind",
    }
}

/// `Layer::Staging` — `::set` is a staging directive (dsl §7.3.4), the same
/// layer `set_op.rs`'s own `diag` builds.
fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_SET_TYPE.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
