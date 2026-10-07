use std::collections::BTreeSet;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Node;

use crate::decide::{LiteralCmpHit, LiteralCmpKind};
use crate::match_check::E_WHEN_LITERAL_DOMAIN;


/// `E-ARM-DEAD` (dsl 0.4.0 §5.2): a `<when>` arm or `<choice>` that can
/// provably never fire — a decided-false guard, or an `is` pattern subsumed
/// by earlier unguarded sibling arms (first-match-wins, dsl §11.2).
pub(crate) const E_ARM_DEAD: &str = "E-ARM-DEAD";

/// Cause-1 message (dsl 0.4.0 §5.2 rule 1): names the guard text and states
/// it is provably false. `kind` is `"arm"` (a `<when test>`) or `"choice"`
/// (a `<choice when>`).
pub(super) fn dead_guard_message(kind: &str, raw: &str) -> String {
    format!(
        "{kind} can never fire: guard `{}` is provably false (dsl 0.4 §5.2)",
        raw.trim()
    )
}

/// Cause-2 message (dsl 0.4.0 §5.2 rule 2), matching the §5.4 worked
/// example's shape: `` arm can never fire: its pattern `gold` is fully
/// covered by the earlier unguarded arm at 2:3 (`gold | silver`) —
/// first-match-wins (dsl 0.4 §5.2) ``. When no single earlier arm covers
/// the pattern but several together do (`joint`), the earliest of them is
/// cited as the first of several.
pub(super) fn subsumption_message(pattern: &str, cov_span: Span, cov_pattern: &str, joint: bool) -> String {
    let by = if joint {
        "earlier unguarded arms together, the first at"
    } else {
        "earlier unguarded arm at"
    };
    format!(
        "arm can never fire: its pattern `{pattern}` is fully covered by the {by} {}:{} \
         (`{cov_pattern}`) — first-match-wins (dsl 0.4 §5.2)",
        cov_span.line, cov_span.column
    )
}

/// dsl 0.24.0: an arm every literal of which the enclosing body's `when`
/// rules out — the body runs only once that guard held.
pub(super) fn assumed_dead_message(pattern: &str, when: &str) -> String {
    format!(
        "arm can never fire: its pattern `{pattern}` is ruled out by the body's `when` guard \
         `{when}`, which holds whenever this body runs (dsl 0.24.0)"
    )
}

/// `W-OTHERWISE-DEAD` (dsl 0.4.0 §5.2): an `<otherwise>` that is provably
/// unreachable because earlier unguarded `is` arms already cover the
/// subject's whole domain. A warning (not an error) — a defensive
/// `<otherwise>` is a legitimate hedge against schema evolution (0.3 §12).
pub(crate) const W_OTHERWISE_DEAD: &str = "W-OTHERWISE-DEAD";

/// `E-QUEST-UNREACHABLE` (dsl 0.4.0 §5.3): a `<quest>` that can provably
/// never complete — `start` decides false (never activates) or `fail`
/// decides true (fails at the first evaluation instant, precedence over
/// completion, `0.2 §6.3`). ONE diagnostic per quest naming whichever
/// standalone cause(s) hold (D21).
pub(crate) const E_QUEST_UNREACHABLE: &str = "E-QUEST-UNREACHABLE";

/// `E-OBJECTIVE-UNSATISFIABLE` (dsl 0.4.0 §5.3): an `<objective>` whose
/// `done` predicate decides false — it can never complete on any run. A
/// REQUIRED (`!optional`) objective additionally makes the enclosing quest
/// unreachable; that consequence rides as a NOTE on this diagnostic, never
/// as a second `E-QUEST-UNREACHABLE` (C4).
pub(crate) const E_OBJECTIVE_UNSATISFIABLE: &str = "E-OBJECTIVE-UNSATISFIABLE";

/// `E-OBJECTIVE-CONTRADICTION` (dsl 0.10.0 §5.2, D-G): two REQUIRED objectives
/// of one `<quest>` whose in-domain `done` predicates name the same declared
/// scalar path and whose solution sets over that path's declared type do not
/// intersect. Neither objective is individually dead — so neither draws
/// [`E_OBJECTIVE_UNSATISFIABLE`] — and the quest consequence rides as a note on
/// this diagnostic (C4, D-O), never as a second [`E_QUEST_UNREACHABLE`].
pub(crate) const E_OBJECTIVE_CONTRADICTION: &str = "E-OBJECTIVE-CONTRADICTION";

