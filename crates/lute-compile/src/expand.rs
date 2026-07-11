//! D4: compile-time `@ref`/`@fn(args)`/`$` → inline-CEL expansion.
//!
//! The text-level expander (`DefTable`, `expand_cel`, `expand_ref`,
//! `substitute_params`, `subject_text`) moved to `lute_check::cel_expand`
//! (0.4.0 T1, D2 — the checker performs `@def` expansion itself ahead of
//! `decide()`). This module keeps the AST-walking driver: it threads the
//! enclosing `<match>` subject through every CEL slot in the document and
//! calls the moved `expand_cel` per slot. The artifact carries no defs
//! table — output CEL is `@`/`$`-free.

use lute_check::cel_expand::{expand_cel, DefTable};
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_syntax::ast::{Arm, Attr, AttrValue, CelSlot, ClipNode, Document, Node};

/// Expand every CEL slot in the document in place. Returns diagnostics for
/// expander failures (`E-COMPILE-EXPAND`: cycle / unknown def / arity — the
/// latter two gate-proven unreachable, kept total). Never panics.
pub fn expand_document(doc: &mut Document, defs: &DefTable<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for shot in &mut doc.shots {
        expand_nodes(&mut shot.body, defs, None, &mut diags);
    }
    for quest in &mut doc.quests {
        if let Some(s) = &mut quest.start {
            expand_slot(s, defs, None, &mut diags);
        }
        if let Some(f) = &mut quest.fail {
            expand_slot(f, defs, None, &mut diags);
        }
        expand_attrs(&mut quest.attrs, defs, None, &mut diags);
        expand_nodes(&mut quest.body, defs, None, &mut diags);
    }
    diags
}

