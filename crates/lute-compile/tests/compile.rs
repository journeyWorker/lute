//! `compile()` orchestration: the D6 gate, the folded-state envelope, id
//! stamping, CEL expansion in situ, and byte determinism.

use lute_check::{CheckInput, Mode};
use lute_compile::{compile, Command};

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    }
}

const SCENE: &str = r#"---
character: bianca
season: 1
episode: 2
title: Compile me
state:
  scene.affect.bianca: { type: number, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.bianca >= 1" }
---

## Shot 1.

::bg{location="family_restaurant" time="afternoon" assetId="BG.x"}
::auto{character="bianca" action="fade-in-up"}
:bianca{code="0010" emotion="surprised"}: Oh!

<branch id="number">
  <choice id="blunt" label="Flat">
    :fixer{code="0010"}: Number.
  </choice>
  <choice id="soft" label="Gentle">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>

<match on="scene.choices.number">
  <when test="@fond">
    :fixer{delivery="thought"}: Nice.
  </when>
  <when test="$ == 'blunt'">
    :fixer{delivery="thought"}: Flat.
  </when>
  <otherwise>
    :fixer{delivery="thought"}: Hm.
  </otherwise>
</match>
"#;

#[test]
fn error_doc_emits_no_artifact() {
    // Undeclared state write => Error diagnostic => gate refuses (D6).
    let bad =
        "---\ncharacter: b\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n::set{scene.nope = 1}\n";
    let err = compile(&input(bad)).unwrap_err();
    assert!(err.iter().any(|d| d.code == "E-UNDECLARED"), "{err:#?}");
}

#[test]
fn valid_hub_doc_compiles_to_hub_record() {
    // Plan C: `<hub>` now LOWERS to a `hub` record (IR A2). A check-passing hub
    // doc COMPILES — the transitional compile-time hub gate is gone.
    const HUB: &str = r#"---
character: b
season: 1
episode: 1
state:
  scene.affect.b: { type: number, default: 0 }
---

## Shot 1.

<hub id="chat">
  <choice id="ask" label="Ask" once>
    :narrator: Sure.
  </choice>
  <choice id="curious" label="Be curious" when="scene.affect.b >= 1">
    :narrator: Hmm.
  </choice>
  <choice id="leave" label="Leave" exit>
    :narrator: Bye.
  </choice>
</hub>
"#;
    // Precondition: the hub doc checks clean (B6 hub checking), so compile reaches
    // lowering instead of bouncing off the D6 gate.
    assert!(lute_check::check(&input(HUB)).ok, "hub doc must pass check");
    let artifact = compile(&input(HUB)).expect("hub doc compiles to a hub record");

    // The `hub` record: id, recordKey alias, filled converge, three options.
    let hub = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Hub(h) => Some(h),
            _ => None,
        })
        .expect("hub record");
    assert_eq!(hub.id, "chat");
    assert_eq!(hub.record_key, "scene.choices.chat");
    assert!(!hub.converge.is_empty(), "converge addr filled by address pass");
    assert_eq!(hub.options.len(), 3);
    let opt = |id: &str| hub.options.iter().find(|o| o.id == id).expect("option");
    let ask = opt("ask");
    assert!(ask.once && !ask.exit, "ask: once, not exit");
    assert!(ask.when.is_none() && ask.expr.is_none(), "ask is unguarded");
    let curious = opt("curious");
    assert!(!curious.once && !curious.exit, "curious: neither once nor exit");
    assert!(curious.when.is_some(), "guarded option carries the raw `when`");
    assert!(curious.expr.is_some(), "guarded option carries the lowered A7 expr");
    let leave = opt("leave");
    assert!(!leave.once && leave.exit, "leave: exit, not once");
    for o in &hub.options {
        assert!(!o.target.is_empty(), "option {} target resolved", o.id);
        // Option `lineId` = {character}.s{season}ep{episode}.<hubId>.<optId>.
        assert_eq!(o.line_id, format!("b.s01ep01.chat.{}", o.id));
    }

    // Flat-VM contract (A2 §7): the EXIT arm ends in a forward Jump→converge;
    // NON-exit arms emit NO trailing jump. This doc has no other fork, so the
    // total Jump count is exactly 1 (from `leave`), targeting the hub converge.
    let jumps: Vec<&str> = artifact
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::Jump(j) => Some(j.target.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(jumps.len(), 1, "only the exit arm jumps to converge, got {jumps:?}");
    assert_eq!(jumps[0], hub.converge, "the exit-arm jump targets the hub converge");

    // Serialized shape: kind:"hub", recordKey, options[*].once/exit are bools.
    let json = serde_json::to_value(
        artifact
            .commands
            .iter()
            .find(|c| matches!(c, Command::Hub(_)))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(json["kind"], "hub");
    assert_eq!(json["recordKey"], "scene.choices.chat");
    assert!(
        json["converge"].as_str().is_some_and(|s| !s.is_empty()),
        "converge present"
    );
    for o in json["options"].as_array().unwrap() {
        assert!(o["once"].is_boolean(), "once is an always-present bool");
        assert!(o["exit"].is_boolean(), "exit is an always-present bool");
    }

    // Folded state envelope (via `fold_env`, reusing lute-check's B6 hub fold):
    // the implicit `scene.choices.chat` enum + per-choice `scene.visited.chat.*`.
    let entry = |path: &str| {
        artifact
            .state
            .iter()
            .find(|s| s.path == path)
            .unwrap_or_else(|| panic!("missing state entry {path}"))
    };
    let choices = entry("scene.choices.chat");
    assert_eq!(choices.ty, "enum");
    let dom = choices.domain.as_deref().expect("enum domain");
    for m in ["ask", "curious", "leave", "unset"] {
        assert!(dom.contains(&m.to_string()), "domain has {m}, got {dom:?}");
    }
    assert_eq!(choices.default, Some(serde_json::json!("unset")));
    assert_eq!(choices.provenance.as_deref(), Some("branch:chat"));
    for cid in ["ask", "curious", "leave"] {
        let v = entry(&format!("scene.visited.chat.{cid}"));
        assert_eq!(v.ty, "bool", "visited {cid} is bool");
        assert_eq!(v.default, Some(serde_json::json!(false)), "visited {cid} default false");
    }
}

#[test]
fn clean_doc_compiles_with_envelope_expansion_and_ids() {
    let artifact = compile(&input(SCENE)).expect("clean compile");
    assert_eq!(artifact.lute, "0.0.1");
    assert_eq!(artifact.meta.character, "bianca");
    assert_eq!(artifact.meta.episode_id, "S01EP02");
    assert_eq!(artifact.meta.title.as_deref(), Some("Compile me"));

    // Folded state envelope: author decl + implicit branch decl (§4.1).
    let paths: Vec<&str> = artifact.state.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(paths, vec!["scene.affect.bianca", "scene.choices.number"]);
    let choice_entry = &artifact.state[1];
    assert_eq!(choice_entry.ty, "enum");
    assert_eq!(
        choice_entry.domain.as_deref(),
        Some(["blunt".to_string(), "soft".to_string(), "unset".to_string()].as_slice())
    );
    assert_eq!(choice_entry.provenance.as_deref(), Some("branch:number"));
    // §4.1: an implicit choice slot is seeded `default: "unset"` so the runtime
    // can init the branch record key before any choice is taken.
    assert_eq!(choice_entry.default, Some(serde_json::json!("unset")));
    let affect = &artifact.state[0];
    assert_eq!(affect.ty, "number");
    assert_eq!(affect.default, Some(serde_json::json!(0)));

    // First record: the bg, addressed densely.
    let json = serde_json::to_value(&artifact.commands[0]).unwrap();
    assert_eq!(json["kind"], "background");
    assert_eq!(json["addr"], "001-0100");

    // Match arms expanded: @fond parenthesized; $ replaced by the subject.
    let m = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Match(m) => Some(m),
            _ => None,
        })
        .expect("match record");
    assert_eq!(m.arms[0].test, "(scene.affect.bianca >= 1)");
    assert_eq!(m.arms[1].test, "scene.choices.number == 'blunt'");
    assert!(m.otherwise.is_some());

    // No symbolic labels or DSL tokens survive anywhere.
    let all = serde_json::to_string(&artifact).unwrap();
    assert!(!all.contains("\"@"), "unexpanded/unresolved: {all}");
    assert!(!all.contains("textUnitId"));

    // Back-filled thought-line ids (fixer max authored 0010 -> 0020/0030/0040),
    // monologue => no voiceKey.
    let thoughts: Vec<(&str, Option<&str>)> = artifact
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::Line(l) if l.text != "Number." && l.speaker == "fixer" => {
                Some((l.line_id.as_str(), l.voice_key.as_deref()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        thoughts,
        vec![
            ("bianca.s01ep02.fixer_0020", None),
            ("bianca.s01ep02.fixer_0030", None),
            ("bianca.s01ep02.fixer_0040", None),
        ]
    );
}

#[test]
fn injection_warnings_do_not_gate_and_output_is_byte_stable() {
    // The ::auto has no anchor => an anchor is INJECTED (a warning-free case);
    // W-INJECT-CONFLICT-class warnings never gate (only Errors do, D6).
    let a1 = compile(&input(SCENE)).expect("ok");
    let a2 = compile(&input(SCENE)).expect("ok");
    let s1 = serde_json::to_string_pretty(&a1).unwrap();
    let s2 = serde_json::to_string_pretty(&a2).unwrap();
    assert_eq!(s1, s2, "same input => byte-identical artifact");
    // And serializing the SAME artifact twice is stable too.
    assert_eq!(s1, serde_json::to_string_pretty(&a1).unwrap());
}

#[test]
fn implicit_choice_slot_defaults_unset_without_forcing_author_entries() {
    // Two author `state:` decls — one WITH a default, one WITHOUT — plus a
    // `<branch>` whose implicit `scene.choices.couch` slot must be seeded
    // `default: "unset"` (§4.1) while neither author entry is force-unset.
    const DOC: &str = r#"---
character: bianca
season: 1
episode: 3
state:
  run.seen: { type: bool }
  scene.affect.bianca: { type: number, default: 0 }
---

## Shot 1.

::bg{location="family_restaurant" time="afternoon" assetId="BG.x"}
::auto{character="bianca" action="fade-in-up"}
:bianca{code="0010"}: Hi.

<branch id="couch">
  <choice id="help" label="Help">
    :fixer{code="0010"}: Sure.
  </choice>
  <choice id="ignore" label="Ignore">
    :fixer{code="0020"}: No.
  </choice>
</branch>
"#;
    let artifact = compile(&input(DOC)).expect("clean compile");
    let by_path = |p: &str| {
        artifact
            .state
            .iter()
            .find(|s| s.path == p)
            .unwrap_or_else(|| panic!("missing state entry {p}: {:?}", artifact.state))
    };

    // Implicit choice slot: enum of choice ids ∪ `unset`, seeded `default:"unset"`.
    let couch = by_path("scene.choices.couch");
    assert_eq!(couch.ty, "enum");
    assert_eq!(
        couch.domain.as_deref(),
        Some(
            [
                "help".to_string(),
                "ignore".to_string(),
                "unset".to_string()
            ]
            .as_slice()
        )
    );
    assert_eq!(couch.default, Some(serde_json::json!("unset")));
    assert_eq!(couch.provenance.as_deref(), Some("branch:couch"));

    // Author bool decl WITHOUT a default keeps `None` — no false unset.
    let seen = by_path("run.seen");
    assert_eq!(seen.ty, "bool");
    assert_eq!(seen.default, None, "author entry must not be force-unset");
    assert_eq!(seen.provenance, None);

    // Author number decl keeps its own declared default.
    let affect = by_path("scene.affect.bianca");
    assert_eq!(affect.default, Some(serde_json::json!(0)));
}

#[test]
fn author_scene_choices_enum_without_branch_is_not_forced_unset() {
    // An author `state:` decl at a `scene.choices.*` path with NO matching
    // `<branch>` (§9.3 allows any `scene.*` path) is a plain author enum, NOT an
    // implicit branch slot: it keeps `default: None`, its declared domain (no
    // phantom `unset`), and no `branch:` provenance. The real `<branch
    // id="couch">` in the same doc IS an implicit slot: seeded `default:
    // "unset"`, domain ∪ `unset`, `branch:couch` provenance.
    const DOC: &str = r#"---
character: bianca
season: 1
episode: 3
state:
  run.seen: { type: bool }
  scene.affect.bianca: { type: number, default: 0 }
  scene.choices.manual: { type: { enum: [a, b] } }
---

## Shot 1.

::bg{location="family_restaurant" time="afternoon" assetId="BG.x"}
::auto{character="bianca" action="fade-in-up"}
:bianca{code="0010"}: Hi.

<branch id="couch">
  <choice id="help" label="Help">
    :fixer{code="0010"}: Sure.
  </choice>
  <choice id="ignore" label="Ignore">
    :fixer{code="0020"}: No.
  </choice>
</branch>
"#;
    let artifact = compile(&input(DOC)).expect("clean compile");
    let by_path = |p: &str| {
        artifact
            .state
            .iter()
            .find(|s| s.path == p)
            .unwrap_or_else(|| panic!("missing state entry {p}: {:?}", artifact.state))
    };

    // Author enum at a `scene.choices.*` path with no branch: plain author entry.
    let manual = by_path("scene.choices.manual");
    assert_eq!(manual.ty, "enum");
    assert_eq!(
        manual.domain.as_deref(),
        Some(["a".to_string(), "b".to_string()].as_slice()),
        "author enum keeps its declared domain — no phantom `unset`"
    );
    assert_eq!(
        manual.default, None,
        "author `scene.choices.*` enum without a branch must NOT be force-unset"
    );
    assert_eq!(
        manual.provenance, None,
        "no branch => no `branch:` provenance"
    );

    // Real branch slot: the full implicit-choice envelope.
    let couch = by_path("scene.choices.couch");
    assert_eq!(
        couch.domain.as_deref(),
        Some(
            [
                "help".to_string(),
                "ignore".to_string(),
                "unset".to_string()
            ]
            .as_slice()
        )
    );
    assert_eq!(couch.default, Some(serde_json::json!("unset")));
    assert_eq!(couch.provenance.as_deref(), Some("branch:couch"));

    // The pre-existing author bool/number entries stay unaffected.
    assert_eq!(by_path("run.seen").default, None);
    assert_eq!(
        by_path("scene.affect.bianca").default,
        Some(serde_json::json!(0))
    );
}
