//! `{{…}}` interpolation checks (dsl §7.6) and the `@ref`-looking content-line
//! warning.

use super::*;

/// Validate the `{{…}}` interpolation referents on a content line (dsl §7.6).
/// An interpolation is a state READ, so each referent gets the SAME cel-layer
/// treatment a `<when>` guard / `::set` RHS read gets: the referent is routed
/// through the shared [`check_cel_slot`] resolver, so a `Path` resolves against
/// the folded `state:` schema (`E-UNDECLARED`) and a `Ref` against `defs:`
/// (`E-UNDECLARED-REF`) exactly as a guard read does. A `Ref` additionally MUST
/// produce a **renderable** type (number/bool/enum, §7.6) — a declared def of any
/// other type is `E-REF-TYPE`. The reserved `userName` token always renders. The
/// definite-assignment half (`E-MAYBE-UNSET`, §9.4) is proven in the
/// path-sensitive `defassign` pass at the line's position — not here.
///
/// A free function (not a `Walker` method) so BOTH the scene walk and the
/// component-body walk (dsl §13) validate interps with their OWN `Env`: a scene's
/// `state:`/`defs:` or a component's `@param` namespace, respectively.
pub(super) fn check_interps(interps: &[Interp], ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    // An interpolation is content `Text`, never a `<match>` arm test, so `$` is
    // out of scope there (dsl §8.2). Resolve in a forced NON-match context so a
    // `{{$}}` (which the parser classifies as a `Path` raw `"$"`) fires
    // `E-DOLLAR-OUTSIDE-MATCH`, regardless of any enclosing arm ctx the caller
    // threads in.
    let interp_ctx = Ctx {
        env: ctx.env,
        in_match: false,
        match_subject: None,
    };
    for interp in interps {
        // §7.6 grammar: an interp is EXACTLY a bare state `Path`, a def `Ref`
        // (`@name` / `@name(args)`), or the reserved `userName` — NOT an arbitrary
        // CEL expression (the ONLY CEL admitted inside `{{…}}` is a `@fn(args)`
        // argument). Enforce the grammar BEFORE CEL-validating, so a profile-valid
        // but interp-illegal body like `{{run.coins + 1}}` is rejected rather than
        // silently accepted as a generic slot. The `$` subject (parser-classified
        // as a `Path` raw `"$"`) is exempt here — the resolver owns its scope
        // diagnostic (`E-DOLLAR-OUTSIDE-MATCH`, §8.2), so let it flow through.
        let referent = match interp.kind {
            // The reserved player-name token always renders (dsl §7.6); only
            // a format hint on it can be wrong (dsl 0.24.0 §4).
            InterpKind::Reserved => {
                check_interp_format(interp, ctx.env, false, diags);
                continue;
            }
            InterpKind::Path => {
                let has_dollar = scan_refs(&interp.raw).iter().any(|r| r.is_dollar);
                // dsl 0.27.0 §3: `{{user.bond[occasion.target]}}` reads the
                // raised member's family path, judged per member like a guard.
                let indexed = crate::cel_paths::occasion_indexed_family(&interp.raw).is_some();
                if !has_dollar && !indexed && !is_bare_state_path(&interp.raw) {
                    diags.push(interp_grammar_diag(&interp.raw, interp.span));
                    continue;
                }
                &interp.raw
            }
            InterpKind::Ref => {
                if !is_bare_ref(&interp.raw) {
                    diags.push(interp_grammar_diag(&interp.raw, interp.span));
                    continue;
                }
                &interp.raw
            }
        };
        check_interp_referent(referent, interp, &interp_ctx, false, diags);
    }
}

