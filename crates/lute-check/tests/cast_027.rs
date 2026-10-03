//! dsl 0.27.0 §9 (T3-25): a quest's `questComplete` handler assumes the
//! quest's completion — one required objective's `done` held — for
//! `W-CAST-ABSENT`.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::CapabilitySnapshot;

const ABSENT: &str = "W-CAST-ABSENT";

/// The hollow-ward shape: Tobias is present while he follows.
const VOCAB: &str = "entities:\n  person: { members: [tobias] }\n\
    relations:\n  following: { args: [person], tier: run }\n\
    state:\n  run.fate: { type: string, default: \"\" }\n  run.night: { type: int, default: 0 }\n\
    defs:\n  withToby: { type: bool, cel: \"holds('following', ['tobias'])\" }\n  \
    out: { type: bool, cel: \"run.fate == 'escaped'\" }\n  \
    dead: { type: bool, cel: \"run.fate == 'lost'\" }\n";

fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    for (id, present) in [("tobias", Some("holds('following', ['tobias'])")), ("nell", None)] {
        snap.cast.insert(
            id.into(),
            CastMember {
                id: id.into(),
                name: None,
                present: present.map(str::to_string),
                emotions: None,
                assume: None,
                shared_name: None,
            },
        );
    }
    snap
}

/// A quest document: `<quest {attrs}>` with `body`.
fn quest(attrs: &str, body: &str) -> String {
    format!(
        "---\nkind: quest\nid: night\ntitle: Night\n{VOCAB}---\n\
         <quest id=\"findTobias\" title=\"Find Tobias\" start=\"true\"{attrs}>\n{body}\n</quest>\n"
    )
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "night".into(),
        snapshot: snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

/// The spoken text of every `W-CAST-ABSENT` line, after asserting the
/// fixture resolved (a silent verdict must come from the analysis).
fn absent_said(src: &str) -> Vec<String> {
    let ds = diags(src);
    assert!(
        !ds.iter().any(|d| d.code.starts_with("E-")),
        "fixture must resolve: {ds:?}"
    );
    ds.iter()
        .filter(|d| d.code == ABSENT)
        .map(|d| {
            let line = &src[d.span.byte_start..];
            let line = &line[..line.find('\n').unwrap_or(line.len())];
            line[line.find(": ").map_or(0, |i| i + 2)..].to_string()
        })
        .collect()
}

const TOBIAS: &str = "@tobias: You came in. I told you not to come in.";

#[test]
fn a_complete_handler_assumes_its_completing_objective_done() {
    // Round 5, hollow-ward `quests/night.lute`: either objective completes
    // the quest last, and both need Tobias along.
    let objectives = "<objective id=\"find\" title=\"Find him\" done=\"@withToby\"/>\n\
        <objective id=\"out\" title=\"Out\" done=\"@out && @withToby\" by=\"run.night >= 2\"/>\n\
        <objective id=\"extra\" title=\"Extra\" done=\"run.night >= 1\" optional/>\n";
    let src = quest(
        " fail=\"@dead || (@out && !@withToby)\"",
        &format!("{objectives}<on event=\"questComplete\">\n{TOBIAS}\n@nell: You tell me a lot of things.\n</on>"),
    );
    assert!(absent_said(&src).is_empty());
    // `complete="any"`: whichever objective completes it held.
    let any = quest(
        " complete=\"any\"",
        &format!("{objectives}<on event=\"questComplete\">\n{TOBIAS}\n</on>"),
    );
    assert!(absent_said(&any).is_empty());
}

#[test]
fn a_complete_handler_warns_when_a_required_objective_does_not_imply_presence() {
    // `run.night >= 2` alone may complete the quest last.
    let src = quest(
        "",
        &format!(
            "<objective id=\"find\" title=\"Find him\" done=\"@withToby\"/>\n\
             <objective id=\"wait\" title=\"Wait\" done=\"run.night >= 2\"/>\n\
             <on event=\"questComplete\">\n{TOBIAS}\n</on>"
        ),
    );
    assert_eq!(absent_said(&src), [TOBIAS.trim_start_matches("@tobias: ")]);
    // No required objective: completion assumes nothing.
    let none = quest(
        "",
        &format!(
            "<objective id=\"find\" title=\"Find him\" done=\"@withToby\" optional/>\n\
             <on event=\"questComplete\">\n{TOBIAS}\n</on>"
        ),
    );
    assert_eq!(absent_said(&none).len(), 1);
}

#[test]
fn a_write_before_the_handler_ends_the_completion_assumption() {
    // The objective's body plays before the handler and lets him go.
    let src = quest(
        "",
        &format!(
            "<objective id=\"find\" title=\"Find him\" done=\"@withToby\">\n\
             ::retract{{following(tobias)}}\n</objective>\n\
             <on event=\"questComplete\">\n{TOBIAS}\n</on>"
        ),
    );
    assert_eq!(absent_said(&src).len(), 1);
    // So does an earlier `questComplete` handler; a later one does not count.
    let handlers = quest(
        "",
        &format!(
            "<objective id=\"find\" title=\"Find him\" done=\"@withToby\"/>\n\
             <on event=\"questComplete\">\n@tobias: First.\n</on>\n\
             <on event=\"questComplete\">\n::retract{{following(tobias)}}\n</on>\n\
             <on event=\"questComplete\">\n@tobias: Third.\n</on>"
        ),
    );
    assert_eq!(absent_said(&handlers), ["Third."]);
}

#[test]
fn a_failed_handler_does_not_assume_completion() {
    let src = quest(
        " fail=\"@dead\"",
        &format!(
            "<objective id=\"find\" title=\"Find him\" done=\"@withToby\"/>\n\
             <on event=\"questFailed\">\n{TOBIAS}\n</on>\n\
             <on event=\"questActive\">\n@tobias: Still here.\n</on>"
        ),
    );
    assert_eq!(
        absent_said(&src),
        [TOBIAS.trim_start_matches("@tobias: "), "Still here."]
    );
}
