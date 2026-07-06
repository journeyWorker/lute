//! `textDocument/foldingRange` (Task 6.4).
//!
//! A pure function over a parsed [`Document`] (plus the backend's
//! [`lute_core_span::TextIndex`], which owns the byte -> line math) that emits one
//! [`FoldingRange`] per FOLDABLE multi-line region:
//!
//! - every shot (`## …` heading through its last body node);
//! - every `<...>` logic/staging block: `<branch>`, `<match>`, `<timeline>`;
//! - every `<track>` inside a `<timeline>`.
//!
//! A region that begins and ends on the SAME source line is not foldable (there
//! is nothing to collapse), so single-line blocks are dropped. `<choice>` /
//! `<when>` / `<otherwise>` bodies are traversed for the nested `<branch>`/
//! `<match>`/`<timeline>` they may contain, but the arm/choice wrappers
//! themselves are not folded — the brief scopes folds to shots, `<...>` blocks,
//! and per-`<track>`, matching the architecture's structural units.
//!
//! ## Why a `TextIndex`
//! A [`FoldingRange`] is line-based: `start_line`/`end_line` are 0-based document
//! lines. A [`lute_core_span::Span`] carries a 1-based start `line`, but its END
//! line must be derived from `byte_end`. Rather than re-count newlines here, we
//! map both endpoints through the same `TextIndex` the backend already built —
//! the one convention the whole crate shares, so no positions ever drift.

use lute_core_span::{Span, TextIndex};
use lute_syntax::ast::{Arm, Document, Node};
use tower_lsp_server::ls_types::{FoldingRange, FoldingRangeKind};

/// Every foldable multi-line region of `doc`, as 0-based line ranges.
///
/// Order follows a depth-first document walk: each shot's own range is emitted
/// before the ranges of the blocks nested inside it.
pub fn folding_ranges(doc: &Document, idx: &TextIndex) -> Vec<FoldingRange> {
    let mut out = Vec::new();
    for shot in &doc.shots {
        push_fold(&mut out, &shot.span, idx);
        fold_nodes(&shot.body, idx, &mut out);
    }
    out
}

/// Emit the folds for a body's `<...>` blocks (and their nested blocks).
fn fold_nodes(nodes: &[Node], idx: &TextIndex, out: &mut Vec<FoldingRange>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                push_fold(out, &b.span, idx);
                for c in &b.choices {
                    fold_nodes(&c.body, idx, out);
                }
            }
            Node::Match(m) => {
                push_fold(out, &m.span, idx);
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            fold_nodes(body, idx, out);
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                push_fold(out, &t.span, idx);
                for track in &t.tracks {
                    push_fold(out, &track.span, idx);
                }
            }
            Node::Hub(h) => {
                push_fold(out, &h.span, idx);
                for c in &h.choices {
                    fold_nodes(&c.body, idx, out);
                }
            }
            // Leaf nodes (`:line`, `::directive`, `::set`) are single constructs,
            // not foldable regions.
            Node::Line(_) | Node::Directive(_) | Node::Set(_) => {}
        }
    }
}

