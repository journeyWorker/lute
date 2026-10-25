//! dsl 0.51.0 §3.4 (cast presence row): a loop head is reached again after
//! its region ran, so the walk applies `kill_writes` to the region at the
//! head — the treatment a `<hub>` gets for its bodies. A write inside the
//! region kills a guard read at the head; no iteration is involved.

use lute_check::{check, CheckInput, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::schema::{CastMember, OccasionDecl, OccasionSelect};

/// The core snapshot with `corvin` present while `run.withUs`, and a
/// `hubVisit` occasion.
fn input(text: &str) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.cast.insert(
        "corvin".into(),
        CastMember {
            id: "corvin".into(),
            name: None,
            present: Some("run.withUs == true".into()),
            emotions: None,
            assume: None,
            shared_name: None,
        },
    );
    snapshot.occasions.insert(
        "hubVisit".into(),
        OccasionDecl {
            name: "hubVisit".into(),
            select: OccasionSelect::First,
            ..Default::default()
        },
    );
    CheckInput::new(
        text.to_string(),
        "presence".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snapshot,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

/// A lore beat run while `run.withUs`, whose loop has `corvin` speak at its
/// head and then runs `write`.
fn beat(write: &str) -> String {
    format!(
        "---\nkind: lore\nid: walk\ntitle: Walk\nstate:\n  run.withUs: {{ type: bool, default: \
         false }}\n  run.x: {{ type: int, default: 0 }}\n---\n\
         <beat id=\"road\" on=\"hubVisit\" once=\"false\" when=\"run.withUs == true\">\n  \
         ::label{{name=\"top\"}}\n  @corvin: Onward.\n  {write}\n  \
         ::jump{{to=\"top\" max=\"2\"}}\n</beat>\n"
    )
}

fn absent(text: &str) -> Vec<Diagnostic> {
    check(&input(text))
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "W-CAST-ABSENT")
        .collect()
}

#[test]
fn a_write_inside_the_loop_kills_the_guard_at_the_head() {
    let ds = absent(&beat("::set{run.withUs = false}"));
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].span.line, 11, "the `@corvin: Onward.` line: {ds:?}");
    // A write the guard does not read leaves it live.
    assert_eq!(absent(&beat("::set{run.x = 1}")), Vec::new());
}
