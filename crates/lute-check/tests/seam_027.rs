//! dsl 0.27.0 §4 (T2-3, T2-4): occasion `raisedWhen` gates and the
//! schema's `terminal:` in the checker — a beat is judged under its
//! occasion's gate and `!terminal`, and both conditions are checked like any
//! condition slot.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};

fn input(text: &str, gates: &[(&str, bool, &str)], terminal: Option<&str>) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    for (name, targeted, gate) in gates {
        snapshot.occasions.insert(
            (*name).into(),
            OccasionDecl {
                name: (*name).into(),
                select: OccasionSelect::First,
                target: if *targeted {
                    OccasionTarget::Domain {
                        prefix: "room".into(),
                        entity: "room".into(),
                        members: None,
                    }
                } else {
                    false.into()
                },
                raised_when: (!gate.is_empty()).then(|| gate.to_string()),
                ..Default::default()
            },
        );
    }
    let mut imports = SchemaImports::default();
    if let Some(t) = terminal {
        imports.terminal.push((
            std::path::PathBuf::from("/p/world.schema.yaml"),
            t.to_string(),
            lute_core_span::Span {
                byte_start: 0,
                byte_end: 0,
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        ));
    }
    CheckInput {
        text: text.to_string(),
        uri: "seam".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn errors(input: &CheckInput) -> Vec<(String, String)> {
    check(input)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

const VOCAB: &str = "entities:\n  room: { members: [hall, office] }\n\
                     relations:\n  canEnter: { args: [room] }\n\
                     enums:\n  fate: { members: [alive, taken] }\n\
                     state:\n  run.fate: { type: { domain: fate }, default: alive }\n  \
                     run.hp: { type: number, default: 3 }\n";

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: ward\ntitle: Ward\n{VOCAB}---\n{body}")
}

#[test]
fn a_beat_needing_the_terminal_state_is_unreachable() {
    let text = lore(
        "<beat id=\"afterDeath\" on=\"chime\" once=\"false\" when=\"run.fate == 'taken'\">\n  @narrator: too late\n</beat>\n\
         <beat id=\"alive\" on=\"chime\" once=\"false\" when=\"run.hp > 1\">\n  @narrator: still here\n</beat>\n",
    );
    let errs = errors(&input(
        &text,
        &[("chime", false, "")],
        Some("run.fate == 'taken'"),
    ));
    let dead: Vec<&String> = errs
        .iter()
        .filter(|(c, _)| c == "E-BEAT-UNREACHABLE")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(dead.len(), 1, "{errs:?}");
    assert!(
        dead[0].contains("ward.afterDeath")
            && dead[0].contains("`terminal: run.fate == 'taken'` does not hold yet"),
        "{dead:?}"
    );
    // Without the terminal condition the same beat is fine.
    assert!(errors(&input(&text, &[("chime", false, "")], None)).is_empty());
}

#[test]
fn a_beat_whose_when_contradicts_the_gate_is_unreachable() {
    let text = lore(
        "<beat id=\"late\" on=\"chime\" once=\"false\" when=\"run.hp == 0\">\n  @narrator: x\n</beat>\n\
         <beat id=\"fine\" on=\"chime\" once=\"false\" when=\"run.hp == 2\">\n  @narrator: y\n</beat>\n",
    );
    let errs = errors(&input(&text, &[("chime", false, "run.hp > 0")], None));
    let dead: Vec<&String> = errs
        .iter()
        .filter(|(c, _)| c == "E-BEAT-UNREACHABLE")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(dead.len(), 1, "{errs:?}");
    assert!(
        dead[0].contains("ward.late") && dead[0].contains("gate `raisedWhen: run.hp > 0`"),
        "{dead:?}"
    );
}

#[test]
fn a_gate_that_never_holds_kills_every_beat_of_the_occasion() {
    let text = lore(
        "<beat id=\"a\" on=\"enter\" target=\"room.office\" once=\"false\">\n  @narrator: x\n</beat>\n\
         <entry id=\"e\" on=\"enter\" once=\"false\">\n  @narrator: y\n</entry>\n",
    );
    let errs = errors(&input(
        &text,
        &[(
            "enter",
            true,
            "occasion.target == 'hall' && run.hp < 0 && run.hp > 0",
        )],
        None,
    ));
    assert!(
        errs.iter().any(|(c, m)| c == "E-BEAT-UNREACHABLE"
            && m.contains("ward.a")
            && m.contains("raised only when")),
        "{errs:?}"
    );
    assert!(
        errs.iter()
            .any(|(c, m)| c == "E-ENTRY-UNREACHABLE" && m.contains("entry `e`")),
        "{errs:?}"
    );
}

#[test]
fn a_gate_reading_the_target_is_judged_for_the_beat_target_only() {
    // The gate holds only in the hall: a beat restricted to the office can
    // never be presented, one restricted to the hall (or to any room) can.
    let text = lore(
        "<beat id=\"office\" on=\"enter\" target=\"room.office\" once=\"false\">\n  @narrator: x\n</beat>\n\
         <beat id=\"hall\" on=\"enter\" target=\"room.hall\" once=\"false\">\n  @narrator: y\n</beat>\n\
         <beat id=\"any\" on=\"enter\" once=\"false\">\n  @narrator: z\n</beat>\n",
    );
    let errs = errors(&input(
        &text,
        &[("enter", true, "occasion.target == 'hall'")],
        None,
    ));
    let dead: Vec<&String> = errs
        .iter()
        .filter(|(c, _)| c == "E-BEAT-UNREACHABLE")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(dead.len(), 1, "{errs:?}");
    assert!(dead[0].contains("ward.office"), "{dead:?}");
}

#[test]
fn gate_and_terminal_texts_are_checked_like_conditions() {
    let text = lore("<beat id=\"a\" on=\"enter\" once=\"false\">\n  @narrator: x\n</beat>\n");
    let errs = errors(&input(
        &text,
        &[(
            "enter",
            true,
            "holds(canEnter(occasion.target)) && run.hpp > 0",
        )],
        Some("run.fat == 'taken'"),
    ));
    assert!(
        errs.iter().any(|(c, m)| c == "E-UNDECLARED"
            && m.contains("occasion `enter`'s `raisedWhen:")
            && m.contains("run.hpp")),
        "{errs:?}"
    );
    assert!(
        errs.iter().any(|(c, m)| c == "E-UNDECLARED"
            && m.contains("`terminal: run.fat == 'taken'`")
            && m.contains("world.schema.yaml")),
        "{errs:?}"
    );
    // `occasion.target` in the gate of an occasion raised for no target.
    let errs = errors(&input(
        &text,
        &[("enter", false, "occasion.target == 'hall'")],
        None,
    ));
    assert!(
        errs.iter()
            .any(|(c, m)| c == "E-BEAT-ATTR" && m.contains("raised for no target")),
        "{errs:?}"
    );
}

/// The `W-BEAT-PRIORITY-TIE` messages of `input`'s one document.
fn ties(input: &CheckInput) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(&input.text);
    let folded = lute_check::fold_env(&doc, input).0;
    let docs = vec![(std::path::PathBuf::from("ward.lute"), doc)];
    let producers = lute_check::cast::fact_producers(&docs, &Default::default());
    lute_check::check_project_beats(&docs, &[&folded], &producers, None)
        .into_iter()
        .filter(|(_, d)| d.code == "W-BEAT-PRIORITY-TIE")
        .map(|(_, d)| d.message)
        .collect()
}

/// A beat the seam makes unreachable — its occasion's gate never holds, or
/// it needs the terminal state — is never eligible at all, so it ties with
/// nothing: it used to be listed as one that "can be eligible at once".
#[test]
fn a_beat_the_seam_makes_unreachable_ties_with_nothing() {
    let text = lore(
        "<beat id=\"k1\" on=\"knock\" once=\"false\" priority=\"5\" when=\"run.hp > 1\">\n  @narrator: x\n</beat>\n\
         <beat id=\"k2\" on=\"knock\" once=\"false\" priority=\"5\" when=\"run.hp > 2 || run.fate == 'taken'\">\n  @narrator: y\n</beat>\n",
    );
    // An open gate: the two tie.
    let open = ties(&input(&text, &[("knock", false, "run.hp > 0")], None));
    assert_eq!(open.len(), 1, "{open:?}");
    // A gate that never holds: neither is ever presented.
    let dead = ties(&input(
        &text,
        &[("knock", false, "run.hp < 0 && run.hp > 0")],
        None,
    ));
    assert!(dead.is_empty(), "{dead:?}");
    // `k1` needs the game over: under `terminal:` it is never presented;
    // without one it ties with `k2`.
    let text2 = text.replace("run.hp > 1", "run.fate == 'taken'");
    let terminal = ties(&input(
        &text2,
        &[("knock", false, "")],
        Some("run.fate == 'taken'"),
    ));
    assert!(terminal.is_empty(), "{terminal:?}");
    assert_eq!(ties(&input(&text2, &[("knock", false, "")], None)).len(), 1);
}
