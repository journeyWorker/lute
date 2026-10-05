use cel_parser::ast::{operators as op, Expr};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_syntax::ast::{Node, Objective, Quest};

use crate::cel_expand::DefTable;
use crate::decide::{analyze_literal_comparisons, decide_slot, DecideCtx, Decided};
use crate::solution::{disjoint, solution_set, SolutionSet};

use super::diagnostics::{diag, push_literal_cmp_diags};
use super::{
    E_OBJECTIVE_CONTRADICTION, E_OBJECTIVE_UNSATISFIABLE, E_QUEST_UNREACHABLE,
    W_DEADLINE_BEFORE_DONE, W_DEADLINE_NEVER, W_OBJECTIVE_HIDDEN,
};

pub(super) fn check_quest_reach(quest: &Quest, defs: &DefTable<'_>, ctx: &DecideCtx<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // dsl 0.5.2 §2.1: `<quest start/fail>` are listed guard slots too — the
    // lint fires independently of `E-QUEST-UNREACHABLE`, which this spec
    // revision's §2.3 ownership clause does NOT scope (it names only
    // `E-ARM-DEAD`/`W-OTHERWISE-DEAD`), so no suppression accompanies this.
    // dsl 0.28.0 §1 (T1-5): `rearm` is a condition slot like `start`.
    for slot in [&quest.start, &quest.fail, &quest.rearm]
        .into_iter()
        .flatten()
    {
        let analysis = analyze_literal_comparisons(&slot.raw, defs, ctx);
        push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&slot.raw), slot.span);
    }
    let dead_start = quest
        .start
        .as_ref()
        .is_some_and(|s| matches!(decide_slot(&s.raw, defs, ctx), Some(Decided::Bool(false))));
    let true_fail = quest
        .fail
        .as_ref()
        .is_some_and(|f| matches!(decide_slot(&f.raw, defs, ctx), Some(Decided::Bool(true))));
    if !dead_start && !true_fail {
        return diags;
    }
    diags.push(diag(
        E_QUEST_UNREACHABLE,
        Severity::Error,
        quest_unreachable_message(dead_start, true_fail),
        quest.span,
    ));
    diags
}

/// dsl 0.24.0 (round-3 T3-11): `W-QUEST-HANDLER-DEAD` for an `<on event="E"
/// when="G">` whose `G` implies the quest's completion — every REQUIRED
/// objective's `done` (any one of them under `complete="any"`). Quests
/// settle after every write, and a world event reaches a quest's handlers
/// only while it is active, so whenever `G` holds the quest has already
/// completed and the body never runs. The lifecycle events (`questComplete`,
/// `questFailed`) are dispatched by the transition itself and are exempt.
/// Decided only where it is certain: the quest has a required objective and
/// every one counted is settle-judged (no `on=` occasion objective, no
/// `quest=` child), and implication is conjunct containment after `@def`
/// expansion — each top-level `&&` conjunct of `done` is literally a
/// conjunct of `G`.
pub(super) fn check_handler_after_completion(quest: &Quest, defs: &DefTable<'_>) -> Vec<Diagnostic> {
    let required: Vec<&Objective> = quest
        .body
        .iter()
        .filter_map(|n| match n {
            Node::Objective(o) if !o.optional => Some(o),
            _ => None,
        })
        .collect();
    if required.is_empty() || required.iter().any(|o| o.on.is_some() || o.quest.is_some()) {
        return Vec::new();
    }
    let any = quest.completes_on_any();
    let mut diags = Vec::new();
    for node in &quest.body {
        let Node::On(on) = node else { continue };
        if matches!(on.event.as_str(), "questComplete" | "questFailed") {
            continue;
        }
        let Some(when) = on.when.as_ref().filter(|w| !w.raw.trim().is_empty()) else {
            continue;
        };
        let guard = text_conjuncts(&expand_text(&when.raw, defs));
        let implied = |o: &&Objective| {
            let done = text_conjuncts(&expand_text(&o.done.raw, defs));
            !done.is_empty() && done.iter().all(|c| guard.contains(c))
        };
        let completes = if any {
            required.iter().any(implied)
        } else {
            required.iter().all(implied)
        };
        if !completes {
            continue;
        }
        diags.push(diag(
            crate::project_check::W_QUEST_HANDLER_DEAD,
            Severity::Warning,
            format!(
                "`<on event=\"{}\">` never runs: its `when` implies {} of quest `{}` is done, so \
                 the quest has already completed when it holds, and an event reaches only an \
                 active quest's handlers; move the body to `<on event=\"questComplete\">` or \
                 the objective's own body (dsl 0.24.0)",
                on.event,
                if any {
                    "a required objective"
                } else {
                    "every required objective"
                },
                quest.id
            ),
            on.event_span,
        ));
    }
    diags
}

