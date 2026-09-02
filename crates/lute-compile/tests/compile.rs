//! `compile()` orchestration: the D6 gate, the folded-state envelope, id
//! stamping, CEL expansion in situ, and byte determinism.

use lute_check::{CheckInput, Mode};
use lute_compile::{compile, ArtifactMeta, Command, Role};

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_test_vocab::vocab_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
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
@bianca{code="0010" emotion="surprised"}: Oh!

<branch id="number">
  <choice id="blunt" label="Flat">
    @fixer{code="0010"}: Number.
  </choice>
  <choice id="soft" label="Gentle">
    ::set{scene.affect.bianca += 1}
  </choice>
</branch>

<match on="scene.choices.number">
  <when test="@fond">
    @fixer{mono}: Nice.
  </when>
  <when test="$ == 'blunt'">
    @fixer{mono}: Flat.
  </when>
  <otherwise>
    @fixer{mono}: Hm.
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
    @narrator: Sure.
  </choice>
  <choice id="curious" label="Be curious" when="scene.affect.b >= 1">
    @narrator: Hmm.
  </choice>
  <choice id="leave" label="Leave" exit>
    @narrator: Bye.
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
    assert!(
        !hub.converge.is_empty(),
        "converge addr filled by address pass"
    );
    assert_eq!(hub.options.len(), 3);
    let opt = |id: &str| hub.options.iter().find(|o| o.id == id).expect("option");
    let ask = opt("ask");
    assert!(ask.once && !ask.exit, "ask: once, not exit");
    assert!(ask.when.is_none() && ask.expr.is_none(), "ask is unguarded");
    let curious = opt("curious");
    assert!(
        !curious.once && !curious.exit,
        "curious: neither once nor exit"
    );
    assert!(
        curious.when.is_some(),
        "guarded option carries the raw `when`"
    );
    assert!(
        curious.expr.is_some(),
        "guarded option carries the lowered A7 expr"
    );
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
    assert_eq!(
        jumps.len(),
        1,
        "only the exit arm jumps to converge, got {jumps:?}"
    );
    assert_eq!(
        jumps[0], hub.converge,
        "the exit-arm jump targets the hub converge"
    );

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
        assert_eq!(
            v.default,
            Some(serde_json::json!(false)),
            "visited {cid} default false"
        );
    }
}

