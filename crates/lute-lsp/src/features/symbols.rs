//! `textDocument/documentSymbol` (Task 6.4).
//!
//! A pure function over a parsed [`Document`] (plus the backend's
//! [`lute_core_span::TextIndex`]) that projects the document outline:
//!
//! - one [`DocumentSymbol`] per shot — [`SymbolKind::MODULE`], named by the shot
//!   heading (the `## …` text);
//! - one [`DocumentSymbol`] per top-level `<quest>` (dsl 0.2.0 §6.3) —
//!   [`SymbolKind::NAMESPACE`], named by its `id`;
//! - each `<branch>` / `<match>` inside a shot (or quest) as a nested child —
//!   [`SymbolKind::ENUM`] for a branch (a closed set of choices) and
//!   [`SymbolKind::OBJECT`] for a match (a subject dispatched over arms) — found
//!   depth-first so a branch nested in a match arm (or a choice body) still nests
//!   under its shot/quest;
//! - each `<on>` / `<objective>` inside a quest as a nested child (dsl 0.2.0
//!   §4, §6.4) — [`SymbolKind::EVENT`] named by the trigger's `event`, and
//!   [`SymbolKind::PROPERTY`] named by the objective's `id`.
//!
//! ## Ranges
//! `range` is the construct's full span; `selection_range` is the "interesting"
//! sub-span the editor reveals — a shot's `## …` heading line, a block's open
//! keyword (`<branch` / `<match`). Both are mapped from byte spans through the
//! shared [`lute_core_span::TextIndex`] by [`crate::backend::span_to_range`], so
//! symbol positions carry the same UTF-16-correct ranges as every other surface.

use lute_core_span::TextIndex;
use lute_syntax::ast::{Arm, Document, Match, Node, Quest, Shot};
use tower_lsp_server::ls_types::{DocumentSymbol, Range, SymbolKind};

use crate::backend::{byte_to_position, span_to_range};
use crate::features::byte_span;

/// The document outline: one shot symbol per shot, with its `<branch>`/`<match>`
/// blocks nested as children.
pub fn document_symbols(doc: &Document, idx: &TextIndex) -> Vec<DocumentSymbol> {
    let mut out: Vec<DocumentSymbol> = doc.shots.iter().map(|s| shot_symbol(s, idx)).collect();
    out.extend(doc.quests.iter().map(|q| quest_symbol(q, idx)));
    out
}

/// A shot → a MODULE symbol named by its heading, children = nested blocks.
fn shot_symbol(shot: &Shot, idx: &TextIndex) -> DocumentSymbol {
    let range = span_to_range(&shot.span, idx);
    // Selection = the `## <heading>` line: from the shot start across `## ` + text.
    let head_start = shot.span.byte_start;
    let head_end = head_start + "## ".len() + shot.heading.len();
    let selection_range = span_to_range(&byte_span(head_start, head_end), idx);
    let mut children = Vec::new();
    collect_children(&shot.body, idx, &mut children);
    symbol(
        shot.heading.clone(),
        SymbolKind::MODULE,
        range,
        selection_range,
        children,
    )
}

/// A `<quest>` -> a top-level symbol named by its id, children = its nested
/// `<on>`/`<objective>` arms (dsl 0.2.0 §6.3). `Quest` is not a [`Node`] (a
/// top-level declaration alongside [`Shot`]), so it gets its own entry point
/// mirroring `shot_symbol`.
fn quest_symbol(quest: &Quest, idx: &TextIndex) -> DocumentSymbol {
    let range = span_to_range(&quest.span, idx);
    let sel = keyword_range(quest.span.byte_start, "<quest", idx);
    let mut children = Vec::new();
    collect_children(&quest.body, idx, &mut children);
    symbol(quest.id.clone(), SymbolKind::NAMESPACE, range, sel, children)
}

