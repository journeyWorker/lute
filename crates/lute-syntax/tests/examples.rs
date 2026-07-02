//! Integration test: the worked bianca example must parse cleanly (dsl §4.3, §7).

use lute_syntax::ast::{Arm, ClipNode, Node, TrackKey};

const BIANCA: &str = "../../docs/examples/bianca-s01ep02.lute";

#[test]
fn parses_bianca_example_without_parse_errors() {
    let text = std::fs::read_to_string(BIANCA).unwrap();
    let (doc, diags) = lute_syntax::parse(&text);
    let parse_errs: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .collect();
    assert!(
        parse_errs.is_empty(),
        "unexpected parse errors: {parse_errs:?}"
    );
    assert_eq!(doc.shots.len(), 5); // Shot 1..5
}

/// The example exercises every §7 construct — assert the recursive assembly, not
/// just the shot count, so a structural regression is caught.
#[test]
fn bianca_block_assembly_is_correct() {
    let text = std::fs::read_to_string(BIANCA).unwrap();
    let (doc, _diags) = lute_syntax::parse(&text);

    // Document title (H1) survives the multi-byte em-dash.
    assert_eq!(
        doc.title.as_ref().map(|(t, _)| t.as_str()),
        Some("S01EP02 — Behold the Performance of All-Purpose Bianca"),
    );
    assert_eq!(
        doc.shots.iter().map(|s| s.number).collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3), Some(4), Some(5)]
    );

    // Shot 3: <timeline duration="1.4"> with 4 tracks; the beam clip lands at 0.5.
    let timeline = doc.shots[2]
        .body
        .iter()
        .find_map(|n| {
            if let Node::Timeline(t) = n {
                Some(t)
            } else {
                None
            }
        })
        .expect("shot 3 has a timeline");
    assert_eq!(
        timeline.duration.as_ref().map(|c| c.raw.as_str()),
        Some("1.4")
    );
    assert_eq!(timeline.tracks.len(), 4);
    assert!(matches!(&timeline.tracks[0].key, TrackKey::Subject(s) if s == "camera"));
    assert!(matches!(&timeline.tracks[1].key, TrackKey::Channel(c) if c == "fg"));
    // First camera clip omits `at`; the second carries at="0.5".
    assert_eq!(timeline.tracks[0].clips[0].at, None);
    assert_eq!(timeline.tracks[0].clips[1].at, Some(0.5));
    // `at` is lifted onto the Clip, not left on the directive attrs.
    if let ClipNode::Directive(d) = &timeline.tracks[0].clips[1].node {
        assert!(
            d.attrs.iter().all(|a| a.key != "at"),
            "at must move to Clip.at"
        );
    } else {
        panic!("expected a directive clip");
    }

    // Shot 4: <branch id="number"> with two choices (the second sets affect).
    let branch = doc.shots[3]
        .body
        .iter()
        .find_map(|n| {
            if let Node::Branch(b) = n {
                Some(b)
            } else {
                None
            }
        })
        .expect("shot 4 has a branch");
    assert_eq!(branch.id, "number");
    assert_eq!(branch.choices.len(), 2);
    assert_eq!(branch.choices[1].id, "soft");
    assert_eq!(branch.choices[1].label, "Ask gently");
    assert_eq!(branch.choices[1].body.len(), 2); // :line + ::set

    // Shot 5: <match on="…"> with two <when>s + <otherwise>.
    let m = doc.shots[4]
        .body
        .iter()
        .find_map(|n| {
            if let Node::Match(m) = n {
                Some(m)
            } else {
                None
            }
        })
        .expect("shot 5 has a match");
    assert_eq!(m.subject.raw, "scene.choices.number");
    assert_eq!(m.arms.len(), 3);
    assert!(matches!(&m.arms[0], Arm::When { test, .. } if test.raw == "@fond"));
    assert!(matches!(&m.arms[2], Arm::Otherwise { .. }));
}
