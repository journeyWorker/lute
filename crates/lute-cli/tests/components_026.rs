//! dsl 0.26.0 §3 components and §4 directive `when=`, end to end through the
//! CLI: a bridge call a component expands declares its result slot in the
//! host (T1-2), a string argument keeps its placeholders and `as=@who` shows
//! the cast name (T1-6), param defaults and own-result reads (§3.3), `@@who:`
//! (§3.2) and `when=` on directives (§4) in check, compile, play and test.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-comp026-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn count(out: &str, code: &str) -> usize {
    out.lines()
        .filter(|l| !l.starts_with(' ') && l.contains(&format!("[{code}]")))
        .count()
}

/// The compiled commands of `scene` (every `commands` array, flattened).
fn commands(dir: &Path, scene: &str) -> Vec<serde_json::Value> {
    let out = dir.join("out.json");
    let o = run(
        dir,
        &[
            "compile",
            scene,
            "--project",
            ".",
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", text(&o));
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
    let mut cmds = Vec::new();
    fn walk(v: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(serde_json::Value::Array(cs)) = m.get("commands") {
                    out.extend(cs.iter().cloned());
                }
                m.values().for_each(|v| walk(v, out));
            }
            serde_json::Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    walk(&v, &mut cmds);
    cmds
}

const PLUGIN: &str = "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n  directives: directives/\n  bridge: bridge/\n  state: state/\n  cast: cast/\n";

const DIRECTIVES: &str = r#"directives:
  - name: give
    attrs:
      - { name: item, required: true, type: string }
  - name: battle
    layer: bridge
    attrs:
      - { name: foe, required: true, type: string }
      - { name: resultKey, required: true, type: { slotId: { namespace: scene.battle } } }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    state:
      declares:
        - { scope: scene, path: [battle, { fromAttr: { name: resultKey, slotType: localId } }], shape: battleResult }
    effects:
      writes:
        - { scope: scene, path: [battle, { fromAttr: { name: resultKey } }, won], value: { fromBridgeResult: won } }
    bridge: { service: battle, operation: fight }
"#;

/// A project with a `talk` occasion, `::give` (passthrough) and `::battle`
/// (bridge) directives, a cast, a world schema and the given components.
fn project(tag: &str, cast: &str, schema: &str, components: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "plugins/p/plugin.yaml", PLUGIN);
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        "occasions: { talk: { select: first } }\n",
    );
    write(&dir, "plugins/p/directives/d.yaml", DIRECTIVES);
    write(
        &dir,
        "plugins/p/bridge/b.yaml",
        "bridgeCapabilities:\n  - service: battle\n    operation: fight\n    replay: recorded\n    \
         result:\n      - { name: won, type: bool }\n",
    );
    write(
        &dir,
        "plugins/p/state/s.yaml",
        "stateShapes:\n  - name: battleResult\n    fields:\n      - { name: won, type: bool, default: false }\n",
    );
    write(&dir, "plugins/p/cast/c.yaml", &format!("cast:\n{cast}"));
    write(&dir, "world.schema.yaml", schema);
    let mut paths = String::new();
    for (name, body) in components {
        let rel = format!("components/{name}.component.lute");
        write(&dir, &rel, body);
        paths.push_str(&format!("{rel}, "));
    }
    write(
        &dir,
        "lute.project.yaml",
        &format!(
            "pluginsDir: plugins/\ndefaultProfile: base\nprofiles:\n  base: {{ plugins: {{ p: true }} }}\n\
             defaults:\n  luteVersion: \"0.25.1\"\n  uses: [world.schema.yaml]\n  components: [{}]\n",
            paths.trim_end_matches(", ")
        ),
    );
    dir
}

fn scene(id: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\non: talk\n---\n\n## {id}\n\n{body}\n")
}

const SCHEMA: &str = r#"enums:
  emotion: [happy, sad]
  action:
    members: [show, hide]
    exits: [hide]
  anchor:
    members: [left, center, right]
    default: center
state:
  run.gate: { type: bool, default: false }
  run.coins: { type: number, default: 0 }
  run.maybe: { type: bool }
  run.num: { type: number }
  run.adaHere: { type: bool, default: false }
relations:
  met: { args: [person], tier: run }
entities:
  person: { members: [joey, ada] }
defs:
  numDef: { type: number, cel: "run.num" }