/// `raw` with its `@def`s expanded (the raw text when expansion fails).
fn expand_text(raw: &str, defs: &DefTable<'_>) -> String {
    let mut stack = Vec::new();
    crate::cel_expand::expand_cel(raw, defs, None, &mut stack).unwrap_or_else(|_| raw.to_string())
}

/// The top-level `&&` conjuncts of CEL text, each with whitespace outside
/// string literals removed and redundant outer parentheses stripped, a
/// parenthesized conjunction flattened. A text with a top-level `||` or
/// `?:` is one conjunct (`&&` binds tighter, so splitting it would be
/// wrong).
fn text_conjuncts(raw: &str) -> Vec<String> {
    fn normalize(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut quote: Option<char> = None;
        let mut esc = false;
        for c in s.chars() {
            match quote {
                Some(q) => {
                    out.push(c);
                    if esc {
                        esc = false;
                    } else if c == '\\' {
                        esc = true;
                    } else if c == q {
                        quote = None;
                    }
                }
                None if c.is_whitespace() => {}
                None => {
                    if c == '\'' || c == '"' {
                        quote = Some(c);
                    }
                    out.push(c);
                }
            }
        }
        out
    }
    /// Byte offsets of the depth-0 `&&`s, and whether a depth-0 `||`/`?`
    /// occurs, in normalized text; `None` when brackets are unbalanced.
    fn top_level(s: &str) -> Option<(Vec<usize>, bool)> {
        let b = s.as_bytes();
        let (mut depth, mut quote, mut esc) = (0i32, None::<u8>, false);
        let (mut ands, mut other) = (Vec::new(), false);
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if let Some(q) = quote {
                if esc {
                    esc = false;
                } else if c == b'\\' {
                    esc = true;
                } else if c == q {
                    quote = None;
                }
                i += 1;
                continue;
            }
            match c {
                b'\'' | b'"' => quote = Some(c),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => {
                    depth -= 1;
                    if depth < 0 {
                        return None;
                    }
                }
                b'&' if depth == 0 && b.get(i + 1) == Some(&b'&') => {
                    ands.push(i);
                    i += 1;
                }
                b'|' | b'?' if depth == 0 => other = true,
                _ => {}
            }
            i += 1;
        }
        (depth == 0 && quote.is_none()).then_some((ands, other))
    }
    /// `s` without one pair of parentheses enclosing all of it.
    fn unwrap(s: &str) -> Option<&str> {
        let inner = s.strip_prefix('(')?.strip_suffix(')')?;
        top_level(inner).map(|_| inner)
    }
    fn split(s: &str, out: &mut Vec<String>) {
        let mut s = s;
        while let Some(inner) = unwrap(s) {
            s = inner;
        }
        match top_level(s) {
            Some((ands, false)) if !ands.is_empty() => {
                let mut start = 0;
                for at in ands {
                    split(&s[start..at], out);
                    start = at + 2;
                }
                split(&s[start..], out);
            }
            _ if !s.is_empty() => out.push(s.to_string()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    split(&normalize(raw), &mut out);
    out
}

/// Per-`<objective>` engine (dsl 0.4.0 §5.3 rules 1 and 3). `done` deciding
/// false is `E-OBJECTIVE-UNSATISFIABLE` — appending the required-quest note
/// when `!optional` (C4: NEVER a second `E-QUEST-UNREACHABLE`; enforced
/// here by construction, since `check_quest_reach` never looks at
/// objectives at all). A REQUIRED objective (`!optional`) whose `when`
/// decides false is separately `W-OBJECTIVE-HIDDEN` — independent of
/// whether `done` is itself decided, since visibility and completion are
/// evaluated independently (§5.3). `ctx.dollar` MUST be `None` — no `$` is
/// in scope at an `<objective>`'s attrs.
pub(super) fn check_objective_reach(
    o: &Objective,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // dsl 0.5.2 §2.1: `<objective when/done>` are listed guard slots too —
    // independent of `E-OBJECTIVE-UNSATISFIABLE`/`W-OBJECTIVE-HIDDEN`, which
    // §2.3's ownership clause does NOT scope (it names only
    // `E-ARM-DEAD`/`W-OTHERWISE-DEAD`), so no suppression accompanies this.
    if let Some(when) = &o.visible_when {
        let analysis = analyze_literal_comparisons(&when.raw, defs, ctx);
        push_literal_cmp_diags(&mut diags, &analysis.hits, Some(&when.raw), when.span);
    }
    let done_analysis = analyze_literal_comparisons(&o.done.raw, defs, ctx);
    push_literal_cmp_diags(
        &mut diags,
        &done_analysis.hits,
        Some(&o.done.raw),
        o.done.span,
    );
    if let Some(Decided::Bool(false)) = decide_slot(&o.done.raw, defs, ctx) {
        diags.push(diag(
            E_OBJECTIVE_UNSATISFIABLE,
            Severity::Error,
            objective_unsat_message(!o.optional, &o.done.raw),
            o.span,
        ));
    }
    if !o.optional {
        if let Some(when) = &o.visible_when {
            if let Some(Decided::Bool(false)) = decide_slot(&when.raw, defs, ctx) {
                diags.push(diag(
                    W_OBJECTIVE_HIDDEN,
                    Severity::Warning,
                    objective_hidden_message(),
                    o.span,
                ));
            }
        }
    }
    // HW27-05: a `by` that can never hold never fails the objective — most
    // often a deadline past the end of a clock that ends.
    if let Some(by) = o.by.as_ref().filter(|b| !b.raw.trim().is_empty()) {
        if decide_slot(&by.raw, defs, ctx) == Some(Decided::Bool(false)) {
            let (id, deadline) = (&o.id, by.raw.trim());
            let message = match crate::clock::false_reason(&by.raw, defs, ctx) {
                Some(why) => format!(
                    "objective `{id}` never fails: its deadline `by: {deadline}` can never hold \
                     ({why}) — write a deadline the clock can reach, or drop it (dsl 0.24.0 §2.1)"
                ),
                None => format!(
                    "objective `{id}` never fails: its deadline `by: {deadline}` can never hold — \
                     write a deadline the clock can reach, or drop it (dsl 0.24.0 §2.1)"
                ),
            };
            diags.push(diag(W_DEADLINE_NEVER, Severity::Warning, message, by.span));
        }
    }
    // dsl 0.24.0 §2.1: `done ⇒ by` on an `on=` objective — proven as
    // "`done && !by` decides false"; undecided stays silent.
    if let (Some((on, _)), Some(by), None) = (&o.on, &o.by, &o.until) {
        let (done, deadline) = (o.done.raw.trim(), by.raw.trim());
        if !done.is_empty()
            && !deadline.is_empty()
            && decide_slot(done, defs, ctx) != Some(Decided::Bool(false))
            && decide_slot(&format!("({done}) && !({deadline})"), defs, ctx)
                == Some(Decided::Bool(false))
        {
            diags.push(diag(
                W_DEADLINE_BEFORE_DONE,
                Severity::Warning,
                deadline_before_done_message(&o.id, on, done, deadline),
                by.span,
            ));
        }
    }
    diags
}

/// One objective that participates in pairing.
struct Gate<'a> {
    id: &'a str,
    path: String,
    raw: &'a str,
    set: SolutionSet,
    span: Span,
}

/// dsl 0.10.0 §5.2 (D-G): every pair of REQUIRED, in-domain, same-path `done`
/// predicates of one quest whose solution sets do not intersect.
///
/// **Direct children of `quest.body` only**, mirroring `E-OBJECTIVE-ID-DUP`'s
/// own scoping (`match_check.rs:650-651`). An objective nested in a `<match>`
/// arm or a `<branch>` choice cannot be shown to coexist with one in a sibling
/// arm, so pairing across them would manufacture a contradiction between
/// objectives that never meet — the one outcome §5.2 may not produce.
pub(super) fn check_objective_contradiction(
    quest: &Quest,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let mut gates: Vec<Gate<'_>> = Vec::new();
    for node in &quest.body {
        let Node::Objective(o) = node else { continue };
        // An OPTIONAL objective never participates: the quest can still
        // complete without it, so a pair including one is not a contradiction
        // ABOUT THE QUEST. An id-less objective has its own diagnostic.
        if o.optional || o.id.is_empty() {
            continue;
        }
        // An individually dead `done` is `E-OBJECTIVE-UNSATISFIABLE`'s, and the
        // two codes MUST NOT both fire for one pair. A comparison over a
        // declared path is never `decide`-false today (a `number` path is
        // `Domain::Number`, which R2 leaves undecided like `Infinite`), but
        // stating the exclusion structurally is cheaper than relying on that
        // staying true.
        if matches!(
            decide_slot(&o.done.raw, defs, ctx),
            Some(Decided::Bool(false))
        ) {
            continue;
        }
        if let Some(g) = in_domain_gate(o, ctx) {
            gates.push(g);
        }
    }
    let mut diags = Vec::new();
    for j in 1..gates.len() {
        for i in 0..j {
            if gates[i].path != gates[j].path || !disjoint(&gates[i].set, &gates[j].set) {
                continue;
            }
            // Anchored at the SECOND objective, reported once per pair.
            diags.push(diag(
                E_OBJECTIVE_CONTRADICTION,
                Severity::Error,
                contradiction_message(&gates[i], &gates[j]),
                gates[j].span,
            ));
        }
    }
    diags
}

/// §5.2's in-domain test: the predicate is, IN ITS ENTIRETY, a single
/// comparison `<declared scalar state path> <op> <literal>` with the literal
/// well-typed for the path's declared type. An `&&`, an `||`, a `!`, a fact
/// query, a `@ref`, a second path, or anything else puts it out of domain.
///
/// Parsed through the SAME expand-then-marked-reparse pipeline `decide_slot`
/// and the project guard pass (`fact_check.rs`) use, so this analysis and
/// reachability can never see different trees for the same raw text.
fn in_domain_gate<'a>(o: &'a Objective, ctx: &DecideCtx<'_>) -> Option<Gate<'a>> {
    let raw = o.done.raw.trim();
    // A `@ref` anywhere puts it out of domain, and `parse_slot_marked_refs`
    // would hide that by turning the ref into a marker ident.
    if !lute_cel::scan_refs(raw).is_empty() {
        return None;
    }
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, raw)?;
    let ided = arena.get(handle)?;
    let (path, set) = comparison_set(&ided.expr, ctx.schema)?;
    Some(Gate {
        id: &o.id,
        path,
        raw: o.done.raw.trim(),
        set,
        span: o.span,
    })
}

