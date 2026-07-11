//! `Line.when` — the §7.2 gated-line guard slot (dsl 0.4.0 T9). Pins the typed
//! extraction (mirrors `Choice.when`) and the walk-order stability guarantee
//! (B1: a document without `when=` must walk the same slot sequence as before).

use lute_syntax::ast::{CelKind, Document, Line, Node};
use lute_syntax::walk::for_each_cel_slot;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

fn parse_body(src: &str) -> Document {
    let (doc, diags) = lute_syntax::parse(&format!("{HDR}{src}\n"));
    assert!(
        diags
            .iter()
            .all(|d| d.severity != lute_core_span::Severity::Error),
        "unexpected: {diags:?}"
    );
    doc
}

fn first_line(doc: &Document) -> &Line {
    doc.shots[0]
        .body
        .iter()
        .find_map(|n| match n {
            Node::Line(l) => Some(l),
            _ => None,
        })
        .expect("a Line node")
}

#[test]
fn when_attr_is_extracted_to_slot() {
    let doc = parse_body(
        r#"@sofia{when="run.metHelpfully" emotion="soft"}: You helped me back then."#,
    );
    let line = first_line(&doc);
    let w = line.when.as_ref().expect("when extracted");
    assert_eq!(w.raw, "run.metHelpfully");
    assert!(matches!(w.kind, CelKind::Condition));
    assert!(
        line.attrs.iter().all(|a| a.key != "when"),
        "residual attrs keep emotion only"
    );
    assert!(
        line.attrs.iter().any(|a| a.key == "emotion"),
        "emotion attr survives extraction"
    );
}

#[test]
fn line_without_when_is_none() {
    // B1: a plain line (no `when=`) must parse identically — `when` is None.
    let doc = parse_body(r#"@sofia: plain line, no guard."#);
    let line = first_line(&doc);
    assert!(line.when.is_none());
}

#[test]
fn walk_visits_the_when_slot() {
    let doc = parse_body(r#"@sofia{when="run.metHelpfully"}: Guarded."#);
    let mut raws = Vec::new();
    for_each_cel_slot(&doc, &mut |slot| raws.push(slot.raw.clone()));
    assert!(
        raws.contains(&"run.metHelpfully".to_string()),
        "walk missed the when slot: {raws:?}"
    );
}

#[test]
fn slot_order_is_stable() {
    // Two-slot doc WITHOUT `when=` — B1: the walk visits the same slots in
    // the same order the pre-T9 walker did (both are plain `@ref` attrs; no
    // `when` slot precedes them since there is none).
    let doc = parse_body(r#"@sofia{mood=@moodRef focus=@focusRef}: no guard here."#);
    let mut raws = Vec::new();
    for_each_cel_slot(&doc, &mut |slot| raws.push(slot.raw.clone()));
    assert_eq!(raws, vec!["@moodRef".to_string(), "@focusRef".to_string()]);
}