"#;

const CAST: &str = "  joey: { name: Youngster Joey, emotions: [happy] }\n  ada: { name: Lass Ada, emotions: [sad], present: \"run.adaHere\" }\n";

// ── §3.1 (T1-2): a component's bridge call declares its slot in the host ──

const FIGHT: &str =
    "---\ncomponent: fight\neffects: true\nparams:\n  key: string\n---\n\n## Fight\n\n\
    @narrator: Let us battle!\n::battle{foe=\"joey\" resultKey=\"k\"}\n";

#[test]
fn a_bridge_call_a_component_expands_is_typed_by_its_slot_in_the_host() {
    let dir = project("bridge", CAST, SCHEMA, &[("fight", FIGHT)]);
    write(
        &dir,
        "scenes/via.lute",
        &scene(
            "via",
            "::use{component=\"fight\" key=\"k\"}\n<match on=\"scene.battle.k.won\">\n\
             <when is=\"true\">\n@narrator: arm true\n</when>\n\
             <otherwise>\n@narrator: arm otherwise\n</otherwise>\n</match>",
        ),
    );
    let o = run(&dir, &["check-project", "."]);
    assert!(o.status.success(), "{}", text(&o));
    let out = dir.join("state.json");
    let o = run(
        &dir,
        &[
            "compile",
            "scenes/via.lute",
            "--project",
            ".",
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", text(&o));
    let art: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap();
    let won = art["state"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "scene.battle.k.won")
        .unwrap_or_else(|| panic!("{}", art["state"]));
    assert_eq!(won["type"], "bool");

    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: talk\n    bridges: { battle: [ { won: true } ] }\n",
    );
    let o = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(
        t.contains("(bridge answered: won=true)") && t.contains("@narrator: arm true"),
        "{t}"
    );

    write(
        &dir,
        "tests/via.test.yaml",
        "file: ../scenes/via.lute\nbridges: { battle: [ { won: true } ] }\n\
         expect:\n  transcriptContains: [\"arm true\"]\n  transcriptLacks: [\"arm otherwise\"]\n",
    );
    let o = run(&dir, &["test", "tests/via.test.yaml", "--project", "."]);
    assert!(o.status.success(), "{}", text(&o));
}

// ── §3.1 (T1-6): arguments render as direct text does ────────────────────

#[test]
fn a_string_argument_keeps_its_placeholders_and_as_at_who_shows_the_cast_name() {
    let dir = project(
        "args",
        CAST,
        SCHEMA,
        &[
            ("say", "---\ncomponent: say\nparams:\n  msg: string\n---\n\n## Say\n\n@narrator: {{@msg}}\n"),
            (
                "hello",
                "---\ncomponent: hello\nparams:\n  who: speaker\n---\n\n## Hello\n\n\
                 @narrator{as=\"{{@who}}\"}: A.\n@narrator{as=@who}: B.\n",
            ),
        ],
    );
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "::use{component=\"say\" msg=\"Hello, {{userName}}.\"}\n::use{component=\"hello\" who=\"joey\"}",
        ),
    );
    let cmds = commands(&dir, "scenes/a.lute");
    let line = |t: &str| {
        cmds.iter()
            .find(|c| c["kind"] == "line" && c["text"] == t)
            .unwrap_or_else(|| panic!("no line {t}: {cmds:#?}"))
            .clone()
    };
    assert_eq!(
        line("Hello, {{userName}}.")["placeholders"],
        serde_json::json!([{ "kind": "reserved", "token": "userName" }])
    );
    assert_eq!(line("A.")["as"], "Youngster Joey");
    assert_eq!(line("B.")["as"], "Youngster Joey");
}

// ── §3.3: param defaults and own results ────────────────────────────────

const DUEL: &str = "---\ncomponent: duel\neffects: true\nparams:\n  prize: { type: number, default: \"3\" }\n  cheer: { type: string, default: \"Well fought.\" }\n---\n\n\
    ## Duel\n\n::battle{foe=\"joey\" resultKey=\"fight\"}\n<match on=\"scene.battle.fight.won\">\n\
    <when is=\"true\">\n@narrator: {{@cheer}}\n::set{run.coins += @prize}\n</when>\n\
    <otherwise>\n@narrator: Lost.\n</otherwise>\n</match>\n";