/// Validate ONE non-`Reserved` interpolation referent (`Path` or `Ref`, dsl
/// §7.6) whose grammar the caller already confirmed: parse `referent` as a
/// value-read CEL slot pinned to the interp's span, run [`check_cel_slot`]
/// (so a `Path` resolves against the caller's `ctx.env.state`/`defs` and a
/// `Ref` against `defs:`), then — for a `Ref` — the renderable-type check.
/// Shared by [`check_interps`] (scene bodies) and [`component_interp_scan`]'s
/// `Ref`/`$` branches (component bodies, dsl 0.4.0 §6.2): the two callers
/// differ ONLY in how an ordinary `Path` referent is treated before reaching
/// here — `check_interps` routes it here too (declared-`state:` lookup),
/// `component_interp_scan` never does (a component-body `Path` is always
/// ambient state, dsl §6.2) — except `{{$}}`, which stays on this shared path
/// everywhere so `E-DOLLAR-OUTSIDE-MATCH` fires uniformly.
///
/// `component_body` (dsl 0.23.0 §5): in a component body every `@name` in
/// scope is a param (`component_env`), and a bare `{{@p}}` of a `string`
/// param — or (dsl 0.27.0 §7) an entity-kind / named-enum param, as its
/// label — renders: expansion splices the `::use` site's literal into the
/// text (`check_use_interp_args` holds the caller to a literal).
pub(super) fn check_interp_referent(
    referent: &str,
    interp: &Interp,
    interp_ctx: &Ctx<'_>,
    component_body: bool,
    diags: &mut Vec<Diagnostic>,
) {
    // Reuse the guard/`::set` read-check verbatim: parse the referent as a
    // value-read CEL slot over the interpolation's span, then run the shared
    // resolver. `CelKind::AttrValue` is a non-guard read context — the
    // guard-only §9.6 `run.choiceLog` rule must not fire on a content
    // interpolation. A parse failure leaves `ast = None`, so the resolver
    // skips its AST pass and never double-reports malformed CEL.
    let mut arena = CelArena::default();
    let mut slot = CelSlot::raw(CelKind::AttrValue, referent.to_string(), interp.span);
    if let Ok(handle) = parse_slot(&mut arena, &slot.raw, interp.span.byte_start) {
        slot.ast = Some(handle);
    }
    // Every cel-layer diagnostic here pertains to THIS interpolation; pin its
    // span to the whole `{{…}}` (matching the resolver's own state-path
    // fallback) so ref/arity/undeclared spans stay consistent and never shift
    // to the leading `{` from the interior-relative `scan_refs` offsets.
    for mut d in check_cel_slot(&slot, &arena, interp_ctx, None) {
        d.span = interp.span;
        diags.push(d);
    }
    // §7.6 rendering: an interpolated `@ref` MUST resolve to a renderable
    // type (number/bool/enum). A DECLARED def whose produced type is known and
    // non-renderable is `E-REF-TYPE`; an undeclared ref already flagged
    // `E-UNDECLARED-REF` above (its name is absent from `def_types`, so this
    // never double-reports).
    let mut type_flagged = false;
    if interp.kind == InterpKind::Ref {
        if let Some(name) = scan_refs(referent)
            .into_iter()
            .find(|r| !r.is_dollar)
            .map(|r| r.name)
        {
            if let Some(ty) = interp_ctx.env.def_types.get(&name) {
                // dsl 0.27.0 §7: a `{ entity: K }` / `{ domain: K }` param
                // splices the same way, as its kind's label for the id.
                let string_param = component_body
                    && matches!(ty, Type::Str | Type::Domain(_) | Type::Entity(_))
                    && bare_param_ref(referent).as_deref() == Some(name.as_str());
                if !is_renderable(ty) && !string_param {
                    type_flagged = true;
                    diags.push(Diagnostic {
                        code: "E-REF-TYPE".to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "`@{name}` produces a non-renderable type; a `{{{{…}}}}` interpolation renders only number/bool/enum (dsl §7.6)"
                        ),
                        span: interp.span,
                        layer: Layer::Cel,
                        fixits: Vec::new(),
                        provenance: None,
                        covered: Vec::new(),
                        related: Vec::new(),
                    });
                }
            }
        }
    }
    check_interp_format(interp, interp_ctx.env, type_flagged, diags);
}

/// §7.6 renderable types for an interpolated `@ref`: a **number** (shortest
/// decimal), a **bool** (`true`/`false`), or an **enum** (member text). Any other
/// produced type cannot render inside `{{…}}` and is a static error.
pub(super) fn is_renderable(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Number | Type::Bool | Type::Enum(_) | Type::EnumFromOption(_)
    )
}

