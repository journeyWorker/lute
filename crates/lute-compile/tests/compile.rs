//! `compile()` orchestration: the D6 gate, the folded-state envelope, id
//! stamping, CEL expansion in situ, and byte determinism.

use lute_check::{CheckInput, Mode};
use lute_compile::{compile, ArtifactMeta, Command};

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

/// Unwrap a scene artifact's untagged `meta` (0.2.0 kind envelope) — these
/// pre-0.2.0 tests exercise `kind: scene` docs only.
fn scene_meta(a: &lute_compile::Artifact) -> &lute_compile::SceneMeta {
    match &a.meta {
        ArtifactMeta::Scene(m) => m,
        ArtifactMeta::Quest(_) => panic!("expected scene meta"),
    }
}

const SCENE: &str = r#"---
kind: scene
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
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n::set{scene.nope = 1}\n";
    let err = compile(&input(bad)).unwrap_err();
    assert!(err.iter().any(|d| d.code == "E-UNDECLARED"), "{err:#?}");
}

#[test]
fn valid_hub_doc_compiles_to_hub_record() {
    // Plan C: `<hub>` now LOWERS to a `hub` record (IR A2). A check-passing hub
    // doc COMPILES — the transitional compile-time hub gate is gone.
    const HUB: &str = r#"---
kind: scene
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
    let inp = input(SCENE);
    let artifact = compile(&inp).expect("clean compile");
    // A9 envelope hardening: language pin, IR schema version, capability stamp.
    assert_eq!(artifact.lute, "0.2.0");
    assert_eq!(artifact.ir_version, "0.2.0");
    assert_eq!(artifact.capability_version, inp.snapshot.version);
    assert!(
        !artifact.capability_version.is_empty(),
        "capabilityVersion must be a non-empty snapshot stamp"
    );
    assert_eq!(scene_meta(&artifact).character, "bianca");
    // A4/A9: episodeId normalized lowercase to match the lineId episode segment.
    assert_eq!(scene_meta(&artifact).episode_id, "s01ep02");
    assert_eq!(scene_meta(&artifact).title.as_deref(), Some("Compile me"));

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

    // A4/A9 byte-for-byte: every lineId's episode segment == meta.episodeId.
    for cmd in &artifact.commands {
        if let Command::Line(l) = cmd {
            if l.line_id.is_empty() {
                continue;
            }
            let seg = l.line_id.split('.').nth(1).expect("lineId episode segment");
            assert_eq!(
                seg, scene_meta(&artifact).episode_id,
                "lineId {} episode segment must equal meta.episodeId byte-for-byte",
                l.line_id
            );
        }
    }
}

#[test]
fn authored_episode_id_is_used_verbatim_in_meta_and_line_ids() {
    // A4: an authored frontmatter `episodeId` is used VERBATIM for both
    // `meta.episodeId` and the lineId episode segment (no lowercasing, no
    // `s{s}ep{e}` reformat) — pinning survives episode renumbering.
    const AUTHORED: &str = r#"---
kind: scene
character: bianca
season: 1
episode: 2
episodeId: ep02final
---

## Shot 1.

:narrator: The stage is set.
"#;
    let artifact = compile(&input(AUTHORED)).expect("authored episodeId doc compiles");
    assert_eq!(scene_meta(&artifact).episode_id, "ep02final");
    let line = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Line(l) => Some(l),
            _ => None,
        })
        .expect("a line record");
    assert_eq!(line.line_id, "bianca.ep02final.narrator_0010");
    assert_eq!(
        line.line_id.split('.').nth(1),
        Some("ep02final"),
        "authored episodeId must be the lineId episode segment verbatim"
    );
}

