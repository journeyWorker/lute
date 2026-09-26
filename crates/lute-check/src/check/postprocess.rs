//! Final diagnostic post-processing: suppressions, dedup, same-root collapse,
//! and span normalization.

use super::*;

/// Drop `E-MAYBE-UNSET` diagnostics whose span is a domain-exhaustive `<match>`
/// subject (T4.6 x T4.4 carry-forward). A subject read that maybe-unset on entry
/// is nonetheless safe when the match's arms cover every case (the join is an
/// intersection over all arms, not a fall-through), so the read cannot escape
/// unhandled — reporting it would be a false positive.
pub(super) fn suppress_exhaustive_subject_reads(
    diags: &mut Vec<Diagnostic>,
    subject_spans: &[Span],
) {
    if subject_spans.is_empty() {
        return;
    }
    diags.retain(|d| {
        !(d.code == "E-MAYBE-UNSET"
            && subject_spans
                .iter()
                .any(|s| s.byte_start == d.span.byte_start && s.byte_end == d.span.byte_end))
    });
}

/// C4 (dsl 0.4.0 §5.2/§8.2): drop any `W-OVERLAP-ARMS` whose span overlaps
/// an `E-ARM-DEAD` arm — the dead-arm error is the root; the pre-existing
/// literal-overlap warning would otherwise pile on the same arm.
pub(super) fn suppress_dead_arm_overlaps(diags: &mut Vec<Diagnostic>) {
    let dead_spans: Vec<Span> = diags
        .iter()
        .filter(|d| d.code == crate::reachability::E_ARM_DEAD)
        .map(|d| d.span)
        .collect();
    if dead_spans.is_empty() {
        return;
    }
    diags.retain(|d| {
        !(d.code == "W-OVERLAP-ARMS" && dead_spans.iter().any(|s| spans_overlap(*s, d.span)))
    });
}

/// The parse failures that can cost a block element a CHILD outright: a line
/// that classified as nothing, a tag whose close never resolved, an opener
/// wrapped past its newline, a body (and close) written on the opener's own
/// line. Membership rule: after one of these, the child list a verdict below
/// reads is NOT the child list the author wrote. Deliberately EXCLUDES
/// `E-LOGIC-CONTENT`/`E-TIMELINE-CONTENT` (the children parsed; one of them is
/// merely illegal in that body) and the content-line failures
/// `E-CONTENT-LINE-BRACKET`/`E-LEGACY-CONTENT-SIGIL` (they cost a child's own
/// BODY line, never the child) — for those the verdict is still earned.
const CHILD_PARSE_FAILURE_CODES: &[&str] = &[
    "E-TAG-INLINE-BODY",
    "E-TAG-NOT-ONE-LINE",
    "E-UNCLASSIFIED",
    "E-UNCLOSED-TAG",
];

/// The verdicts drawn from a block's CHILD LIST alone — each says "these
/// children do not cover / do not include X" and anchors at the whole block's
/// span: a `<match>`'s coverage trio ([`crate::match_check`]'s
/// `E-NONEXHAUSTIVE`/`E-UNSET-UNCOVERED`/`E-AGE-GATE`, all read off one
/// `has_otherwise`/`covered` derivation over `m.arms`), plus `<branch>`'s and
/// `<hub>`'s "no `<choice>` at all" / "no exit choice" verdicts over
/// `choices`. `E-BRANCH-ALL-GUARDED` is NOT here and needs no gate: it skips
/// the empty branch by construction, and a child that parses keeps its own
/// `when`, so no lost child can flip its verdict.
const CHILD_LIST_VERDICT_CODES: &[&str] = &[
    "E-AGE-GATE",
    "E-BRANCH-EMPTY",
    "E-HUB-NO-EXIT",
    "E-NONEXHAUSTIVE",
    "E-UNSET-UNCOVERED",
];