/// `W-OBJECTIVE-HIDDEN` (dsl 0.4.0 §5.3): a REQUIRED (`!optional`)
/// objective whose `when` visibility gate decides false — provably never
/// visible or tracked, yet still gates completion (the `0.2 §6.3` softlock
/// prose made checkable). A warning: `done` is evaluated independently of
/// visibility, so completion may still be reachable.
pub(crate) const W_OBJECTIVE_HIDDEN: &str = "W-OBJECTIVE-HIDDEN";

/// `W-DEADLINE-BEFORE-DONE` (dsl 0.24.0 §2.1): an `on=` objective with a
/// `by=` (no `until=`) whose `done` provably implies `by`. `by` is judged at
/// every settle but an `on=` objective's `done` only at its occasion raise,
/// so `by` comes true — and fails the objective — before `done` is ever
/// judged, unless both happen in the step that raises the occasion.
pub(crate) const W_DEADLINE_BEFORE_DONE: &str = "W-DEADLINE-BEFORE-DONE";

/// `W-DEADLINE-NEVER` (dsl 0.24.0 §2.1): an objective's `by=` deadline is
/// provably false — typically a moment past the end of a clock that ends —
/// so it never fails the objective.
pub(crate) const W_DEADLINE_NEVER: &str = "W-DEADLINE-NEVER";

/// `E-ENTRY-UNREACHABLE` (dsl 0.20.0 §5): a lore entry whose `when`
/// eligibility guard provably never holds — the entry is never presented.
/// Decided per file when the guard is scalar-decidable (here), and by the
/// project pass (`crate::fact_check`) when only the fact envelope decides it.
pub const E_ENTRY_UNREACHABLE: &str = "E-ENTRY-UNREACHABLE";

/// `E-UNSET-LITERAL` (dsl 0.5.2 §2): a CEL guard slot (`<when test>`,
/// `<choice when>`, `<match on>` subject, `<objective when/done>`, or
/// `<quest start/fail>`) comparing a maybe-unset finite-domain subject to
/// the FOREIGN string `'unset'` — the most common misspelling of the DSL's
/// *unset* sentinel (CEL `null`, `0.1 §11.2`), not the string `'unset'`. An
/// INDEPENDENT AST lint (§2.1): fires for BOTH `==` (decides false, R2) and
/// `!=` (decides true — never reaches the dead-arm path, yet the identical
/// mistake), regardless of `decide_slot`'s outcome, and possibly nested
/// inside a larger boolean expression. Owns (suppresses) the derivative
/// `E-ARM-DEAD`/`W-OTHERWISE-DEAD` it would otherwise produce (§2.3,
/// mirrors D4 above).
pub(crate) const E_UNSET_LITERAL: &str = "E-UNSET-LITERAL";

/// Push one diagnostic per faulty literal comparison (§2.1: every distinct
/// comparison in the slot, not just the first): a misspelt unset sentinel is
/// `E-UNSET-LITERAL`, a string no member of the subject's finite domain is
/// `E-WHEN-LITERAL-DOMAIN` (dsl 0.26.0) — the code a foreign `<when is>`
/// literal gets, since `S == 'x'` is the same claim. Each points AT THE
/// LITERAL inside `text` — the slot's own text, when `span` is where it is
/// written (CEL `Expr` nodes carry no span of their own, so the quoted
/// literal is found in the text, in hit order); otherwise at `span`.
pub(crate) fn push_literal_cmp_diags(
    diags: &mut Vec<Diagnostic>,
    hits: &[LiteralCmpHit],
    text: Option<&str>,
    span: Span,
) {
    // A text that does not fill the span (an unescaped attribute value, a
    // YAML scalar) has no byte-for-byte mapping into the source.
    let text = text.filter(|t| span.byte_end.saturating_sub(span.byte_start) == t.len());
    let mut cursor = 0usize;
    for hit in hits {
        let (code, message, literal) = match &hit.kind {
            LiteralCmpKind::UnsetSentinel { not_equals } => (
                E_UNSET_LITERAL,
                unset_literal_message(&hit.subject, *not_equals),
                "unset",
            ),
            LiteralCmpKind::ForeignMember { literal, members } => (
                E_WHEN_LITERAL_DOMAIN,
                foreign_comparison_message(&hit.subject, literal, members),
                literal.as_str(),
            ),
            // `E-CEL-TYPE`'s, reported by the slot's own type pass.
            LiteralCmpKind::TypeMismatch => continue,
        };
        let message = match &hit.via {
            Some(def) => format!("in {def}: {message}"),
            None => message,
        };
        let at = text
            .and_then(|t| quoted_literal_at(t, literal, cursor).map(|r| (t, r)))
            .map_or(span, |(t, (start, end))| {
                cursor = end;
                sub_span(span, t, start, end)
            });
        diags.push(diag(code, Severity::Error, message, at));
    }
}