/// Collect the `<branch>`/`<match>`/`<on>`/`<objective>` blocks in `nodes` as
/// child symbols, descending through nested bodies so any depth of nesting is
/// preserved.
fn collect_children(nodes: &[Node], idx: &TextIndex, out: &mut Vec<DocumentSymbol>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                let mut kids = Vec::new();
                for c in &b.choices {
                    collect_children(&c.body, idx, &mut kids);
                }
                let name = if b.id.is_empty() {
                    "branch".to_string()
                } else {
                    b.id.clone()
                };
                let sel = keyword_range(b.span.byte_start, "<branch", idx);
                out.push(symbol(
                    name,
                    SymbolKind::ENUM,
                    span_to_range(&b.span, idx),
                    sel,
                    kids,
                ));
            }
            Node::Match(m) => {
                let mut kids = Vec::new();
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_children(body, idx, &mut kids);
                        }
                    }
                }
                let sel = keyword_range(m.span.byte_start, "<match", idx);
                out.push(symbol(
                    match_name(m),
                    SymbolKind::OBJECT,
                    span_to_range(&m.span, idx),
                    sel,
                    kids,
                ));
            }
            Node::Hub(h) => {
                let mut kids = Vec::new();
                for c in &h.choices {
                    collect_children(&c.body, idx, &mut kids);
                }
                let sel = keyword_range(h.span.byte_start, "<hub", idx);
                out.push(symbol(
                    "hub".to_string(),
                    SymbolKind::ENUM,
                    span_to_range(&h.span, idx),
                    sel,
                    kids,
                ));
            }
            Node::On(o) => {
                let mut kids = Vec::new();
                collect_children(&o.body, idx, &mut kids);
                let sel = keyword_range(o.span.byte_start, "<on", idx);
                out.push(symbol(
                    o.event.clone(),
                    SymbolKind::EVENT,
                    span_to_range(&o.span, idx),
                    sel,
                    kids,
                ));
            }
            Node::Objective(ob) => {
                let mut kids = Vec::new();
                collect_children(&ob.body, idx, &mut kids);
                let sel = keyword_range(ob.span.byte_start, "<objective", idx);
                out.push(symbol(
                    ob.id.clone(),
                    SymbolKind::PROPERTY,
                    span_to_range(&ob.span, idx),
                    sel,
                    kids,
                ));
            }
            // Leaves and staging blocks are not outline symbols.
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
        }
    }
}

/// A match's display name: its subject text (`scene.choices.number`), or the bare
/// keyword when the `on=` subject is absent.
fn match_name(m: &Match) -> String {
    if m.subject.raw.is_empty() {
        "match".to_string()
    } else {
        m.subject.raw.clone()
    }
}

/// The `Range` of an open keyword (`<branch`/`<match`) starting at `start`.
fn keyword_range(start: usize, keyword: &str, idx: &TextIndex) -> Range {
    Range {
        start: byte_to_position(start, idx),
        end: byte_to_position(start + keyword.len(), idx),
    }
}

