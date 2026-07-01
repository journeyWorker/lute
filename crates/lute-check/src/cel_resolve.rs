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
use crate::Ctx;

/// Validate a single CEL slot's `@ref`, `$`, and state-path reads (dsl §8, §9.4,
/// §9.6). All diagnostics are [`Layer::Cel`].
pub fn check_cel_slot(slot: &CelSlot, arena: &CelArena, ctx: &Ctx) -> Vec<Diagnostic> {
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
        } else if !ctx.defs.contains(&r.name) {
            // `@name` must resolve to a declared `defs:` entry (dsl §8.1).
            // NOTE: `E-REF-TYPE` (type-context mismatch, dsl §8) is deferred: it
            // needs per-def type info that is not yet threaded into `Ctx`.
            diags.push(diag(
                "E-UNDECLARED-REF",
                format!("`@{}` is not a declared def (dsl §8.1)", r.name),
                span,
            ));
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

/// Classify one reconstructed state path and emit its diagnostic, if any.
fn check_state_path(path: &str, slot: &CelSlot, ctx: &Ctx, diags: &mut Vec<Diagnostic>) {
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
fn is_declared(path: &str, ctx: &Ctx) -> bool {
    ctx.state.decls.keys().any(|k| path == k || path.starts_with(&format!("{k}.")))
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
    use lute_syntax::ast::CelKind;
    use std::collections::BTreeSet;

    fn test_span() -> Span {
        Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }

    fn ctx_no_match() -> Ctx {
        Ctx::default()
    }

    fn ctx_with_defs(names: &[&str]) -> Ctx {
        Ctx {
            defs: names.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
            ..Ctx::default()
        }
    }

    fn ctx_in_match() -> Ctx {
        Ctx { in_match: true, ..Ctx::default() }
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
        let ctx = ctx_no_match();
        let slot = cel_slot_condition("$ == 'x'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx);
        assert!(errs.iter().any(|e| e.code == "E-DOLLAR-OUTSIDE-MATCH"));
    }

    #[test]
    fn undeclared_ref_errors() {
        let ctx = ctx_with_defs(&["fond"]);
        let slot = cel_slot_condition("@warm");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx);
        assert!(errs.iter().any(|e| e.code == "E-UNDECLARED-REF"));
    }

    #[test]
    fn choicelog_read_in_guard_errors() {
        let ctx = ctx_in_match();
        let slot = cel_slot_condition("run.choiceLog.ep02.couch == 'help'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx);
        assert!(errs.iter().any(|e| e.code == "E-CHOICELOG-READ"));
    }
}