/// `expr` as ONE in-domain comparison `<declared scalar state path> <op>
/// <literal>` (either operand order) with its solution set over the path's
/// declared type; `None` for anything else.
pub(super) fn comparison_set(expr: &Expr, schema: &crate::meta::StateSchema) -> Option<(String, SolutionSet)> {
    comparison_set_polar(expr, schema, false)
}

/// [`comparison_set`] of `expr`, or (`negated`) of `!expr` — the complementary
/// operator (`!(x < 1)` is `x >= 1`, `!(x == c)` is `x != c`).
pub(super) fn comparison_set_polar(
    expr: &Expr,
    schema: &crate::meta::StateSchema,
    negated: bool,
) -> Option<(String, SolutionSet)> {
    let Expr::Call(c) = expr else {
        return None;
    };
    if c.target.is_some() || c.args.len() != 2 {
        return None;
    }
    // Normalise to `path <op> literal`; a `literal <op> path` form flips the
    // operator so the solution set is always computed path-side.
    let (path, opname, lit) = match (
        crate::cel_paths::select_path(&c.args[0].expr),
        &c.args[1].expr,
        crate::cel_paths::select_path(&c.args[1].expr),
        &c.args[0].expr,
    ) {
        (Some(p), Expr::Literal(v), _, _) => (p, c.func_name.as_str(), v),
        (_, _, Some(p), Expr::Literal(v)) => (p, flip(&c.func_name)?, v),
        _ => return None,
    };
    let opname = if negated { complement(opname)? } else { opname };
    let declared = crate::set_op::resolve_type(&path, schema)?;
    let set = solution_set(declared, opname, lit)?;
    Some((path, set))
}