/// dsl §2.3: a block whose children did not parse gives these verdicts nothing
/// to judge. Each would be read off a child list the author never wrote, so it
/// lands as a FALSE claim about their logic stacked on the real (parse) error —
/// the most harmful cascade there is, since `E-NONEXHAUSTIVE` on an exhaustive
/// `<match>` sends the author to rewrite logic that was already correct. Any
/// [`CHILD_PARSE_FAILURE_CODES`] diagnostic landing inside the block therefore
/// drops every [`CHILD_LIST_VERDICT_CODES`] verdict for it (span overlap: a
/// verdict carries the whole block's span, so a failure on one of its lines is
/// always inside it).
///
/// Same shape as [`suppress_dead_arm_overlaps`] (span-scoped, so ONE broken
/// `<match>` never silences a healthy one elsewhere in the document) and the
/// same principle as C3/D12's [`suppress_unproven_absence`]: a failed
/// parse/resolution suppresses the claims that depend on what it never built.
/// [`crate::cel_resolve`] applies it upstream too — a `slot.ast: None` skips
/// the AST pass outright rather than cascade off unparsed CEL.
pub(super) fn suppress_unparsed_child_list_verdicts(diags: &mut Vec<Diagnostic>) {
    let unparsed: Vec<Span> = diags
        .iter()
        .filter(|d| CHILD_PARSE_FAILURE_CODES.contains(&d.code.as_str()))
        .map(|d| d.span)
        .collect();
    if unparsed.is_empty() {
        return;
    }
    diags.retain(|d| {
        !(CHILD_LIST_VERDICT_CODES.contains(&d.code.as_str())
            && unparsed.iter().any(|s| spans_overlap(*s, d.span)))
    });
}

/// Collapse overlapping `E-UNDECLARED` diagnostics to the single most precise
/// (narrowest) span per location (carry-forward #4). The same undeclared `::set`
/// target is flagged by `check_set` (`Layer::Staging`, precise `path_span`) and
/// `check_definite_assignment` (`Layer::Logic`); we keep one. Non-`E-UNDECLARED`
/// diagnostics pass through untouched.
pub(super) fn dedup_undeclared(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut undeclared: Vec<Diagnostic> = Vec::new();
    let mut out: Vec<Diagnostic> = Vec::new();
    for d in diags {
        if d.code == "E-UNDECLARED" {
            undeclared.push(d);
        } else {
            out.push(d);
        }
    }
    // Narrowest span first at each start offset, so the most precise entry is the
    // one kept when a wider overlapping entry follows.
    undeclared.sort_by(|a, b| {
        a.span.byte_start.cmp(&b.span.byte_start).then_with(|| {
            (a.span.byte_end - a.span.byte_start).cmp(&(b.span.byte_end - b.span.byte_start))
        })
    });
    let mut kept: Vec<Diagnostic> = Vec::new();
    for d in undeclared {
        let dp = undeclared_path(&d.message);
        // Collapse only when an already-kept entry names the SAME state path AND
        // overlaps in span. Two distinct undeclared paths that share one CEL
        // slot's whole-slot fallback span (cel-parser 0.10.1 has no per-node
        // offsets) therefore BOTH survive; the sanctioned T4.4(Logic)+T4.5(Staging)
        // pair for one `::set` target (identical path + span) still merges to one.
        if !kept
            .iter()
            .any(|k| undeclared_path(&k.message) == dp && spans_overlap(k.span, d.span))
        {
            kept.push(d);
        }
    }
    out.extend(kept);
    out
}

/// dsl 0.27.0 §4: one of each identical diagnostic re-homed at another
/// file's declaration ([`crate::rel_schema::at_origin`] /
/// [`crate::rel_schema::at_plugin_origin`]) — every use of one faulty
/// declaration in this document carries the same report, which is about
/// the declaration, not the use.
pub(super) fn dedup_rehomed(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::with_capacity(diags.len());
    for d in diags {
        let rehomed = !d.related.is_empty() && d.span.byte_end == 0;
        if rehomed
            && out.iter().any(|k| {
                k.code == d.code
                    && k.message == d.message
                    && k.span == d.span
                    && k.related.len() == d.related.len()
                    && k.related
                        .iter()
                        .zip(&d.related)
                        .all(|(a, b)| a.file == b.file && a.diagnostic.span == b.diagnostic.span)
            })
        {
            continue;
        }
        out.push(d);
    }
    out
}