fn expand_nodes(
    nodes: &mut [Node],
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Line(l) => expand_attrs(&mut l.attrs, defs, subject, diags),
            Node::Directive(d) => expand_attrs(&mut d.attrs, defs, subject, diags),
            Node::Set(s) => expand_slot(&mut s.expr, defs, subject, diags),
            Node::Branch(b) => {
                expand_attrs(&mut b.attrs, defs, subject, diags);
                for c in &mut b.choices {
                    if let Some(w) = &mut c.when {
                        expand_slot(w, defs, subject, diags);
                    }
                    expand_attrs(&mut c.attrs, defs, subject, diags);
                    expand_nodes(&mut c.body, defs, subject, diags);
                }
            }
            Node::Match(m) => {
                // The subject itself expands in the OUTER scope (a nested
                // match's `$` refers to its own subject only after this).
                expand_slot(&mut m.subject, defs, subject, diags);
                let inner = m.subject.raw.clone();
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            expand_slot(test, defs, Some(&inner), diags);
                            expand_nodes(body, defs, Some(&inner), diags);
                        }
                        Arm::Otherwise { body, .. } => {
                            expand_nodes(body, defs, Some(&inner), diags)
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                if let Some(d) = &mut t.duration {
                    expand_slot(d, defs, subject, diags);
                }
                for track in &mut t.tracks {
                    for clip in &mut track.clips {
                        match &mut clip.node {
                            ClipNode::Directive(d) => {
                                expand_attrs(&mut d.attrs, defs, subject, diags)
                            }
                            ClipNode::Set(s) => expand_slot(&mut s.expr, defs, subject, diags),
                        }
                    }
                }
            }
            Node::Hub(h) => {
                expand_attrs(&mut h.attrs, defs, subject, diags);
                for c in &mut h.choices {
                    if let Some(w) = &mut c.when {
                        expand_slot(w, defs, subject, diags);
                    }
                    expand_attrs(&mut c.attrs, defs, subject, diags);
                    expand_nodes(&mut c.body, defs, subject, diags);
                }
            }
            Node::On(on) => {
                if let Some(w) = &mut on.when {
                    expand_slot(w, defs, subject, diags);
                }
                expand_attrs(&mut on.attrs, defs, subject, diags);
                expand_nodes(&mut on.body, defs, subject, diags);
            }
            Node::Objective(o) => {
                expand_slot(&mut o.done, defs, subject, diags);
                if let Some(w) = &mut o.when {
                    expand_slot(w, defs, subject, diags);
                }
                expand_attrs(&mut o.attrs, defs, subject, diags);
                expand_nodes(&mut o.body, defs, subject, diags);
            }
            // Fact args are ground (no `@ref`/`@fn`/`$`, no `{{…}}`) — nothing
            // to expand (0.3.0 T2).
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

fn expand_attrs(
    attrs: &mut [Attr],
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    for a in attrs {
        if let AttrValue::Ref(slot) = &mut a.value {
            expand_slot(slot, defs, subject, diags);
        }
    }
}

fn expand_slot(
    slot: &mut CelSlot,
    defs: &DefTable<'_>,
    subject: Option<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    match expand_cel(&slot.raw, defs, subject, &mut Vec::new()) {
        Ok(s) => slot.raw = s,
        Err(message) => diags.push(Diagnostic {
            code: "E-COMPILE-EXPAND".to_string(),
            severity: Severity::Error,
            message,
            span: slot.span,
            layer: Layer::Cel,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lute_manifest::types::Type;

    use super::*;

    type Tables = (
        BTreeMap<String, String>,
        BTreeMap<String, Vec<(String, Type)>>,
    );

    fn tables(bodies: &[(&str, &str)], params: &[(&str, &[&str])]) -> Tables {
        let b = bodies
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let p = params
            .iter()
            .map(|(k, ps)| {
                (
                    k.to_string(),
                    ps.iter().map(|n| (n.to_string(), Type::Number)).collect(),
                )
            })
            .collect();
        (b, p)
    }

    #[test]
    fn expand_document_rewrites_slots_with_match_subject_scope() {
        let src = "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n\n## Shot 1.\n\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    @fixer{mono}: a\n  </when>\n  <when test=\"$ == 'blunt'\">\n    @fixer{mono}: b\n  </when>\n  <otherwise>\n    @fixer{mono}: c\n  </otherwise>\n</match>\n";
        let (mut doc, diags) = lute_syntax::parse(src);
        assert!(diags
            .iter()
            .all(|d| d.severity != lute_core_span::Severity::Error));
        let t = tables(&[("fond", "scene.affect.bianca >= 1")], &[]);
        let defs = DefTable {
            bodies: &t.0,
            params: &t.1,
        };
        let ediags = expand_document(&mut doc, &defs);
        assert!(ediags.is_empty(), "{ediags:#?}");
        let lute_syntax::ast::Node::Match(m) = &doc.shots[0].body[0] else {
            panic!("first node is the match");
        };
        let tests: Vec<&str> = m
            .arms
            .iter()
            .filter_map(|a| match a {
                lute_syntax::ast::Arm::When { test, .. } => Some(test.raw.as_str()),
                lute_syntax::ast::Arm::Otherwise { .. } => None,
            })
            .collect();
        assert_eq!(
            tests,
            vec![
                "(scene.affect.bianca >= 1)",
                "scene.choices.number == 'blunt'"
            ]
        );
    }

    // Plan D review (Important finding 1): `expand_document` walked only
    // `doc.shots` and `Node::On`/`Node::Objective` were a no-op, so a
    // checker-clean quest using a declared `@def` in an objective's `done`
    // (or `$` in a `<match>` nested in a quest body, or `@def` in an `<on
    // when>`) reached `stage::walk_quest` UN-expanded — `@`/`$` leaked into
    // the artifact instead of `@`/`$`-free CEL.
    #[test]
    fn expand_document_traverses_quest_bodies_and_expands_on_objective_slots() {
        let src = "---\nkind: quest\nstate:\n  run.region: { type: string, default: \"\" }\n  run.act: { type: number, default: 0 }\n---\n\n<quest id=\"q1\" title=\"Q1\">\n<objective id=\"o1\" title=\"O1\" done=\"@inGrove\"/>\n\n<match on=\"run.region\">\n  <when test=\"$ == 'grove'\">\n  ::set{run.act = 1}\n  </when>\n  <otherwise>\n  ::set{run.act = 0}\n  </otherwise>\n</match>\n\n<on event=\"questComplete\" when=\"@inGrove\">\n@narrator: done\n</on>\n</quest>\n";
        let (mut doc, diags) = lute_syntax::parse(src);
        assert!(
            diags.iter().all(|d| d.severity != lute_core_span::Severity::Error),
            "{diags:#?}"
        );
        assert_eq!(doc.quests.len(), 1, "fixture must parse one <quest>");

        let t = tables(&[("inGrove", "run.region == 'grove'")], &[]);
        let defs = DefTable {
            bodies: &t.0,
            params: &t.1,
        };
        let ediags = expand_document(&mut doc, &defs);
        assert!(ediags.is_empty(), "{ediags:#?}");

        let quest = &doc.quests[0];
        let Node::Objective(o) = &quest.body[0] else {
            panic!("expected objective, got {:?}", quest.body.first());
        };
        assert_eq!(o.done.raw, "(run.region == 'grove')");
        assert!(!o.done.raw.contains('@'));

        let Node::Match(m) = &quest.body[1] else {
            panic!("expected match, got {:?}", quest.body.get(1));
        };
        let tests: Vec<&str> = m
            .arms
            .iter()
            .filter_map(|a| match a {
                Arm::When { test, .. } => Some(test.raw.as_str()),
                Arm::Otherwise { .. } => None,
            })
            .collect();
        assert_eq!(tests, vec!["run.region == 'grove'"]);

        let Node::On(on) = &quest.body[2] else {
            panic!("expected on, got {:?}", quest.body.get(2));
        };
        let when = on.when.as_ref().expect("on.when");
        assert_eq!(when.raw, "(run.region == 'grove')");
        assert!(!when.raw.contains('@'));
    }
}