#[test]
fn an_omitted_argument_takes_its_default_and_a_component_reads_its_own_result() {
    let dir = project("duel", CAST, SCHEMA, &[("duel", DUEL)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "::use{component=\"duel\"}\n::use{component=\"duel\" prize=\"10\"}",
        ),
    );
    let o = run(&dir, &["check-project", "."]);
    assert!(o.status.success(), "{}", text(&o));
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: talk\n    bridges: { battle: [ { won: true }, { won: false } ] }\n    \
         expect: { state: { run.coins: 3 } }\n",
    );
    let o = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    assert!(
        t.contains("@narrator: Well fought.") && t.contains("@narrator: Lost."),
        "{t}"
    );
}

#[test]
fn a_component_reading_a_slot_it_does_not_own_is_still_component_state() {
    let other = DUEL.replace(
        "<match on=\"scene.battle.fight.won\">",
        "<match on=\"scene.battle.other.won\">",
    );
    let dir = project("notown", CAST, SCHEMA, &[("duel", &other)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "::use{component=\"duel\"}"),
    );
    let t = text(&run(&dir, &["check-project", "."]));
    assert!(t.contains("[E-COMPONENT-STATE]"), "{t}");
}

#[test]
fn a_default_naming_no_def_in_the_host_is_component_arg() {
    let bad = DUEL.replace("default: \"3\"", "default: \"@nowhere\"");
    let dir = project("baddef", CAST, SCHEMA, &[("duel", &bad)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "::use{component=\"duel\"}"),
    );
    let t = text(&run(&dir, &["check-project", "."]));
    assert!(
        t.contains("[E-COMPONENT-ARG]") && t.contains("@nowhere"),
        "{t}"
    );
}

// ── §3.2: `@@who:` ───────────────────────────────────────────────────────

const TALK: &str =
    "---\ncomponent: talk\nparams:\n  who: speaker\n  line: string\n---\n\n## Talk\n\n\
    @@who: {{@line}}\n@@who{emotion=\"happy\"}: Grr.\n@@who: Bye.\n";