/// Push a `region` fold for `span` if it covers more than one line. `start_line`
/// / `end_line` come from the shared `TextIndex` (0-based, de-1-indexed).
fn push_fold(out: &mut Vec<FoldingRange>, span: &Span, idx: &TextIndex) {
    let start_line = idx.position(span.byte_start).line - 1;
    let end_line = idx.position(span.byte_end).line - 1;
    if end_line <= start_line {
        return; // single-line region: nothing to collapse.
    }
    out.push(FoldingRange {
        start_line,
        end_line,
        kind: Some(FoldingRangeKind::Region),
        ..Default::default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::parse;

    /// The full worked example — the fold-count acceptance fixture.
    const BIANCA: &str = include_str!("../../../../docs/examples/bianca-s01ep02.lute");

    fn folds(text: &str) -> Vec<FoldingRange> {
        let (doc, _) = parse(text);
        folding_ranges(&doc, &TextIndex::new(text))
    }

    /// Independently count the foldable multi-line regions of `doc` by the SAME
    /// structural rule the impl uses, so the assertion is derived, not guessed.
    fn expected_multiline(text: &str) -> (usize, usize, usize, usize, usize) {
        let (doc, _) = parse(text);
        let idx = TextIndex::new(text);
        let ml = |s: &Span| idx.position(s.byte_end).line > idx.position(s.byte_start).line;
        let (mut shots, mut timelines, mut tracks, mut branches, mut matches) = (0, 0, 0, 0, 0);
        for shot in &doc.shots {
            if ml(&shot.span) {
                shots += 1;
            }
            count_nodes(
                &shot.body,
                &idx,
                &mut timelines,
                &mut tracks,
                &mut branches,
                &mut matches,
            );
        }
        (shots, timelines, tracks, branches, matches)
    }

    fn count_nodes(
        nodes: &[Node],
        idx: &TextIndex,
        tl: &mut usize,
        tr: &mut usize,
        br: &mut usize,
        mt: &mut usize,
    ) {
        let ml = |s: &Span| idx.position(s.byte_end).line > idx.position(s.byte_start).line;
        for node in nodes {
            match node {
                Node::Branch(b) => {
                    if ml(&b.span) {
                        *br += 1;
                    }
                    for c in &b.choices {
                        count_nodes(&c.body, idx, tl, tr, br, mt);
                    }
                }
                Node::Match(m) => {
                    if ml(&m.span) {
                        *mt += 1;
                    }
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                count_nodes(body, idx, tl, tr, br, mt);
                            }
                        }
                    }
                }
                Node::Timeline(t) => {
                    if ml(&t.span) {
                        *tl += 1;
                    }
                    for track in &t.tracks {
                        if ml(&track.span) {
                            *tr += 1;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// ACCEPTANCE: fold count on the bianca example. The document contributes
    /// 5 shots + 1 `<timeline>` + 4 `<track>`s + 1 `<branch>` + 1 `<match>` = 12
    /// foldable multi-line regions. We assert the exact per-category breakdown
    /// (so a regression that drops or double-counts a category is caught) and the
    /// total.
    #[test]
    fn bianca_fold_count_is_shots_plus_blocks() {
        let (shots, timelines, tracks, branches, matches) = expected_multiline(BIANCA);
        assert_eq!(
            (shots, timelines, tracks, branches, matches),
            (5, 1, 4, 1, 1)
        );
        let expected = shots + timelines + tracks + branches + matches;
        assert_eq!(
            expected, 12,
            "5 shots + 1 timeline + 4 tracks + 1 branch + 1 match"
        );
        assert_eq!(folds(BIANCA).len(), expected);
    }

    /// ACCEPTANCE (added): the `<timeline>` fold spans its full line range. In the
    /// bianca example the block opens at `<timeline duration="1.4">` and closes at
    /// `</timeline>` fifteen lines later.
    #[test]
    fn timeline_fold_spans_full_block() {
        let idx = TextIndex::new(BIANCA);
        // The real open tag carries `duration=`; a bare `<timeline>` also appears
        // in the file's block comment, which the parser blanks.
        let open = BIANCA.find("<timeline duration").unwrap();
        let close = BIANCA.find("</timeline>").unwrap();
        let want_start = idx.position(open).line - 1;
        let want_end = idx.position(close).line - 1;
        assert!(
            want_end > want_start + 1,
            "timeline is genuinely multi-line"
        );
        let tl = folds(BIANCA)
            .into_iter()
            .find(|f| f.start_line == want_start)
            .expect("a fold starting on the <timeline> line");
        assert_eq!(tl.end_line, want_end, "fold ends on the </timeline> line");
        assert_eq!(tl.kind, Some(FoldingRangeKind::Region));
    }

    /// Each of the five shots yields exactly one shot-level fold, starting on its
    /// `## ` heading line.
    #[test]
    fn every_shot_folds_from_its_heading() {
        let idx = TextIndex::new(BIANCA);
        let all = folds(BIANCA);
        for marker in [
            "## Shot 1.",
            "## Shot 2.",
            "## Shot 3.",
            "## Shot 4.",
            "## Shot 5.",
        ] {
            let head = idx.position(BIANCA.find(marker).unwrap()).line - 1;
            assert!(
                all.iter()
                    .any(|f| f.start_line == head && f.end_line > head),
                "no fold anchored on {marker}"
            );
        }
    }

    /// Leaf nodes never contribute a fold: a shot whose body is a single `:line`
    /// is two source lines (heading + line), so the SHOT folds — but the lone
    /// `:line` node inside it does not add a second fold.
    #[test]
    fn leaf_nodes_do_not_fold() {
        let text = "## Shot 1.\n:narrator: only prose.\n";
        let all = folds(text);
        assert_eq!(all.len(), 1, "only the shot folds; the lone :line does not");
    }
}