/// Half-open byte-interval overlap.
fn spans_overlap(a: Span, b: Span) -> bool {
    a.byte_start < b.byte_end && b.byte_start < a.byte_end
}

/// Extract the state path an `E-UNDECLARED` message names, for path-aware dedup.
/// All three producers embed the path as the first backtick-quoted token that
/// starts with a state tier (`scene.`/`run.`/`user.`/`app.`/`quest.`) —
/// `set_op` also quotes `::set` first, so we scan for the tier-prefixed token,
/// not just the first quote. `None` (no tier token) falls back to span-only
/// collapse.
fn undeclared_path(message: &str) -> Option<&str> {
    message.split('`').find(|tok| {
        tok.starts_with("scene.")
            || tok.starts_with("run.")
            || tok.starts_with("user.")
            || tok.starts_with("app.")
            || tok.starts_with("quest.")
            || tok.starts_with("entry.")
    })
}

/// C3 (dsl 0.4.0 §8.2/D12): a failed `uses:`/`extends:`/`components:` import
/// means the checker never built the merge an absence diagnostic's claim
/// depends on, so any such diagnostic is unproven noise on top of the real
/// root cause (the import failure itself). Document-wide, not per-slot: ANY
/// `E-USES-NOT-FOUND`/`E-USES-PARSE`/`E-USES-CYCLE` drops `E-UNDECLARED`/
/// `E-UNDECLARED-REF`/`E-MAYBE-UNSET`/`E-RELATION-UNKNOWN`; ANY
/// `E-COMPONENT-PARSE` drops `E-COMPONENT-UNDECLARED`. Runs AFTER
/// `dedup_undeclared` (whose overlap-merge is orthogonal and still useful
/// pre-suppression), BEFORE `collapse_same_root` (D12) — a diagnostic
/// suppressed here never reaches C1's key-building pass at all.
pub(super) fn suppress_unproven_absence(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let uses_failed = diags.iter().any(|d| {
        matches!(
            d.code.as_str(),
            "E-USES-NOT-FOUND" | "E-USES-PARSE" | "E-USES-CYCLE"
        )
    });
    let component_failed = diags.iter().any(|d| d.code == "E-COMPONENT-PARSE");
    if !uses_failed && !component_failed {
        return diags;
    }
    diags
        .into_iter()
        .filter(|d| {
            let uses_dependent = uses_failed
                && matches!(
                    d.code.as_str(),
                    "E-UNDECLARED" | "E-UNDECLARED-REF" | "E-MAYBE-UNSET" | "E-RELATION-UNKNOWN"
                );
            let component_dependent = component_failed && d.code == "E-COMPONENT-UNDECLARED";
            !(uses_dependent || component_dependent)
        })
        .collect()
}

/// C1's collapsible code set (dsl 0.4.0 §8.2/D11): every "this subject was
/// never declared/available" diagnostic whose message repeats identically at
/// every read site. Site-specific analyses — `E-MAYBE-UNSET` chief among them,
/// whose verdict depends on each site's own dominators/guards — are EXEMPT:
/// not in this set, so each of their occurrences stays independently
/// meaningful.
const COLLAPSE_CODES: &[&str] = &[
    "E-UNDECLARED",
    "E-UNDECLARED-REF",
    "E-RELATION-UNKNOWN",
    "E-CHOICELOG-READ",
    "E-COMPONENT-UNDECLARED",
];

