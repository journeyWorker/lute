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
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{CelKind, CelSlot};

use crate::cel_paths::collect_path_uses;
use crate::ctx::ExpectedType;
use crate::Ctx;
use lute_manifest::types::Type;

/// A CEL construct outside the closed Lute-CEL profile (dsl §8.4): any function
/// or macro call other than the `isSet()` extension (the valid `has()` macro
/// parses as an `Expr::Select`, not a call), plus comprehension macros and
/// map/struct literals. Emitted at the slot span. New in 0.1.0.
pub const E_CEL_PROFILE: &str = "E-CEL-PROFILE";

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
                }
            }
        }
    }

    diags
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
fn check_cel_profile(expr: &Expr, slot: &CelSlot, diags: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call(c) => {
            let name = c.func_name.as_str();
            // Exempt: a compile-time `@ref(args)` macro (marker-prefixed name;
            // §8.1 owns its arity), a profile operator, or a well-formed
            // `isSet(<path>)` call. Anything else — including `scene.x.isSet()`
            // (receiver) and `isSet(a, b)` (wrong arity) — is out of profile.
            if name.starts_with(lute_cel::REF_MARKER)
                || is_profile_operator(name)
                || is_profile_isset_call(c)
            {
                // Structural — recurse into target + args to catch any nested
                // out-of-profile call.
                if let Some(t) = &c.target {
                    check_cel_profile(&t.expr, slot, diags);
                }
                for a in &c.args {
                    check_cel_profile(&a.expr, slot, diags);
                }
            } else {
                // An out-of-profile function/method call. Report and stop
                // descending (the whole call is rejected).
                diags.push(diag(
                    E_CEL_PROFILE,
                    format!(
                        "`{name}(…)` is outside the Lute-CEL profile — only operators, \
                         literals, lists, `?:`, `in`, `has()`, and `isSet()` are \
                         permitted (dsl §8.4)"
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
        // Idents and scalar literals are in profile; `Unspecified` is inert.
        Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => {}
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

/// True when this `Call` is the in-profile `isSet(<path>)` extension: named
/// `isSet` (case-insensitively, as elsewhere), with NO receiver and EXACTLY one
/// argument (the single state path). `scene.x.isSet()` (a receiver) and
/// `isSet(a, b)` (wrong arity) are NOT the extension and are out of profile.
/// `has()` is never here — its valid form is an `Expr::Select`, not a `Call`.
fn is_profile_isset_call(c: &cel_parser::ast::CallExpr) -> bool {
    c.func_name.eq_ignore_ascii_case("isSet") && c.target.is_none() && c.args.len() == 1
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
        Type::ProviderRef(_) | Type::SlotId { .. } | Type::AssetKind(_)
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
        Type::SlotId { .. } => "a slot id".to_string(),
        Type::AssetKind(_) => "an asset kind".to_string(),
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
            "a ? b : c",
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
        // The `isSet(<path>)` extension takes exactly one arg and no receiver.
        // A receiver form or wrong arity is NOT the extension -> E-CEL-PROFILE.
        let env = Env::default();
        let ctx = mk_ctx(&env);
        for bad in ["scene.x.isSet()", "isSet(a, b)"] {
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
