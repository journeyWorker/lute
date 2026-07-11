//! CEL slot resolution: `@ref` / `$` / state-path validation (dsl §8, §9.4, §9.6).
//!
//! Two independent passes feed one diagnostic list, forced apart by the
//! cel-parser 0.10.1 carry-forward (T3.1): a SUCCESSFUL CEL parse drops every
//! source position, so the stored AST is STRUCTURE-only.
//!
//! 1. **`@ref` / `$` (dsl §8)** — resolved from [`lute_cel::scan_refs`], which
//!    runs on the ORIGINAL `slot.raw` (pre-substitution) and returns precise
//!    byte spans. Token substitution rewrites `@fond`->`fond` and `$`->`_` in
//!    the AST, so the AST can NOT see these; only `scan_refs` can. Spans map into
//!    the document via `slot.span.byte_start`.
//! 2. **State-path reads (dsl §9.4/§9.6)** — reconstructed by walking the
//!    `IdedExpr`/`Expr` `Select`/`Ident` chains, whose idents ARE real (unaffected
//!    by substitution). Per the carry-forward, per-node offsets are unavailable,
//!    so their diagnostic span falls back to the whole-slot `slot.span`.
//!
//! If `slot.ast` is `None` (the CEL failed to parse — already reported in Phase
//! 3), the AST pass is SKIPPED so no cascade/duplicate errors fire; the
//! `scan_refs` pass still runs on the raw.

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{CelKind, CelSlot};
use lute_syntax::datalog::{BodyLiteral, FactArg, FactTerm};

use crate::cel_paths::collect_path_uses;
use crate::ctx::ExpectedType;
use crate::rel_schema::{check_atom, RelVocab};
use crate::Ctx;
use lute_manifest::types::Type;

/// A CEL construct outside the closed Lute-CEL profile (dsl §8.4): any function
/// or macro call other than the `isSet()` extension (the valid `has()` macro
/// parses as an `Expr::Select`, not a call), plus comprehension macros and
/// map/struct literals. Emitted at the slot span. New in 0.1.0.
pub const E_CEL_PROFILE: &str = "E-CEL-PROFILE";

/// dsl 0.3.0 §9.3 + D7: names whose read implies a hidden non-monotonic
/// dependency on the fact store or narrative time — banned inside a rule-body
/// CEL guard (`cel("...")` in a `rules:` entry). `now` is D7's deliberate
/// extension beyond the spec's `holds`/`count`/`validAt` — a guard has no
/// business reading the clock either.
const GUARD_FIREWALL_CALLS: &[&str] = &["holds", "count", "validAt", "now"];

/// `E-DATALOG-GUARD-FACT` (0.3.0, §7.2/§7.3 + D7): a rule-body guard reads
/// the fact store or narrative time via `holds`/`count`/`validAt`/`now`.
pub const E_DATALOG_GUARD_FACT: &str = "E-DATALOG-GUARD-FACT";

/// dsl 0.3.0 §6: `validAt` queried against a `derive:true` relation whose
/// rule closure carries a CEL guard in some feeding stratum (`guard_tainted`,
/// Task 9) — a derived fact's history is not reconstructible once a guard
/// makes membership depend on a scalar read, so a POINT-IN-TIME query over it
/// is ill-defined. `holds`/`count` stay fine on the SAME relation (they only
/// read "now", never history).
pub const E_VALIDAT_DERIVED: &str = "E-VALIDAT-DERIVED";

/// dsl 0.3.0 §8: a `<match on>` subject is a fact query (`holds`/`count`/
/// `validAt`). Relations are guard-only — a match subject must stay
/// enum/bool/scalar so exhaustiveness analysis (`match_check.rs`) stays
/// decidable.
pub const E_MATCH_RELATION_SUBJECT: &str = "E-MATCH-RELATION-SUBJECT";

