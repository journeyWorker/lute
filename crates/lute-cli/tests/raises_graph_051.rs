//! 0.51.0 §4.4–§4.7 (#170): declared raises in the scenario graph, its
//! analyses and the artifact — `[raises]` may-edges, raise anchors,
//! `W-OCCASION-NEVER-RAISED`, the clock analyses, the node envelope's writer
//! provenance, the IR `raises` and the `plugin` / `grant` records.
//!
//! On 0.50.0 `effects.raises` and `raisedBy` are unknown keys: the plugin
//! fails to load (`E-PLUGIN-KEY`, then `E-PLUGIN-MISSING-ACTIVE`), so no
//! project below checks clean, no `[raises]` edge is drawn (`lute scenario`
//! lists `intro` and `chapter2` as one layer of unordered entry points) and
//! no record carries `raises`.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  \
                      occasions: occasions/\n  rewardKinds: rewardkinds/\n";

/// `cutscene` is raised only by declared raises; `visit` by the host at any
/// time; `begin` starts a play.
const OCCASIONS: &str = "occasions:\n  \
     cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }\n  \
     visit: { target: { prefix: scene, entity: cutsceneId } }\n  \
     begin: {}\n";

const DIRECTIVES: &str = concat!(
    "directives:\n",
    "  - name: playScene\n",
    "    attrs:\n",
    "      - { name: scene, type: { entity: cutsceneId }, default: chapter2 }\n",
    "    effects: { raises: [{ occasion: cutscene, target: \"@scene\" }] }\n",
    "  - name: visitScene\n",
    "    attrs:\n",
    "      - { name: scene, required: true, type: { entity: cutsceneId } }\n",
    "    effects: { raises: [{ occasion: visit, target: \"@scene\" }] }\n",
);

const KINDS: &str = "rewardKinds:\n  \
     CUTSCENE: { target: { entity: cutsceneId, required: true }, effects: { raises: [{ occasion: \
     cutscene, target: \"@target\" }] } }\n  \
     MIX: { target: { entity: cutsceneId, required: true }, effects: { asserts: [\"seen(@target)\"], \
     accepts: [second], raises: [{ occasion: cutscene, target: \"@target\" }] } }\n";

const SCHEMA: &str = "entities:\n  cutsceneId: { members: [intro, chapter2, chapter3, epilogue] }\n  \
                      person: { members: [mira] }\nrelations:\n  met: { args: [person], tier: run }\n  \
                      seen: { args: [cutsceneId], tier: run }\nstate:\n  run.x: { type: int, \
                      default: 0 }\n  run.y: { type: int }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The plugin above plus `files` (a later file of the same path wins, so a
/// test may replace the occasions, directives or schema).
fn project(tag: &str, files: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.stage/occasions/o.yaml", OCCASIONS);
    write(&dir, "plugins/acme.stage/directives/d.yaml", DIRECTIVES);
    write(&dir, "plugins/acme.stage/rewardkinds/r.yaml", KINDS);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    for (rel, text) in files {
        write(&dir, rel, text);
    }
    dir
}

/// A scene document: `id`, extra frontmatter lines `head`, a body.
fn scene(id: &str, head: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{head}---\n\n## Shot 1.\n\n{body}\n")
}

/// `chapter2`, the answerer of `cutscene` for `scene.chapter2`.
fn chapter2(head: &str, body: &str) -> String {
    scene(
        "chapter2",
        &format!("on: cutscene\ntarget: scene.chapter2\n{head}"),
        body,
    )
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    text(&lute(dir, &["check", "--project", "."]))
        .lines()
        .filter_map(|l| {
            let l = l.strip_prefix("./")?;
            let (at, rest) = l.split_once(": ")?;
            let (_, d) = rest.split_once(' ')?;
            d.starts_with('[').then(|| format!("{at} {d}"))
        })
        .collect()
}

/// The codes `check` reports, in output order.
fn codes(dir: &Path) -> Vec<String> {
    check(dir)
        .iter()
        .filter_map(|l| {
            let (_, rest) = l.split_once(" [")?;
            Some(rest.split_once(']')?.0.to_string())
        })
        .collect()
}