/// §7.6 interpolation-grammar violation: a `{{…}}` interior that is neither a
/// bare state path, a well-formed `@ref`/`@ref(args)`, nor `userName`. Reuses
/// [`E_CEL_PROFILE`] (the closed-CEL-surface code) — a bare CEL expression here
/// is CEL used where the profile does not admit it (§7.6: the only CEL inside
/// `{{…}}` is a `@fn(args)` argument).
pub(super) fn interp_grammar_diag(raw: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: crate::cel_resolve::E_CEL_PROFILE.to_string(),
        severity: Severity::Error,
        message: format!(
            "interpolation `{raw}` is not a valid `{{{{…}}}}` form — only a state path, a \
             family path indexed by the raised member (`user.bond[occasion.target]`), a def \
             `@ref` / `@ref(args)`, or `userName` are permitted; a bare CEL expression is not \
             (name a computed value with a `@def`, dsl §7.6)"
        ),
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// dsl 0.24.0 §4 / 0.25.0 §8 / 0.27.0 §7: validate an interpolation's format
/// hint ([`Interp::format`], the `:ordinal` of `{{user.deaths:ordinal}}`).
/// `ordinal`, `ordinalWord` and `plural(one|other)` are the hints
/// ([`lute_syntax::ast::INTERP_FORMATS`]); any other, a `plural` without
/// exactly two non-empty forms, and an ordinal with a `(…)` are
/// `E-CEL-PROFILE`, the §7.6 interpolation-grammar code
/// ([`interp_grammar_diag`]) — the hint is part of the `{{…}}` form. Each
/// formats a number, so a referent whose type is KNOWN and not a number — a
/// declared state path, a def's produced type, the reserved `userName`
/// string — is `E-REF-TYPE`, the interpolation rendering-type code. An
/// unresolved referent (already `E-UNDECLARED` / `E-UNDECLARED-REF`) and one
/// `type_flagged` as non-renderable already are not flagged again.
pub(super) fn check_interp_format(
    interp: &Interp,
    env: &crate::ctx::Env,
    type_flagged: bool,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(format) = interp.format.as_deref() else {
        return;
    };
    let raw = &interp.raw;
    let hint = interp.hint_text().unwrap_or_default();
    let plural = format == lute_syntax::ast::INTERP_FORMAT_PLURAL;
    let (code, message) = if !lute_syntax::ast::INTERP_FORMATS.contains(&format) {
        (
            crate::cel_resolve::E_CEL_PROFILE,
            format!(
                "`{{{{{raw}:{hint}}}}}` names an unknown format `{format}` — the \
                 interpolation formats are `:ordinal`, `:ordinalWord` and \
                 `:plural(one|other)` (dsl 0.27.0 §7)"
            ),
        )
    } else if plural
        && !matches!(&interp.forms, Some(f) if f.len() == 2 && f.iter().all(|f| !f.is_empty()))
    {
        (
            crate::cel_resolve::E_CEL_PROFILE,
            format!(
                "`{{{{{raw}:{hint}}}}}` needs a singular and a plural form, as \
                 `{{{{{raw}:plural(lantern|lanterns)}}}}` — the first is shown when the \
                 number is 1, the second otherwise, and `#` in a form is the number \
                 (dsl 0.27.0 §7)"
            ),
        )
    } else if !plural && interp.forms.is_some() {
        (
            crate::cel_resolve::E_CEL_PROFILE,
            format!("`:{format}` takes no `(…)` — write `{{{{{raw}:{format}}}}}` (dsl 0.24.0 §4)"),
        )
    } else {
        let ty = match interp.kind {
            InterpKind::Reserved => Some(Type::Str),
            InterpKind::Path => match crate::cel_paths::occasion_indexed_family(raw) {
                // Every member path of a family shares its declared type.
                Some(family) => env
                    .state
                    .decls
                    .iter()
                    .find(|(k, _)| {
                        k.strip_prefix(family)
                            .and_then(|m| m.strip_prefix('.'))
                            .is_some_and(|m| !m.contains('.'))
                    })
                    .map(|(_, d)| d.ty.clone()),
                None => crate::set_op::resolve_type(raw, &env.state).cloned(),
            },
            InterpKind::Ref => scan_refs(raw)
                .into_iter()
                .find(|r| !r.is_dollar)
                .and_then(|r| env.def_types.get(&r.name).cloned()),
        };
        match ty {
            Some(ty) if ty != Type::Number && !type_flagged => (
                "E-REF-TYPE",
                format!(
                    "`:{format}` formats a number, but `{raw}` is {} — write `{{{{{raw}}}}}` \
                     without the hint, or interpolate a number (dsl 0.24.0 §4)",
                    crate::cel_resolve::ty_desc(&ty)
                ),
            ),
            _ => return,
        }
    };
    diags.push(Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: interp.span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    });
}