#[test]
fn clean_doc_compiles_with_envelope_expansion_and_ids() {
    let inp = input(SCENE);
    let artifact = compile(&inp).expect("clean compile");
    // A9 envelope hardening: language pin, IR schema version, capability stamp.
    assert_eq!(artifact.lute, "0.15.1");
    assert_eq!(artifact.ir_version, "0.15.1");
    assert_eq!(artifact.capability_version, inp.snapshot.version);
    assert!(
        !artifact.capability_version.is_empty(),
        "capabilityVersion must be a non-empty snapshot stamp"
    );
    assert_eq!(scene_meta(&artifact).character.as_deref(), Some("bianca"));
    // A4/A9: episodeId normalized lowercase to match the lineId episode segment.
    assert_eq!(scene_meta(&artifact).episode_id.as_deref(), Some("s01ep02"));
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

    // A4/A9 byte-for-byte: every lineId's episode segment == meta.episodeId
    // (still true post-0.15 for the derived-key path — `episode_id` stays
    // Some and resolved as today; the lineId prefix is now `meta.id` which
    // for this legacy doc equals `{character}.{episodeId}`).
    let expected_episode_id = scene_meta(&artifact)
        .episode_id
        .as_deref()
        .expect("legacy doc keeps a resolved episodeId");
    for cmd in &artifact.commands {
        if let Command::Line(l) = cmd {
            if l.line_id.is_empty() {
                continue;
            }
            let seg = l.line_id.split('.').nth(1).expect("lineId episode segment");
            assert_eq!(
                seg, expected_episode_id,
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

@narrator: The stage is set.
"#;
    let artifact = compile(&input(AUTHORED)).expect("authored episodeId doc compiles");
    assert_eq!(
        scene_meta(&artifact).episode_id.as_deref(),
        Some("ep02final")
    );
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

@narrator: The beam lands full-frame.
"#;
    let artifact = compile(&input(DOC)).expect("clean cut doc compiles through the D6 gate");
    let cut = artifact
        .commands
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .find(|v| v["kind"] == "cut")
        .expect("a kind:\"cut\" record");
    assert_eq!(
        cut["wait"], false,
        "cut carries the resolved family default"
    );
}

#[test]
fn injection_warnings_do_not_gate_and_output_is_byte_stable() {
    // The ::auto has no anchor => an anchor is INJECTED (a warning-free case);
    // warnings never gate at all (only Errors do, D6).
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
@bianca{code="0010"}: Hi.

<branch id="couch">
  <choice id="help" label="Help">
    @fixer{code="0010"}: Sure.
  </choice>
  <choice id="ignore" label="Ignore">
    @fixer{code="0020"}: No.
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
@bianca{code="0010"}: Hi.

<branch id="couch">
  <choice id="help" label="Help">
    @fixer{code="0010"}: Sure.
  </choice>
  <choice id="ignore" label="Ignore">
    @fixer{code="0020"}: No.
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

@bianca{code="0010"}: Hi {{userName}}, {{run.coins}} left, {{@fond}}.
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
    @narrator: Done.
  </choice>
  <choice id="keep" label="Keep them">
    @narrator: Fine.
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
    let give = choice
        .options
        .iter()
        .find(|o| o.id == "give")
        .expect("give option");
    let keep = choice
        .options
        .iter()
        .find(|o| o.id == "keep")
        .expect("keep option");
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
/// `@narrator:` line. `run.*` paths read by `start`/`done` are declared
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
@narrator: The quest is complete.
</on>
</quest>
"#;

#[test]
fn quest_doc_compiles_to_quest_artifact() {
    let art = compile(&input(QUEST_SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    assert_eq!(j["kind"], "quest");
    let cmds = j["commands"].as_array().unwrap();
    let q = cmds
        .iter()
        .find(|c| c["kind"] == "quest")
        .expect("quest record");
    assert_eq!(q["id"], "rescueHalsin");
    assert_eq!(q["objectives"].as_array().unwrap().len(), 2);
    assert!(cmds
        .iter()
        .any(|c| c["kind"] == "on" && c["event"] == "questComplete"));
    // an <on> body content line lowered as a line record with a {questId} lineId:
    assert!(cmds.iter().any(|c| c["kind"] == "line"
        && c["lineId"]
            .as_str()
            .is_some_and(|s| s.starts_with("rescueHalsin."))));
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
@narrator: A quest begins.
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
        narrator_line["lineId"]
            .as_str()
            .map(|s| s.starts_with("rescueHalsin.")),
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
    let on_body = on["body"]
        .as_str()
        .expect("on.body must be a string addr, never null/@n");
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

// --- dsl 0.16.0: declarative rewards (Task 3) -------------------------------

const QUEST_REWARDS_SRC: &str = r#"---
kind: quest
state:
  run.act: { type: bool, default: false }
  run.region: { type: bool, default: false }
  run.freed: { type: bool, default: false }
  run.dead: { type: bool, default: false }
---

<quest id="rescueHalsin" title="Rescue" start="run.act" fail="run.dead">
<reward kind="XP" amount="100"/>
<reward kind="GOLD" target="party" amount="50..200" when="run.freed"/>
<reward kind="TROPHY" target="halsin" on="failed"/>
<objective id="reachGrove" done="run.region">
<reward kind="XP"/>
<reward kind="ITEM" target="map" amount="1"/>
</objective>
<objective id="freeHalsin" done="run.freed"/>
</quest>
"#;

/// dsl 0.16.0 §2/§3 (Task 3): quest-level and objective-level `<reward/>`
/// entries lower into `QuestCmd.rewards` / `ObjectiveEntry.rewards` in
/// DECLARATION ORDER with the exact wire field names — `amountMin`/
/// `amountMax` for a range, `amount` for a scalar (defaulting to `1` when
/// unauthored), `when.raw` verbatim, and `on` present only for a
/// quest-level entry authored `on="failed"`. Every other Option is
/// `skip_serializing_if`, so the JSON carries exactly what the author
/// wrote (no reordering, no synthesized keys).
#[test]
fn rewards_serialize_in_declaration_order_with_wire_names() {
    let art = compile(&input(QUEST_REWARDS_SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let quest = j["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "quest")
        .expect("quest record");

    // Quest-level rewards, declaration order preserved.
    let quest_rewards = quest["rewards"]
        .as_array()
        .expect("quest.rewards is an array");
    assert_eq!(
        quest_rewards.len(),
        3,
        "three quest-level `<reward/>` entries: {quest_rewards:#?}"
    );
    // Scalar amount: `amount` present, no min/max, no `on`/`target`/`when`.
    assert_eq!(
        quest_rewards[0],
        serde_json::json!({"kind":"XP","amount":100}),
        "scalar reward serializes with `amount` alone"
    );
    // Range amount: `amountMin`/`amountMax`; `amount` skipped; `when.raw` verbatim.
    assert_eq!(
        quest_rewards[1]["kind"], "GOLD",
        "second reward kind preserved in declaration order"
    );
    assert_eq!(quest_rewards[1]["target"], "party");
    assert!(
        quest_rewards[1].get("amount").is_none(),
        "range reward must NOT carry a scalar `amount` key: {}",
        quest_rewards[1]
    );
    assert_eq!(quest_rewards[1]["amountMin"], 50);
    assert_eq!(quest_rewards[1]["amountMax"], 200);
    assert_eq!(
        quest_rewards[1]["when"]["raw"], "run.freed",
        "when.raw preserved verbatim (wire contract)"
    );
    // `on="failed"` reaches the wire on a quest-level entry, default amount=1.
    assert_eq!(
        quest_rewards[2],
        serde_json::json!({"kind":"TROPHY","target":"halsin","amount":1,"on":"failed"}),
        "on=\"failed\" survives on quest-level; unauthored amount defaults to 1"
    );

    // Objective-level rewards on the reachGrove objective.
    let objectives = quest["objectives"].as_array().unwrap();
    let reach = &objectives[0];
    assert_eq!(reach["id"], "reachGrove");
    let obj_rewards = reach["rewards"]
        .as_array()
        .expect("objective.rewards is an array");
    assert_eq!(obj_rewards.len(), 2, "two objective rewards in order");
    // Unauthored amount → default 1; no target, no when, no on.
    assert_eq!(obj_rewards[0], serde_json::json!({"kind":"XP","amount":1}));
    assert_eq!(
        obj_rewards[1],
        serde_json::json!({"kind":"ITEM","target":"map","amount":1})
    );
    assert!(
        obj_rewards.iter().all(|r| r.get("on").is_none()),
        "objective-level entries NEVER carry `on` (wire contract)"
    );

    // The rewardless objective omits the `rewards` key entirely.
    let free = &objectives[1];
    assert_eq!(free["id"], "freeHalsin");
    assert!(
        free.get("rewards").is_none(),
        "rewardless objective must omit the `rewards` key: {free}"
    );

    // Declaration-order stability contract: the emitted JSON object keys
    // (in insertion order) place `rewards` AFTER `objectives` on the quest
    // and AFTER `quest` on each objective — the exact field-declaration
    // order pinned by the ir.rs header. This is what keeps every
    // pre-0.16.0 artifact byte-identical below `objectives[].quest`.
    let quest_keys: Vec<&str> = quest
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let (obj_i, rew_i) = (
        quest_keys.iter().position(|k| *k == "objectives").unwrap(),
        quest_keys.iter().position(|k| *k == "rewards").unwrap(),
    );
    assert!(
        obj_i < rew_i,
        "quest keys: `rewards` must follow `objectives`: {quest_keys:?}"
    );
}

/// A rewardless quest artifact must be BYTE-IDENTICAL to the pre-change
/// output — proven directly by diffing against the same source's earlier-
/// task golden shape via serde_json::Value (deterministic key set) plus a
/// belt-and-suspenders check that the `rewards` key exists nowhere in the
/// artifact JSON. `skip_serializing_if = "Vec::is_empty"` on both fields
/// is the load-bearing invariant; a regression would surface as either the
/// literal `"rewards":` bytes reappearing or the merged Value diverging.
#[test]
fn rewardless_quest_stays_byte_identical_to_pre_change() {
    let art = compile(&input(QUEST_SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let wire = serde_json::to_string(&art).unwrap();
    assert!(
        !wire.contains("\"rewards\""),
        "rewardless artifact must not carry a `rewards` key anywhere: {wire}"
    );

    // Every quest record and every objective row must OMIT the key
    // (guarding against a shape-only `Some([])`-style regression on either
    // field independently).
    for c in j["commands"].as_array().unwrap() {
        if c["kind"] == "quest" {
            assert!(
                c.get("rewards").is_none(),
                "rewardless quest carries no `rewards`: {c}"
            );
            for o in c["objectives"].as_array().unwrap() {
                assert!(
                    o.get("rewards").is_none(),
                    "rewardless objective carries no `rewards`: {o}"
                );
            }
        }
    }
}

/// Subquest synthesis (0.14.0 §2.1/§2.2) runs BEFORE reward lowering — the
/// two features are orthogonal, and neither may perturb the other. This
/// pins both directions: a parent whose objective is `quest="child"` still
/// carries its own AUTHORED rewards on the parent-level entry AND on
/// unrelated sibling objectives; the referenced child quest independently
/// carries its own rewards untouched (subquest passes never move, drop,
/// merge, or rewrite reward vectors — the inverse of the 0.14.0 done/fail
/// synthesis that MUST touch CEL). Task 4/5 vocabulary/runtime work will
/// consume these vectors verbatim.
#[test]
fn subquest_synthesis_leaves_rewards_untouched() {
    const SUBQUEST_REWARDS: &str = r#"---
kind: quest
luteVersion: "0.15.1"
state:
  run.act: { type: bool, default: false }
  run.done: { type: bool, default: false }
---

<quest id="parent" title="Parent" start="run.act">
<reward kind="PARENT_XP" amount="500"/>
<objective id="hookChild" quest="child"/>
<objective id="ownDone" done="run.done">
<reward kind="STEP" amount="10"/>
</objective>
</quest>

<quest id="child" title="Child" start="run.act">
<reward kind="CHILD_XP" amount="1..3"/>
<objective id="childStep" done="run.done"/>
</quest>
"#;
    let art = compile(&input(SUBQUEST_REWARDS)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let quests: Vec<&serde_json::Value> = j["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "quest")
        .collect();
    assert_eq!(quests.len(), 2, "two quests emitted");

    let parent = quests.iter().find(|q| q["id"] == "parent").unwrap();
    // Parent-level authored reward survives.
    assert_eq!(
        parent["rewards"],
        serde_json::json!([{"kind":"PARENT_XP","amount":500}]),
        "parent quest-level reward untouched"
    );
    // Subquest hook objective has NO rewards and NO `rewards` key emitted;
    // sibling objective's authored reward survives verbatim.
    let parent_objs = parent["objectives"].as_array().unwrap();
    let hook = &parent_objs[0];
    assert_eq!(hook["id"], "hookChild");
    assert_eq!(
        hook["quest"], "child",
        "subquest synthesis stamps `quest:` on the hook"
    );
    assert!(
        hook.get("rewards").is_none(),
        "rewardless subquest hook must omit `rewards`: {hook}"
    );
    let own = &parent_objs[1];
    assert_eq!(own["id"], "ownDone");
    assert_eq!(
        own["rewards"],
        serde_json::json!([{"kind":"STEP","amount":10}]),
        "sibling objective's reward untouched by subquest synthesis"
    );

    let child = quests.iter().find(|q| q["id"] == "child").unwrap();
    // Child-level reward survives across subquest synthesis (which only
    // touches the PARENT's `fail` and the HOOK's `done`).
    assert_eq!(
        child["rewards"],
        serde_json::json!([{"kind":"CHILD_XP","amountMin":1,"amountMax":3}]),
        "child quest-level reward untouched"
    );
    // The child's own objectives were never subquest-hooked; no rewards
    // authored, no `rewards` key emitted.
    for o in child["objectives"].as_array().unwrap() {
        assert!(
            o.get("rewards").is_none(),
            "child objective without authored rewards omits `rewards`: {o}"
        );
    }
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
         @narrator: A familiar face steps into the light.\n",
    );
    assert!(
        comp_diags
            .iter()
            .all(|d| d.severity != lute_core_span::Severity::Error),
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
    @narrator: Bye.
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
fn hub_choice_into_synthesizes_trailing_set_record() {
    // Companion regression: an `into="run.metGreeted"` record sugar on a <hub>
    // choice must synthesize a trailing ::set, exactly like a <branch> choice
    // (dsl 0.6.0 §2.1; the record trigger is `into=` alone — `persist=` was
    // removed from the language). The gap was purely in `lute-compile`'s
    // normalize pass never visiting Hub.
    const HUB_INTO: &str = r#"---
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
    @narrator: Hi.
  </choice>
  <choice id="thank" label="Thank her" exit into="run.metGreeted">
    @narrator: Thanks.
  </choice>
</hub>
"#;
    let inp = input(HUB_INTO);
    let check_result = lute_check::check(&inp);
    assert!(
        check_result.ok,
        "into sugar on a <hub> choice must check clean: {:#?}",
        check_result.diagnostics
    );

    let artifact = compile(&inp).expect("hub into doc compiles");
    let set = artifact.commands.iter().find_map(|c| match c {
        Command::Set(s) if s.path == "run.metGreeted" => Some(s),
        _ => None,
    });
    assert!(
        set.is_some(),
        "into=\"run.metGreeted\" on a hub choice must synthesize a ::set \
         (before the fix, synth is never called for Hub): {:#?}",
        artifact.commands
    );
    let set = set.unwrap();
    assert_eq!(set.op, "=");
    assert_eq!(set.value, "true");
}

#[test]
fn offscreen_and_voiceover_lines_are_voiced_and_emit_no_extra_sprite() {
    // dsl 0.2.2 §D7: `os`/`vo` change role (and are voiced — heard, just
    // with no on-screen sprite this line) but do NOT themselves introduce a
    // sprite command — `lower_line` only ever lowers a `:line` to
    // `Command::Line`; sprite records come exclusively from `::auto` (char-
    // cast §7.1 currently has no per-line sprite-resolution path to skip).
    const DOC: &str = r#"---
kind: scene
character: b
season: 1
episode: 1
---

## Shot 1.

::auto{character="fixer" anchor="center" action="fade-in-up"}
@fixer{vo}: A voiceover aside.
@fixer{os}: Behind the door.
@fixer: Back on stage.
"#;
    let artifact = compile(&input(DOC)).expect("os/vo doc compiles");
    let sprite_count = artifact
        .commands
        .iter()
        .filter(|c| matches!(c, Command::Sprite(_)))
        .count();
    assert_eq!(
        sprite_count, 1,
        "only ::auto's own sprite record — os/vo lines add none: {:#?}",
        artifact.commands
    );

    let by_text = |t: &str| {
        artifact
            .commands
            .iter()
            .find_map(|c| match c {
                Command::Line(l) if l.text == t => Some(l),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing line {t:?}"))
    };
    let vo = by_text("A voiceover aside.");
    assert_eq!(vo.role, Role::Voiceover);
    assert!(vo.voice_key.is_some(), "voiceover is voiced (heard)");

    let os = by_text("Behind the door.");
    assert_eq!(os.role, Role::Offscreen);
    assert!(os.voice_key.is_some(), "offscreen is voiced (heard)");

    let dlg = by_text("Back on stage.");
    assert_eq!(dlg.role, Role::Dialogue);
    assert!(dlg.voice_key.is_some());
}

/// dsl 0.3.0 §5 delta lowering (0.3.0 T14): an `<on>` arm interleaving
/// `::set`/`::assert`/`::retract` lowers each in DOCUMENT ORDER, and every
/// assert/retract record carries a real (non-empty, addressed) `addr` —
/// never left as an empty placeholder.
#[test]
fn assert_retract_interleave_with_set_in_document_order() {
    const SRC: &str = r#"---
kind: quest
entities:
  c: { members: [ana] }
relations:
  inParty: { args: [c] }
  atLoc: { args: [c, c] }
state:
  run.act: { type: bool, default: false }
---

<quest id="rescueHalsin" title="Rescue">
<objective id="reachGrove" done="run.act"/>

<on event="questComplete">
::set{run.act = true}
::assert{ inParty(ana) }
::retract{ atLoc(ana, _) }
</on>
</quest>
"#;
    let art = compile(&input(SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let cmds = j["commands"].as_array().unwrap();
    let kinds: Vec<&str> = cmds.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    let set_i = kinds.iter().position(|k| *k == "set").expect("set record");
    let assert_i = kinds
        .iter()
        .position(|k| *k == "assert")
        .expect("assert record");
    let retract_i = kinds
        .iter()
        .position(|k| *k == "retract")
        .expect("retract record");
    assert!(
        set_i < assert_i && assert_i < retract_i,
        "document order preserved: {kinds:?}"
    );

    let assert_rec = &cmds[assert_i];
    assert_eq!(assert_rec["relation"], "inParty");
    assert_eq!(assert_rec["args"], serde_json::json!(["ana"]));
    assert!(assert_rec["addr"].as_str().is_some_and(|s| !s.is_empty()));

    let retract_rec = &cmds[retract_i];
    assert_eq!(retract_rec["relation"], "atLoc");
    assert_eq!(retract_rec["args"], serde_json::json!(["ana", "_"]));
    assert!(retract_rec["addr"].as_str().is_some_and(|s| !s.is_empty()));
}

// -- dsl 0.12.0: forward jump (`::mark`/line `id=`/`::next`) ----------------

const FORWARD_JUMP_SCENE: &str = r#"---
kind: scene
character: bianca
season: 1
episode: 2
state:
  run.blessed: { type: bool, default: false }
---

## Shot 1.

<branch id="pick">
  <choice id="a" label="A">
    ::next{to="join"}
  </choice>
  <choice id="b" label="B">
    @narrator: taking the b path
  </choice>
</branch>

## Shot 2.

::mark{id="join"}
@narrator{id="afterJoin"}: we joined here
::next{to="tail" when="run.blessed"}
@narrator: fallthrough content
::end{reason="completed"}

## Shot 3.

::mark{id="tail"}
@narrator: tail reached
::end{reason="tailed"}
"#;

/// One command's `kind`/`addr`/`target` (Jump) as a plain triple, easing
/// index/assert readability below.
fn kind_addr(v: &serde_json::Value) -> (&str, &str) {
    (v["kind"].as_str().unwrap(), v["addr"].as_str().unwrap())
}

/// dsl 0.12.0 §1: mark/line-id/next normal-path check+compile — an
/// UNCONDITIONAL `::next{to}` (in a `<branch>` choice) resolves to the addr
/// of the NEXT record after a `::mark{id}` in a LATER shot; a `::mark`'s own
/// label resolves identically to a content line's `id=` at the SAME record
/// (both name the shot-2 opener). `check()` must accept this document
/// clean, and `compile()` must produce a `jump` record whose `target`
/// equals the label site's real `addr` — never a `"@n"`/`"#id"` symbol.
#[test]
fn unconditional_next_resolves_across_shots_to_mark_and_line_id() {
    let ci = input(FORWARD_JUMP_SCENE);
    let check = lute_check::check(&ci);
    assert!(check.ok, "{:#?}", check.diagnostics);

    let art = compile(&ci).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let cmds = j["commands"].as_array().unwrap();

    // The unconditional `::next{to="join"}` inside choice `a` lowers to a
    // plain `jump` record (no new Command kind — reuses the SAME `JumpCmd`
    // shape a branch/match converge already emits).
    let unguarded_jump = cmds
        .iter()
        .find(|c| c["kind"] == "jump" && c["target"] != serde_json::Value::Null)
        .expect("an unconditional ::next lowers to a jump record");
    let target = unguarded_jump["target"].as_str().unwrap();
    assert!(
        target.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "target must be a resolved real addr, never a `@n`/`#id` symbol: {target}"
    );

    // `::mark{id="join"}` emits NO record of its own; the label resolves to
    // whatever record comes right after it — the line carrying `id="afterJoin"`
    // — so BOTH names must resolve to the exact same addr, and the jump's
    // target must be that addr.
    let joined_line = cmds
        .iter()
        .find(|c| c["kind"] == "line" && c["text"] == "we joined here")
        .expect("the joined-to line");
    let (_, joined_addr) = kind_addr(joined_line);
    assert_eq!(
        target, joined_addr,
        "::next{{to=\"join\"}} must land exactly on the mark's bound record"
    );

    // Shot ordering: the target addr's shot segment must be STRICTLY greater
    // than the jump's own shot segment (forward across a shot boundary).
    let (_, jump_addr) = kind_addr(unguarded_jump);
    assert!(
        jump_addr.split('-').next().unwrap() < joined_addr.split('-').next().unwrap(),
        "the jump ({jump_addr}) must land in a LATER shot than it is authored in ({joined_addr})"
    );
}

/// dsl 0.12.0 §3/§4: a GUARDED `::next{to when}` desugars
/// (`normalize::synth_when_next_match`) into the SAME canonical one-arm
/// `<match>` a gated line uses — this pins BOTH arms' compiled output: the
/// `when`-true arm ends in a `jump` targeting the far shot's `::mark{id="tail"}`;
/// the fall-through (`otherwise`) arm's body (`fallthrough content` + the
/// FIRST `::end{reason="completed"}`) is untouched, never merged with the
/// jump arm. Confirms "가드 next의 양갈래 하강" end to end.
#[test]
fn guarded_next_lowers_to_two_arm_match_both_branches() {
    let ci = input(FORWARD_JUMP_SCENE);
    let art = compile(&ci).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    let cmds = j["commands"].as_array().unwrap();

    // The guarded next desugars to a `match` record (dsl IR §4.4: `arms`
    // carries only `When` arms; `Otherwise` is the SEPARATE `otherwise`
    // target field) — locate it by its one-arm shape plus a present
    // `otherwise` (the synthesized empty-body Otherwise, dsl 0.12.0).
    let matches: Vec<&serde_json::Value> = cmds.iter().filter(|c| c["kind"] == "match").collect();
    assert_eq!(
        matches.len(),
        1,
        "exactly one guarded ::next desugars to one match: {cmds:#?}"
    );
    let m = matches[0];
    let arms = m["arms"].as_array().unwrap();
    assert_eq!(
        arms.len(),
        1,
        "the desugar is a canonical one `When` arm: {arms:#?}"
    );
    assert!(
        m["otherwise"].as_str().is_some(),
        "the desugar's synthesized empty-body Otherwise still resolves to a converge target: {m:#?}"
    );

    // Arm 1 (`When test="$"`) is TRUE iff the hoisted subject (`run.blessed`)
    // decides true — its body is the now-unconditional `::next{to="tail"}`,
    // lowered through the ORDINARY unconditional-jump path (no new lowering
    // code): find the `jump` record whose target lands on shot 3's tail mark.
    let tail_line = cmds
        .iter()
        .find(|c| c["kind"] == "line" && c["text"] == "tail reached")
        .expect("the tail line");
    let (_, tail_addr) = kind_addr(tail_line);
    let guarded_jump = cmds
        .iter()
        .find(|c| c["kind"] == "jump" && c["target"] == tail_addr)
        .expect("the guarded ::next's true arm reaches the tail mark via an ordinary jump record");
    let (_, guarded_jump_addr) = kind_addr(guarded_jump);
    assert!(
        guarded_jump_addr.split('-').next().unwrap() < tail_addr.split('-').next().unwrap(),
        "the guarded jump must also land in a strictly later shot"
    );

    // Arm 2 (`Otherwise`, the guard-false fall-through) is untouched: the
    // fallthrough line and the FIRST `::end{reason="completed"}` still exist,
    // in document order, right after the guard's converge.
    let fallthrough = cmds
        .iter()
        .find(|c| c["kind"] == "line" && c["text"] == "fallthrough content")
        .expect("fall-through content survives the guarded next's false arm");
    let first_end = cmds
        .iter()
        .find(|c| c["kind"] == "end" && c["reason"] == "completed")
        .expect("the first ::end{reason=completed} survives, distinct from the tail shot's ::end");
    let (_, fallthrough_addr) = kind_addr(fallthrough);
    let (_, first_end_addr) = kind_addr(first_end);
    assert!(
        fallthrough_addr < first_end_addr,
        "fallthrough content precedes its own ::end"
    );
    // The fall-through path never reaches shot 3's tail: a second, distinct
    // `::end{reason="tailed"}` record exists for that path.
    let tail_end = cmds
        .iter()
        .find(|c| c["kind"] == "end" && c["reason"] == "tailed")
        .expect("the tail shot's own ::end{reason=tailed} is a SEPARATE record (multi-end)");
    assert_ne!(
        first_end["addr"], tail_end["addr"],
        "two independent ::end records — the multi-end combination"
    );
}

/// dsl 0.12.0: a check-clean document with a forward `::next` never fires
/// `E-NEXT-UNDEFINED`/`E-NEXT-BACKWARD`/`E-MARK-DUP`, and produces NO
/// `E-COMPILE-INTERNAL` (the compiler-bug fallback for an unresolved label,
/// `address::assign_addresses`) — every named-label placeholder must be
/// resolved.
#[test]
fn forward_jump_scene_check_and_compile_are_both_clean() {
    let ci = input(FORWARD_JUMP_SCENE);
    let check = lute_check::check(&ci);
    assert!(check.ok, "{:#?}", check.diagnostics);
    let art = compile(&ci).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    for cmd in j["commands"].as_array().unwrap() {
        if let Some(t) = cmd.get("target").and_then(|t| t.as_str()) {
            assert!(
                !t.starts_with('@') && !t.starts_with('#'),
                "every target must be a resolved real addr, found unresolved symbol {t:?} in {cmd:#?}"
            );
        }
    }
}

// --- dsl 0.15.0 §2/§3: authored scene identity + descriptive meta block ----

/// dsl 0.15.0 §2: an authored `id:` becomes the ONE canonical scene key —
/// stamped into `meta.id`, prefixed onto every `lineId`, joined into
/// `prereqEdges[].node`, and echoed as the index `document_key`. All four
/// consumers must land byte-identical (the "one shared resolution point"
/// architecture the plan turns on).
#[test]
fn authored_id_flows_into_meta_line_prefix_prereq_and_document_key() {
    const DOC: &str = r#"---
kind: scene
id: anseo.s01ep01
after: "visited('kestrel.s01ep01')"
---

## Shot 1.

@narrator: Hello.
"#;
    let art = compile(&input(DOC)).expect("authored-id doc compiles");
    let v = serde_json::to_value(&art).unwrap();

    // `meta.id` — the shared canonical key.
    assert_eq!(v["meta"]["id"], serde_json::json!("anseo.s01ep01"));

    // lineId prefix — the same `anseo.s01ep01.` string, per lineId (`{prefix}.{speaker}_{code}`).
    let line = art
        .commands
        .iter()
        .find_map(|c| match c {
            Command::Line(l) => Some(l),
            _ => None,
        })
        .expect("a content line");
    assert!(
        line.line_id.starts_with("anseo.s01ep01."),
        "lineId {} must be prefixed by the authored id",
        line.line_id
    );

    // `prereqEdges[].node` — same string.
    assert_eq!(
        v["prereqEdges"][0]["node"],
        serde_json::json!("anseo.s01ep01")
    );
    assert_eq!(
        v["prereqEdges"][0]["after"],
        serde_json::json!("visited('kestrel.s01ep01')")
    );

    // Index `document_key` — same string.
    assert_eq!(
        lute_compile::index::document_key(&art),
        "anseo.s01ep01",
        "document_key must equal meta.id verbatim"
    );
}

/// dsl 0.15.0 §7 wire-compat: a legacy (no `id:`) document's 0.15 artifact
/// differs from a pinned 0.14-shape expectation ONLY by the added `id`
/// field and the two version strings (`lute`, `irVersion`). Asserted via
/// `serde_json::Value` diff, not string compare — field order changes are
/// deliberate here (the `id` line lands FIRST inside `meta`).
#[test]
fn legacy_document_artifact_differs_from_014_only_by_id_and_versions() {
    const DOC: &str = r#"---
kind: scene
character: bianca
season: 1
episode: 2
title: Legacy
---

## Shot 1.

@narrator: Hi.
"#;
    let art = compile(&input(DOC)).expect("legacy doc compiles");
    let capability_version = art.capability_version.clone();
    let actual = serde_json::to_value(&art).unwrap();

    // What a 0.14 emit would have produced for the SAME document: the same
    // envelope, minus `meta.id`, with `lute`/`irVersion` pinned to 0.14.0.
    let mut expected = actual.clone();
    expected["lute"] = serde_json::json!("0.14.0");
    expected["irVersion"] = serde_json::json!("0.14.0");
    let expected_meta = expected["meta"].as_object_mut().unwrap();
    expected_meta.remove("id");

    // Reconstruct the pinned 0.14 shape from the ground up (not from the
    // 0.15 output) to prove `expected` isn't tautologically = actual.
    let pinned_014 = serde_json::json!({
        "kind": "scene",
        "lute": "0.14.0",
        "irVersion": "0.14.0",
        "capabilityVersion": capability_version,
        "meta": {
            "character": "bianca",
            "season": 1,
            "episode": 2,
            "episodeId": "s01ep02",
            "title": "Legacy",
        },
        "state": actual["state"].clone(),
        "commands": actual["commands"].clone(),
        "shots": actual["shots"].clone(),
    });
    assert_eq!(
        expected, pinned_014,
        "0.14 shape reconstruction must match the artifact minus id/versions"
    );

    // The core §7 claim: exactly three keys differ (`meta.id` added, two
    // version strings bumped) — nothing else moved.
    assert_eq!(actual["kind"], pinned_014["kind"]);
    assert_eq!(actual["capabilityVersion"], pinned_014["capabilityVersion"]);
    assert_eq!(actual["commands"], pinned_014["commands"]);
    assert_eq!(actual["meta"]["character"], pinned_014["meta"]["character"]);
    assert_eq!(actual["meta"]["season"], pinned_014["meta"]["season"]);
    assert_eq!(actual["meta"]["episode"], pinned_014["meta"]["episode"]);
    assert_eq!(actual["meta"]["episodeId"], pinned_014["meta"]["episodeId"]);
    assert_eq!(actual["meta"]["title"], pinned_014["meta"]["title"]);
    assert_eq!(actual["meta"]["id"], serde_json::json!("bianca.s01ep02"));
    assert_eq!(actual["lute"], serde_json::json!("0.15.1"));
    assert_eq!(actual["irVersion"], serde_json::json!("0.15.1"));
}

/// dsl 0.15.0 §3: the authored `extra:` block lands under `meta.extra`
/// key-sorted (BTreeMap serialization), scalar and flat-scalar-list values
/// pass through JSON-shaped. Sanctioned on both scene and quest roots.
#[test]
fn extra_block_lands_under_meta_extra_key_sorted() {
    const SCENE_META: &str = r#"---
kind: scene
id: anseo.s01ep01
extra:
  arc: main
  tags: [harbor, night]
  weight: 3
---

## Shot 1.

@narrator: Yo.
"#;
    let art = compile(&input(SCENE_META)).expect("meta:-block scene compiles");
    let v = serde_json::to_value(&art).unwrap();
    let extra_block = v["meta"]["extra"]
        .as_object()
        .expect("meta.extra serializes as an object");
    let keys: Vec<&str> = extra_block.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec!["arc", "tags", "weight"],
        "keys sort lexicographically"
    );
    assert_eq!(extra_block["arc"], serde_json::json!("main"));
    assert_eq!(extra_block["tags"], serde_json::json!(["harbor", "night"]));
    assert_eq!(extra_block["weight"], serde_json::json!(3));

    // Quest kind gets the same treatment — the wire contract covers both.
    const QUEST_META: &str = r#"---
kind: quest
extra:
  arc: main
  region: harbor
---

<quest id="q1">
<objective id="o" done="true"/>
</quest>
"#;
    let qart = compile(&input(QUEST_META)).expect("meta:-block quest compiles");
    let qv = serde_json::to_value(&qart).unwrap();
    let qextra = qv["meta"]["extra"]
        .as_object()
        .expect("quest meta.extra object");
    let qkeys: Vec<&str> = qextra.keys().map(String::as_str).collect();
    assert_eq!(qkeys, vec!["arc", "region"]);

    // Byte-stability: a doc that authors no `extra:` omits the key entirely
    // (`skip_serializing_if = BTreeMap::is_empty`).
    let bare = compile(&input(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n",
    ))
    .unwrap();
    let bv = serde_json::to_value(&bare).unwrap();
    assert!(
        bv["meta"].get("meta").is_none(),
        "meta.meta must be skipped when the document authors no meta: block; got {}",
        bv["meta"]
    );
}

/// dsl 0.15.0 §2: the derived-key path (no `id:` authored) still emits all
/// four legacy fields resolved as 0.14.0 did, with `meta.id` = the derived
/// `{character}.{episodeId}` join stamped on top. Pins the wire contract
/// the loc/play consumers (Task 4) will read.
#[test]
fn derived_key_path_emits_meta_id_alongside_all_four_legacy_fields() {
    const LEGACY: &str = r#"---
kind: scene
character: kestrel
season: 1
episode: 3
---

## Shot 1.

@narrator: Legacy.
"#;
    let art = compile(&input(LEGACY)).expect("legacy doc compiles");
    let v = serde_json::to_value(&art).unwrap();
    assert_eq!(v["meta"]["id"], serde_json::json!("kestrel.s01ep03"));
    assert_eq!(v["meta"]["character"], serde_json::json!("kestrel"));
    assert_eq!(v["meta"]["season"], serde_json::json!(1));
    assert_eq!(v["meta"]["episode"], serde_json::json!(3));
    assert_eq!(v["meta"]["episodeId"], serde_json::json!("s01ep03"));
}

/// dsl 0.15.0 §2: the authored-`id:` path emits ONLY the legacy keys the
/// author WROTE (raw frontmatter is the source of truth — a project-level
/// `defaults:` fallback must not resurrect a dropped key). A scene that
/// authors only `id:` emits `id` alone from the legacy identity block.
#[test]
fn authored_id_only_scene_omits_all_four_legacy_fields() {
    const AUTH: &str = r#"---
kind: scene
id: anseo.s01ep01
---

## Shot 1.

@narrator: Hi.
"#;
    let art = compile(&input(AUTH)).expect("authored-only doc compiles");
    let v = serde_json::to_value(&art).unwrap();
    assert_eq!(v["meta"]["id"], serde_json::json!("anseo.s01ep01"));
    for k in ["character", "season", "episode", "episodeId"] {
        assert!(
            v["meta"].get(k).is_none(),
            "legacy key {k} must be skipped on the authored-id path; got {}",
            v["meta"]
        );
    }
}