/// C1 (dsl 0.4.0 §8.2/D11): collapse same-(code, root-subject) diagnostics
/// over [`COLLAPSE_CODES`] to ONE primary at the first document-order
/// occurrence, carrying every further occurrence's span onto
/// `primary.covered` (also document order — "five reads of one typo are ONE
/// error"). Runs AFTER the `(byte_start, code)` sort, so `diags` already IS
/// document order: the FIRST time a key is seen becomes the primary and stays
/// exactly where it was pushed; every later diagnostic with the same key is
/// folded into that primary's `covered` instead of being pushed at all — so
/// collapse never reorders a primary and, `check()` being per-file, never
/// crosses files by construction.
pub(super) fn collapse_same_root(diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let mut primaries: std::collections::HashMap<(String, String), usize> =
        std::collections::HashMap::new();
    for d in diags {
        let root = if COLLAPSE_CODES.contains(&d.code.as_str()) {
            if d.code == "E-UNDECLARED" {
                // Finding 5: an `E-UNDECLARED` message's first backtick token
                // IS the root subject for most producers (`cel_resolve.rs`,
                // `defassign.rs`, the persist-target check) — but `set_op.rs`
                // quotes the DIRECTIVE first (`` `::set` target `run.x` … ``),
                // so collapsing on the shared literal "::set" wrongly merged
                // every DISTINCT undeclared `::set` target into one primary.
                // `undeclared_path` (the same tier-prefixed-token extractor
                // `dedup_undeclared` already uses) finds the REAL root path
                // instead; fall back to the first backtick token only when no
                // tier-prefixed token is present (never guessed at).
                undeclared_path(&d.message).or_else(|| first_backtick_token(&d.message))
            } else {
                first_backtick_token(&d.message)
            }
        } else {
            None
        };
        if let Some(root) = root {
            let key = (d.code.clone(), root.to_string());
            if let Some(&idx) = primaries.get(&key) {
                out[idx].covered.push(d.span);
                continue;
            }
            primaries.insert(key, out.len());
        }
        out.push(d);
    }
    out
}

/// The root subject of a C1-eligible diagnostic: the first backtick-quoted
/// token in its message (generalizing [`undeclared_path`] per D11) — the
/// undeclared path, ref name, relation name, reserved path, or component
/// name, whichever the producer names first. `None` when the message carries
/// no backtick token at all, in which case the diagnostic is left uncollapsed
/// (never guessed at) rather than merged on an unproven identity.
pub(super) fn first_backtick_token(message: &str) -> Option<&str> {
    message.split('`').nth(1)
}

/// Re-derive every diagnostic's `line`/`column`/`utf16_range` from its byte
/// offsets through one shared [`TextIndex`], so both the CLI and the LSP report
/// identical positions (the divergence golden). Every fixit `TextEdit` span
/// gets the same treatment — a fixit-carrying diagnostic (`E-PERSIST-REMOVED`,
/// dsl 0.6.0 §2.2) can leave its edit spans zeroed exactly like the
/// house zero-then-normalize convention for `d.span` itself. Offsets are
/// clamped to the text length defensively; they are within bounds by
/// construction.
pub(super) fn normalize_spans(idx: &TextIndex, text: &str, diags: &mut [Diagnostic]) {
    let len = text.len();
    let fix_up = |span: Span| -> Span {
        let mut start = span.byte_start.min(len);
        let mut end = span.byte_end.min(len).max(start);
        // Snap to char boundaries so from_bytes never slices mid-code-point
        // (honors the "never panics" contract even if a producer ever emits an
        // interior offset; unreachable today, all producers emit boundary offsets).
        while start > 0 && !text.is_char_boundary(start) {
            start -= 1;
        }
        while end < len && !text.is_char_boundary(end) {
            end += 1;
        }
        Span::from_bytes(idx, start, end)
    };
    for d in diags {
        d.span = fix_up(d.span);
        for f in &mut d.fixits {
            for e in &mut f.edit {
                e.span = fix_up(e.span);
            }
        }
    }
}