/// Validate a single CEL slot's `@ref`, `$`, and state-path reads (dsl §8, §9.4,
/// §9.6). All diagnostics are [`Layer::Cel`].
pub fn check_cel_slot(
    slot: &CelSlot,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    expected: Option<&ExpectedType>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Pass 1: `@ref` / `$` from the raw source (spans are precise).
    for r in lute_cel::scan_refs(&slot.raw) {
        let span = map_span(slot, r.span);
        if r.is_dollar {
            // `$` (the match subject) is legal only inside a `<match>` (dsl §8.2).
            if !ctx.in_match {
                diags.push(diag(
                    "E-DOLLAR-OUTSIDE-MATCH",
                    "`$` (match subject) is only valid inside a `<match>` block".to_string(),
                    span,
                ));
            }
        } else if !ctx.env.defs.contains(&r.name) {
            // `@name` must resolve to a declared `defs:` entry (dsl §8.1).
            diags.push(diag(
                "E-UNDECLARED-REF",
                format!("`@{}` is not a declared def (dsl §8.1)", r.name),
                span,
            ));
        } else {
            // The name IS a declared def (dsl §8.1). Two independent checks run
            // here and may BOTH fire — neither suppresses the other:
            //   * arity (E-REF-ARITY): the `@name(args)` call MUST supply exactly
            //     as many arguments as the def declares params (a bare `@name` is
            //     0 args). Determinism is handled by the caller's final sort.
            if let Some(params) = ctx.env.def_params.get(&r.name) {
                let got = r.call.as_ref().map_or(0, |c| c.args.len());
                if got != params.len() {
                    diags.push(diag(
                        "E-REF-ARITY",
                        format!(
                            "`@{}` expects {} argument(s) but got {} (dsl §8.1)",
                            r.name,
                            params.len(),
                            got
                        ),
                        span,
                    ));
                }
            }
            //   * produced-type (E-REF-TYPE): only when the `@ref` IS the whole CEL
            //     value does the def's produced type equal the slot's value type.
            //     Two whole-slot forms (dsl §8.1): a bare `@name`, or the
            //     parameterized call `@name(args)` whose group consumes the
            //     remainder. In a compound expression (`@num > 0`, or
            //     `@toNum(x) == @toNum(y)`) the def types only a subexpression, so
            //     comparing to the slot's expected type would false-positive —
            //     treat it as non-whole and skip conservatively. `scan_refs` runs
            //     on `slot.raw`, so `r.span`/`r.call.span` byte offsets are relative
            //     to `slot.raw`; require the ref (and its call group, if any) to
            //     span the trimmed content exactly — nothing before or after it.
            if let (Some(expected), Some(produced)) = (expected, ctx.env.def_types.get(&r.name)) {
                let raw = &slot.raw;
                let content_start = raw.len() - raw.trim_start().len();
                let content_end = raw.trim_end().len();
                let is_whole_slot = r.span.byte_start == content_start
                    && match r.call.as_ref() {
                        None => r.span.byte_end == content_end, // bare `@name` reaches the end
                        Some(c) => c.span.byte_end == content_end, // `@name(...)` group reaches the end
                    };
                if is_whole_slot && !compatible(produced, expected) {
                    diags.push(diag(
                        "E-REF-TYPE",
                        format!(
                            "`@{}` produces {} but this position expects {} (dsl §8)",
                            r.name,
                            ty_desc(produced),
                            expected_desc(expected)
                        ),
                        span,
                    ));
                }
            }
            //   * per-argument type (E-REF-ARG-TYPE): when the `@name(args)`
            //     call is present AND its arity already matches the def's param
            //     count (a wrong arity is reported above — don't double-report
            //     on a mismatch), each positional arg whose static type IS
            //     resolvable must be compatible with the corresponding param's
            //     declared type. Conservative: unresolvable args (compound
            //     expressions, unknown paths) are silently skipped — only a
            //     PROVABLY-wrong arg flags, never a false positive.
            if let (Some(call), Some(params)) = (r.call.as_ref(), ctx.env.def_params.get(&r.name)) {
                if call.args.len() == params.len() {
                    for (arg_span, (_pname, pty)) in call.args.iter().zip(params.iter()) {
                        // `arg_span` byte offsets are relative to `slot.raw` (what
                        // `scan_refs` runs on) — index it directly; `map_span`
                        // offsets into the document like the `@ref` span.
                        let raw = &slot.raw[arg_span.byte_start..arg_span.byte_end];
                        if let Some(at) = resolve_arg_type(raw, ctx) {
                            if !compatible(&at, &ExpectedType::Ty(pty.clone())) {
                                diags.push(diag(
                                    "E-REF-ARG-TYPE",
                                    format!(
                                        "argument to `@{}` produces {} but the parameter expects {} (dsl §8.1)",
                                        r.name,
                                        ty_desc(&at),
                                        ty_desc(pty)
                                    ),
                                    map_span(slot, *arg_span),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    // Pass 2: state-path reads from the shared AST. Skip when the slot did not
    // parse (already reported in Phase 3) so no cascade/duplicate errors fire.
    if let Some(handle) = slot.ast.clone() {
        if let Some(root) = arena.get(handle) {
            for use_ in collect_path_uses(&root.expr) {
                check_state_path(&use_.path, slot, ctx, &mut diags);
            }
            // Pass 3: the Lute-CEL profile gate (dsl §8.4). A parameterized
            // `@ref(args)` and a same-named runtime call both collapse to an
            // identical `Call` under the shared AST's `@`->' ' substitution, so we
            // re-parse with `@` rewritten to `REF_MARKER`: a ref then carries a
            // marker-prefixed name and is distinguishable per site (structure-only
            // re-parse — all diagnostics use the slot span). Gated on `slot.ast`
            // so malformed CEL is not double-reported.
            // A hand-written identifier beginning with the reserved `REF_MARKER`
            // token would parse to a marker-named `Call` with no real `@` sigil and
            // masquerade as an exempt `@ref`. The token is reserved-internal and
            // must never appear in authored CEL, so its presence is itself out of
            // profile — flag once here so the walk below only ever sees markers the
            // re-parse injected at genuine `@` sites.
            if raw_uses_reserved_marker(&slot.raw) {
                diags.push(diag(
                    E_CEL_PROFILE,
                    format!(
                        "`{}` is a reserved internal token and must not appear in CEL (dsl §8.4)",
                        lute_cel::REF_MARKER
                    ),
                    slot.span,
                ));
            }
            let mut marked = CelArena::default();
            if let Some(mh) = lute_cel::parse_slot_marked_refs(&mut marked, &slot.raw) {
                if let Some(mroot) = marked.get(mh) {
                    check_cel_profile(&mroot.expr, slot, &mut diags);
                    // Vocabulary-aware fact-query pass (dsl 0.3.0 §6/§8, T11):
                    // `holds`/`count`/`validAt` patterns against `RelVocab`
                    // (E-RELATION-UNKNOWN/-ARITY/E-FACT-DOMAIN), the
                    // guard-tainted-derived `validAt` restriction
                    // (E-VALIDAT-DERIVED), and the match-subject firewall
                    // (E-MATCH-RELATION-SUBJECT). Runs on the SAME marker
                    // re-parse as the profile gate above — gated on `slot.ast`
                    // by the same outer `if`, so malformed CEL never cascades.
                    check_fact_queries(&mroot.expr, slot, ctx, &mut diags);
                    // Narrative-time ordering pass (dsl 0.3.0 §6, T12): a
                    // third INDEPENDENT pass over the SAME marker re-parse —
                    // `now()`/an engine-declared narrative-time anchor path
                    // may appear only as one side of an admitted ordering
                    // comparison against another narrative-time value, or as
                    // `validAt`'s second argument (`E-TEMPORAL-ARG`).
                    crate::temporal::check_temporal(&mroot.expr, slot, ctx, &mut diags);
                }
            }
        }
    }

    diags
}

/// Validate every rule guard's CEL (dsl 0.3.0 §7.2/§7.3, 0.3.0 T8): the
/// firewall (`holds`/`count`/`validAt`/`now` → [`E_DATALOG_GUARD_FACT`], D7)
/// plus the ordinary closed CEL profile ([`check_cel_profile`]) and
/// path-declaredness ([`collect_path_uses`] → `E-UNDECLARED`) checks against
/// the folded schema — all three passes run unconditionally over the SAME
/// parse, so more than one may fire for a single guard (matching
/// `check_cel_slot`'s own independent-pass discipline). Implemented here (not
/// `datalog_check.rs`) so `check_cel_profile`/`check_state_path` stay
/// private. Uses its own local [`CelArena`] per guard — a rule guard's raw
/// CEL text was never parsed by the document's normal `fill_document` pass
/// (it lives inside a Datalog rule string, Task 1's grammar), so this is its
/// first and only parse. A guard whose CEL fails to parse is silently
/// skipped — no evaluator, no cascade (matches `check_cel_slot`'s
/// `slot.ast: None` skip); the rule's own shape was already validated by
/// `datalog_check::check_rules`.
pub fn check_rule_guards(vocab: &RelVocab, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for rule in &vocab.rules {
        for lit in &rule.rule.body {
            let BodyLiteral::Guard { cel, .. } = lit else {
                continue;
            };
            let mut arena = CelArena::default();
            let Some(handle) = lute_cel::parse_slot_marked_refs(&mut arena, cel) else {
                continue;
            };
            let Some(root) = arena.get(handle) else {
                continue;
            };
            check_guard_fact_access(&root.expr, rule.span, &mut diags);
            let slot = CelSlot::raw(CelKind::Condition, cel.clone(), rule.span);
            for use_ in collect_path_uses(&root.expr) {
                check_state_path(&use_.path, &slot, ctx, &mut diags);
            }
            check_cel_profile(&root.expr, &slot, &mut diags);
        }
    }
    diags
}

/// The [`GUARD_FIREWALL_CALLS`] walk (D7): a `Call` (with or without a
/// receiver — matched by name alone) reaches [`E_DATALOG_GUARD_FACT`] and
/// stops descending (the whole call is rejected, mirroring
/// [`check_cel_profile`]'s own stop-on-reject shape); everything else
/// recurses the same way `check_cel_profile` does.
fn check_guard_fact_access(expr: &Expr, span: Span, diags: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call(c) => {
            let name = c.func_name.as_str();
            if GUARD_FIREWALL_CALLS.contains(&name) {
                diags.push(diag(
                    E_DATALOG_GUARD_FACT,
                    format!(
                        "`{name}(…)` reads the fact store or narrative time inside a rule guard; \
                         rules have no access to time or facts (dsl 0.3.0 §9.3, D7)"
                    ),
                    span,
                ));
                return;
            }
            if let Some(t) = &c.target {
                check_guard_fact_access(&t.expr, span, diags);
            }
            for a in &c.args {
                check_guard_fact_access(&a.expr, span, diags);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                check_guard_fact_access(&el.expr, span, diags);
            }
        }
        Expr::Select(sel) => check_guard_fact_access(&sel.operand.expr, span, diags),
        Expr::Comprehension(_)
        | Expr::Map(_)
        | Expr::Struct(_)
        | Expr::Ident(_)
        | Expr::Literal(_)
        | Expr::Unspecified => {}
    }
}

/// The Lute-CEL profile gate (dsl §8.4). The environment is **closed**: the only
/// callable function is the `isSet()` Lute extension. Everything else the profile
/// permits — a fixed set of CEL operators, literals, list literals, the `in`
/// membership operator, and the ternary conditional — is *not* a user-callable
/// function. Any other function/method call (`size`, `matches`, `startsWith`, …)
/// or comprehension macro (`map`, `filter`, `exists`, `all`, `existsOne`) is a
/// static error ([`E_CEL_PROFILE`]) at the slot span.
///
/// Runs over the **marker re-parse** (`parse_slot_marked_refs`), where each DSL
/// `@ref` sigil was rewritten to [`lute_cel::REF_MARKER`]. That distinction is
/// load-bearing:
/// * a compile-time `@name(args)` reference (dsl §8.1) parses to a `Call` whose
///   `func_name` starts with `REF_MARKER` — exempt, but we still recurse into its
///   args so a nested out-of-profile call is caught (`@pick(size(x))` flags
///   `size`). A same-named *runtime* call keeps its bare name and is NOT exempt,
///   closing the `@gate && gate(x)` bypass.
/// * CEL lowers operators to synthetic `Call` names ([`is_profile_operator`]),
///   matched against an EXPLICIT allow-list — `%` (`_%_`, not in §8.4) and
///   leading-dot global calls are therefore rejected, not blanket-accepted.
/// * the valid `has(path)` macro parses as a test-only [`Expr::Select`], never a
///   `Call` — so a residual `Call` named `has` (`has(x,y)`, `x.has()`) is NOT the
///   macro and IS rejected. The only allowed `Call` is `isSet(<path>)` with NO
///   receiver and exactly one arg ([`is_profile_isset_call`]).
/// * comprehension macros lower to [`Expr::Comprehension`]; map/struct literals
///   to [`Expr::Map`]/[`Expr::Struct`] (only *list* literals are in profile) —
///   all rejected.
/// * a `holds`/`count`/`validAt`/`now` fact-query/narrative-time call
///   ([`is_profile_fact_query`], dsl 0.3.0 §6/§8, T11) is exempt but does
///   NOT get the ordinary structural recursion: the pattern arg (`holds`/
///   `count`'s sole arg, `validAt`'s first arg) is a relation `Call`, not a
///   CEL sub-expression — its bare idents would otherwise trip
///   [`is_profile_ident_root`] below. Only `validAt`'s SECOND arg (a genuine
///   CEL expr, e.g. `now()`) is recursed into; the pattern itself is
///   validated by [`check_fact_queries`] instead.
fn check_cel_profile(expr: &Expr, slot: &CelSlot, diags: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call(c) => {
            let name = c.func_name.as_str();
            // Exempt: a compile-time `@ref(args)` macro (marker-prefixed name;
            // §8.1 owns its arity), a profile operator, a well-formed
            // `isSet(<path>)` call, or a well-shaped fact-query/`now()` call
            // (dsl 0.3.0 §6/§8). Anything else — including `scene.x.isSet()`
            // (receiver), `isSet(a, b)` (wrong arity), `holds()` (wrong
            // arity), and `holds(scene.x)` (non-call pattern arg) — is out of
            // profile.
            if name.starts_with(lute_cel::REF_MARKER)
                || is_profile_operator(name)
                || is_profile_isset_call(c)
                || is_profile_fact_query(c)
            {
                if is_profile_fact_query(c) {
                    // A fact-query/now() call: do NOT recurse into the
                    // pattern arg (args[0], a relation Call — validated by
                    // check_fact_queries, never a CEL sub-expression).
                    // `validAt`'s second arg IS a genuine CEL expr and gets
                    // the ordinary recursion.
                    if name == "validAt" {
                        if let Some(t) = c.args.get(1) {
                            check_cel_profile(&t.expr, slot, diags);
                        }
                    }
                } else {
                    // Structural — recurse into target + args to catch any
                    // nested out-of-profile call.
                    if let Some(t) = &c.target {
                        check_cel_profile(&t.expr, slot, diags);
                    }
                    for a in &c.args {
                        check_cel_profile(&a.expr, slot, diags);
                    }
                }
            } else {
                // An out-of-profile function/method call. Report and stop
                // descending (the whole call is rejected).
                diags.push(diag(
                    E_CEL_PROFILE,
                    format!(
                        "`{name}(…)` is outside the Lute-CEL profile — only operators, \
                         literals, lists, `?:`, `in`, `has()`, `isSet()`, `holds()`, \
                         `count()`, `validAt()`, and `now()` are permitted (dsl §8.4, \
                         0.3.0 §8)"
                    ),
                    slot.span,
                ));
            }
        }
        Expr::Comprehension(_) => diags.push(diag(
            E_CEL_PROFILE,
            "comprehension macros (map/filter/exists/all/existsOne) are outside the \
             Lute-CEL profile — only operators, literals, lists, `?:`, `in`, \
             `has()`, and `isSet()` are permitted (dsl §8.4)"
                .to_string(),
            slot.span,
        )),
        Expr::Map(_) | Expr::Struct(_) => diags.push(diag(
            E_CEL_PROFILE,
            "map/struct literals are outside the Lute-CEL profile — only list \
             literals are permitted (dsl §8.4)"
                .to_string(),
            slot.span,
        )),
        // List literals + the ternary/operator operands reach here as their child
        // exprs; recurse so a call nested inside a list is still caught.
        Expr::List(list) => {
            for el in &list.elements {
                check_cel_profile(&el.expr, slot, diags);
            }
        }
        // A field selection: recurse into the operand (`foo().bar` hides a call).
        Expr::Select(sel) => check_cel_profile(&sel.operand.expr, slot, diags),
        // A bare identifier is in profile ONLY as a legal expression root
        // ([`is_profile_ident_root`]): a state tier, the substituted `$` subject
        // `_`, or a marker-rewritten `@ref`. Every other bare name is a free
        // variable reference — there are no un-namespaced state names (dsl §9.1) —
        // and is out of profile. Scalar literals are in profile; `Unspecified` is
        // inert.
        Expr::Ident(name) => {
            if !is_profile_ident_root(name) {
                diags.push(diag(
                    E_CEL_PROFILE,
                    format!(
                        "`{name}` is not a state path (`scene`/`run`/`user`/`app`), the \
                         match subject `$`, or a def `@ref` — bare identifiers are \
                         outside the Lute-CEL profile (dsl §8.4, §9.1)"
                    ),
                    slot.span,
                ));
            }
        }
        Expr::Literal(_) | Expr::Unspecified => {}
    }
}

/// True when `func_name` is one of the CEL built-in **operators** the Lute-CEL
/// profile permits (dsl §8.4). cel-parser 0.10.1 lowers each operator to a fixed
/// synthetic name; we match that EXACT allow-list (via `cel_parser::ast::operators`
/// constants) so out-of-profile operators are NOT accepted just for being
/// punctuated. Deliberately EXCLUDED: modulo `_%_` (no integer domain, §8.4),
/// the optional operators `_[?_]`/`_?._`, and the internal `@not_strictly_false`.
fn is_profile_operator(func_name: &str) -> bool {
    use cel_parser::ast::operators as op;
    // The profile's operators: `? :`, `&& || !`, `+ - * /`, `== != >= <= > <`,
    // unary `-`, index `[]`, and `in`. EXCLUDES `_%_` (modulo), the optional
    // operators, and the internal `@not_strictly_false`.
    const ALLOWED: &[&str] = &[
        op::CONDITIONAL,
        op::LOGICAL_AND,
        op::LOGICAL_OR,
        op::LOGICAL_NOT,
        op::ADD,
        op::SUBSTRACT,
        op::MULTIPLY,
        op::DIVIDE,
        op::EQUALS,
        op::NOT_EQUALS,
        op::GREATER_EQUALS,
        op::LESS_EQUALS,
        op::GREATER,
        op::LESS,
        op::NEGATE,
        op::INDEX,
        op::IN,
    ];
    ALLOWED.contains(&func_name)
}

/// True when this `Call` is the in-profile `isSet(<path>)` extension (dsl §8.4):
/// named `isSet` (case-insensitively, as elsewhere), with NO receiver, EXACTLY
/// one argument, and that argument is a **static state path** — a pure
/// `Ident`/`Select` chain (`crate::cel_paths::select_path` returns `Some`, which
/// also admits the substituted `$` subject `Ident("_")`). `scene.x.isSet()`
/// (receiver), `isSet(a, b)` (arity), and `isSet(1 + 2)` / `isSet(scene.x + 1)`
/// (non-path argument) are all out of profile. `has()` is never here — its valid
/// form is an `Expr::Select`, not a `Call`.
fn is_profile_isset_call(c: &cel_parser::ast::CallExpr) -> bool {
    c.func_name.eq_ignore_ascii_case("isSet")
        && c.target.is_none()
        && c.args.len() == 1
        && crate::cel_paths::select_path(&c.args[0].expr).is_some()
}

/// True iff `c` is a structurally well-shaped fact-query/narrative-time call
/// (dsl 0.3.0 §6/§8): `holds(Call)` | `count(Call)` | `validAt(Call, expr)` |
/// `now()` — NO receiver, EXACT arity, and (for `holds`/`count`/`validAt`) a
/// `Call`-shaped first argument (the relation pattern — its OWN shape/
/// vocabulary validity is [`check_fact_queries`]'s job, never recursed into
/// here). Matched by EXACT name (mirrors [`GUARD_FIREWALL_CALLS`], not
/// [`is_profile_isset_call`]'s case-insensitive match). A malformed shape —
/// wrong arity, a non-`Call` pattern arg (`holds(scene.x)`), a receiver
/// (`x.holds(…)`), or an unrecognized name — is NOT admitted here and falls
/// into the ordinary [`E_CEL_PROFILE`] rejection.
/// `pub(crate)`: reused verbatim by `temporal.rs`'s own walk (Task 12) so the
/// two independent passes agree on exactly which calls exempt their pattern
/// argument from ordinary CEL-subexpression treatment.
pub(crate) fn is_profile_fact_query(c: &cel_parser::ast::CallExpr) -> bool {
    if c.target.is_some() {
        return false;
    }
    match c.func_name.as_str() {
        "holds" | "count" => c.args.len() == 1 && matches!(c.args[0].expr, Expr::Call(_)),
        "validAt" => c.args.len() == 2 && matches!(c.args[0].expr, Expr::Call(_)),
        "now" => c.args.is_empty(),
        _ => false,
    }
}

/// Vocabulary-aware fact-query pass (dsl 0.3.0 §6/§8, T11): validates every
/// `holds`/`count`/`validAt` pattern against `ctx.env.rel_vocab`. Mirrors
/// [`check_cel_profile`]'s own recursion shape (own recursion into `Call`
/// target/args, `List` elements, `Select` operand; leaves are inert) so a
/// fact query nested inside an operator call (`count(x) + 1 <= 3`) or a
/// list is still found. Runs on the SAME marker re-parse as the profile
/// gate — called from `check_cel_slot` right after `check_cel_profile`, so a
/// malformed (non-admitted) fact-query shape is already `E_CEL_PROFILE`-
/// flagged there and is left alone here (an ordinary `Call` with no relation
/// pattern to check).
fn check_fact_queries(expr: &Expr, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call(c) => {
            if is_profile_fact_query(c) {
                check_fact_query_call(c, slot, ctx, diags);
                // `validAt`'s second arg is a genuine CEL expr (may itself
                // nest another fact query, e.g. `validAt(rel(a), now())`).
                if c.func_name == "validAt" {
                    if let Some(t) = c.args.get(1) {
                        check_fact_queries(&t.expr, slot, ctx, diags);
                    }
                }
                return;
            }
            // Not a (well-shaped) fact query: an ordinary call — recurse into
            // target + args so a fact query nested inside an operator call
            // (`count(x) >= 1`, itself the synthetic `_>=_` Call) is found.
            if let Some(t) = &c.target {
                check_fact_queries(&t.expr, slot, ctx, diags);
            }
            for a in &c.args {
                check_fact_queries(&a.expr, slot, ctx, diags);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                check_fact_queries(&el.expr, slot, ctx, diags);
            }
        }
        Expr::Select(sel) => check_fact_queries(&sel.operand.expr, slot, ctx, diags),
        Expr::Comprehension(_)
        | Expr::Map(_)
        | Expr::Struct(_)
        | Expr::Ident(_)
        | Expr::Literal(_)
        | Expr::Unspecified => {}
    }
}

/// Validate one admitted fact-query `Call` (dsl 0.3.0 §6/§8): `now()` has no
/// pattern (admitted here, TYPED as narrative-time in Task 12 — nothing to
/// check yet); `holds`/`count`/`validAt` carry a relation pattern in
/// `args[0]` (guaranteed `Expr::Call` by [`is_profile_fact_query`]).
fn check_fact_query_call(
    c: &cel_parser::ast::CallExpr,
    slot: &CelSlot,
    ctx: &Ctx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = c.func_name.as_str();
    if name == "now" {
        return;
    }
    // §8: relations are guard-only — a `<match on>` subject must stay
    // enum/bool/scalar so exhaustiveness analysis stays decidable. Flag and
    // skip pattern validation entirely (don't cascade unknown-relation/arity/
    // domain noise onto an already-illegal subject).
    if slot.kind == CelKind::MatchSubject {
        diags.push(diag(
            E_MATCH_RELATION_SUBJECT,
            "relations are guard-only; a `<match on>` subject must stay \
             enum/bool/scalar so exhaustiveness stays decidable (dsl 0.3.0 §8)"
                .to_string(),
            slot.span,
        ));
        return;
    }
    let Expr::Call(pattern) = &c.args[0].expr else {
        unreachable!("is_profile_fact_query guarantees args[0] is a Call");
    };
    let relation = pattern.func_name.as_str();
    let Some(args) = pattern_terms(pattern) else {
        diags.push(diag(
            E_CEL_PROFILE,
            "fact-query patterns take compile-time-ground literals or `_` \
             (dsl 0.3.0 §5/§8)"
                .to_string(),
            slot.span,
        ));
        return;
    };
    let vocab: &RelVocab = &ctx.env.rel_vocab;
    // 0.3.0 T11 fix: `check_atom`'s `domains` parameter (the merged
    // plugin/core/project catalog vocabulary, A4) is threaded here from
    // `ctx.env.domains` — the SAME merged view `fold_env` computes and
    // `check_assert`/`check_retract`/`build_rel_vocab` already consult.
    // Previously this passed an empty map, so a relation arg declared
    // against a plugin/core/project *domain* (`build_rel_vocab`'s
    // `domains.contains_key(arg)` acceptance, rel_schema.rs) rather than a
    // RelVocab entity kind or `enums:` name silently skipped `E-FACT-DOMAIN`
    // membership checking inside a `holds`/`count`/`validAt` query pattern —
    // a soundness gap the seed/write paths never had.
    diags.extend(check_atom(
        vocab,
        &ctx.env.domains,
        relation,
        &args,
        /* wildcard_ok */ true,
        slot.span,
    ));
    // §6: `validAt` over a `derive:true` relation whose rule closure carries
    // a CEL guard in some feeding stratum is ill-defined — a guard makes
    // membership depend on a scalar read, and scalars keep no history.
    // `holds`/`count` stay fine on the SAME relation (they only read "now").
    if name == "validAt" {
        if let Some(decl) = vocab.relations.get(relation) {
            if decl.derive && vocab.guard_tainted.contains(relation) {
                diags.push(diag(
                    E_VALIDAT_DERIVED,
                    format!(
                        "`validAt` over derived relation `{relation}` is ill-defined — \
                         its rule closure carries a CEL guard, and scalars keep no \
                         history (dsl 0.3.0 §6)"
                    ),
                    slot.span,
                ));
            }
        }
    }
}

/// Convert a fact-query pattern `Call`'s args into [`FactArg`]s for
/// [`check_atom`] (dsl 0.3.0 §5/§8, the adapter T11's plan calls for):
/// `Ident("_")` (a literal wildcard, OR the substituted `$` match subject —
/// same token, same meaning here) -> [`FactTerm::Wildcard`]; any other
/// `Ident` NOT marker-prefixed -> [`FactTerm::Ident`]; a boolean `Literal`
/// -> [`FactTerm::Bool`]. Anything else — a path `Select`, arithmetic, a
/// nested `Call`, a non-bool literal, or a marker-prefixed `@ref` ident — is
/// NOT compile-time-ground; returns `None` for the WHOLE pattern (a single
/// non-ground arg invalidates it, per `Iterator::collect`'s `Option`
/// short-circuit). Spans are unavailable (cel-parser drops sub-expression
/// positions) so every [`FactArg::span`] is a `(0, 0)` placeholder —
/// `check_atom` never reads it, always reporting at the caller-supplied span.
fn pattern_terms(c: &cel_parser::ast::CallExpr) -> Option<Vec<FactArg>> {
    c.args
        .iter()
        .map(|a| match &a.expr {
            Expr::Ident(name) if name == "_" => Some(FactArg {
                term: FactTerm::Wildcard,
                span: (0, 0),
            }),
            Expr::Ident(name) if !name.starts_with(lute_cel::REF_MARKER) => Some(FactArg {
                term: FactTerm::Ident(name.clone()),
                span: (0, 0),
            }),
            Expr::Literal(Val::Boolean(b)) => Some(FactArg {
                term: FactTerm::Bool(*b),
                span: (0, 0),
            }),
            _ => None,
        })
        .collect()
}

/// The bare identifiers the Lute-CEL profile admits as an expression root (dsl
/// §8.4, §9.1). Everything else is a free variable reference and is out of
/// profile — there are no bare, un-namespaced state names (§9.1):
/// * a **state-tier** root (`scene`/`run`/`user`/`app`) — the head of a declared
///   state path (`crate::cel_paths::STATE_ROOTS`);
/// * the substituted `$` **match subject**, which token substitution rewrites to
///   `Ident("_")` — its `<match>`-scope validity is a separate `scan_refs`
///   concern (`E-DOLLAR-OUTSIDE-MATCH`), so the gate never flags it here;
/// * a `@ref` — the marker re-parse rewrites a bare `@name` (no call) to an
///   `Ident` whose name starts with [`lute_cel::REF_MARKER`] (a `@name(args)`
///   becomes a `Call`, handled in the `Call` arm). Both are §8.1 compile-time
///   macros and exempt.
fn is_profile_ident_root(name: &str) -> bool {
    crate::cel_paths::STATE_ROOTS.contains(&name)
        || name == "_"
        || name.starts_with(lute_cel::REF_MARKER)
}

/// True when the ORIGINAL slot text uses the reserved internal [`lute_cel::REF_MARKER`]
/// token as (part of) an identifier — i.e. the token appears OUTSIDE a string
/// literal. The marker is injected by the profile re-parse only at genuine `@`
/// sites; authored CEL must never contain it, else a hand-written
/// `__lute_at_ref__foo(...)` would parse to a marker-named `Call` and masquerade
/// as an exempt `@ref`. Its presence is therefore itself out of profile (§8.4).
fn raw_uses_reserved_marker(raw: &str) -> bool {
    let marker = lute_cel::REF_MARKER.as_bytes();
    if raw.len() < marker.len() {
        return false;
    }
    let mask = lute_cel::cel_string_mask(raw);
    raw.as_bytes()
        .windows(marker.len())
        .enumerate()
        .any(|(i, w)| w == marker && !mask[i])
}

/// Conservative type-compatibility for `E-REF-TYPE` (dsl §8): return `true` (no
/// flag) for everything not PROVABLY incompatible.
pub(crate) fn compatible(produced: &Type, expected: &ExpectedType) -> bool {
    match expected {
        ExpectedType::Bool => matches!(produced, Type::Bool),
        ExpectedType::Ty(t) => {
            if is_id_type(produced) || is_id_type(t) {
                return true; // id types: always compatible (never flag)
            }
            if is_string_family(produced) && is_string_family(t) {
                return true; // {Str, Enum, EnumFromOption} mutually compatible
            }
            produced == t // structural equality (Type: PartialEq)
        }
    }
}

/// Namespaced/provider id types — value-level strings whose membership validity
/// is a separate concern; always treated as compatible.
fn is_id_type(t: &Type) -> bool {
    matches!(
        t,
        Type::ProviderRef(_) | Type::Domain(_) | Type::SlotId { .. } | Type::AssetKind(_)
    )
}

/// The mutually-compatible string family: an enum value is a string at the
/// value level, and def CEL produces string-ish values.
fn is_string_family(t: &Type) -> bool {
    matches!(t, Type::Str | Type::Enum(_) | Type::EnumFromOption(_))
}

/// Short human label for a produced [`Type`] in an `E-REF-TYPE` message.
fn ty_desc(t: &Type) -> String {
    match t {
        Type::Bool => "a bool".to_string(),
        Type::Number => "a number".to_string(),
        Type::Str => "a string".to_string(),
        Type::Enum(_) | Type::EnumFromOption(_) => "an enum".to_string(),
        Type::List(_) => "a list".to_string(),
        Type::Record(_) => "a record".to_string(),
        Type::Map { .. } => "a map".to_string(),
        Type::ProviderRef(_) => "a provider ref".to_string(),
        Type::Domain(_) => "a domain ref".to_string(),
        Type::SlotId { .. } => "a slot id".to_string(),
        Type::AssetKind(_) => "an asset kind".to_string(),
        Type::NarrativeTime => "a narrative-time value".to_string(),
    }
}

/// Short human label for an [`ExpectedType`] in an `E-REF-TYPE` message.
fn expected_desc(e: &ExpectedType) -> String {
    match e {
        ExpectedType::Bool => "a bool".to_string(),
        ExpectedType::Ty(t) => ty_desc(t),
    }
}

/// Best-effort static type of a single call argument's raw source (dsl §8.1
/// `@name(args)`). Returns `None` (skip — never flag) for anything not trivially
/// typeable, keeping `E-REF-ARG-TYPE` conservative (no false positives).
fn resolve_arg_type(arg_raw: &str, ctx: &Ctx<'_>) -> Option<Type> {
    let a = arg_raw.trim();
    if a == "true" || a == "false" {
        return Some(Type::Bool);
    }
    if a.parse::<f64>().is_ok() {
        return Some(Type::Number);
    }
    if (a.starts_with('\'') && a.ends_with('\'') && a.len() >= 2)
        || (a.starts_with('"') && a.ends_with('"') && a.len() >= 2)
    {
        return Some(Type::Str);
    }
    if let Some(name) = a.strip_prefix('@') {
        // a nested bare `@ref` (no call) -> its produced type
        if name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return ctx.env.def_types.get(name).cloned();
        }
    }
    // a bare, resolvable state path
    crate::set_op::resolve_type(a, &ctx.env.state).cloned()
}

/// Classify one reconstructed state path and emit its diagnostic, if any.
fn check_state_path(path: &str, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    // Reserved `run.choiceLog.*` read inside a guard/condition (dsl §9.6).
    if is_guard(slot.kind) && (path == "run.choiceLog" || path.starts_with("run.choiceLog.")) {
        diags.push(diag(
            "E-CHOICELOG-READ",
            format!("`{path}` is reserved and cannot be read in a guard/condition (dsl §9.6)"),
            slot.span,
        ));
        return;
    }
    // Otherwise the path must be declared in the inline `state:` schema (dsl §9.4).
    if !is_declared(path, ctx) {
        diags.push(diag(
            "E-UNDECLARED",
            format!("state path `{path}` is not declared in `state:` (dsl §9.4)"),
            slot.span,
        ));
    }
}

/// A path is declared when it exactly matches a `state:` key or is a descendant
/// field of one (`scene.player` declared => `scene.player.hp` reads are ok).
fn is_declared(path: &str, ctx: &Ctx<'_>) -> bool {
    ctx.env
        .state
        .decls
        .keys()
        .any(|k| path == k || path.starts_with(&format!("{k}.")))
}

/// Guard/condition slots (dsl §9.6): a `<match>` subject or any boolean guard.
fn is_guard(kind: CelKind) -> bool {
    matches!(kind, CelKind::Condition | CelKind::MatchSubject)
}

/// Map a `scan_refs` byte span (relative to `slot.raw`) into the document by
/// offsetting with the slot's start byte. Line/column/utf16 stay zeroed: the
/// caller's `TextIndex` recomputes them at report time (matching `scan_refs`).
fn map_span(slot: &CelSlot, local: Span) -> Span {
    let base = slot.span.byte_start;
    Span {
        byte_start: base + local.byte_start,
        byte_end: base + local.byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Build a `Layer::Cel` error diagnostic.
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Env;
    use lute_syntax::ast::CelKind;
    use std::collections::BTreeSet;

    fn test_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn env_with_defs(names: &[&str]) -> Env {
        Env {
            defs: names.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
            ..Env::default()
        }
    }

    fn env_with_def(name: &str, ty: Type) -> Env {
        Env {
            defs: std::iter::once(name.to_string()).collect(),
            def_types: std::iter::once((name.to_string(), ty)).collect(),
            ..Env::default()
        }
    }

    fn mk_ctx(env: &Env) -> Ctx<'_> {
        Ctx {
            env,
            in_match: false,
            match_subject: None,
        }
    }

    fn mk_ctx_in_match(env: &Env) -> Ctx<'_> {
        Ctx {
            env,
            in_match: true,
            match_subject: None,
        }
    }

    /// Build a `Condition` slot and parse it into a fresh arena so `ast` is `Some`.
    fn cel_slot_condition(raw: &str) -> CelSlot {
        let mut slot = CelSlot::raw(CelKind::Condition, raw.to_string(), test_span());
        let mut arena = CelArena::default();
        if let Ok(h) = lute_cel::parse_slot(&mut arena, &slot.raw, slot.span.byte_start) {
            slot.ast = Some(h);
        }
        slot
    }

    /// Re-parse the slot's raw into a fresh arena, reproducing the same handle
    /// index the slot recorded (each parse into an empty arena yields handle 0).
    fn arena_for(slot: &CelSlot) -> CelArena {
        let mut arena = CelArena::default();
        let _ = lute_cel::parse_slot(&mut arena, &slot.raw, slot.span.byte_start);
        arena
    }

    #[test]
    fn dollar_outside_match_errors() {
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("$ == 'x'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(errs.iter().any(|e| e.code == "E-DOLLAR-OUTSIDE-MATCH"));
    }

    #[test]
    fn undeclared_ref_errors() {
        let env = env_with_defs(&["fond"]);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@warm");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(errs.iter().any(|e| e.code == "E-UNDECLARED-REF"));
    }

    #[test]
    fn choicelog_read_in_guard_errors() {
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        let slot = cel_slot_condition("run.choiceLog.ep02.couch == 'help'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(errs.iter().any(|e| e.code == "E-CHOICELOG-READ"));
    }

    #[test]
    fn ref_type_mismatch_flags() {
        let env = env_with_def("num", Type::Number);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@num"); // referenced in a bool position
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_compatible_is_clean() {
        let env = env_with_def("flag", Type::Bool);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@flag");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_unknown_expected_no_false_positive() {
        let env = env_with_def("num", Type::Number);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@num");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None); // expected unknown
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_string_family_clean() {
        // def produces Str used where an Enum is expected -> string family -> no flag
        let env = env_with_def("s", Type::Str);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@s");
        let d = check_cel_slot(
            &slot,
            &arena_for(&slot),
            &ctx,
            Some(&ExpectedType::Ty(Type::Enum(vec!["a".into(), "b".into()]))),
        );
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_id_type_clean() {
        // expected an id type -> always compatible
        let env = env_with_def("n", Type::Number);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@n");
        let d = check_cel_slot(
            &slot,
            &arena_for(&slot),
            &ctx,
            Some(&ExpectedType::Ty(Type::ProviderRef("prov".into()))),
        );
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_undeclared_ref_no_reftype() {
        // name not in ctx.defs -> E-UNDECLARED-REF, NOT E-REF-TYPE (no double report)
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@ghost");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(d.iter().any(|x| x.code == "E-UNDECLARED-REF"));
        assert!(!d.iter().any(|x| x.code == "E-REF-TYPE"));
    }

    #[test]
    fn ref_type_compound_expr_no_false_positive() {
        // `@num > 0` in a bool slot: @num (Number) types a numeric subexpression;
        // the whole expression is boolean -> must NOT flag E-REF-TYPE.
        let env = env_with_def("num", Type::Number);
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("@num > 0");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
        assert!(
            !d.iter().any(|x| x.code == "E-REF-TYPE"),
            "compound expression must not flag E-REF-TYPE; got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn out_of_profile_call_rejected() {
        // dsl §8.4: the Lute-CEL environment is CLOSED — only operators/literals/
        // lists/`?:`/`in`/`has()`/`isSet()` are allowed. Any other function call
        // or comprehension macro is `E-CEL-PROFILE`.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for raw in ["size(scene.x) > 0", "[1, 2].exists(x, x > 0)", "matches(a, b)"] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "expected E-CEL-PROFILE for `{raw}`, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn in_profile_exprs_pass() {
        // The closed set never trips the gate: `has`/`isSet`, `in`, arithmetic +
        // comparison operators, and the ternary conditional.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for ok in [
            "has(scene.x)",
            "isSet(run.y)",
            "$ in ['a', 'b']",
            "scene.n + 1 > 2",
            // the ternary conditional operator itself is in profile; operands are
            // state paths / literals (bare idents are NOT in profile — see
            // `bare_ident_rejected`).
            "scene.n > 0 ? run.a : run.b",
        ] {
            let slot = cel_slot_condition(ok);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "unexpected E-CEL-PROFILE for `{ok}`, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn bare_ident_rejected() {
        // dsl §8.4/§9.1: a bare identifier that is not a state-tier root, the `$`
        // subject, or a `@ref` is a free variable reference — out of profile.
        // `when="typo"` and a non-state-rooted `isSet(foo.bar)` both flag.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for raw in ["typo", "isSet(foo.bar)", "foo.bar", "a && b"] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "bare identifier `{raw}` must flag E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn legal_ident_roots_pass() {
        // The legal roots never trip the gate: state paths (any tier), the `$`
        // subject, a `has()` guard, and CEL keyword literals.
        let env = Env::default();
        let ctx = mk_ctx_in_match(&env);
        for ok in [
            "scene.x == 1",
            "run.y",
            "user.z || app.w",
            "$ == 'gold'",
            "has(scene.x)",
            "true",
            "false ? 1 : null",
        ] {
            let slot = cel_slot_condition(ok);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "legal root `{ok}` must not trip E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn def_ref_call_form_not_flagged() {
        // A parameterized def reference `@name(args)` is a COMPILE-TIME macro
        // invocation (dsl §8.1), not a runtime CEL function call. The marker
        // re-parse gives it a `REF_MARKER`-prefixed name so it is exempt — while a
        // same-named runtime call is not (see `ref_call_not_shadowed_by_at_ref`).
        for (env, raw) in [
            (env_with_defs(&["atLeast"]), "@atLeast(2)"),
            (Env::default(), "@ghost(1)"), // undeclared ref -> E-UNDECLARED-REF only
            (env_with_defs(&["pick"]), "@pick(scene.n, 3) > 0"),
        ] {
            let ctx = mk_ctx(&env);
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "def-ref call `{raw}` must not trip E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn ref_call_not_shadowed_by_at_ref() {
        // Bypass (1): a real runtime call must NOT be exempted just because a
        // same-named `@ref` appears in the slot. `@gate` is a bare ref (Ident);
        // `gate(scene.x)` is a genuine out-of-profile call -> E-CEL-PROFILE.
        let env = env_with_defs(&["gate"]);
        let ctx = mk_ctx(&env);
        for raw in ["@gate && gate(scene.x)", "@gate(1) && gate(2)"] {
            let slot = cel_slot_condition(raw);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "runtime `gate(...)` must flag E-CEL-PROFILE despite `@gate` in `{raw}`, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn has_call_form_rejected_but_macro_ok() {
        // Bypass (2): the valid `has(path)` macro parses as an `Expr::Select`, so
        // any residual `Call` named `has` (wrong arity / receiver form) is NOT the
        // macro and must flag; the real macro must stay clean.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        for bad in ["has(a, b)", "scene.x.has()"] {
            let slot = cel_slot_condition(bad);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "non-macro `has` call `{bad}` must flag E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
        let slot = cel_slot_condition("has(scene.x)");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().all(|e| e.code != E_CEL_PROFILE),
            "valid has() macro must stay clean, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn modulo_operator_rejected_arithmetic_ok() {
        // Bypass (3): `%` lowers to `_%_`, which is NOT in the §8.4 operator set
        // (no integer domain) -> E-CEL-PROFILE; `+ - * /` stay in profile.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let slot = cel_slot_condition("scene.a % 2");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().any(|e| e.code == E_CEL_PROFILE),
            "modulo `%` must flag E-CEL-PROFILE, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
        for ok in ["scene.a + 1", "scene.a - 1 * 2 / 3"] {
            let slot = cel_slot_condition(ok);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().all(|e| e.code != E_CEL_PROFILE),
                "arithmetic `{ok}` must stay in profile, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn reserved_marker_in_raw_rejected() {
        // A hand-written identifier beginning with the reserved marker token must
        // not masquerade as an exempt `@ref` — it is itself out of profile.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        let raw = format!("{}x + 1", lute_cel::REF_MARKER);
        let slot = cel_slot_condition(&raw);
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().any(|e| e.code == E_CEL_PROFILE),
            "reserved-marker identifier `{raw}` must flag E-CEL-PROFILE, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn isset_arity_and_receiver_enforced() {
        // The `isSet(<path>)` extension takes exactly one arg, no receiver, and
        // that arg MUST be a static state path — a receiver, wrong arity, or a
        // non-path argument is NOT the extension -> E-CEL-PROFILE.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        for bad in [
            "scene.x.isSet()",
            "isSet(a, b)",
            "isSet(1 + 2)",
            "isSet(scene.x + 1)",
        ] {
            let slot = cel_slot_condition(bad);
            let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
            assert!(
                d.iter().any(|e| e.code == E_CEL_PROFILE),
                "malformed isSet `{bad}` must flag E-CEL-PROFILE, got {:?}",
                d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
            );
        }
        let slot = cel_slot_condition("isSet(run.y)");
        let d = check_cel_slot(&slot, &arena_for(&slot), &ctx, None);
        assert!(
            d.iter().all(|e| e.code != E_CEL_PROFILE),
            "well-formed isSet(run.y) must stay clean, got {:?}",
            d.iter().map(|x| x.code.clone()).collect::<Vec<_>>()
        );
    }
}
