//! dsl 0.51.0 §3.4 (stage state, compiler row): the compile walk binds the
//! stage state at each `::label` and loop head from the checker's function
//! and emits once, from the stable state. A line never auto-shows (0.50.0
//! §7), so a character absent on some route injects nothing at her line,
//! and a later explicit `::actor` on that route is a fresh show with its
//! injected anchor.

use lute_check::{CheckInput, Mode};

/// Each record of `text`'s artifact: its kind, its character or speaker,
/// and the rule that injected it, if any (`actor mira +auto-anchor-on-show`).
fn records(body: &str) -> Vec<String> {
    let text = format!(
        "---\nkind: scene\nid: flow\nstate:\n  run.f: {{ type: bool, default: false }}\n\
         enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
         anchor:\n    members: [left, center, right]\n    default: center\n---\n## Shot 1.\n\
         {body}\n"
    );
    let input = CheckInput::new(
        text,
        "flow".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    );
    let ir = lute_compile::compile(&input).expect("the fixture compiles");
    let json = serde_json::to_value(&ir).unwrap();
    json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| matches!(c["kind"].as_str(), Some("actor" | "line")))
        .map(|c| {
            let who = c["character"]
                .as_str()
                .or(c["speaker"].as_str())
                .unwrap_or("");
            let by = c["provenance"]["by"]
                .as_str()
                .map(|by| format!(" +{by}"))
                .unwrap_or_default();
            format!("{} {who}{by}", c["kind"].as_str().unwrap())
        })
        .collect()
}

/// The forward-jump reproduction: on the jump's route `mira` is off stage at
/// `l`, so `Again.` injects nothing and the explicit `::actor` after it is a
/// fresh show. On 0.50.0 the walk threads the re-show's state through the
/// label, and the second `::actor` injects no anchor.
#[test]
fn a_reentrance_after_a_jump_past_one_is_a_fresh_show() {
    assert_eq!(
        records(
            "@mira: Hi.\n::actor{character=\"mira\" action=\"hide\"}\n\
             ::jump{to=\"l\" when=\"run.f\"}\n::actor{character=\"mira\"}\n::label{name=\"l\"}\n\
             @mira: Again.\n::actor{character=\"mira\"}\n@mira: Back."
        ),
        [
            "line mira",
            "actor mira",
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "line mira",
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "line mira",
        ]
    );
}

/// The fixed-point mutation test (stage): `mira` exits late in the body, so
/// at the head she may be off stage — her line at the top of the body
/// injects nothing, and an explicit `::actor` there is compiled as a fresh
/// show with its injected anchor. With the iteration disabled the head keeps
/// her on stage and the `::actor` injects no anchor.
#[test]
fn an_actor_at_the_head_of_a_loop_whose_body_exits_is_a_fresh_show() {
    assert_eq!(
        records(
            "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\
             ::actor{character=\"mira\"}\n@mira: Here.\n\
             ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"2\"}"
        ),
        [
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "line mira",
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "line mira",
            "actor mira",
        ]
    );
}