/// Assemble a [`DocumentSymbol`], omitting the optional `children` when empty.
fn symbol(
    name: String,
    kind: SymbolKind,
    range: Range,
    selection_range: Range,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    #[allow(deprecated)] // `deprecated` field is required by the struct literal.
    DocumentSymbol {
        name,
        detail: None,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::parse;

    const BIANCA: &str = include_str!("../../../../docs/examples/bianca-s01ep02.lute");

    fn symbols(text: &str) -> Vec<DocumentSymbol> {
        let (doc, _) = parse(text);
        document_symbols(&doc, &TextIndex::new(text))
    }

    /// ACCEPTANCE: the bianca example has 5 shots → exactly 5 top-level symbols,
    /// each a MODULE named by its heading.
    #[test]
    fn bianca_has_five_shot_symbols() {
        let syms = symbols(BIANCA);
        assert_eq!(syms.len(), 5, "5 shots → 5 top-level symbols");
        assert!(syms.iter().all(|s| s.kind == SymbolKind::MODULE));
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["Shot 1.", "Shot 2.", "Shot 3.", "Shot 4.", "Shot 5."]
        );
    }

    /// ACCEPTANCE (added): the `<branch id="number">` in shot 4 is a nested child
    /// symbol (ENUM) under its shot, named by the branch id.
    #[test]
    fn branch_is_a_nested_child_symbol() {
        let syms = symbols(BIANCA);
        let shot4 = &syms[3]; // "Shot 4." holds the `<branch id="number">`.
        assert_eq!(shot4.name, "Shot 4.");
        let kids = shot4.children.as_ref().expect("shot 4 has children");
        let branch = kids
            .iter()
            .find(|c| c.kind == SymbolKind::ENUM)
            .expect("a branch child (ENUM)");
        assert_eq!(branch.name, "number", "named by the branch id");
        // The selection range must be contained by the enclosing range.
        assert!(branch.selection_range.start.line >= branch.range.start.line);
        assert!(branch.selection_range.end.line <= branch.range.end.line);
    }

    /// The `<match on="scene.choices.number">` in shot 5 nests as an OBJECT child
    /// named by its subject.
    #[test]
    fn match_is_a_nested_child_symbol() {
        let syms = symbols(BIANCA);
        let shot5 = &syms[4];
        assert_eq!(shot5.name, "Shot 5.");
        let kids = shot5.children.as_ref().expect("shot 5 has children");
        let m = kids
            .iter()
            .find(|c| c.kind == SymbolKind::OBJECT)
            .expect("a match child (OBJECT)");
        assert_eq!(m.name, "scene.choices.number", "named by the match subject");
    }

    /// A shot with no logic block has no children (the `children` field is `None`,
    /// not an empty vector).
    #[test]
    fn shot_without_blocks_has_no_children() {
        let text = "## Shot 1.\n:narrator: just prose.\n::bg{location=\"x\"}\n";
        let syms = symbols(text);
        assert_eq!(syms.len(), 1);
        assert!(syms[0].children.is_none(), "no branch/match → no children");
    }

    /// The shot's `selection_range` is the heading line and is contained by the
    /// full `range`.
    #[test]
    fn shot_selection_range_is_the_heading() {
        let text = "## Shot 1.\n:narrator: prose.\n:narrator: more.\n";
        let s = &symbols(text)[0];
        assert_eq!(s.selection_range.start.line, 0, "heading is line 0");
        assert_eq!(s.selection_range.start.character, 0);
        // `## Shot 1.` is 10 UTF-16 units.
        assert_eq!(s.selection_range.end.character, "## Shot 1.".len() as u32);
        assert!(
            s.range.end.line >= s.selection_range.end.line,
            "range encloses selection"
        );
    }

    // ---- dsl 0.2.0 §6.3/§4: quest / on / objective symbols ----

    const QUEST_DOC: &str = "---\nkind: quest\n---\n\
        <quest id=\"q\">\n\
        <objective id=\"o\" done=\"a\">\n:narrator: hi\n</objective>\n\
        <on event=\"questComplete\">\n:narrator: bye\n</on>\n\
        </quest>\n";

    /// ACCEPTANCE: a `<quest>` is a top-level symbol named by its id, with an
    /// EVENT child for `<on>` and a PROPERTY child for `<objective>` — before
    /// the fix, `document_symbols` walked `doc.shots` only (a quest doc has
    /// none) and `<on>`/`<objective>` were Plan-A no-ops, so a quest doc
    /// yielded NO symbols at all.
    #[test]
    fn quest_is_a_top_level_symbol_with_on_and_objective_children() {
        let syms = symbols(QUEST_DOC);
        assert_eq!(syms.len(), 1, "one top-level quest symbol");
        let q = &syms[0];
        assert_eq!(q.name, "q", "named by the quest id");
        let kids = q.children.as_ref().expect("quest has children");
        assert_eq!(kids.len(), 2);
        let on = kids
            .iter()
            .find(|c| c.kind == SymbolKind::EVENT)
            .expect("an <on> child (EVENT)");
        assert_eq!(on.name, "questComplete", "named by the event");
        let obj = kids
            .iter()
            .find(|c| c.kind == SymbolKind::PROPERTY)
            .expect("an <objective> child (PROPERTY)");
        assert_eq!(obj.name, "o", "named by the objective id");
    }
}