/// The byte range of the string literal `literal` written in `text` —
/// `'lit'` or `"lit"`, quotes included — at or after `from`, else anywhere.
fn quoted_literal_at(text: &str, literal: &str, from: usize) -> Option<(usize, usize)> {
    let find = |from: usize| {
        ['\'', '"'].into_iter().find_map(|q| {
            let needle = format!("{q}{literal}{q}");
            text.get(from..)?
                .find(&needle)
                .map(|i| (from + i, from + i + needle.len()))
        })
    };
    find(from).or_else(|| find(0))
}

/// `text[start..end]` as a span, `text` being written at `span`: line,
/// character column and UTF-16 offsets advance by the prefix's own counts.
fn sub_span(span: Span, text: &str, start: usize, end: usize) -> Span {
    let prefix = &text[..start];
    let u16 = |s: &str| s.encode_utf16().count() as u32;
    let (line, column) = match prefix.rfind('\n') {
        Some(nl) => (
            span.line + prefix.matches('\n').count() as u32,
            prefix[nl + 1..].chars().count() as u32 + 1,
        ),
        None => (span.line, span.column + prefix.chars().count() as u32),
    };
    let u16_start = span.utf16_range.0 + u16(prefix);
    Span {
        byte_start: span.byte_start + start,
        byte_end: span.byte_start + end,
        line,
        column,
        utf16_range: (u16_start, u16_start + u16(&text[start..end])),
    }
}

/// `W-CODE-AFTER-END` (dsl 0.8.0): a record following `::end` in the SAME
/// straight-line body. `::end` terminates the walk at its own record
/// (`lute.core`'s `terminatesWalk`), so nothing after it in that body can
/// ever run.
///
/// Scoped to the IMMEDIATELY ENCLOSING sequence — a shot body, one
/// `<choice>` body, one `<when>`/`<otherwise>` arm, one `<on>`/
/// `<objective>` body. An `::end` inside one `<choice>` says nothing about
/// a sibling choice or about content after the enclosing `<branch>`: those
/// are DIFFERENT bodies, reached by a different route, and the walk that
/// terminated never entered them. Cross-body reachability is
/// `E-CONN-UNREACHABLE`'s job (a whole-project graph), not this local lint.
///
/// A WARNING, never an error: unreachable content is inert, not
/// ill-formed — the same call `W-OTHERWISE-DEAD` makes for a defensive
/// `<otherwise>`.
pub(crate) const W_CODE_AFTER_END: &str = "W-CODE-AFTER-END";

/// A `W-CODE-AFTER-END` for each dead stretch of `nodes` (a single
/// straight-line body): what follows a `::end` up to the next node a
/// `::jump` can jump into, anchored at the stretch's FIRST node — the place
/// an author would cut from. One per stretch: everything past its first
/// node is unreachable for the SAME reason, and N warnings for one mistake
/// is noise.
///
/// dsl 0.27.0 (round-5 T3-7): a `::label` / `id=` line some `::jump{to}` in
/// the document names (`targets`, [`crate::next_labels::next_targets`]) —
/// or a node holding one at any depth — is an entry point: the walk
/// resumes there, so it and what follows are live until the next `::end`.
/// A mark nothing targets stays dead.
///
/// Dispatch is by TAG ([`lute_manifest::core::END_DIRECTIVE`]), the same
/// key `lower_directive` lowers on — see the `terminatesWalk` note in
/// `lute_manifest::validate::SEMANTICS_VOCAB` for why the flag declares
/// the semantics but never drives the dispatch.
pub(crate) fn check_code_after_end(nodes: &[Node], targets: &BTreeSet<String>, diags: &mut Vec<Diagnostic>) {
    let is_end =
        |n: &Node| matches!(n, Node::Directive(d) if d.tag == lute_manifest::core::END_DIRECTIVE);
    for dead in dead_stretches(nodes, is_end, targets) {
        diags.push(diag(
            W_CODE_AFTER_END,
            Severity::Warning,
            "unreachable content after `::end` (the walk terminates here)".to_string(),
            crate::admission::node_span(dead),
        ));
    }
}