/// dsl 0.27.0 (T3-3): `path in [lit, …]` as solution sets over the path's
/// declared type — the union of the members' `==` sets; `negated`
/// (`!(path in […])`), one `!=` set per member. `None` for any other shape,
/// a non-literal or ill-typed member, or an empty list (which holds nowhere
/// and so constrains nothing we can use).
fn contradiction_message(first: &Gate<'_>, second: &Gate<'_>) -> String {
    format!(
        "required objectives `{}` and `{}` cannot both complete: `done=\"{}\"` and \
         `done=\"{}\"` have no common value of `{}` (dsl 0.10.0 §5.2){}",
        first.id, second.id, first.raw, second.raw, first.path, REQUIRED_QUEST_NOTE
    )
}

/// `E-QUEST-UNREACHABLE` message (dsl 0.4.0 §5.3 rule 2, D21): joins
/// whichever standalone cause(s) hold — never a `start`-only phrase when
/// `fail` also holds, or vice versa (the §5.4 worked example's
/// parenthetical: "distinct roots ... so both appear").
fn quest_unreachable_message(dead_start: bool, true_fail: bool) -> String {
    let mut causes = Vec::new();
    if dead_start {
        causes.push("`start` decides false — the quest never activates");
    }
    if true_fail {
        causes.push(
            "`fail` decides true — fail precedes completion (0.2 §6.3), so an activated \
             instance fails at the first evaluation instant",
        );
    }
    format!(
        "quest can never complete: {} (dsl 0.4 §5.3)",
        causes.join("; ")
    )
}

