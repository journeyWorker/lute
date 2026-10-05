use super::{profile::*, facts::*, state::*};
//
// Two independent passes feed one diagnostic list, forced apart by the
// cel-parser 0.10.1 carry-forward (T3.1): a SUCCESSFUL CEL parse drops every
// source position, so the stored AST is STRUCTURE-only.
//
// 1. **`@ref` / `$` (dsl §8)** — resolved from [`lute_cel::scan_refs`], which
//    runs on the ORIGINAL `slot.raw` (pre-substitution) and returns precise
//    byte spans. Token substitution rewrites `@fond`->`fond` and `$`->`_` in
//    the AST, so the AST can NOT see these; only `scan_refs` can. Spans map into
//    the document via `slot.span.byte_start`.
// 2. **State-path reads (dsl §9.4/§9.6)** — reconstructed by walking the
//    `IdedExpr`/`Expr` `Select`/`Ident` chains, whose idents ARE real (unaffected
//    by substitution). Per the carry-forward, per-node offsets are unavailable,
//    so their diagnostic span falls back to the whole-slot `slot.span`.
//
// If `slot.ast` is `None` (the CEL failed to parse — already reported in Phase
// 3), the AST pass is SKIPPED so no cascade/duplicate errors fire; the
// `scan_refs` pass still runs on the raw.

use cel_parser::ast::Expr;
use lute_core_span::{Diagnostic, Span};
use lute_cel::CelArena;
use lute_syntax::ast::{CelKind, CelSlot};

use crate::cel_paths::collect_path_uses;
use crate::ctx::ExpectedType;
use crate::Ctx;
use lute_manifest::types::Type;

/// A CEL construct outside the closed Lute-CEL profile (dsl §8.4): any function
/// or macro call other than the supported list-form host calls (`holds`, `count`,
/// `validAt`, `now`, `visited`) and the `has()` presence macro, plus
/// comprehension macros and map/struct literals. Emitted at the slot span.
pub const E_CEL_PROFILE: &str = "E-CEL-PROFILE";

/// dsl 0.32.0 §1: an operand of integer `%` that is not an integer — a
/// non-numeric operand (`'a' % 2`, `run.flag % 2`) or a fractional literal
/// (`run.day % 2.5`). An `int` path is accepted: whether its value is integral
/// is a runtime question (`%` of a fractional `double` value is unknown).
/// Emitted at the slot span.
pub const E_CEL_TYPE: &str = "E-CEL-TYPE";