/// The first node of each stretch of `nodes` that follows a terminator
/// (`ends`) and precedes the next jump entry point — a node that is or holds
/// a label in `targets` ([`crate::next_labels::holds_label`]).
fn dead_stretches<'n>(
    nodes: &'n [Node],
    ends: impl Fn(&Node) -> bool,
    targets: &BTreeSet<String>,
) -> Vec<&'n Node> {
    let mut out = Vec::new();
    // `dead`: a terminator ran and no entry point followed yet;
    // `reported`: this stretch already has its warning.
    let (mut dead, mut reported) = (false, false);
    for node in nodes {
        if dead && crate::next_labels::holds_label(node, targets) {
            dead = false;
        }
        if dead {
            if !reported {
                out.push(node);
                reported = true;
            }
        } else if ends(node) {
            (dead, reported) = (true, false);
        }
    }
    out
}

/// `W-CODE-AFTER-JUMP` (dsl 0.12.0): a record following an UNGUARDED
/// `::jump` in the SAME straight-line body — mirrors [`W_CODE_AFTER_END`]
/// exactly: an unconditional forward jump leaves this body the same way
/// `::end` does, so nothing after it in that body can ever run. A GUARDED
/// `::jump{when=}` does NOT qualify (fall-through exists; see the
/// `Node::Directive` arm in [`walk_reach`]).
pub(crate) const W_CODE_AFTER_NEXT: &str = "W-CODE-AFTER-JUMP";

/// `W-CODE-AFTER-JUMP` for `nodes`, mirroring [`check_code_after_end`]
/// verbatim except the terminator predicate (unguarded `::jump` — dispatch
/// by TAG, [`lute_manifest::core::NEXT_DIRECTIVE`], AND `d.when.is_none()`).
pub(crate) fn check_code_after_next(nodes: &[Node], targets: &BTreeSet<String>, diags: &mut Vec<Diagnostic>) {
    let is_unguarded_next = |n: &Node| matches!(n, Node::Directive(d) if d.tag == lute_manifest::core::JUMP_DIRECTIVE && d.when.is_none());
    for dead in dead_stretches(nodes, is_unguarded_next, targets) {
        diags.push(diag(
            W_CODE_AFTER_NEXT,
            Severity::Warning,
            "unreachable content after `::jump` (the walk jumps away here)".to_string(),
            crate::admission::node_span(dead),
        ));
    }
}
fn unset_literal_message(subject: &str, not_equals: bool) -> String {
    let cmp = if not_equals { "!=" } else { "==" };
    format!(
        "comparing `{subject}` {cmp} the string `'unset'`, which is never equal to the DSL's \
         unset sentinel (the CEL `null` literal, dsl 0.1 §11.2). Test for unset with \
         `!isSet({subject})`, or in a `<match subject=\"{subject}\">` use `<when is=\"unset\">` \
         (dsl 0.2 §5.2)"
    )
}

/// `E-WHEN-LITERAL-DOMAIN` message for a guard's comparison (dsl 0.26.0):
/// the `<when is>` wording — the literal and the subject's members — plus
/// the nearest member when one is close. dsl 0.28.0 (T1-5): a member written
/// with a prefix (`'place.village'`, the spelling a target or a step uses)
/// names the bare member.
fn foreign_comparison_message(subject: &str, literal: &str, members: &[String]) -> String {
    let prefixed = literal
        .rsplit_once('.')
        .filter(|(_, m)| members.iter().any(|x| x == m));
    let hint = match prefixed {
        Some((prefix, m)) => format!(
            " — did you mean `'{m}'`? `{subject}` holds the member alone, without the \
             `{prefix}.` prefix"
        ),
        None => {
            let members = || members.iter().map(String::as_str);
            lute_manifest::suggest::nearest(literal, members(), 2)
                .or_else(|| lute_manifest::suggest::abbreviated(literal, members()))
                .map_or_else(String::new, |near| format!(" — did you mean `'{near}'`?"))
        }
    };
    format!(
        "`'{literal}'` is not a member of `{subject}`'s domain [{}]{hint} (dsl 0.4 §5.2)",
        members.join(", ")
    )
}

/// Build a `Layer::Logic` diagnostic (a §5.2 reachability check).
pub(crate) fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    let evidence = match crate::evidence::classification(code) {
        Some(crate::evidence::DiagnosticClass::Analysis { evidence }) => Some(evidence),
        _ => None,
    };
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
        evidence,
    }
}
