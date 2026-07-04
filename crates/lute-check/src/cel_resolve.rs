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

use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{CelKind, CelSlot};

use crate::cel_paths::collect_path_uses;
use crate::ctx::ExpectedType;
use crate::Ctx;
use lute_manifest::types::Type;

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

    // Pass 2: state-path reads from the AST. Skip when the slot did not parse
    // (already reported in Phase 3) so no cascade/duplicate errors fire.
    if let Some(handle) = slot.ast.clone() {
        if let Some(root) = arena.get(handle) {
            for use_ in collect_path_uses(&root.expr) {
                check_state_path(&use_.path, slot, ctx, &mut diags);
            }
        }
    }

    diags
}

/// Conservative type-compatibility for `E-REF-TYPE` (dsl §8): return `true` (no
/// flag) for everything not PROVABLY incompatible.
fn compatible(produced: &Type, expected: &ExpectedType) -> bool {
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
}
