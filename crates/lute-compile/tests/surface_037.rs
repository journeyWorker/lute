//! dsl 0.37.0 §5 lowering consequences that are new in 0.37: the `sequence`
//! record, camera domain fields, resolved `cg`/`video` display, actor
//! emotion/costume, a `voiceKey` on every line role, the reward `id`, and a
//! `family` on every emitted record.

use std::collections::BTreeMap;

use lute_check::{CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::snapshot::{capability_version, CapabilitySnapshot, Domain};
use serde_json::Value;

/// The shared test vocabulary plus the project domains the 0.37 staging
/// directives name (`sequence`, `transition`, `cgLayout`, `costume`) and the
/// `textStyle` members inline modifiers name.
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_test_vocab::vocab_snapshot();
    for (name, members) in [
        ("sequence", &["harborArrival"][..]),
        ("transition", &["fade"][..]),
        ("cgLayout", &["full"][..]),
        ("costume", &["apron"][..]),
        ("textStyle", &["emphasis", "whisper"][..]),
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
    serde_json::to_value(compile_ir(text)).unwrap()
}

fn compile_ir(text: &str) -> lute_compile::ExecutionIr {
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
    compile(&input).unwrap_or_else(|diags| panic!("compiles: {diags:#?}"))
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
    assert_eq!(seq["family"], "staging");
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
fn every_record_carries_kind_family_and_position() {
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
        assert_eq!(c["family"], want[kind], "{c}");
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
    assert_eq!(quest["family"], "declaration");
    let rewards = quest["rewards"].as_array().unwrap();
    assert_eq!(rewards[0]["id"], "xp");
    assert!(rewards[1].get("id").is_none(), "unauthored id is omitted");
}

/// The record marker is `family`, so a lore `<entry category=…>` keeps its
/// own `category` field: exactly one key of each name on the wire.
#[test]
fn entry_category_does_not_collide_with_the_record_family() {
    let art = compile_json(
        "---\nkind: lore\nid: haven.notes\n---\n\n\
         <entry id=\"note1\" target=\"item.captains_log\" category=\"x\">\n\
         @captain: Day one.\n</entry>\n",
    );
    let entry = first(&art, "entry");
    let wire = serde_json::to_string(entry).unwrap();
    assert_eq!(wire.matches("\"category\"").count(), 1, "{wire}");
    assert_eq!(wire.matches("\"family\"").count(), 1, "{wire}");
    assert_eq!(entry["category"], "x");
    assert_eq!(entry["family"], "declaration");
}

const MODIFIED: &str = r#"---
kind: scene
character: marina
season: 1
episode: 2
pov: marina
---

## Arrival {#arrival}

@narrator: :speed[Hello :emphasis[big :whisper[world]] {{userName}}]{rate=1.25}:pause{s=0.5}!
@marina: Plain \: line.
@narrator: :emphasis[a]:emphasis[b] c :speed[:speed[d]{rate=2}]{rate=0.5}
"#;

fn lines(art: &Value) -> Vec<&Value> {
    commands(art).iter().filter(|c| c["kind"] == "line").collect()
}

/// dsl 0.37.0 §3.6: `text` is the plain derivation; `segments` carry the
/// active styles outermost-first and the innermost rate, `{pause}` leaves,
/// interpolation inside a span, and no empty run.
#[test]
fn a_modified_line_lowers_to_plain_text_and_segments() {
    let art = compile_json(MODIFIED);
    let lines = lines(&art);
    assert_eq!(lines[0]["text"], "Hello big world {{userName}}!");
    assert_eq!(
        lines[0]["segments"],
        serde_json::json!([
            {"text": "Hello ", "rate": 1.25},
            {"text": "big ", "styles": ["emphasis"], "rate": 1.25},
            {"text": "world", "styles": ["emphasis", "whisper"], "rate": 1.25},
            {"text": " {{userName}}", "rate": 1.25},
            {"pause": 0.5},
            {"text": "!"}
        ])
    );
    assert_eq!(
        lines[0]["placeholders"],
        serde_json::json!([{"kind": "reserved", "token": "userName"}])
    );
}

#[test]
fn an_unmodified_line_has_plain_text_and_no_segments() {
    let art = compile_json(MODIFIED);
    let line = lines(&art)[1];
    assert_eq!(line["text"], "Plain : line.", "escapes decode");
    assert!(line.get("segments").is_none(), "{line}");
}

/// Adjacent runs with one style coalesce; a nested `speed` wins with the
/// innermost rate.
#[test]
fn segments_coalesce_and_the_innermost_rate_wins() {
    let art = compile_json(MODIFIED);
    let line = lines(&art)[2];
    assert_eq!(line["text"], "ab c d");
    assert_eq!(
        line["segments"],
        serde_json::json!([
            {"text": "ab", "styles": ["emphasis"]},
            {"text": " c "},
            {"text": "d", "rate": 2.0}
        ])
    );
}

fn line_ids(ir: &lute_compile::ExecutionIr) -> Vec<String> {
    ir.commands
        .iter()
        .filter_map(|c| match c {
            lute_compile::Command::Line(l) => Some(l.line_id.clone()),
            _ => None,
        })
        .collect()
}

fn merged(
    translations: &[(usize, &str)],
) -> (Value, Vec<lute_core_span::Diagnostic>) {
    let mut ir = compile_ir(MODIFIED);
    let ids = line_ids(&ir);
    let bundle = lute_compile::locale::LocaleBundle::from_triples(
        translations
            .iter()
            .map(|(i, text)| (ids[*i].clone(), "ja-JP".to_string(), text.to_string())),
    );
    let diags = lute_compile::locale::merge_locales(&mut ir, &bundle);
    (serde_json::to_value(&ir).unwrap(), diags)
}

/// dsl 0.37.0 §6: a translation with the source's modifier multiset (in any
/// position) merges its plain text into `texts` and its segments into
/// `localeSegments`; an unmodified line gets `texts` only.
#[test]
fn a_matching_translation_merges_texts_and_locale_segments() {
    let (art, diags) = merged(&[
        (0, ":pause{s=0.5}:speed[やあ :emphasis[大きな :whisper[世界]] {{userName}}]{rate=1.25}"),
        (1, "ふつう"),
        (2, ":speed[:speed[d]{rate=2.0}]{rate=0.5} c :emphasis[a]:emphasis[b]"),
    ]);
    assert!(diags.iter().all(|d| d.code != "E-L10N-MODIFIERS"), "{diags:#?}");
    let lines = lines(&art);
    assert_eq!(lines[0]["texts"]["ja-JP"], "やあ 大きな 世界 {{userName}}");
    assert_eq!(
        lines[0]["localeSegments"]["ja-JP"],
        serde_json::json!([
            {"pause": 0.5},
            {"text": "やあ ", "rate": 1.25},
            {"text": "大きな ", "styles": ["emphasis"], "rate": 1.25},
            {"text": "世界", "styles": ["emphasis", "whisper"], "rate": 1.25},
            {"text": " {{userName}}", "rate": 1.25}
        ])
    );
    assert_eq!(lines[1]["texts"]["ja-JP"], "ふつう");
    assert!(lines[1].get("localeSegments").is_none(), "{}", lines[1]);
    assert_eq!(lines[2]["texts"]["ja-JP"], "d c ab", "rate=2.0 normalizes to rate=2");
}

/// A missing, extra or differently parameterized modifier — or one on a
/// source line that has none — is `E-L10N-MODIFIERS`, and that translation
/// is not merged.
#[test]
fn a_translation_with_other_modifiers_is_refused() {
    for (i, text) in [
        (0, ":speed[やあ :emphasis[大きな :whisper[世界]] {{userName}}]{rate=1.25}!"),
        (0, ":speed[やあ :emphasis[大きな :whisper[世界]] {{userName}}]{rate=1.25}:pause{s=0.5}:emphasis[!]"),
        (0, ":speed[やあ :emphasis[大きな :whisper[世界]] {{userName}}]{rate=1.5}:pause{s=0.5}!"),
        (1, ":emphasis[ふつう]"),
        (1, "ふつう :emphasis[壊れた"),
    ] {
        let (art, diags) = merged(&[(i, text)]);
        assert!(
            diags.iter().any(|d| d.code == "E-L10N-MODIFIERS"
                && d.severity == lute_core_span::Severity::Error),
            "{text}: {diags:#?}"
        );
        let line = lines(&art)[i];
        assert!(line.get("texts").is_none(), "{text}: {line}");
        assert!(line.get("localeSegments").is_none(), "{text}: {line}");
    }
}

/// `E-DUP-VOICEKEY` compares plain text: markup alone is not a difference.
#[test]
fn voice_key_collision_compares_plain_text() {
    let doc = |line: &str| format!("---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\npov: marina\n---\n\n## A\n\n@marina{{code=\"0010\"}}: {line}\n");
    let plain = compile_ir(&doc("Hello world."));
    let marked = compile_ir(&doc("Hello :emphasis[world]:pause{s=1}."));
    let other = compile_ir(&doc("Hello there."));
    let input = |path: &str, ir| lute_compile::index::IndexInput {
        path: path.to_string(),
        artifact_path: format!("{path}.json"),
        artifact: ir,
    };
    assert!(lute_compile::index::voice_key_collisions(&[
        input("a.lute", &plain),
        input("b.lute", &marked),
    ])
    .is_empty());
    let hit = lute_compile::index::voice_key_collisions(&[
        input("a.lute", &marked),
        input("c.lute", &other),
    ]);
    assert_eq!(hit.len(), 1, "{hit:?}");
}
