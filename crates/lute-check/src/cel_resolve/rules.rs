use super::*;
use super::{core::*, profile::*, state::*};
/// `datalog_check::check_rules`.
pub fn check_rule_guards(vocab: &RelVocab, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for rule in &vocab.rules {
        let first = diags.len();
        for lit in &rule.rule.body {
            let BodyLiteral::Guard { cel, .. } = lit else {
                continue;
            };
            // dsl 0.24.0 §3: `run.approval[P]` reads the member bound to `P`;
            // validated here, then checked below as a member path.
            let cel =
                &match crate::rule_index::check_indexed_guard(&rule.rule, cel, vocab, rule.span) {
                    Ok(cel) => cel,
                    Err(ds) => {
                        diags.extend(ds);
                        continue;
                    }
                };
            // dsl 0.27.0 §3 (T2-2): `run.stalker == S` compares a domain-typed
            // path with the member bound to `S`.
            let cel = &match crate::rule_index::check_var_guard(
                &rule.rule,
                cel,
                vocab,
                &ctx.env.state,
                &ctx.env.domains,
                rule.span,
            ) {
                Ok(cel) => cel,
                Err(ds) => {
                    diags.extend(ds);
                    continue;
                }
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
            check_cel_profile(&root.expr, &slot, &ProfileScope::of(ctx), &mut diags);
            check_modulo_operands(&root.expr, rule.span, &ctx.env.state, &mut diags);
            // dsl 0.28.0 §1: a rule guard's comparisons are typed too.
            crate::cel_types::check_types(
                &root.expr,
                rule.span,
                true,
                &crate::cel_types::Typing::of(ctx, true),
                &mut diags,
            );
            // dsl 0.28.0 §1 (T1-5): and the literal-domain checks every
            // condition slot gets (the guard is already def-expanded).
            let (bodies, params) = (Default::default(), Default::default());
            diags.extend(crate::gates::condition_literals(
                cel,
                &crate::cel_expand::DefTable {
                    bodies: &bodies,
                    params: &params,
                },
                &ctx.env.state,
                None,
                rule.span,
            ));
        }
        // dsl 0.24 T3-6: an imported rule's guard problems at the schema line.
        if let Some(origin) = vocab.origins.rules.get(&rule.raw) {
            for d in &mut diags[first..] {
                *d = crate::rel_schema::at_origin(d.clone(), Some(origin));
            }
        }
    }
    diags
}

/// dsl 0.24 T1-1: a rule `cel("…")` guard whose `@def` / `@def(args)` cannot
/// be expanded — the def is undefined, takes a different number of args, or
/// expands through a cycle — or that uses the match subject `$`, which a rule
/// guard never has. Emitted at the rule's span.
pub const E_RULE_GUARD_DEF: &str = "E-RULE-GUARD-DEF";

/// Expand every `@def` / `@def(args)` in one rule-guard fragment against the
/// project's def table (dsl 0.24 T1-1). The output is `@`-free; an argument
/// naming a rule variable (`@open(L)`) is spliced as `(L)` and bound by the
/// evaluator exactly as an inline `L` is. A guard without refs is returned
/// verbatim. `Err` says why the guard cannot be expanded.
pub fn expand_rule_guard(
    cel: &str,
    defs: &crate::cel_expand::DefTable<'_>,
) -> Result<String, String> {
    for r in lute_cel::scan_refs(cel) {
        if r.is_dollar {
            return Err("`$` (a match subject) has no meaning in a rule guard".to_string());
        }
        let Some(params) = defs.bodies.get(&r.name).map(|_| defs.params.get(&r.name)) else {
            return Err(format!("`@{}` names no def", r.name));
        };
        let want = params.map_or(0, Vec::len);
        let got = r.call.as_ref().map_or(0, |c| c.args.len());
        if want != got {
            return Err(format!("`@{}` takes {want} arg(s), got {got}", r.name));
        }
    }
    crate::cel_expand::expand_cel(cel, defs, None, &mut Vec::new())
        .map_err(|error| error.to_string())
}

/// dsl 0.24 T1-1: rewrite every rule guard of `vocab` to its def-expanded
/// text, in place, BEFORE the vocabulary is frozen — so the checker's own
/// guard passes ([`check_rule_guards`]: the fact/time firewall, profile and
/// path checks), the compiled IR `rules` and trace's `Program::from_vocab`
/// all see one expanded body, and the runtime never evaluates an `@ref` it
/// cannot resolve. A guard that cannot be expanded keeps its text and is
/// [`E_RULE_GUARD_DEF`].
pub fn expand_rule_guards(
    vocab: &mut RelVocab,
    defs: &crate::cel_expand::DefTable<'_>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let RelVocab { rules, origins, .. } = vocab;
    for rule in rules.iter_mut() {
        for lit in &mut rule.rule.body {
            let BodyLiteral::Guard { cel, .. } = lit else {
                continue;
            };
            match expand_rule_guard(cel, defs) {
                Ok(expanded) => *cel = expanded,
                Err(why) => diags.push(crate::rel_schema::at_origin(
                    diag(
                        E_RULE_GUARD_DEF,
                        format!(
                            "rule `{}`: guard `cel(\"{cel}\")` cannot be expanded: {why}; a \
                             rule guard may use any def a condition may (dsl 0.24 T1-1)",
                            rule.raw
                        ),
                        rule.span,
                    ),
                    origins.rules.get(&rule.raw),
                )),
            }
        }
    }
    diags
}

/// 0.21.1 T1-6: the closed-profile gate over one def BODY (`defs: name: { cel }`).
/// A `@name` use site is exempt from the gate as a compile-time macro, and
/// nothing else ever looked at the body it expands to, so `size(x)` in a def
/// passed `check` while the same CEL inline was `E-CEL-PROFILE` — and the
/// runner, which cannot evaluate it, silently took `<otherwise>`. The body
/// now gets the SAME [`check_cel_profile`] walk (plus the reserved-marker,
/// [`W_QUEST_STATE_HAS`] and integer-`%` [`E_CEL_TYPE`] checks) an inline
/// slot gets, with the def's own
/// `params` admitted as bare identifiers. Every diagnostic lands at `span`
/// (the def's key) and names the def. A body that does not parse returns
/// nothing here — the caller reports it as `E-CEL-PARSE`.
pub(crate) fn check_def_body(
    name: &str,
    cel: &str,
    params: &[String],
    span: Span,
    schema: &crate::meta::StateSchema,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let slot = CelSlot::raw(CelKind::Condition, cel.to_string(), span);
    if raw_uses_reserved_marker(cel) {
        diags.push(diag(
            E_CEL_PROFILE,
            format!(
                "`{}` is a reserved internal token and must not appear in CEL (dsl §8.4)",
                lute_cel::REF_MARKER
            ),
            span,
        ));
    }
    let mut marked = CelArena::default();
    if let Some(root) =
        lute_cel::parse_slot_marked_refs(&mut marked, cel).and_then(|h| marked.get(h))
    {
        check_cel_profile(
            &root.expr,
            &slot,
            &ProfileScope {
                bound: params,
                relations: None,
                schema: Some(schema),
            },
            &mut diags,
        );
        check_quest_state_isset(&root.expr, span, &mut diags);
        check_modulo_operands(&root.expr, span, schema, &mut diags);
        // dsl 0.28.0 §1: a def body's comparisons are typed like an inline
        // slot's (the body may be any type, so only its parts are judged).
        crate::cel_types::check_types(
            &root.expr,
            span,
            false,
            &crate::cel_types::Typing {
                schema,
                defs: &crate::set_type::DefTypes::new(),
                dollar: None,
                clock: None,
                arithmetic: true,
            },
            &mut diags,
        );
    }
    for d in &mut diags {
        d.message = format!("def `{name}`: {}", d.message);
    }
    diags
}

/// 0.32.0: `quest.<id>.state` is an ALWAYS-ASSIGNED lifecycle enum —
/// `unset | active | complete | failed`, the engine writing `unset` for every
/// quest before it activates — so `has(quest.<id>.state)` is always true.
/// The warning names the comparison that means what the author wanted.
pub const W_QUEST_STATE_HAS: &str = "W-QUEST-STATE-HAS";