/// `true` when `s` is a `CelIdent` (dsl §4.4): a leading `_`/ASCII-letter then
/// `_`/ASCII-alphanumerics. No `-` (CEL parses it as subtraction, §8.4). Empty
/// is not an ident.
pub(crate) fn is_cel_ident(s: &str) -> bool {
    let mut it = s.bytes();
    matches!(it.next(), Some(c) if c == b'_' || c.is_ascii_alphabetic())
        && it.all(|c| c == b'_' || c.is_ascii_alphanumeric())
}

/// `true` when `raw` (trimmed) is EXACTLY a bare dotted state path (dsl §7.6/§9.1):
/// a state-tier root (`scene`/`run`/`user`/`app`) followed by `.`-separated
/// `CelIdent` segments — no operators, whitespace, calls, or literals. `run.coins`
/// passes; `run.coins + 1`, `size(x)`, `foo.bar` (non-tier root) do not.
fn is_bare_state_path(raw: &str) -> bool {
    crate::cel_paths::is_state_path(raw) && raw.split('.').all(is_cel_ident)
}

/// `true` when `raw` (trimmed) is EXACTLY a `@name` or `@name(args)` reference
/// (dsl §7.6/§8.1): the OUTERMOST `@ref` starts at byte 0 and its bare/call group
/// reaches the end — nothing before or trailing. Nested `@ref`s inside a
/// `@fn(args)` argument are legitimate CEL (§8.1) and do not disqualify.
pub(super) fn is_bare_ref(raw: &str) -> bool {
    scan_refs(raw).iter().any(|r| {
        !r.is_dollar
            && r.span.byte_start == 0
            && match r.call.as_ref() {
                None => r.span.byte_end == raw.len(),
                Some(c) => c.span.byte_end == raw.len(),
            }
    })
}

/// `W-TEXT-LOOKS-LIKE-REF` (0.21.1 T3-7, lamplight F3): a content line whose
/// whole text is `@<name>` for a declared def or component param.
pub const W_TEXT_LOOKS_LIKE_REF: &str = "W-TEXT-LOOKS-LIKE-REF";

/// Text after `: ` is literal by design (dsl §7.6), so `@narrator: @memory`
/// ships the string "@memory" to the player. When the text is EXACTLY one
/// `@<ident>` and that ident is declared in this scope (a scene's `defs:`, a
/// component's `params:` — both live in `ctx.env.defs`), the author almost
/// certainly meant the interpolation. Anchored at the line text.
pub(super) fn text_looks_like_ref(l: &lute_syntax::ast::Line, ctx: &Ctx<'_>) -> Option<Diagnostic> {
    let name = l.text.trim().strip_prefix('@')?;
    let is_ident = name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !is_ident || !ctx.env.defs.contains(name) {
        return None;
    }
    Some(Diagnostic {
        code: W_TEXT_LOOKS_LIKE_REF.to_string(),
        severity: Severity::Warning,
        message: format!(
            "line text is exactly `@{name}`, which ships as the literal string \"@{name}\" — \
             line text is never evaluated; to show the value of `@{name}` write \
             `{{{{@{name}}}}}` (dsl §7.6)"
        ),
        span: l.text_span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    })
}