/// dsl 0.3.0 §9.3 + D7: names whose read implies a hidden non-monotonic
/// dependency on the fact store or narrative time — banned inside a rule-body
/// CEL guard (`cel("...")` in a `rules:` entry). `now` is D7's deliberate
/// extension beyond the spec's `holds`/`count`/`validAt` — a guard has no
/// business reading the clock either.
pub(crate) const GUARD_FIREWALL_CALLS: &[&str] = &["holds", "count", "countDistinct", "validAt", "now"];

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
            // A `$name` (Yarn's variable sigil) in a parsed slot is the CEL
            // profile's `$oil` error (pass 2), which names the path; it is
            // not also a stray subject.
            let sigil = slot.raw[r.span.byte_end..]
                .starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
            if !ctx.in_match && !(sigil && slot.ast.is_some()) {
                diags.push(diag(
                    "E-DOLLAR-OUTSIDE-MATCH",
                    "`$` (match subject) is only valid inside a `<match>` block".to_string(),
                    span,
                ));
            }
        } else if !ctx.env.defs.contains(&r.name) {
            // `@name` must resolve to a declared `defs:` entry (dsl §8.1).
            let hint = lute_manifest::suggest::nearest(
                &r.name,
                ctx.env.defs.iter().map(String::as_str),
                2,
            )
            .map_or_else(String::new, |near| format!(" — did you mean `@{near}`?"));
            diags.push(diag(
                "E-UNDECLARED-REF",
                format!("`@{}` is not a declared def{hint} (dsl §8.1)", r.name),
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

    // A condition that is one bare state path of another type (`live:
    // "user.day"`, `rearm="user.loginStreak"`) is never true: the same
    // `E-REF-TYPE` a whole-slot `@def` of that type gets (G-5).
    if let Some(ExpectedType::Bool) = expected {
        let path = slot.raw.trim();
        let bare = path.contains('.')
            && path.split('.').all(|seg| {
                seg.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                    && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
        if let Some(decl) = ctx.env.state.decls.get(path).filter(|_| bare) {
            if !compatible(&decl.ty, &ExpectedType::Bool) {
                let example = match &decl.ty {
                    Type::Int | Type::Double => format!("{path} > 0"),
                    Type::Str => format!("{path} != ''"),
                    Type::Enum(members) if !members.is_empty() => {
                        format!("{path} == '{}'", members[0])
                    }
                    _ => format!("{path} == '…'"),
                };
                diags.push(diag(
                    "E-REF-TYPE",
                    format!(
                        "`{path}` is {} but this position expects a bool — compare it (for \
                         example `{example}`) (dsl §8)",
                        ty_desc(&decl.ty)
                    ),
                    slot.span,
                ));
            }
        }
    }

    // dsl 0.27.0 §3: a slot reading `occasion.target` as a fact-query
    // argument or family index is judged once per member of its kind beat
    // (`holds('owned', ['aria'])`, `user.bond.aria`, …). Outside any kind beat
    // the read is `E-UNDECLARED` (a lore document's stray read is already
    // `check_occasion_target_scope`'s).
    if crate::occasion_bind::binds_target(&slot.raw) {
        match ctx
            .env
            .occasion_scopes
            .members_at(slot.span.byte_start, slot.span.byte_end)
        {
            Some(members) => diags.extend(check_per_member(slot, ctx, members)),
            None if ctx
                .env
                .state
                .decls
                .contains_key(crate::beats::OCCASION_TARGET) => {}
            None => diags.push(diag(
                "E-UNDECLARED",
                crate::beats::occasion_target_scope_message(),
                slot.span,
            )),
        }
        return diags;
    }

    // Pass 2: state-path reads from the shared AST. Skip when the slot did not
    // parse (already reported in Phase 3) so no cascade/duplicate errors fire.
    if let Some(handle) = slot.ast.clone() {
        if let Some(root) = arena.get(handle) {
            check_parsed_slot(&root.expr, slot, ctx, &mut diags);
        }
    }

    diags
}

/// [`check_cel_slot`]'s passes over the parsed slot `expr`: state-path
/// reads, the CEL profile, fact queries, narrative time, `%` operands.
pub(crate) fn check_parsed_slot(expr: &Expr, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    // A name that is not an identifier after a `.` (`quest.zero-coke-001
    // .state`) parses as a subtraction; report the path once, with its
    // quoted-index spelling, not the findings the subtraction trips.
    let glued = crate::cel_paths::glued_state_paths(&slot.raw);
    if !glued.is_empty() {
        for g in glued {
            let at = Span {
                byte_start: g.start,
                byte_end: g.end,
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            };
            diags.push(diag(
                crate::cel_paths::E_PATH_IDENT,
                g.message(&slot.raw),
                map_span(slot, at),
            ));
        }
        return;
    }
    for use_ in collect_path_uses(expr) {
        check_state_path(&use_.path, slot, ctx, diags);
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
            check_cel_profile(&mroot.expr, slot, &ProfileScope::of(ctx), diags);
            // Vocabulary-aware fact-query pass (dsl 0.3.0 §6/§8, T11):
            // `holds`/`count`/`validAt` patterns against `RelVocab`
            // (E-RELATION-UNKNOWN/-ARITY/E-FACT-DOMAIN), the
            // guard-tainted-derived `validAt` restriction
            // (E-VALIDAT-DERIVED), and the match-subject firewall
            // (E-MATCH-RELATION-SUBJECT). Runs on the SAME marker
            // re-parse as the profile gate above — gated on `slot.ast`
            // by the same outer `if`, so malformed CEL never cascades.
            check_fact_queries(&mroot.expr, slot, ctx, diags);
            // Narrative-time ordering pass (dsl 0.3.0 §6, T12): a
            // third INDEPENDENT pass over the SAME marker re-parse —
            // `now()`/an engine-declared narrative-time anchor path
            // may appear only as one side of an admitted ordering
            // comparison against another narrative-time value, or as
            // `validAt`'s second argument (`E-TEMPORAL-ARG`).
            crate::temporal::check_temporal(&mroot.expr, slot, ctx, diags);
            check_quest_state_isset(&mroot.expr, slot.span, diags);
            check_modulo_operands(&mroot.expr, slot.span, &ctx.env.state, diags);
            // dsl 0.28.0 §1 (T1-4): comparisons, logical operands and the
            // condition itself are typed.
            crate::cel_types::check_types(
                &mroot.expr,
                slot.span,
                slot.kind == CelKind::Condition,
                &crate::cel_types::Typing::of(ctx, slot.kind != CelKind::SetExpr),
                diags,
            );
        }
    }
}

/// dsl 0.27.0 §3: [`check_parsed_slot`] over `slot` instantiated for each
/// member ([`crate::occasion_bind::instantiate_bound`]). A finding every
/// member shares is reported once. So is one that several members hit and
/// that differs only by the member's name (the swapped arguments of
/// `holds('bondRank', ['r1', occasion.target])` fail for every hero alike):
/// it reads `occasion.target` where the member stood and lists the members.
/// A finding only one member hits names it (`… (for occasion.target = bram)`),
/// since the beat is still raised for the others.
pub(crate) fn check_per_member(slot: &CelSlot, ctx: &Ctx<'_>, members: &[String]) -> Vec<Diagnostic> {
    group_per_member(members, |m| {
        let raw = crate::occasion_bind::instantiate_bound(&slot.raw, m);
        let inst = CelSlot::raw(slot.kind, raw, slot.span);
        let mut arena = CelArena::default();
        let mut ds = Vec::new();
        if let Ok(h) = lute_cel::parse_slot(&mut arena, &inst.raw, 0) {
            if let Some(root) = arena.get(h) {
                check_parsed_slot(&root.expr, &inst, ctx, &mut ds);
            }
        }
        ds
    })
}

/// dsl 0.27.0 §3: `judge` run once for each of `members` (a slot, write or
/// argument reading `occasion.target`, instantiated for that member), its
/// findings reported as [`check_per_member`] describes: shared ones once,
/// the others naming the members that hit them.
pub(crate) fn group_per_member(
    members: &[String],
    mut judge: impl FnMut(&str) -> Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let per: Vec<(&str, Vec<Diagnostic>)> =
        members.iter().map(|m| (m.as_str(), judge(m))).collect();
    let key = |d: &Diagnostic| (d.code.clone(), d.message.clone());
    // Each finding with its members, in first-seen order: an exact message
    // every member shares keys on itself; any other keys on its message
    // with the member's name abstracted.
    let mut groups: Vec<((String, String), Diagnostic, Vec<&str>)> = Vec::new();
    for (member, ds) in &per {
        for d in ds {
            let shared = per.iter().all(|(_, o)| o.iter().any(|x| key(x) == key(d)));
            let k = if shared {
                key(d)
            } else {
                (d.code.clone(), abstract_member(&d.message, member))
            };
            match groups.iter_mut().find(|(gk, _, _)| *gk == k) {
                Some((_, _, ms)) => {
                    if !ms.contains(member) {
                        ms.push(member)
                    }
                }
                None => groups.push((k, d.clone(), vec![member])),
            }
        }
    }
    let target = crate::beats::OCCASION_TARGET;
    groups
        .into_iter()
        .map(|((_, abstracted), mut d, ms)| {
            if ms.len() == members.len() && abstracted == d.message {
                // Shared verbatim: the member never mattered.
            } else if ms.len() == 1 {
                d.message = format!(
                    "{} (for `{target}` = `{}`, dsl 0.27.0 §3)",
                    d.message, ms[0]
                );
            } else {
                const SHOWN: usize = 8;
                let mut list = ms
                    .iter()
                    .take(SHOWN)
                    .map(|m| format!("`{m}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                if ms.len() > SHOWN {
                    list.push_str(&format!(" and {} more", ms.len() - SHOWN));
                }
                let which = if ms.len() == members.len() {
                    "every member"
                } else {
                    "members"
                };
                d.message = format!(
                    "{} (for `{target}` = {which} {list}, dsl 0.27.0 §3)",
                    abstracted
                        .replace(&format!(".{MEMBER_MARK}"), &format!("[{target}]"))
                        .replace(MEMBER_MARK, target)
                );
            }
            d
        })
        .collect()
}

/// Stands in for the member's name in an abstracted per-member message.
pub(crate) const MEMBER_MARK: &str = "\u{0}member\u{0}";

/// `message` with each whole-identifier occurrence of `member` replaced by
/// [`MEMBER_MARK`], so two members' copies of one finding compare equal.
pub(crate) fn abstract_member(message: &str, member: &str) -> String {
    let ident = |c: char| c == '_' || c.is_ascii_alphanumeric();
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(at) = rest.find(member) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + member.len()..].chars().next();
        out.push_str(&rest[..at]);
        if before.is_some_and(ident) || after.is_some_and(ident) {
            out.push_str(member);
        } else {
            out.push_str(MEMBER_MARK);
        }
        rest = &rest[at + member.len()..];
    }
    out.push_str(rest);
    out
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

const _: () = ();