#[test]
fn cut_wait_default_is_reachable_through_the_compile_gate() {
    // C5 review: `::cut`'s manifest declares only assetId/action/full — NO
    // `wait` — so an authored `wait` on `::cut` is rejected `E-UNKNOWN-ATTR` by
    // the D6 check gate and never reaches lowering (the author-override path
    // does not exist for `cut`; only `video`/`camera` declare `wait`, dsl §999).
    // Prove the A8 materialization END-TO-END: a check-clean `::cut` compiles
    // Ok and its record carries the resolved family default `wait: false` (v1
    // non-blocking) — the same value the e2e goldens pin.
    const DOC: &str = r#"---
kind: scene
character: bianca
season: 1
episode: 2
title: Cut gate
---

## Shot 1.

::cut{assetId="CUT.scenarios.bianca.s01ep02.01" action="show" full="true"}

:narrator: The beam lands full-frame.
"#;
    let artifact = compile(&input(DOC)).expect("clean cut doc compiles through the D6 gate");
    let cut = artifact
        .commands
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .find(|v| v["kind"] == "cut")
        .expect("a kind:\"cut\" record");
    assert_eq!(cut["wait"], false, "cut carries the resolved family default");
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
kind: scene
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
kind: scene
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

// --- IR A3: `{{…}}` interpolation placeholders -------------------------------

/// A content line carrying `{{…}}` interps gets an ordered, kind-keyed
/// `placeholders` list (reserved/path/ref), while `text` stays byte-verbatim
/// (the `{{…}}` markers are retained — that string is the localization source).
#[test]
fn content_line_carries_ordered_kind_keyed_placeholders() {
    const DOC: &str = r#"---
kind: scene
character: bianca
season: 1
episode: 2
state:
  run.coins: { type: number, default: 0 }
defs:
  fond: { type: bool, cel: "run.coins >= 1" }
---

## Shot 1.

:bianca{code="0010"}: Hi {{userName}}, {{run.coins}} left, {{@fond}}.
"#;
    let artifact = compile(&input(DOC)).expect("clean compile");
    let line = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Line(l) if l.speaker == "bianca" => Some(l),
            _ => None,
        })
        .expect("bianca line");
    // `text` verbatim: the `{{…}}` markers survive into the artifact.
    assert_eq!(line.text, "Hi {{userName}}, {{run.coins}} left, {{@fond}}.");
    let json = serde_json::to_value(line).unwrap();
    assert_eq!(
        json["placeholders"],
        serde_json::json!([
            { "kind": "reserved", "token": "userName" },
            { "kind": "path", "path": "run.coins" },
            { "kind": "ref", "ref": "@fond" }
        ]),
        "ordered kind-keyed placeholders mirror the interps left-to-right; got {json}"
    );
}

/// A content line with NO interps omits `placeholders` entirely (skip-if-empty)
/// — byte-stability for the existing goldens.
#[test]
fn interp_free_line_omits_placeholders() {
    let artifact = compile(&input(SCENE)).expect("clean compile");
    let line = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Line(l) => Some(l),
            _ => None,
        })
        .expect("a content line");
    let json = serde_json::to_value(line).unwrap();
    assert!(
        json.get("placeholders").is_none(),
        "interp-free line must omit `placeholders`; got {json}"
    );
}

/// A `<choice>` option whose LABEL interpolates carries `placeholders` (scanned
/// from the label string); a plain label omits it. `label` stays verbatim.
#[test]
fn option_label_interp_carries_placeholders() {
    const DOC: &str = r#"---
kind: scene
character: b
season: 1
episode: 1
state:
  run.coins: { type: number, default: 0 }
---

## Shot 1.

<branch id="pick">
  <choice id="give" label="Give {{run.coins}} coins">
    :narrator: Done.
  </choice>
  <choice id="keep" label="Keep them">
    :narrator: Fine.
  </choice>
</branch>
"#;
    let artifact = compile(&input(DOC)).expect("clean compile");
    let choice = artifact
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Choice(ch) => Some(ch),
            _ => None,
        })
        .expect("choice record");
    let give = choice.options.iter().find(|o| o.id == "give").expect("give option");
    let keep = choice.options.iter().find(|o| o.id == "keep").expect("keep option");
    // Label verbatim, interps retained.
    assert_eq!(give.label, "Give {{run.coins}} coins");
    let give_json = serde_json::to_value(give).unwrap();
    assert_eq!(
        give_json["placeholders"],
        serde_json::json!([{ "kind": "path", "path": "run.coins" }]),
        "interpolating label carries its placeholder; got {give_json}"
    );
    let keep_json = serde_json::to_value(keep).unwrap();
    assert!(
        keep_json.get("placeholders").is_none(),
        "non-interpolating label omits `placeholders`; got {keep_json}"
    );
}

// --- dsl 0.2.0: kind: quest compile flow -------------------------------------

/// Mirrors the DSL Appendix D worked example (trimmed): one `<quest>` with 2
/// objectives + an `<on event="questComplete">` arm carrying a `::set` + a
/// `:narrator:` line. `run.*` paths read by `start`/`done` are declared
/// inline via `state:` (with defaults, so defassign is clean) so `check()`
/// passes.
const QUEST_SRC: &str = r#"---
kind: quest
state:
  run.act: { type: bool, default: false }
  run.region: { type: bool, default: false }
---

<quest id="rescueHalsin" title="Rescue" start="run.act">
<objective id="reachGrove" title="Reach" done="run.region"/>
<objective id="freeHalsin" done="run.act"/>

<on event="questComplete">
::set{run.act = true}
:narrator: The quest is complete.
</on>
</quest>
"#;

