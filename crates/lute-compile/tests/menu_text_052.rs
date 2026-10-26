//! dsl 0.52.0 §6.2 (#218): a `<branch>`/`<hub>` prompt with a marker ships
//! `promptPlaceholders`, built as an option's `placeholders` are, and its
//! expressions keep the requirements an option label's would
//! (Review052 F3): a prompt whose only interpolation reads a temporal def
//! requires `lute.knowledge.temporal/1`.

use lute_check::{CheckInput, Mode};
use lute_compile::{compile, ExecutionIr};
use serde_json::{json, Value as Json};

fn artifact(body: &str) -> ExecutionIr {
    let input = CheckInput::new(
        format!(
            "---\nkind: scene\nid: gold\nstate:\n  run.gold: {{ type: int, default: 5 }}\n\
             defs:\n  late: {{ type: bool, cel: \"now() == now()\" }}\n---\n## Shot 1.\n{body}\n"
        ),
        "menu-text-052".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            lute_test_vocab::vocab_snapshot(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    );
    compile(&input).unwrap_or_else(|d| panic!("the source compiles: {d:?}"))
}

fn menu(ir: &ExecutionIr, kind: &str) -> Json {
    let commands = serde_json::to_value(&ir.commands).unwrap();
    commands
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == kind)
        .cloned()
        .unwrap_or_else(|| panic!("a {kind} record: {commands:#}"))
}

fn branch(prompt: &str, label: &str) -> String {
    format!(
        "<branch id=\"pay\" prompt=\"{prompt}\">\n  <choice id=\"yes\" text=\"{label}\">\n    \
         @narrator: Paid.\n  </choice>\n</branch>"
    )
}

fn hub(prompt: &str, label: &str) -> String {
    format!(
        "<hub id=\"purse\" prompt=\"{prompt}\">\n  <choice id=\"look\" text=\"{label}\">\n    \
         @narrator: Looked.\n  </choice>\n  <choice id=\"leave\" text=\"Leave\" exit>\n    \
         @narrator: Bye.\n  </choice>\n</hub>"
    )
}

#[test]
fn a_prompt_with_a_marker_ships_its_placeholders() {
    for (kind, body) in [
        ("choice", branch("You have {{run.gold}} gold.", "Pay")),
        ("hub", hub("You have {{run.gold}} gold.", "Look")),
    ] {
        let rec = menu(&artifact(&body), kind);
        assert_eq!(rec["prompt"], "You have {{run.gold}} gold.", "{rec:#}");
        assert_eq!(
            rec["promptPlaceholders"],
            json!([{ "kind": "path", "path": "run.gold" }]),
            "{rec:#}"
        );
    }
}

/// No marker, no field: a marker-less prompt record is byte-identical.
#[test]
fn a_plain_prompt_ships_no_placeholders() {
    for (kind, body) in [
        ("choice", branch("Pay up?", "Pay")),
        ("hub", hub("Pay up?", "Look")),
    ] {
        let rec = menu(&artifact(&body), kind);
        assert!(rec.get("promptPlaceholders").is_none(), "{rec:#}");
    }
}

/// The same temporal-def interpolation requires the same semantics in a
/// prompt as in a label — and in a prompt alone.
#[test]
fn a_prompt_reading_a_temporal_def_requires_temporal_semantics() {
    for build in [branch as fn(&str, &str) -> String, hub] {
        let in_label = artifact(&build("Pay up?", "Late: {{@late}}"));
        let in_prompt = artifact(&build("Late: {{@late}}", "Pay"));
        assert!(
            in_label
                .required_semantics
                .iter()
                .any(|id| id == "lute.knowledge.temporal/1"),
            "{:?}",
            in_label.required_semantics
        );
        assert_eq!(in_prompt.required_semantics, in_label.required_semantics);
        let rec = menu(
            &in_prompt,
            if build("", "").starts_with("<hub") {
                "hub"
            } else {
                "choice"
            },
        );
        assert_eq!(rec["promptPlaceholders"][0]["kind"], "ref", "{rec:#}");
        assert!(rec["promptPlaceholders"][0]["expr"].is_object(), "{rec:#}");
    }
}
