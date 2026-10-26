//! dsl 0.52.0 §5.4 (#217), compiler row: `walk_hub` emits every option from
//! the checker's hub head ([`lute_check::StageFlow::hub`]), so a character
//! who left in one option is a fresh show in another — an explicit
//! `::actor` there injects its anchor. v0.51.0 forked every option from the
//! hub's entry state and injected nothing there.

use lute_check::{CheckInput, Mode};

/// Each `actor`/`line` record of `body`'s artifact: its kind, its character
/// or speaker, and the rule that injected it, if any.
fn records(body: &str) -> Vec<String> {
    let text = format!(
        "---\nkind: scene\nid: hub\nstate:\n  run.f: {{ type: bool, default: false }}\n\
         enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
         anchor:\n    members: [left, center, right]\n    default: center\n---\n## Shot 1.\n\
         {body}\n"
    );
    let input = CheckInput::new(
        text,
        "hub".into(),
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

/// The fixed-point mutation test (hub): `send` hides `mira`, so at the hub
/// head she may be off stage and `talk`'s `::actor` is a fresh show with
/// its injected anchor. With the iteration disabled (the head computed once
/// from the entry) she stays on stage and the `::actor` injects nothing.
#[test]
fn an_actor_in_an_option_after_an_exit_in_another_is_a_fresh_show() {
    assert_eq!(
        records(
            "::actor{character=\"mira\"}\n<hub id=\"desk\">\n\
             <choice id=\"send\" text=\"Send\">\n::actor{character=\"mira\" action=\"hide\"}\n\
             </choice>\n\
             <choice id=\"talk\" text=\"Talk\">\n::actor{character=\"mira\"}\n@mira: Here.\n\
             </choice>\n\
             <choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n</hub>"
        ),
        [
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "actor mira",
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "line mira",
            "line narrator",
        ]
    );
}

/// The `<return>` body runs from the completion of the non-exit options:
/// after `send`, `mira` is off stage there and its `::actor` is a fresh
/// show (v0.51.0: walked from the join of the options' exits too). A hub of
/// exit options only never runs its `<return>`, which injects nothing.
#[test]
fn the_return_body_starts_from_the_options_completion() {
    assert_eq!(
        records(
            "::actor{character=\"mira\"}\n<hub id=\"desk\">\n\
             <return>\n::actor{character=\"mira\"}\n</return>\n\
             <choice id=\"send\" text=\"Send\">\n::actor{character=\"mira\" action=\"hide\"}\n\
             </choice>\n\
             <choice id=\"leave\" text=\"Leave\" exit>\n@mira: Bye.\n</choice>\n</hub>"
        ),
        [
            "actor mira",
            "actor mira +auto-anchor-on-show",
            "actor mira",
            "line mira",
            "actor mira",
            "actor mira +auto-anchor-on-show",
        ]
    );
    assert_eq!(
        records(
            "<hub id=\"desk\">\n<return>\n::actor{character=\"mira\"}\n</return>\n\
             <choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n</hub>"
        ),
        ["line narrator", "actor mira"]
    );
}

/// #222 at a hub head (summer-station `radioHub`'s shape): an option
/// changes `mira`'s emotion, so the head joins differing sprites; she stays
/// on stage with a dirty pose and the exit option's plain line still resets
/// it (v0.51.0 forked from the entry and reset it; the hub head with the
/// 0.51 join dropped her to maybe-on-stage and lost the reset).
#[test]
fn a_plain_line_after_options_that_change_the_emotion_resets_the_pose() {
    let text = "---\nkind: scene\nid: hub\nenums:\n  action:\n    members: [show, hide]\n    \
                exits: [hide]\n  anchor:\n    members: [left, center, right]\n    \
                default: center\n  emotion:\n    members: [tired, fond]\n---\n## Shot 1.\n\
                ::actor{character=\"mira\"}\n@mira{emotion=\"tired\"}: Morning.\n\
                <hub id=\"radio\">\n<choice id=\"listen\" text=\"Listen\" once>\n\
                @mira{emotion=\"fond\"}: Nice.\n</choice>\n\
                <choice id=\"leave\" text=\"Leave\" exit>\n@mira: Go on.\n</choice>\n</hub>\n";
    let input = CheckInput::new(
        text.to_string(),
        "hub".into(),
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
    let resets = json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["provenance"]["by"] == "auto-pose-reset")
        .count();
    assert_eq!(resets, 1, "{json:#}");
}

/// The `actor`/`line` records of a full scene `text`, as [`records`] lists
/// them.
fn records_of(text: &str) -> Vec<String> {
    let input = CheckInput::new(
        text.to_string(),
        "hub".into(),
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

/// S4 review P1: from the entry (`mira` shown, then dirty) the `pose`
/// option's `::actor` makes her sprite unknown at the head; from that head
/// the same `::actor` injects like a fresh show and its sprite is the
/// entry's again. Re-joined from the entry each pass, the head alternated
/// known/unknown until the bound and its top dropped her presence, so the
/// exit option's line lost the `auto-pose-reset` v0.51.0 emitted.
/// Accumulated from the last head, the head stays unknown and she stays on
/// stage.
#[test]
fn an_unknown_sprite_at_the_hub_head_stays_unknown() {
    let text = "---\nkind: scene\nid: hub\nenums:\n  action:\n    members: [show, hide]\n    \
                exits: [hide]\n  anchor:\n    members: [left, center, right]\n    \
                default: center\n  emotion:\n    members: [neutral, happy]\n---\n## Shot 1.\n\
                ::actor{character=\"mira\" anchor=\"center\"}\n@mira{emotion=\"neutral\"}: Hi.\n\
                <hub id=\"desk\">\n<choice id=\"pose\" text=\"Pose\">\n\
                ::actor{character=\"mira\" action=\"show\" emotion=\"neutral\"}\n</choice>\n\
                <choice id=\"leave\" text=\"Leave\" exit>\n@mira: Bye.\n</choice>\n</hub>\n";
    let records = records_of(text);
    assert_eq!(
        records[records.len() - 2..],
        ["actor mira +auto-pose-reset", "line mira"],
        "{records:#?}"
    );
}