/// The §5.3/C4 quest-consequence note (Task 5 rules, quoted verbatim):
/// appended to [`objective_unsat_message`]'s output when the objective is
/// required (`!optional`). `pub(crate)`: Task 7's `producible.rs` reuses it
/// verbatim for its own THIRD `E-OBJECTIVE-UNSATISFIABLE` cause (a
/// non-producible gated relation) so the required-quest consequence reads
/// identically regardless of which cause triggered the diagnostic.
pub(crate) const REQUIRED_QUEST_NOTE: &str =
    "; the objective — and, being required, the quest — can never complete (dsl 0.4 §5.3)";

/// `E-OBJECTIVE-UNSATISFIABLE` message (dsl 0.4.0 §5.3 rule 1): quotes the
/// `done` predicate's raw text (matching [`dead_guard_message`](super::diagnostics::dead_guard_message)'s style).
/// `required` appends [`REQUIRED_QUEST_NOTE`] verbatim; an `optional`
/// objective's dead `done` still fires the code (it too can never
/// complete), just without the quest-level consequence (C4).
fn objective_unsat_message(required: bool, raw: &str) -> String {
    let mut msg = format!(
        "`done` predicate `{}` is provably false: the objective can never complete on any run",
        raw.trim()
    );
    if required {
        msg.push_str(REQUIRED_QUEST_NOTE);
    } else {
        msg.push_str(" (dsl 0.4 §5.3)");
    }
    msg
}

/// `W-OBJECTIVE-HIDDEN` message (dsl 0.4.0 §5.3 rule 3): carries `0.2
/// §6.3`'s own advice — mark the objective `optional` or fix the gate.
fn objective_hidden_message() -> String {
    "objective's `visibleWhen` is provably false: it is never visible or tracked, yet still gates \
     completion (dsl 0.4 §5.3) — mark it `optional` or fix the gate (0.2 §6.3)"
        .to_string()
}

/// `W-DEADLINE-BEFORE-DONE` message (dsl 0.24.0 §2.1): why the objective
/// fails first, and the `until=` spelling that keeps the deadline at the
/// occasion.
fn deadline_before_done_message(id: &str, on: &str, done: &str, by: &str) -> String {
    format!(
        "objective `{id}`'s `by=\"{by}\"` holds whenever its `done=\"{done}\"` does, and `by` is a \
         moment judged at every settle (dsl 0.24.0 §2.1): it fails the objective at the settle it \
         comes true, before occasion `{on}` ever judges `done` (unless both happen in the step \
         that raises `{on}`) — write `until=\"{by}\"` to judge the deadline only when `{on}` is \
         raised"
    )
}

fn flip(func_name: &str) -> Option<&'static str> {
    Some(match func_name {
        op::EQUALS => op::EQUALS,
        op::NOT_EQUALS => op::NOT_EQUALS,
        op::LESS => op::GREATER,
        op::LESS_EQUALS => op::GREATER_EQUALS,
        op::GREATER => op::LESS,
        op::GREATER_EQUALS => op::LESS_EQUALS,
        _ => return None,
    })
}

/// The operator of the negated comparison (`!(x < 1)` is `x >= 1`).
fn complement(func_name: &str) -> Option<&'static str> {
    Some(match func_name {
        op::EQUALS => op::NOT_EQUALS,
        op::NOT_EQUALS => op::EQUALS,
        op::LESS => op::GREATER_EQUALS,
        op::LESS_EQUALS => op::GREATER,
        op::GREATER => op::LESS_EQUALS,
        op::GREATER_EQUALS => op::LESS,
        _ => return None,
    })
}

