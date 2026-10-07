//! dsl 0.37.0 §5 lowering consequences that are new in 0.37: the `sequence`
//! record, camera domain fields, resolved `cg`/`video` display, actor
//! emotion/costume, a `voiceKey` on every line role, the reward `id`, and a
//! `category` on every emitted record.

use std::collections::BTreeMap;

use lute_check::{CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::snapshot::{capability_version, CapabilitySnapshot, Domain};
use serde_json::Value;

/// The shared test vocabulary plus the project domains the 0.37 staging
/// directives name (`sequence`, `transition`, `cgLayout`, `costume`).
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_test_vocab::vocab_snapshot();
    for (name, members) in [
        ("sequence", &["harborArrival"][..]),
        ("transition", &["fade"][..]),
        ("cgLayout", &["full"][..]),
        ("costume", &["apron"][..]),
    ] {
        let dom = Domain {
            members: members.iter().map(|m| m.to_string()).collect(),
            ..Default::default()
        };
        snap.enums.insert(name.to_string(), dom.members.clone());
        snap.domains.insert(name.to_string(), dom);
    }
    snap.version = capability_version(&snap);
    snap
}

fn compile_json(text: &str) -> Value {
    let input = CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    let artifact = compile(&input).unwrap_or_else(|diags| panic!("compiles: {diags:#?}"));
    serde_json::to_value(&artifact).unwrap()
}

fn commands(art: &Value) -> &Vec<Value> {
    art["commands"].as_array().expect("commands array")
}

fn first<'a>(art: &'a Value, kind: &str) -> &'a Value {
    commands(art)
        .iter()
        .find(|c| c["kind"] == kind)
        .unwrap_or_else(|| panic!("no `{kind}` record in {art:#}"))
}

const SCENE: &str = r#"---
kind: scene
character: marina
season: 1
episode: 2
pov: marina
---

## Arrival {#arrival}

::bg{location="pier" assetId="BG.pier"}
::actor{character="marina" anchor="center" action="fadeInUp" emotion="shy" costume="apron"}
::camera{focus="marina" framing="close" move="shake" transition="fade"}
::sequence{name="harborArrival"}
::cg{assetId="CG.pier.01" layout="full"}
::video{assetId="VID.pier"}
@narrator: The pier creaks.
@marina{mono}: Here again.
@marina: Hello.
"#;

#[test]
fn sequence_lowers_to_a_blocking_sequence_record() {
    let art = compile_json(SCENE);
    let seq = first(&art, "sequence");
    assert_eq!(seq["category"], "staging");
    assert_eq!(seq["name"], "harborArrival");
    assert_eq!(seq["timing"], serde_json::json!({"wait": true}), "{seq}");
}

#[test]
fn camera_emits_its_domain_members() {
    let art = compile_json(SCENE);
    let cam = first(&art, "camera");
    assert_eq!(cam["focus"], "marina");
    assert_eq!(cam["framing"], "close");
    assert_eq!(cam["move"], "shake");
    assert_eq!(cam["transition"], "fade");
    for gone in ["zoom", "moveX", "moveY", "shake", "reset", "easing"] {
        assert!(cam.get(gone).is_none(), "no `{gone}` on camera: {cam}");
    }
}

#[test]
fn cg_and_video_emit_their_resolved_display() {
    let art = compile_json(SCENE);
    let cg = first(&art, "cg");
    assert_eq!(cg["assetId"], "CG.pier.01");
    assert_eq!(cg["display"], "show", "unauthored display resolves to `show`");
    assert_eq!(cg["layout"], "full");
    let video = first(&art, "video");
    assert_eq!(video["display"], "show", "unauthored display resolves to `show`");
    assert!(video.get("action").is_none());
}

#[test]
fn authored_actor_emits_emotion_and_costume() {
    let art = compile_json(SCENE);
    let actor = commands(&art)
        .iter()
        .find(|c| c["kind"] == "actor" && c.get("provenance").is_none())
        .expect("authored actor record");
    assert_eq!(actor["emotion"], "shy");
    assert_eq!(actor["costume"], "apron");
}

#[test]
fn every_line_role_carries_a_voice_key() {
    let art = compile_json(SCENE);
    let lines: Vec<&Value> = commands(&art).iter().filter(|c| c["kind"] == "line").collect();
    let roles: Vec<&str> = lines.iter().map(|l| l["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["narration", "mono", "dialogue"]);
    for line in lines {
        let key = line["voiceKey"].as_str().unwrap_or_default();
        assert!(!key.is_empty(), "every line has a voiceKey: {line}");
    }
}

#[test]
fn every_record_carries_kind_category_and_position() {
    let art = compile_json(SCENE);
    let want: BTreeMap<&str, &str> = [
        ("line", "content"),
        ("bg", "staging"),
        ("actor", "staging"),
        ("camera", "staging"),
        ("sequence", "staging"),
        ("cg", "staging"),
        ("video", "staging"),
    ]
    .into_iter()
    .collect();
    for c in commands(&art) {
        let kind = c["kind"].as_str().unwrap();
        assert_eq!(c["category"], want[kind], "{c}");
        assert!(c["position"].is_string(), "{c}");
        for gone in ["addr", "wait", "duration", "delay", "at", "timeline"] {
            assert!(c.get(gone).is_none(), "no flattened `{gone}`: {c}");
        }
    }
    assert_eq!(
        art["sections"],
        serde_json::json!([{"section": 1, "heading": "Arrival", "id": "arrival"}])
    );
}

#[test]
fn reward_id_reaches_the_ir() {
    let art = compile_json(
        r#"---
kind: quest
state:
  run.act: { type: bool, default: false }
  run.done: { type: bool, default: false }
---

<quest id="rescue" title="Rescue" start="run.act">
<reward id="xp" kind="XP" amount="100"/>
<reward kind="GOLD" amount="5"/>
<objective id="finish" done="run.done"/>
</quest>
"#,
    );
    let quest = first(&art, "quest");
    assert_eq!(quest["category"], "declaration");
    let rewards = quest["rewards"].as_array().unwrap();
    assert_eq!(rewards[0]["id"], "xp");
    assert!(rewards[1].get("id").is_none(), "unauthored id is omitted");
}