fn scenario(dir: &Path, args: &[&str]) -> String {
    let mut all = vec!["scenario", "--project", "."];
    all.extend_from_slice(args);
    let out = lute(dir, &all);
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

/// `lute play` over `steps` (a YAML list body), human transcript.
fn play(dir: &Path, steps: &str) -> String {
    write(dir, "p.play.yaml", &format!("steps:\n{steps}"));
    let out = lute(dir, &["play", "--project", ".", "--script", "p.play.yaml"]);
    assert!(out.status.success(), "{}", text(&out));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `lute play --json` over `steps`: every presented command and quest
/// record of every step.
fn play_records(dir: &Path, steps: &str) -> Vec<serde_json::Value> {
    write(dir, "p.play.yaml", &format!("steps:\n{steps}"));
    let out = lute(
        dir,
        &[
            "play",
            "--project",
            ".",
            "--script",
            "p.play.yaml",
            "--json",
        ],
    );
    assert!(out.status.success(), "{}", text(&out));
    let v = json(&String::from_utf8_lossy(&out.stdout));
    let mut records = Vec::new();
    for step in v["steps"].as_array().into_iter().flatten() {
        records.extend(
            step["presented"]["commands"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        for q in step["quests"].as_array().into_iter().flatten() {
            records.extend(q["commands"].as_array().into_iter().flatten().cloned());
        }
    }
    records
}

/// The compiled IR of `doc` (`lute compile <doc>`).
fn compile(dir: &Path, doc: &str) -> serde_json::Value {
    let out = lute(dir, &["compile", doc]);
    assert!(out.status.success(), "{}", text(&out));
    json(&String::from_utf8_lossy(&out.stdout))
}

fn commands_of<'a>(ir: &'a serde_json::Value, kind: &str) -> Vec<&'a serde_json::Value> {
    ir["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["kind"] == kind)
        .collect()
}

const BEGIN: &str = "  - { occasion: begin }\n";

/// The #170 repro: `intro` raises `cutscene` for `scene.chapter2`, which
/// only declared raises raise — `chapter2` is no longer an unordered entry
/// point but layered after `intro`, in text, `--json` and dot.
#[test]
fn a_declared_raise_orders_its_answerer_after_the_raiser() {
    let dir = project(
        "raises-repro",
        &[
            (
                "scenes/intro.lute",
                &scene(
                    "intro",
                    "on: begin\n",
                    "@narrator: Intro.\n\n::playScene{scene=\"chapter2\"}",
                ),
            ),
            ("scenes/chapter2.lute", &chapter2("", "@narrator: Two.")),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    let t = scenario(&dir, &[]);
    assert!(
        t.contains("    layer 0: scene(intro)\n    layer 1: scene(chapter2)\n"),
        "{t}"
    );
    assert!(
        t.contains("    scene(intro) -> scene(chapter2) [raises]\n"),
        "{t}"
    );
    let j = json(&scenario(&dir, &["--json"]));
    let edges = j.to_string();
    assert!(
        edges.contains(r#"{"from":"scene(intro)","kinds":["raises"],"to":"scene(chapter2)"}"#),
        "{edges}"
    );
    let reach = json(&scenario(
        &dir,
        &["reach", "--node", "scene:chapter2", "--json"],
    ));
    assert_eq!(
        reach["anchors"],
        json(r#"[{"from":["scene(intro)"],"kind":"raises"}]"#),
        "{reach}"
    );
    let dot = scenario(&dir, &["--format", "dot"]);
    assert!(
        dot.contains("  \"scene(intro)\" -> \"scene(chapter2)\" [label=\"raises\"];\n"),
        "{dot}"
    );
    // The node envelope names the raise.
    let env = scenario(&dir, &["envelope", "--node", "scene:chapter2"]);
    assert!(
        env.contains("  raised by `::playScene{scene=\"chapter2\"}` in scene `intro`\n"),
        "{env}"
    );
}

/// Review051b F6: a call that omits `scene=` raises for its declared
/// `default: chapter2` — the edge, the IR record and the runtime record.
#[test]
fn a_defaulted_target_draws_the_edge_and_is_recorded() {
    let dir = project(
        "raises-default",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro.\n\n::playScene"),
            ),
            ("scenes/chapter2.lute", &chapter2("", "@narrator: Two.")),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    assert!(scenario(&dir, &[]).contains("    scene(intro) -> scene(chapter2) [raises]\n"));
    let ir = compile(&dir, "scenes/intro.lute");
    let plugins = commands_of(&ir, "plugin");
    assert_eq!(
        plugins[0]["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter2"}]"#),
        "{ir}"
    );
    let records = play_records(&dir, BEGIN);
    let plugin = records
        .iter()
        .find(|r| r["kind"] == "plugin")
        .expect("plugin record");
    assert_eq!(
        plugin["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter2"}]"#)
    );
    let t = play(&dir, BEGIN);
    assert!(t.contains("  → raises cutscene scene.chapter2\n"), "{t}");
}

/// A call without raises compiles and records exactly as in 0.50: no
/// `raises` key anywhere.
#[test]
fn a_call_without_raises_records_no_raises() {
    let dir = project(
        "raises-none",
        &[(
            "scenes/intro.lute",
            &scene(
                "intro",
                "on: begin\n",
                "@narrator: Intro.\n\n::visitScene{scene=\"chapter2\"}",
            ),
        )],
    );
    write(
        &dir,
        "plugins/acme.stage/directives/d.yaml",
        "directives:\n  - name: visitScene\n    attrs:\n      - { name: scene, required: true, \
         type: { entity: cutsceneId } }\n",
    );
    let ir = compile(&dir, "scenes/intro.lute");
    assert!(!ir.to_string().contains("\"raises\""), "{ir}");
    let records = play_records(&dir, BEGIN);
    assert!(
        !serde_json::Value::Array(records.clone())
            .to_string()
            .contains("\"raises\""),
        "{records:?}"
    );
}

/// A reward-kind raise anchors from its quest; a raises-only reward is one
/// grant site (`W-OCCASION-NEVER-RAISED` silenced, one raise on the grant
/// record) and needs no `lute.quest.rewardEffects/1`.
#[test]
fn a_reward_kind_raise_anchors_its_answerer_from_the_quest() {
    let dir = project(
        "raises-reward",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro."),
            ),
            (
                "scenes/chapter3.lute",
                &scene(
                    "chapter3",
                    "on: cutscene\ntarget: scene.chapter3\n",
                    "@narrator: Three.",
                ),
            ),
            (
                "quests/first.lute",
                "---\nkind: quest\nid: first\n---\n\n<quest id=\"first\" title=\"First\" \
                 start=\"visited('intro')\">\n  <objective id=\"a\" title=\"A\" done=\"true\"/>\n  \
                 <reward kind=\"CUTSCENE\" target=\"chapter3\"/>\n</quest>\n",
            ),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    assert!(
        scenario(&dir, &[]).contains("    quest(first) -> scene(chapter3) [raises]\n"),
        "{}",
        scenario(&dir, &[])
    );
    let records = play_records(&dir, BEGIN);
    let grants: Vec<_> = records.iter().filter(|r| r["kind"] == "grant").collect();
    assert_eq!(grants.len(), 1, "{records:?}");
    assert_eq!(
        grants[0]["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter3"}]"#)
    );
    assert!(
        play(&dir, BEGIN).contains("CUTSCENE 1 -> chapter3  → raises cutscene scene.chapter3\n")
    );
    let ir = compile(&dir, "quests/first.lute");
    let quest = commands_of(&ir, "quest")[0];
    assert_eq!(
        quest["rewards"][0]["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter3"}]"#)
    );
    assert!(
        !ir["requiredSemantics"]
            .to_string()
            .contains("lute.quest.rewardEffects/1"),
        "{}",
        ir["requiredSemantics"]
    );
}

/// Review051b F2: a reward whose kind asserts, accepts and raises is one
/// raise site — one `E-RAISES-TARGET` for an unanswered target, one raise
/// on its grant record — and 0.50's assert and accept still apply.
#[test]
fn a_mixed_reward_is_one_raise_site() {
    let quest = |target: &str| {
        format!(
            "---\nkind: quest\nid: first\n---\n\n<quest id=\"first\" title=\"First\" \
             start=\"visited('intro')\">\n  <objective id=\"a\" title=\"A\" done=\"true\"/>\n  \
             <reward kind=\"MIX\" target=\"{target}\"/>\n</quest>\n\n\
             <quest id=\"second\" title=\"Second\" tier=\"run\">\n  <objective id=\"b\" \
             title=\"B\" done=\"run.x == 1\"/>\n</quest>\n"
        )
    };
    let dir = project(
        "raises-mixed",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro."),
            ),
            (
                "scenes/chapter2.lute",
                &chapter2(
                    "",
                    "@narrator: Two.\n@narrator{when=\"holds('seen', ['chapter2'])\"}: Seen.",
                ),
            ),
            ("quests/first.lute", &quest("chapter2")),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    let records = play_records(&dir, BEGIN);
    let grant = records
        .iter()
        .find(|r| r["kind"] == "grant")
        .expect("grant");
    assert_eq!(
        grant["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter2"}]"#)
    );
    let all = serde_json::Value::Array(records.clone()).to_string();
    assert!(all.contains(r#""kind":"assert""#), "{all}");
    assert!(all.contains(r#""kind":"accept""#), "{all}");
    let t = play(&dir, BEGIN);
    assert_eq!(t.matches("→ raises").count(), 1, "{t}");
    write(&dir, "quests/first.lute", &quest("epilogue"));
    let raises_target: Vec<_> = check(&dir)
        .into_iter()
        .filter(|l| l.contains("[E-RAISES-TARGET]"))
        .collect();
    assert_eq!(raises_target.len(), 1, "{raises_target:?}");
}

/// S5 review: two scenes raising each other, one of them also raised by a
/// reachable entry. The raise edge that would close the cycle is not drawn,
/// so `chapter3` is ordered before its only raiser `chapter2`; the
/// reachability pass still reaches its fixed point — `intro` →
/// `chapter2` → `chapter3` is a route — instead of stopping after one pass
/// with `chapter3` `Unknown`.
#[test]
fn a_raise_cycle_entered_from_a_reachable_raiser_is_reachable() {
    let dir = project(
        "raises-cycle-entered",
        &[
            (
                "scenes/intro.lute",
                &scene(
                    "intro",
                    "on: begin\n",
                    "@narrator: Intro.\n\n::playScene{scene=\"chapter2\"}",
                ),
            ),
            (
                "scenes/chapter2.lute",
                &chapter2("", "@narrator: Two.\n\n::playScene{scene=\"chapter3\"}"),
            ),
            (
                "scenes/chapter3.lute",
                &scene(
                    "chapter3",
                    "on: cutscene\ntarget: scene.chapter3\n",
                    "@narrator: Three.\n\n::playScene{scene=\"chapter2\"}",
                ),
            ),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    let t = scenario(&dir, &[]);
    assert!(
        t.contains("    scene(chapter3) -> scene(chapter2) [raises]\n"),
        "{t}"
    );
    assert!(!t.contains("    scene(chapter2) -> scene(chapter3)"), "{t}");
    for node in ["scene:chapter2", "scene:chapter3"] {
        let reach = scenario(&dir, &["reach", "--node", node]);
        assert!(reach.contains("  verdict: Reachable"), "{reach}");
    }
}

/// §4.7 reachability: an anchor reads `Unknown` when its only raiser is
/// unreachable; a self-raise and a two-scene raise cycle close no
/// `E-CONN-CYCLE`; a written `after:` stays the declared route; a raise in
/// a lore entry anchors nothing.
#[test]
fn raise_anchors_follow_the_reachability_rules() {
    // The raiser can never play: its `after:` waits on a quest that never
    // starts.
    let dir = project(
        "raises-unknown",
        &[
            (
                "scenes/intro.lute",
                &scene(
                    "intro",
                    "on: begin\nafter: \"completed('ghost')\"\n",
                    "@narrator: Intro.\n\n::playScene",
                ),
            ),
            ("scenes/chapter2.lute", &chapter2("", "@narrator: Two.")),
            (
                "quests/ghost.lute",
                "---\nkind: quest\nid: ghost\n---\n\n<quest id=\"ghost\" title=\"Ghost\" \
                 start=\"false\">\n  <objective id=\"o\" title=\"O\" done=\"true\"/>\n</quest>\n",
            ),
        ],
    );
    let lines = check(&dir);
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("scenes/intro.lute") && l.contains("[E-CONN-UNREACHABLE]")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("scenes/chapter2.lute")),
        "{lines:?}"
    );
    let reach = scenario(&dir, &["reach", "--node", "scene:chapter2"]);
    assert!(reach.contains("  verdict: Unknown"), "{reach}");
    assert!(reach.contains("    [raises] scene(intro)\n"), "{reach}");

    // A scene that raises its own occasion again, and two scenes raising
    // each other's.
    let dir = project(
        "raises-cycle",
        &[
            (
                "scenes/chapter2.lute",
                &chapter2(
                    "",
                    "@narrator: Two.\n\n::playScene\n::playScene{scene=\"chapter3\"}",
                ),
            ),
            (
                "scenes/chapter3.lute",
                &scene(
                    "chapter3",
                    "on: cutscene\ntarget: scene.chapter3\n",
                    "@narrator: Three.\n\n::playScene",
                ),
            ),
        ],
    );
    assert!(
        !codes(&dir).contains(&"E-CONN-CYCLE".to_string()),
        "{:?}",
        check(&dir)
    );
    let t = scenario(&dir, &[]);
    assert_eq!(t.matches("[raises]").count(), 1, "{t}");

    // A written `after:` keeps its route: no raise anchor, no `[raises]`.
    let dir = project(
        "raises-after",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro.\n\n::playScene"),
            ),
            (
                "scenes/chapter2.lute",
                &chapter2("after: \"visited('intro')\"\n", "@narrator: Two."),
            ),
        ],
    );
    let t = scenario(&dir, &[]);
    assert!(
        t.contains("    scene(intro) -> scene(chapter2) [visited]\n"),
        "{t}"
    );
    assert!(!t.contains("[raises]"), "{t}");

    // Only a lore entry raises it: a raise (no warning), but no anchor.
    let dir = project(
        "raises-lore",
        &[
            ("scenes/chapter2.lute", &chapter2("", "@narrator: Two.")),
            (
                "lore/codex.lute",
                "---\nkind: lore\nid: codex\n---\n\n<entry id=\"e1\" title=\"One\">\n  \
                 ::playScene\n</entry>\n",
            ),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    let reach = scenario(&dir, &["reach", "--node", "scene:chapter2"]);
    assert!(!reach.contains("anchored"), "{reach}");
    assert!(!scenario(&dir, &[]).contains("[raises]"));
}

/// §4.5 soundness: an unconditional write and assert in a raiser are not
/// guaranteed in its answerer — the answerer reads them as an entry scene
/// does — while the same scene with `after:` on the raiser has both.
#[test]
fn a_raisers_writes_and_facts_are_not_guaranteed_in_the_answerer() {
    let answerer = |head: &str| {
        chapter2(
            head,
            "@narrator{when=\"holds('met', ['mira'])\"}: Hi {{run.y}}.\n\n<match>\n  \
             <when test=\"!holds('met', ['mira'])\">\n    @narrator: Not met.\n  </when>\n  \
             <otherwise>\n    @narrator: Met.\n  </otherwise>\n</match>",
        )
    };
    let intro = scene(
        "intro",
        "on: begin\n",
        "@narrator: Intro.\n\n::set{run.y = 1}\n::assert{met(mira)}\n::playScene",
    );
    let dir = project(
        "raises-sound",
        &[
            ("scenes/intro.lute", &intro),
            ("scenes/chapter2.lute", &answerer("")),
        ],
    );
    assert_eq!(
        codes(&dir),
        ["E-STATE-MAYBE-UNAVAILABLE"],
        "{:?}",
        check(&dir)
    );
    // The control: with `after:` both are guaranteed there.
    write(
        &dir,
        "scenes/chapter2.lute",
        &answerer("after: \"visited('intro')\"\n"),
    );
    assert_eq!(
        codes(&dir),
        ["W-FACT-GUARANTEED", "E-ARM-DEAD"],
        "{:?}",
        check(&dir)
    );

    // A reward-kind raiser: the quest's completion writes are not
    // guaranteed either; with `after: completed('first')` they are.
    let quest = "---\nkind: quest\nid: first\n---\n\n<quest id=\"first\" title=\"First\" \
                 start=\"true\" follows=\"visited('intro')\">\n  <objective id=\"a\" title=\"A\" \
                 done=\"true\"/>\n  <reward kind=\"CUTSCENE\" target=\"chapter2\"/>\n  \
                 <on event=\"questComplete\">\n    ::set{run.y = 1}\n    ::assert{met(mira)}\n  \
                 </on>\n</quest>\n";
    let dir = project(
        "raises-sound-reward",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro."),
            ),
            ("quests/first.lute", quest),
            ("scenes/chapter2.lute", &answerer("")),
        ],
    );
    assert!(scenario(&dir, &[]).contains("    quest(first) -> scene(chapter2) [raises]\n"));
    assert_eq!(
        codes(&dir),
        ["E-STATE-MAYBE-UNAVAILABLE"],
        "{:?}",
        check(&dir)
    );
    write(
        &dir,
        "scenes/chapter2.lute",
        &answerer("after: \"completed('first')\"\n"),
    );
    assert!(codes(&dir).is_empty(), "{:?}", check(&dir));
}

/// Review051b F4: scene C, raised by A and by B (each writing `run.x`),
/// lists neither as an upstream writer — one of them ran — beside the
/// schema default; with `after: visited('a')` A is upstream again.
#[test]
fn alternative_raisers_are_not_provably_before_the_answerer() {
    let raiser = |id: &str| {
        (
            format!("scenes/{id}.lute"),
            scene(
                id,
                "on: begin\n",
                &format!("@narrator: {id}.\n\n::set{{run.x = 1}}\n::playScene"),
            ),
        )
    };
    let (a, b) = (raiser("a"), raiser("b"));
    let dir = project(
        "raises-provenance",
        &[
            (&a.0, &a.1),
            (&b.0, &b.1),
            (
                "scenes/chapter2.lute",
                &chapter2("", "@narrator: C {{run.x}}."),
            ),
        ],
    );
    let env = scenario(&dir, &["envelope", "--node", "scene:chapter2"]);
    assert!(
        env.contains(
            "    - run.x   written by: (nothing on a declared route reaching here — schema \
             default only); also written, but not provably before this node: scene(a), scene(b)\n"
        ),
        "{env}"
    );
    assert!(
        env.contains("  raised by `::playScene` in scene `a`\n"),
        "{env}"
    );
    assert!(
        env.contains("  raised by `::playScene` in scene `b`\n"),
        "{env}"
    );
    write(
        &dir,
        "scenes/chapter2.lute",
        &chapter2("after: \"visited('a')\"\n", "@narrator: C {{run.x}}."),
    );
    let env = scenario(&dir, &["envelope", "--node", "scene:chapter2"]);
    assert!(
        env.contains(
            "    - run.x   written by: scene(a); also written, but not provably before this \
             node: scene(b)\n"
        ),
        "{env}"
    );
}

/// `W-OCCASION-NEVER-RAISED`: an answerer of a `raisedBy: declared`
/// occasion no declared raise reaches — not one a raise for its target, an
/// `occasion.target` raise or (targetless) any raise reaches, nor one on a
/// default occasion or on an occasion the clock raises.
#[test]
fn an_answerer_no_declared_raise_reaches_is_warned() {
    let three = scene(
        "chapter3",
        "on: cutscene\ntarget: scene.chapter3\n",
        "@narrator: Three.",
    );
    let dir = project(
        "never-raised",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro.\n\n::playScene"),
            ),
            ("scenes/chapter2.lute", &chapter2("", "@narrator: Two.")),
            ("scenes/chapter3.lute", &three),
            (
                "scenes/visit.lute",
                &scene(
                    "visit3",
                    "on: visit\ntarget: scene.chapter3\n",
                    "@narrator: V.",
                ),
            ),
        ],
    );
    assert_eq!(
        check(&dir),
        [
            "scenes/chapter3.lute:4:1 [W-OCCASION-NEVER-RAISED] scene `chapter3` answers \
             `cutscene` for `scene.chapter3`, which `acme.stage` declares `raisedBy: declared`, \
             but no `effects.raises` in the project raises it — add a `raises` entry to the \
             directive or reward kind that leads here, or drop `raisedBy: declared` if the host \
             raises `cutscene` on its own"
        ]
    );
    // A raise for its target silences it.
    write(
        &dir,
        "scenes/intro.lute",
        &scene(
            "intro",
            "on: begin\n",
            "@narrator: Intro.\n\n::playScene\n::playScene{scene=\"chapter3\"}",
        ),
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    // So does an `occasion.target` raise in a kind beat (it stands for every
    // answerer); a targetless answerer is reached by any raise.
    write(
        &dir,
        "scenes/intro.lute",
        &scene("intro", "on: begin\n", "@narrator: Intro."),
    );
    write(
        &dir,
        "scenes/visit.lute",
        &scene(
            "visit3",
            "on: visit\ntarget: \"kind:cutsceneId\"\n",
            "@narrator: V.\n\n::playScene{scene=occasion.target}",
        ),
    );
    write(
        &dir,
        "scenes/chapter3.lute",
        &scene("chapter3", "on: cutscene\npriority: 1\n", "@narrator: Any."),
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    let t = scenario(&dir, &[]);
    assert!(
        t.contains("    scene(visit3) -> scene(chapter2) [raises]\n"),
        "{t}"
    );
    assert!(
        t.contains("    scene(visit3) -> scene(chapter3) [raises]\n"),
        "{t}"
    );
}

/// The `occasion.target` raise of a kind beat is bound by the runtime: the
/// IR keeps the reference (the domain prefix applied) marked `dynamic`, the
/// `plugin` record names the member the beat runs for.
#[test]
fn an_occasion_target_raise_is_bound_in_the_record() {
    let dir = project(
        "raises-dynamic",
        &[
            (
                "scenes/visit.lute",
                &scene(
                    "visitor",
                    "on: visit\ntarget: \"kind:cutsceneId\"\n",
                    "@narrator: V.\n\n::playScene{scene=occasion.target}",
                ),
            ),
            ("scenes/chapter2.lute", &chapter2("", "@narrator: Two.")),
        ],
    );
    let ir = compile(&dir, "scenes/visit.lute");
    assert_eq!(
        commands_of(&ir, "plugin")[0]["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.occasion.target","dynamic":true}]"#),
        "{ir}"
    );
    let records = play_records(&dir, "  - { occasion: visit, target: scene.chapter2 }\n");
    let plugin = records
        .iter()
        .find(|r| r["kind"] == "plugin")
        .expect("plugin record");
    assert_eq!(
        plugin["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter2"}]"#)
    );
    assert_eq!(plugin["attrs"]["scene"], "chapter2");
}

/// S5 review: only the compiler's `dynamic` mark makes a target bound. A
/// literal target that merely reads `….occasion.target` (a shape-only
/// occasion admits any dotted target) is static: recorded as written with no
/// mark, and the call runs outside any beat instead of waiting for a member;
/// the record raises the literal, and a step raising it presents its
/// answerer.
#[test]
fn a_static_target_reading_like_the_reference_is_not_bound() {
    let dir = project(
        "raises-static-lookalike",
        &[
            (
                "plugins/acme.stage/occasions/o.yaml",
                "occasions:\n  bell: { target: true }\n  begin: {}\n",
            ),
            (
                "plugins/acme.stage/directives/d.yaml",
                "directives:\n  - name: ring\n    effects: { raises: [{ occasion: bell, target: \
                 scene.occasion.target }] }\n",
            ),
            ("plugins/acme.stage/rewardkinds/r.yaml", "rewardKinds: {}\n"),
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro.\n\n::ring"),
            ),
            (
                "scenes/belfry.lute",
                &scene(
                    "belfry",
                    "on: bell\ntarget: scene.occasion.target\n",
                    "@narrator: Dong.",
                ),
            ),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    let ir = compile(&dir, "scenes/intro.lute");
    let raises = &commands_of(&ir, "plugin")[0]["raises"];
    assert_eq!(
        *raises,
        json(r#"[{"occasion":"bell","target":"scene.occasion.target"}]"#),
        "{ir}"
    );
    assert!(raises[0].get("dynamic").is_none(), "{ir}");
    let steps = format!("{BEGIN}  - {{ occasion: bell, target: scene.occasion.target }}\n");
    let records = play_records(&dir, &steps);
    let plugin = records
        .iter()
        .find(|r| r["kind"] == "plugin")
        .unwrap_or_else(|| panic!("the call runs: {records:?}"));
    assert_eq!(
        plugin["raises"],
        json(r#"[{"occasion":"bell","target":"scene.occasion.target"}]"#)
    );
    let t = play(&dir, &steps);
    assert!(t.contains("  → raises bell scene.occasion.target\n"), "{t}");
    assert!(t.contains("@narrator: Dong."), "{t}");
}

/// A clock schema raising `morning` at a day's first slot and `visit` at
/// every slot an advance stops at; `extra` YAML appended.
fn clock_schema(extra: &str) -> String {
    format!(
        "entities:\n  cutsceneId: {{ members: [intro, chapter2, chapter3] }}\nstate:\n  \
         run.day: {{ type: int, default: 1, owner: engine }}\n  run.slot: {{ type: {{ enum: \
         [dawn, dusk] }}, default: dawn, owner: engine }}\nclock:\n  day: run.day\n  slot: \
         run.slot\n  slots: [dawn, dusk]\n  raise: {{ dayStart: morning, slot: visit }}\n  days: \
         3\n{extra}"
    )
}

/// Review051b F3/F3a: a beat whose `when` holds only where the clock does
/// not raise its occasion is `W-BEAT-UNRAISED` — unless a declared raise
/// reaches it (untargeted, its static target, `occasion.target`, or any
/// target for a targetless beat), which may deliver it at any position.
#[test]
fn a_declared_raise_lifts_schedule_only_unraised_claims() {
    let occasions = "occasions:\n  morning: { select: first }\n  \
                     visit: { target: { prefix: scene, entity: cutsceneId } }\n  \
                     cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: \
                     declared }\n  begin: {}\n";
    let directives = "directives:\n  - name: wake\n    effects: { raises: [{ occasion: morning }] \
                      }\n  - name: go\n    attrs:\n      - { name: to, required: true, type: { \
                      entity: cutsceneId } }\n    effects: { raises: [{ occasion: visit, target: \
                      \"@to\" }] }\n";
    let early = "when: \"run.day == 1 && run.slot == 'dawn'\"\n";
    let dir = project(
        "raises-unraised",
        &[
            ("plugins/acme.stage/occasions/o.yaml", occasions),
            ("plugins/acme.stage/directives/d.yaml", directives),
            ("schema/world.schema.yaml", &clock_schema("")),
            (
                "scenes/arrival.lute",
                &scene(
                    "arrival",
                    "on: morning\nwhen: \"run.day == 1\"\n",
                    "@narrator: Off.",
                ),
            ),
            (
                "scenes/anyvisit.lute",
                &scene(
                    "anyvisit",
                    &format!("on: visit\n{early}"),
                    "@narrator: Any.",
                ),
            ),
            (
                "scenes/introvisit.lute",
                &scene(
                    "introvisit",
                    &format!("on: visit\ntarget: scene.intro\npriority: 1\n{early}"),
                    "@narrator: Intro.",
                ),
            ),
        ],
    );
    let unraised = |dir: &Path| -> Vec<String> {
        check(dir)
            .into_iter()
            .filter(|l| l.contains("[W-BEAT-UNRAISED]"))
            .map(|l| l.split_once(':').map_or(l.clone(), |(p, _)| p.to_string()))
            .collect()
    };
    assert_eq!(
        unraised(&dir),
        [
            "scenes/anyvisit.lute",
            "scenes/arrival.lute",
            "scenes/introvisit.lute"
        ]
    );
    // An untargeted raise of `morning`, and a raise of `visit` for
    // `scene.chapter2`: the targetless `anyvisit` answers it (F3a); the
    // `scene.intro` beat does not.
    let prologue = |body: &str| {
        scene(
            "prologue",
            "on: begin\n",
            &format!("@narrator: P.\n\n{body}"),
        )
    };
    write(
        &dir,
        "scenes/prologue.lute",
        &prologue("::wake\n::go{to=\"chapter2\"}"),
    );
    assert_eq!(unraised(&dir), ["scenes/introvisit.lute"]);
    // Its static target.
    write(
        &dir,
        "scenes/prologue.lute",
        &prologue("::go{to=\"intro\"}"),
    );
    assert_eq!(unraised(&dir), ["scenes/arrival.lute"]);
    // `occasion.target`, in a kind beat.
    write(&dir, "scenes/prologue.lute", &prologue(""));
    write(
        &dir,
        "scenes/porter.lute",
        &scene(
            "porter",
            "on: visit\ntarget: \"kind:cutsceneId\"\npriority: -5\n",
            "@narrator: Porter.\n\n::go{to=occasion.target}",
        ),
    );
    assert_eq!(unraised(&dir), ["scenes/arrival.lute"]);
}

/// F3a, the non-clock side of the one rule: a targetless answerer of a
/// targeted `raisedBy: declared` occasion is reached by a raise for one
/// target — anchored, and not `W-OCCASION-NEVER-RAISED`.
#[test]
fn a_targetless_answerer_is_reached_by_a_raise_for_any_target() {
    let dir = project(
        "raises-targetless",
        &[
            (
                "scenes/intro.lute",
                &scene("intro", "on: begin\n", "@narrator: Intro.\n\n::playScene"),
            ),
            (
                "scenes/any.lute",
                &scene("any", "on: cutscene\n", "@narrator: Any."),
            ),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    assert!(scenario(&dir, &[]).contains("    scene(intro) -> scene(any) [raises]\n"));
}

/// An occasion the clock raises is raised outside any declared raise:
/// `raisedBy: declared` draws no anchor and no `W-OCCASION-NEVER-RAISED`.
#[test]
fn a_clock_raised_declared_occasion_draws_no_anchor_and_no_warning() {
    let occasions = "occasions:\n  morning: { select: first, raisedBy: declared }\n  \
                     visit: { target: { prefix: scene, entity: cutsceneId } }\n  begin: {}\n";
    let directives = "directives:\n  - name: wake\n    effects: { raises: [{ occasion: morning }] \
                      }\n";
    let dir = project(
        "raises-clock-declared",
        &[
            ("plugins/acme.stage/occasions/o.yaml", occasions),
            ("plugins/acme.stage/directives/d.yaml", directives),
            ("schema/world.schema.yaml", &clock_schema("")),
            (
                "scenes/dawnscene.lute",
                &scene("dawnscene", "on: morning\n", "@narrator: Dawn."),
            ),
        ],
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    write(
        &dir,
        "scenes/prologue.lute",
        &scene("prologue", "on: begin\n", "@narrator: P.\n\n::wake"),
    );
    assert!(check(&dir).is_empty(), "{:?}", check(&dir));
    assert!(!scenario(&dir, &[]).contains("[raises]"));
}

/// Review051b F3b: a declared raise removes only schedule-only claims. A
/// `dayStart` chain whose first scene needs the day the run starts no
/// longer stalls once a declared raise reaches it; a chain whose next
/// scene's `when` has expired by the time the scene before it can play
/// still does.
#[test]
fn a_declared_raise_removes_only_schedule_only_chapter_stalls() {
    let occasions = "occasions:\n  dayStart: { select: first }\n  chapter: { select: first }\n  \
                     begin: {}\n";
    let directives = "directives:\n  - name: wake\n    effects: { raises: [{ occasion: dayStart \
                      }] }\n  - name: turn\n    effects: { raises: [{ occasion: chapter }] }\n";
    let manifest = |chain: &str| format!("{PROJECT}chapters:\n{chain}");
    let stalls = |dir: &Path| {
        check(dir)
            .into_iter()
            .filter(|l| l.contains("[W-CHAPTER-STALL]"))
            .count()
    };
    let prologue = |call: &str| {
        scene(
            "prologue",
            "on: begin\n",
            &format!("@narrator: P.\n\n{call}"),
        )
    };

    let dir = project(
        "raises-stall-gone",
        &[
            ("plugins/acme.stage/occasions/o.yaml", occasions),
            ("plugins/acme.stage/directives/d.yaml", directives),
            (
                "lute.project.yaml",
                &manifest("  - on: dayStart\n    scenes: [mon, tue]\n"),
            ),
            (
                "schema/world.schema.yaml",
                "state:\n  run.day: { type: int, default: 1, owner: engine }\nclock:\n  day: \
                 run.day\n  raise: { dayStart: dayStart }\n  days: 7\n",
            ),
            (
                "scenes/mon.lute",
                &scene("mon", "when: \"run.day == 1\"\n", "@narrator: Mon."),
            ),
            ("scenes/tue.lute", &scene("tue", "", "@narrator: Tue.")),
        ],
    );
    assert_eq!(stalls(&dir), 1, "{:?}", check(&dir));
    write(&dir, "scenes/prologue.lute", &prologue("::wake"));
    assert_eq!(stalls(&dir), 0, "{:?}", check(&dir));

    let dir = project(
        "raises-stall-stays",
        &[
            ("plugins/acme.stage/occasions/o.yaml", occasions),
            ("plugins/acme.stage/directives/d.yaml", directives),
            (
                "lute.project.yaml",
                &manifest("  - on: chapter\n    scenes: [parlour, kitchen, garden]\n"),
            ),
            (
                "schema/world.schema.yaml",
                "state:\n  run.day: { type: int, default: 1, owner: engine }\n  run.slot: { \
                 type: { enum: [morning, afternoon, evening] }, default: morning, owner: engine \
                 }\nclock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, afternoon, \
                 evening]\n  raise: { slot: chapter }\n  days: 1\n",
            ),
            (
                "scenes/parlour.lute",
                &scene("parlour", "", "@narrator: P."),
            ),
            (
                "scenes/kitchen.lute",
                &scene(
                    "kitchen",
                    "when: \"run.slot == 'afternoon'\"\n",
                    "@narrator: K.",
                ),
            ),
            ("scenes/garden.lute", &scene("garden", "", "@narrator: G.")),
        ],
    );
    assert_eq!(stalls(&dir), 1, "{:?}", check(&dir));
    write(&dir, "scenes/prologue.lute", &prologue("::turn"));
    let lines = check(&dir);
    assert_eq!(stalls(&dir), 1, "{lines:?}");
    assert!(
        lines.iter().any(|l| l.contains(
            "if `parlour` plays at day 1 evening or later, no later raise of `chapter` meets it"
        )),
        "{lines:?}"
    );
}