#[test]
fn at_at_who_speaks_as_the_member_each_use_names() {
    let dir = project("who", CAST, SCHEMA, &[("talk", TALK)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "::use{component=\"talk\" who=\"joey\" line=\"Hi.\"}"),
    );
    let o = run(&dir, &["check-project", "."]);
    assert!(o.status.success(), "{}", text(&o));
    let speakers: Vec<String> = commands(&dir, "scenes/a.lute")
        .iter()
        .filter(|c| c["kind"] == "line")
        .map(|c| {
            format!(
                "{}: {}",
                c["speaker"].as_str().unwrap(),
                c["text"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(speakers, ["joey: Hi.", "joey: Grr.", "joey: Bye."]);
}

#[test]
fn cast_checks_judge_each_use_by_its_member_once() {
    let dir = project("whocast", CAST, SCHEMA, &[("talk", TALK)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "::use{component=\"talk\" who=\"ada\" line=\"Yo.\"}\n\
             ::use{component=\"talk\" who=\"ada\" line=\"Yo.\" when=\"run.adaHere\"}",
        ),
    );
    let t = text(&run(&dir, &["check", "scenes/a.lute", "--project", "."]));
    // `happy` is not one of ada's emotions, reported at each use once.
    assert_eq!(count(&t, "E-BAD-ENUM"), 2, "{t}");
    // Ada may be absent at the unguarded use only — one warning for its
    // three lines; the guarded use implies her `present:`.
    assert_eq!(count(&t, "W-CAST-ABSENT"), 1, "{t}");
    assert!(
        t.contains("a.lute:9:1:") && t.contains("W-CAST-ABSENT"),
        "{t}"
    );
}

#[test]
fn at_at_who_after_the_member_left_the_stage_is_stage_absent_at_the_use() {
    let dir = project("whostage", CAST, SCHEMA, &[("talk", TALK)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "::auto{character=\"joey\" action=\"show\"}\n::auto{character=\"joey\" action=\"hide\"}\n\
             ::use{component=\"talk\" who=\"joey\" line=\"Hi.\"}",
        ),
    );
    let t = text(&run(&dir, &["check", "scenes/a.lute", "--project", "."]));
    assert!(
        t.lines()
            .any(|l| l.contains("[W-STAGE-ABSENT]") && l.contains("component `talk`")),
        "{t}"
    );
}

#[test]
fn at_at_for_a_param_that_is_no_speaker_is_component_arg() {
    let bad = TALK.replace("@@who: Bye.", "@@line: Bye.\n@@nobody: Hm.");
    let dir = project("whobad", CAST, SCHEMA, &[("talk", &bad)]);
    let t = text(&run(
        &dir,
        &["check", "components/talk.component.lute", "--project", "."],
    ));
    assert!(t.contains("`line` is not a `speaker` param"), "{t}");
    assert!(t.contains("this component has no param `nobody`"), "{t}");
    write(&dir, "scenes/a.lute", &scene("a", "@@joey: Hi."));
    let t = text(&run(&dir, &["check", "scenes/a.lute", "--project", "."]));
    assert!(
        t.contains("[E-COMPONENT-ARG]") && t.contains("no component"),
        "{t}"
    );
}

// ── §4: `when=` on directives ───────────────────────────────────────────

const EFF: &str = "---\ncomponent: eff\neffects: true\nparams:\n  n: number\n---\n\n## Eff\n\n\
    @narrator: Effects {{@n}}.\n::set{run.coins += @n}\n::set{run.coins += 0}\n";

const GUARDED: &str = "::give{item=\"potion\" when=\"run.gate\"}\n\
    ::assert{met(joey) when=\"run.gate\"}\n::retract{met(ada) when=\"run.gate\"}\n\
    ::use{component=\"eff\" n=\"5\" when=\"run.gate\"}\n\
    ::battle{foe=\"joey\" resultKey=\"k\" when=\"run.gate\"}\n@narrator: After.\n\
    ::set{run.gate = true}\n::give{item=\"elixir\" when=\"run.gate\"}\n\
    ::assert{met(ada) when=\"run.gate\"}\n::use{component=\"eff\" n=\"7\" when=\"run.gate\"}\n\
    @narrator: End.";

#[test]
fn a_guarded_directive_compiles_to_a_one_arm_match() {
    let dir = project("desugar", CAST, SCHEMA, &[("eff", EFF)]);
    write(&dir, "scenes/a.lute", &scene("a", GUARDED));
    let cmds = commands(&dir, "scenes/a.lute");
    let first = &cmds[0];
    assert_eq!(first["kind"], "match", "{cmds:#?}");
    assert_eq!(first["subject"], "run.gate");
    assert_eq!(first["arms"].as_array().unwrap().len(), 1);
    let target = first["arms"][0]["target"].as_str().unwrap();
    let leaf = cmds.iter().find(|c| c["addr"] == target).unwrap();
    assert_eq!(leaf["kind"], "plugin");
    assert_eq!(leaf["tag"], "give");
}

#[test]
fn play_and_test_agree_on_guarded_directives_and_play_names_each_skip() {
    let dir = project("parity", CAST, SCHEMA, &[("eff", EFF)]);
    write(&dir, "scenes/a.lute", &scene("a", GUARDED));
    let o = run(&dir, &["check-project", "."]);
    assert!(o.status.success(), "{}", text(&o));
    write(
        &dir,
        "p.play.yaml",
        "steps:\n  - occasion: talk\n    expect:\n      state: { run.coins: 7 }\n      \
         facts: [\"met(ada)\"]\n      notFacts: [\"met(joey)\"]\n",
    );
    let o = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    let t = text(&o);
    assert!(o.status.success(), "{t}");
    for skip in [
        "  skip ::give{item=\"potion\"} — when: false",
        "  skip assert met(joey) — when: false",
        "  skip retract met(ada) — when: false",
        "  skip ::use{component=\"eff\" n=\"5\"} — when: false",
        "  skip ::battle{foe=\"joey\" resultKey=\"k\"} — when: false",
    ] {
        assert!(t.lines().any(|l| l == skip), "missing `{skip}`:\n{t}");
    }
    assert!(
        t.contains("::give{item=\"elixir\"}") && t.contains("@narrator: Effects 7."),
        "{t}"
    );
    assert!(!t.contains("Effects 5.") && !t.contains("match ->"), "{t}");

    write(
        &dir,
        "tests/a.test.yaml",
        "file: ../scenes/a.lute\nexpect:\n  exit: complete\n  state: { run.coins: 7 }\n  \
         facts: [\"met(ada)\"]\n  notFacts: [\"met(joey)\"]\n  \
         transcriptContains: [\"Effects 7.\"]\n  transcriptLacks: [\"Effects 5.\"]\n",
    );
    let o = run(&dir, &["test", "tests/a.test.yaml", "--project", "."]);
    assert!(o.status.success(), "{}", text(&o));
}

#[test]
fn when_on_a_builtin_lowered_directive_or_a_timeline_clip_is_refused() {
    let dir = project("refuse", CAST, SCHEMA, &[("eff", EFF)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "::auto{character=\"joey\" action=\"show\" when=\"run.gate\"}\n\
             <timeline id=\"t\">\n<track id=\"a\">\n::give{item=\"x\" when=\"run.gate\"}\n</track>\n</timeline>",
        ),
    );
    let t = text(&run(&dir, &["check", "scenes/a.lute", "--project", "."]));
    assert!(
        t.contains("[E-UNKNOWN-ATTR]") && t.contains("`::auto` cannot take `when=`"),
        "{t}"
    );
    assert!(
        t.contains("a <track> directive cannot carry `when=`"),
        "{t}"
    );
}

#[test]
fn a_guarded_use_reports_its_guard_once_and_reads_its_arguments_under_it() {
    let dir = project("once", CAST, SCHEMA, &[("eff", EFF)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "::use{component=\"eff\" n=\"2\" when=\"run.maybe\"}\n\
             ::use{component=\"eff\" n=\"3\" when=\"run.x +\"}\n\
             ::use{component=\"eff\" n=@numDef when=\"isSet(run.num)\"}",
        ),
    );
    let t = text(&run(&dir, &["check", "scenes/a.lute", "--project", "."]));
    assert_eq!(count(&t, "E-MAYBE-UNSET"), 1, "{t}");
    assert!(t.contains("`run.maybe`") && !t.contains("`run.num`"), "{t}");
    assert_eq!(count(&t, "E-CEL-PARSE"), 1, "{t}");
    let t = text(&run(&dir, &["check-project", "."]));
    assert!(count(&t, "E-STATE-MAYBE-UNAVAILABLE") <= 1, "{t}");
}

#[test]
fn a_guarded_assert_is_no_guaranteed_fact() {
    let cast = "  joey: { name: Youngster Joey, present: \"holds(met(joey))\" }\n";
    let dir = project("must", cast, SCHEMA, &[]);
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "::assert{met(joey) when=\"run.gate\"}\n@joey: Hi."),
    );
    write(
        &dir,
        "scenes/b.lute",
        &scene("b", "::assert{met(joey)}\n@joey: Hi.").replace("id: b\n", "id: b\npriority: 1\n"),
    );
    let t = text(&run(&dir, &["check-project", "."]));
    let absent: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[W-CAST-ABSENT]"))
        .collect();
    assert_eq!(absent.len(), 1, "{t}");
    assert!(absent[0].contains("a.lute"), "{t}");
}

#[test]
fn a_member_speaking_through_at_at_who_counts_as_spoken_at_the_use_for_display_name_dup() {
    let cast = format!("{CAST}  joe: {{ name: Youngster Joey }}\n");
    let dir = project("whodup", &cast, SCHEMA, &[("talk", TALK)]);
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "::use{component=\"talk\" who=\"joey\" line=\"Hi.\"}"),
    );
    write(
        &dir,
        "scenes/b.lute",
        &scene("b", "::use{component=\"talk\" who=\"joe\" line=\"Yo.\"}")
            .replace("id: b\n", "id: b\npriority: 1\n"),
    );
    for args in [&["check-project", "."][..], &["lint", "."][..]] {
        let t = text(&run(&dir, args));
        let dup: Vec<&str> = t
            .lines()
            .filter(|l| l.contains("[W-DISPLAY-NAME-DUP]"))
            .collect();
        assert!(!dup.is_empty(), "{args:?}: {t}");
        let top = dup[0];
        assert!(!top.contains("never spoken"), "{args:?}: {t}");
        assert!(
            top.contains("scenes/a.lute:9") && top.contains("scenes/b.lute:10"),
            "{args:?}: {t}"
        );
        assert!(
            top.contains("b.lute:10:1"),
            "anchored at the second `::use`: {args:?}: {t}"
        );
    }
}
