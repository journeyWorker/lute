use super::*;
use super::{core::*, profile::*, state::*};
/// pattern to check).
pub(crate) fn check_fact_queries(expr: &Expr, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    match expr {
        Expr::Call(c) => {
            if is_fact_query_name(c) {
                check_fact_query_call(c, slot, ctx, diags);
                // `validAt`'s third argument is a genuine CEL expr (may itself
                // nest another fact query, e.g. `validAt('rel', ['a'], now())`).
                if c.func_name == "validAt" && is_profile_fact_query(c) {
                    if let Some(t) = c.args.get(2) {
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

/// The `E-MATCH-RELATION-SUBJECT` message (dsl 0.3.0 §8), with the fix a
/// writer can paste: a `<match>` with no `on` whose arms test the query
/// (ML-F4). `subject` is the `on` text; `via` names the `@def` that smuggled
/// the query in (dsl 0.27.0 §2, T1-5a); `query` is the text holding the query
/// itself (the subject, or its expansion through `via`). A number-valued
/// subject (`count(…)`, arithmetic) gets a comparison in its example arm.
pub(crate) fn match_relation_subject_message(subject: &str, via: Option<&str>, query: &str) -> String {
    use cel_parser::ast::operators as op;
    let mut arena = CelArena::default();
    let root = lute_cel::parse_slot_marked_refs(&mut arena, query).and_then(|h| arena.get(h));
    let func = root
        .as_ref()
        .and_then(|r| first_relation_query(&r.expr))
        .unwrap_or("holds");
    let numeric = root.is_some_and(|r| {
        matches!(&r.expr, Expr::Call(c) if [
            "count", "countDistinct", op::ADD, op::SUBSTRACT, op::MULTIPLY, op::DIVIDE,
            op::MODULO, op::NEGATE,
        ]
        .contains(&c.func_name.as_str()))
    });
    let subject = subject.trim();
    let lead = match via {
        Some(r) => format!("`@{r}` expands to a `{func}(…)` fact query"),
        None => format!("`{subject}` is a fact query"),
    };
    let test = if numeric {
        format!("{subject} >= 1")
    } else {
        subject.to_string()
    };
    format!(
        "{lead}, and a fact query is only ever a guard, never a `<match subject>` \
         (dsl 0.3.0 §8) — drop `subject` and test the query in each arm: `<match>` with arms \
         like `<when test=\"{test}\">` and an `<otherwise>`; a single line, choice or \
         `::set` takes the same test as its `when=\"…\"`"
    )
}

/// The first relation query (`holds`/`count`/…, not `now`) in `expr`, in
/// [`contains_relation_query`]'s walk order.
pub(crate) fn first_relation_query(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Call(c) if is_profile_fact_query(c) && c.func_name != "now" => {
            Some(c.func_name.as_str())
        }
        Expr::Call(c) => c
            .target
            .iter()
            .map(|t| &**t)
            .chain(c.args.iter())
            .find_map(|a| first_relation_query(&a.expr)),
        Expr::List(list) => list
            .elements
            .iter()
            .find_map(|e| first_relation_query(&e.expr)),
        Expr::Select(sel) => first_relation_query(&sel.operand.expr),
        Expr::Comprehension(_) | Expr::Map(_) | Expr::Struct(_)
        | Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => None,
    }
}

/// dsl 0.27.0 §2 (T1-5a): a `<match on>` subject whose `@def`s expand to a
/// fact query is [`E_MATCH_RELATION_SUBJECT`], exactly as the inline form is
/// — [`check_cel_slot`] sees only the unexpanded text. `None` when the subject
/// names no def, does not expand (another pass reports it), or already holds
/// a query inline (the inline firewall owns that one).
pub(crate) fn check_match_subject_defs(
    slot: &CelSlot,
    defs: &crate::cel_expand::DefTable<'_>,
) -> Option<Diagnostic> {
    let via = lute_cel::scan_refs(&slot.raw)
        .into_iter()
        .find(|r| !r.is_dollar && defs.bodies.contains_key(&r.name))?;
    let has_query = |text: &str| {
        let mut arena = CelArena::default();
        lute_cel::parse_slot_marked_refs(&mut arena, text)
            .and_then(|h| arena.get(h))
            .is_some_and(|root| contains_relation_query(&root.expr))
    };
    if has_query(&slot.raw) {
        return None;
    }
    let expanded = crate::cel_expand::expand_cel(&slot.raw, defs, None, &mut Vec::new()).ok()?;
    has_query(&expanded).then(|| {
        diag(
            E_MATCH_RELATION_SUBJECT,
            match_relation_subject_message(&slot.raw, Some(&via.name), &expanded),
            slot.span,
        )
    })
}

/// Whether `expr` contains a relation query (`holds`/`count`/`validAt`/…;
/// not `now()`), walked in [`check_fact_queries`]' recursion shape.
pub(crate) fn contains_relation_query(expr: &Expr) -> bool {
    first_relation_query(expr).is_some()
}

/// Validate one admitted fact-query `Call` (dsl 0.3.0 §6/§8): `now()` has no
/// pattern (admitted here, TYPED as narrative-time in Task 12 — nothing to
/// check yet); `holds`/`count`/`validAt` carry a relation pattern in
/// `args[0]` (guaranteed `Expr::Call` by [`is_profile_fact_query`]).
pub(crate) fn check_fact_query_call(
    c: &cel_parser::ast::CallExpr,
    slot: &CelSlot,
    ctx: &Ctx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = c.func_name.as_str();
    if name == "now" {
        return;
    }
    if !is_profile_fact_query(c) {
        diags.push(diag(
            "E-FACT-QUERY",
            format!("`{name}` has the wrong list-form fact-query arity"),
            slot.span,
        ));
        return;
    }
    // §8: relations are guard-only — a `<match on>` subject must stay
    // enum/bool/scalar so exhaustiveness analysis stays decidable. Flag and
    // skip pattern validation entirely (don't cascade unknown-relation/arity/
    // domain noise onto an already-illegal subject).
    if slot.kind == CelKind::MatchSubject {
        diags.push(diag(
            E_MATCH_RELATION_SUBJECT,
            match_relation_subject_message(&slot.raw, None, &slot.raw),
            slot.span,
        ));
        return;
    }
    let Some(relation) = query_relation(c) else {
        diags.push(diag(
            "E-FACT-QUERY",
            "fact-query relation name must be a string literal".to_string(),
            slot.span,
        ));
        return;
    };
    if lute_manifest::reserved::is_cel_word(relation) {
        diags.push(diag(
            "E-CEL-PARSE",
            format!(
                "`{relation}` is a reserved CEL name, so `{relation}(…)` cannot be queried as a \
                 relation — rename the relation (dsl 0.24 T3-8)"
            ),
            slot.span,
        ));
        return;
    }
    let Some(args) = pattern_terms(c) else {
        diags.push(diag(
            "E-FACT-QUERY",
            "fact-query lists take string or bool literals, \"_\", or occasion.target".to_string(),
            slot.span,
        ));
        return;
    };
    if name == "countDistinct" && count_distinct_column(c).is_none() {
        diags.push(diag(
            "E-FACT-QUERY",
            "countDistinct requires an in-range int column whose query position is \"_\"".to_string(),
            slot.span,
        ));
    }
    if name == "validAt" && is_profile_fact_query(c) {
        if let crate::set_type::Decision::Ty(ty) =
            crate::set_type::decide(&c.args[2].expr, &ctx.env.state, &ctx.env.def_types)
        {
            if !matches!(ty, Type::Int | Type::NarrativeTime) {
                diags.push(diag(
                    E_CEL_TYPE,
                    "validAt requires an int or narrative-time argument".to_string(),
                    slot.span,
                ));
            }
        }
    }
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

/// Adapt the host query's list to the existing relational closure checker.
pub(crate) fn pattern_terms(c: &cel_parser::ast::CallExpr) -> Option<Vec<FactArg>> {
    let Expr::List(list) = &c.args.get(1)?.expr else {
        return None;
    };
    list.elements.iter().map(|a| {
        let term = match &a.expr {
            Expr::Literal(Val::String(name)) if name == "_" => FactTerm::Wildcard,
            Expr::Literal(Val::String(name)) => FactTerm::Ident(name.clone()),
            Expr::Literal(Val::Boolean(b)) => FactTerm::Bool(*b),
            Expr::Ident(name) if name.starts_with(lute_cel::REF_MARKER) => {
                FactTerm::Param(name[lute_cel::REF_MARKER.len()..].to_string())
            }
            Expr::Call(call) if call.func_name.starts_with(lute_cel::REF_MARKER) => {
                FactTerm::Param(call.func_name[lute_cel::REF_MARKER.len()..].to_string())
            }
            e if crate::cel_paths::select_path(e).as_deref() == Some(lute_manifest::semantics::beats::OCCASION_TARGET) => {
                FactTerm::Target
            }
            _ => return None,
        };
        Some(FactArg { term, span: (0, 0) })
    }).collect()
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
pub(crate) fn is_profile_ident_root(name: &str) -> bool {
    crate::cel_paths::STATE_ROOTS.contains(&name)
        || name == "_"
        || name.starts_with(lute_cel::REF_MARKER)
}

/// True when the ORIGINAL slot text uses the reserved internal [`lute_cel::REF_MARKER`]
/// token as (part of) an identifier — i.e. the token appears OUTSIDE a string
/// literal. The marker is injected by the profile re-parse only at genuine `@`
/// sites; authored CEL must never contain it, else a hand-written
/// `__lute_at_ref__foo(...)` would parse to a marker-named `Call` and masquerade

const _: () = ();
