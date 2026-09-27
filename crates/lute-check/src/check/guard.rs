//! `when=` guard checks: scene-beat `when`, directive/write guards.

use super::*;

/// dsl 0.21.0 §3.1, §5: a scene beat's `when` — the `Bool` CEL-slot checks
/// and the fresh entry-guard definite-assignment check a `<quest start>` gets
/// (nothing dominates a guard evaluated before the scene runs), plus the one
/// beat-only rule: the scene's own `scene.*` state does not exist yet, so a
/// read of it is `E-BEAT-ATTR`. That rule is the root for such a path — the
/// `E-UNDECLARED` / `E-MAYBE-UNSET` it would otherwise also draw here are
/// dropped.
pub(super) fn check_beat_when(
    slot: &CelSlot,
    arena: &CelArena,
    ctx: &Ctx<'_>,
    scope: &crate::defassign::Scope<'_>,
) -> Vec<Diagnostic> {
    let mut diags = check_cel_slot(slot, arena, ctx, Some(&ExpectedType::Bool));
    diags.extend(check_quest_guard_defassign(slot, scope));
    let mut scene_paths: Vec<String> = slot
        .ast
        .clone()
        .and_then(|h| arena.get(h))
        .map(|root| {
            crate::cel_paths::collect_path_uses(&root.expr)
                .into_iter()
                .map(|u| u.path)
                .filter(|p| p == "scene" || p.starts_with("scene."))
                // A member named `scene`, already refused where it is
                // declared, is not a scene read to judge again.
                .filter(|p| !ctx.env.state.is_faulty(p))
                .collect()
        })
        .unwrap_or_default();
    scene_paths.sort();
    scene_paths.dedup();
    if !scene_paths.is_empty() {
        diags.retain(|d| {
            !(matches!(d.code.as_str(), "E-UNDECLARED" | "E-MAYBE-UNSET")
                && first_backtick_token(&d.message)
                    .is_some_and(|p| scene_paths.iter().any(|s| s == p)))
        });
        diags.extend(crate::beats::scene_when_scene_reads(&scene_paths, slot));
    }
    diags
}

/// dsl 0.26.0 §4: a write's or directive's `when=` guard — a `Bool`
/// condition with `$` out of scope (D9), exactly as `::set{… when=}`'s.
pub(super) fn check_guard(
    when: Option<&CelSlot>,
    arena: &CelArena,
    ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    let Some(when) = when else {
        return Vec::new();
    };
    let ctx_no_dollar = Ctx {
        env: ctx.env,
        in_match: false,
        match_subject: None,
    };
    check_cel_slot(when, arena, &ctx_no_dollar, Some(&ExpectedType::Bool))
}

/// dsl 0.26.0 §4: why directive `d` cannot carry a `when=` guard — `None`
/// for `::next`, `::use`, `::accept` and a plugin passthrough directive
/// (and an unknown one, `E-UNKNOWN-DIRECTIVE`'s). A builtin-lowered
/// directive (core staging, `::end`, `::mark`, a plugin `lower:` record or
/// builtin hook) runs unconditionally where it stands; a directive that
/// declares its own `when` attribute reads the guard as that attribute.
pub(crate) fn directive_when_refused(
    d: &Directive,
    snapshot: &CapabilitySnapshot,
) -> Option<String> {
    if matches!(d.tag.as_str(), "next" | "use") || d.is_accept() {
        return None;
    }
    let decl = snapshot.directive(&d.tag)?;
    if decl.attrs.iter().any(|a| a.name == "when") {
        return Some(format!(
            "`::{}` declares an attribute named `when`, but `when=` on a directive is its guard \
             (dsl 0.26.0 §4) — rename the attribute in the plugin",
            d.tag
        ));
    }
    (!decl.lower.is_passthrough()).then(|| {
        format!(
            "`::{}` cannot take `when=`: it lowers to a builtin record and runs where it stands — \
             put it in a `<match>`; `when=` guards `::use`, `::accept`, `::assert`, `::retract`, \
             `::set` and plugin passthrough directives (dsl 0.26.0 §4)",
            d.tag
        )
    })
}

/// dsl 0.26.0 §4: a directive's `when=` — refused where no guard applies
/// ([`directive_when_refused`], `E-UNKNOWN-ATTR` at the guard), else checked
/// as [`check_guard`].
pub(super) fn check_directive_when(
    d: &Directive,
    snapshot: &CapabilitySnapshot,
    arena: &CelArena,
    ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    let Some(when) = &d.when else {
        return Vec::new();
    };
    match directive_when_refused(d, snapshot) {
        Some(message) => vec![use_diag(
            crate::content_line::E_UNKNOWN_ATTR,
            message,
            when.span,
        )],
        None => check_guard(Some(when), arena, ctx),
    }
}