#[test]
fn quest_doc_compiles_to_quest_artifact() {
    let art = compile(&input(QUEST_SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    assert_eq!(j["kind"], "quest");
    let cmds = j["commands"].as_array().unwrap();
    let q = cmds.iter().find(|c| c["kind"] == "quest").expect("quest record");
    assert_eq!(q["id"], "rescueHalsin");
    assert_eq!(q["objectives"].as_array().unwrap().len(), 2);
    assert!(cmds.iter().any(|c| c["kind"] == "on" && c["event"] == "questComplete"));
    // an <on> body content line lowered as a line record with a {questId} lineId:
    assert!(cmds.iter().any(|c| c["kind"] == "line"
        && c["lineId"].as_str().map_or(false, |s| s.starts_with("rescueHalsin."))));
}

/// A checker-admitted DIRECT quest-body-level content line + `::set` (dsl
/// 0.2.0 §6.3/§6.7 — sibling to `<objective>`/`<on>`, not nested inside
/// either) is LOWERED as an ordinary record in the SAME per-quest stream —
/// NEVER silently dropped (IR addendum §3 preamble note).
#[test]
fn direct_quest_body_content_is_lowered_not_dropped() {
    const SRC: &str = r#"---
kind: quest
state:
  run.act: { type: bool, default: false }
---

<quest id="rescueHalsin" title="Rescue">
:narrator: A quest begins.
::set{run.act = true}
<objective id="reachGrove" done="run.act"/>
</quest>
"#;
    let art = compile(&input(SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let cmds = j["commands"].as_array().unwrap();
    let narrator_line = cmds
        .iter()
        .find(|c| c["kind"] == "line" && c["text"] == "A quest begins.")
        .expect("direct quest-body content-line record");
    assert_eq!(
        narrator_line["lineId"].as_str().map(|s| s.starts_with("rescueHalsin.")),
        Some(true)
    );
    assert!(
        cmds.iter()
            .any(|c| c["kind"] == "set" && c["path"] == "run.act"),
        "direct quest-body `::set` must lower, not drop: {cmds:#?}"
    );
}

/// F5 (final review P1): an EMPTY `<on>` body's `body` target is a REQUIRED
/// `String` (dsl 0.2.0 IR addendum §3.3) that MUST resolve to the quest
/// unit's ONE-PAST-END converge, never whatever record happens to follow it
/// in the pass-2 document-order walk. Before the fix, `walk_quest` bound the
/// fresh label immediately after pushing the `on` record; with an empty
/// body `walk_seq` emits nothing, so the label silently attached to the
/// NEXT emitted record (here, the `::set`) — the handler would run the
/// WRONG content when `questComplete` actually fires. The objective arm
/// already guards this (`obj_labels` is `None` for an empty body); `<on>`
/// must match: the empty-on's `body` addr is the quest unit's past-end,
/// `"001-0400"` (quest + on + set = 3 records, `addr_of(1, 3)`), not the
/// `set` record's own addr, and not a dangling `@n` symbolic label.
#[test]
fn empty_on_body_targets_unit_past_end_not_following_content() {
    const SRC: &str = r#"---
kind: quest
state:
  run.act: { type: bool, default: false }
---

<quest id="rescueHalsin" title="Rescue">
<objective id="reachGrove" done="run.act"/>
<on event="questComplete">
</on>
::set{run.act = true}
</quest>
"#;
    let art = compile(&input(SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let cmds = j["commands"].as_array().unwrap();
    let on = cmds
        .iter()
        .find(|c| c["kind"] == "on" && c["event"] == "questComplete")
        .expect("on record");
    let set = cmds
        .iter()
        .find(|c| c["kind"] == "set" && c["path"] == "run.act")
        .expect("set record");
    let on_body = on["body"].as_str().expect("on.body must be a string addr, never null/@n");
    assert!(
        !on_body.starts_with('@'),
        "on.body must be addressed, never a dangling symbolic label: {on_body}"
    );
    assert_ne!(
        on_body,
        set["addr"].as_str().unwrap(),
        "empty <on> body must NOT dangle onto the following `::set` record: {cmds:#?}"
    );
    assert_eq!(
        on_body, "001-0400",
        "empty <on> body must target the quest unit's one-past-end converge          (IR addendum §3.3), not any live record: {cmds:#?}"
    );
}

#[test]
fn hub_choice_use_expands_component_records_with_source_stamp() {
    // REACHABILITY (task-021): a hub choice body is ordinary SceneBody content
    // (admission.rs: "Recursing into a <branch>/<match>/<hub> child body STAYS
    // SceneBody") and check.rs's `Node::Hub` walk validates `::use` in a hub
    // choice exactly like `Node::Branch` (same `check_use` + `self.walk`
    // recursion, check.rs ~L815-843) — so this doc checks clean; `compile()`
    // runs the D6 gate via `check()` internally, proving it end to end.
    let mut table = std::collections::BTreeMap::new();
    let (comp_body, comp_diags) = lute_syntax::parse(
        "---\ncomponent: greet\n---\n\n## Scene 1.\n\n\
         ::auto{character=\"bianca\" action=\"fade-in-up\"}\n\
         :narrator: A familiar face steps into the light.\n",
    );
    assert!(
        comp_diags.iter().all(|d| d.severity != lute_core_span::Severity::Error),
        "{comp_diags:#?}"
    );
    table.insert(
        "greet".to_string(),
        lute_check::ComponentDef {
            params: Vec::new(),
            body: comp_body,
            src: std::path::PathBuf::from("test://greet"),
        },
    );
    let comps = lute_check::ComponentSet {
        table,
        diags: Vec::new(),
    };

    const HUB_USE: &str = r#"---
kind: scene
character: b
season: 1
episode: 1
---

## Shot 1.

<hub id="chat">
  <choice id="ask" label="Ask" once>
    ::use{component="greet"}
  </choice>
  <choice id="leave" label="Leave" exit>
    :narrator: Bye.
  </choice>
</hub>
"#;
    let mut inp = input(HUB_USE);
    inp.components = comps;

    let check_result = lute_check::check(&inp);
    assert!(
        check_result.ok,
        "::use inside a <hub> choice body must check clean: {:#?}",
        check_result.diagnostics
    );

    let artifact = compile(&inp).expect("hub-choice ::use doc compiles");
    let sprite = artifact.commands.iter().find_map(|c| match c {
        Command::Sprite(s) if s.character == "bianca" => Some(s),
        _ => None,
    });
    assert!(
        sprite.is_some(),
        "the component's ::auto record must survive compilation \
         (before the fix it is silently dropped): {:#?}",
        artifact.commands
    );
    let sprite = sprite.unwrap();
    assert_eq!(
        sprite.stamp.source.as_ref().map(|s| s.component.as_str()),
        Some("greet"),
        "component-sourced record must carry the source.component stamp"
    );

    let narrator_line = artifact.commands.iter().find_map(|c| match c {
        Command::Line(l) if l.text.starts_with("A familiar face") => Some(l),
        _ => None,
    });
    assert!(
        narrator_line.is_some(),
        "the component's narrator line must survive compilation: {:#?}",
        artifact.commands
    );
    assert_eq!(
        narrator_line
            .unwrap()
            .stamp
            .source
            .as_ref()
            .map(|s| s.component.as_str()),
        Some("greet")
    );

    // No residual `::use`/component-sentinel record survives lowering.
    assert!(
        artifact
            .commands
            .iter()
            .all(|c| !matches!(c, Command::Other(o) if o.tag == "use")),
        "no residual ::use record"
    );
}

#[test]
fn hub_choice_persist_synthesizes_trailing_set_record() {
    // Companion regression: `persist="run" into="run.metGreeted"` sugar on a
    // <hub> choice must synthesize a trailing ::set, exactly like a <branch>
    // choice (dsl §11.1.1; check.rs's `check_choice_persist` is already shared
    // verbatim between Branch and Hub choices, so the grammar admits it — the
    // gap was purely in `lute-compile`'s normalize pass never visiting Hub).
    const HUB_PERSIST: &str = r#"---
kind: scene
character: b
season: 1
episode: 1
state:
  run.metGreeted: { type: bool, default: false }
---

## Shot 1.

<hub id="chat">
  <choice id="ask" label="Ask" once>
    :narrator: Hi.
  </choice>
  <choice id="thank" label="Thank her" exit persist="run" into="run.metGreeted">
    :narrator: Thanks.
  </choice>
</hub>
"#;
    let inp = input(HUB_PERSIST);
    let check_result = lute_check::check(&inp);
    assert!(
        check_result.ok,
        "persist sugar on a <hub> choice must check clean: {:#?}",
        check_result.diagnostics
    );

    let artifact = compile(&inp).expect("hub persist doc compiles");
    let set = artifact.commands.iter().find_map(|c| match c {
        Command::Set(s) if s.path == "run.metGreeted" => Some(s),
        _ => None,
    });
    assert!(
        set.is_some(),
        "persist=\"run\" into=\"run.metGreeted\" on a hub choice must synthesize \
         a ::set (before the fix, synth_persist is never called for Hub): {:#?}",
        artifact.commands
    );
    let set = set.unwrap();
    assert_eq!(set.op, "=");
    assert_eq!(set.value, "true");
}
