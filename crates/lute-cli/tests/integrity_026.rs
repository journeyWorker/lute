//! dsl 0.26.0 §2 many-authors integrity, end to end through the CLI: state
//! declared in several documents (§2.1, T1-1), duplicate members (§2.2,
//! T1-3), `defaults.uses` globs and `defaults.questTier` (§2.4, T2-6),
//! entity-typed attributes, reward target contracts and `lute refs` (§2.5,
//! T1-8/T2-10), schema diagnostics reported once at the schema (§2.7, T3-1),
//! display-name collisions (§2.8, T3-13) and the member / clash hints (§8,
//! T3-7/T3-8).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-integrity026-{tag}-{}", std::process::id()));
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

/// The report lines (not the indented `related` lines under them) carrying
/// `code`.
fn top_lines<'a>(out: &'a str, code: &str) -> Vec<&'a str> {
    out.lines()
        .filter(|l| !l.starts_with(' ') && l.contains(&format!("[{code}]")))
        .collect()
}

/// A project whose plugin `p` exports the given `(export, file, body)`s,
/// with `defaults` appended under `defaults:`.
fn project(tag: &str, exports: &[(&str, &str, &str)], defaults: &str) -> PathBuf {
    let dir = temp_dir(tag);
    let mut manifest =
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n"
            .to_string();
    for (export, file, body) in exports {
        manifest.push_str(&format!("  {export}: {export}/\n"));
        write(&dir, &format!("plugins/p/{export}/{file}"), body);
    }
    if exports.is_empty() {
        manifest.push_str("  occasions: occasions/\n");
        write(
            &dir,
            "plugins/p/occasions/o.yaml",
            "occasions:\n  go: { select: first }\n",
        );
    }
    write(&dir, "plugins/p/plugin.yaml", &manifest);
    write(
        &dir,
        "lute.project.yaml",
        &format!(
            "pluginsDir: plugins/\ndefaultProfile: base\nprofiles:\n  base: {{ plugins: {{ p: true }} }}\ndefaults:\n  luteVersion: \"0.26.0\"\n{defaults}"
        ),
    );
    dir
}

fn scene(id: &str, frontmatter: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{frontmatter}---\n\n## {id}\n\n@narrator: {id}.\n{body}")
}

// ── §2.1 (T1-1): state declared in several documents ───────────────────────

#[test]
fn one_path_declared_with_two_types_is_e_state_decl_conflict_naming_both() {
    let dir = project("st-type", &[], "");
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "on: go\nstate:\n  run.wins: { type: number, default: 0 }\n",
            "::set{run.wins += 1}\n",
        ),
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q.doc\nstate:\n  run.wins: { type: bool, default: true }\n---\n\n\
         <quest id=\"champ\" title=\"Champ\" start=\"true\" tier=\"run\">\n  \
         <objective id=\"w\" title=\"Win\" done=\"run.wins\"/>\n</quest>\n",
    );
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    let lines = top_lines(&s, "E-STATE-DECL-CONFLICT");
    assert_eq!(lines.len(), 1, "reported once: {s}");
    assert!(
        lines[0].contains("q.lute:5") && lines[0].contains("a.lute:6"),
        "names both declarations by file and line: {s}"
    );
    assert!(
        lines[0].contains("bool") && lines[0].contains("number"),
        "{s}"
    );

    // Play keys state by declared type: it refuses the project rather than
    // read one document's number as the other's bool.
    write(&dir, "plays/p.play.yaml", "steps:\n  - occasion: go\n");
    let play = run(&dir, &["play", ".", "--script", "plays/p.play.yaml"]);
    assert_ne!(play.status.code(), Some(0), "{}", text(&play));
    assert!(text(&play).contains("refusing to play"), "{}", text(&play));
}

#[test]
fn declarations_must_also_agree_on_default_and_a_schema_counts() {
    let dir = project("st-default", &[], "");
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.flag: { type: bool, default: false }\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "uses: ../world.schema.yaml\n", ""),
    );
    write(
        &dir,
        "scenes/b.lute",
        &scene(
            "b",
            "state:\n  run.flag: { type: bool, default: true }\n",
            "",
        ),
    );
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    let lines = top_lines(&s, "E-STATE-DECL-CONFLICT");
    assert_eq!(lines.len(), 1, "{s}");
    assert!(
        lines[0].contains("world.schema.yaml:2"),
        "the schema declaration is named: {s}"
    );
}

#[test]
fn agreeing_declarations_and_scene_locals_do_not_conflict() {
    let dir = project("st-agree", &[], "");
    let fm = "state:\n  run.wins: { type: number, default: 0 }\n";
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            &format!("{fm}  scene.mood: {{ type: number, default: 0 }}\n"),
            "",
        ),
    );
    write(
        &dir,
        "scenes/b.lute",
        &scene(
            "b",
            &format!("{fm}  scene.mood: {{ type: bool, default: false }}\n"),
            "",
        ),
    );
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert!(top_lines(&s, "E-STATE-DECL-CONFLICT").is_empty(), "{s}");
}

// ── §2.2 (T1-3) duplicate members, §2.7 (T3-1) reported once at the schema ──

#[test]
fn a_member_listed_twice_is_e_entity_kind_shape_at_the_schema_line() {
    let dir = project("dup-member", &[], "  uses: [world.schema.yaml]\n");
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  trainer:\n    members:\n      - ada\n      - bo\n      - ada\nenums:\n  mood: [calm, calm]\n",
    );
    write(&dir, "scenes/a.lute", &scene("a", "", ""));
    write(&dir, "scenes/b.lute", &scene("b", "", ""));
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    let lines = top_lines(&s, "E-ENTITY-KIND-SHAPE");
    assert_eq!(
        lines.len(),
        2,
        "one per duplicate, folded across importers: {s}"
    );
    let trainer = lines.iter().find(|l| l.contains("`trainer`")).expect(&s);
    assert!(
        trainer.contains("`ada` more than once (lines 4 and 6)"),
        "{s}"
    );
    assert!(trainer.contains("(+1 more caller)"), "{s}");
    assert!(
        s.contains("    world.schema.yaml:6:"),
        "anchored at the second `ada`: {s}"
    );
    assert!(lines.iter().any(|l| l.contains("enum `mood`")), "{s}");
}

#[test]
fn schema_errors_are_reported_once_at_the_schema_line() {
    let dir = project(
        "fold",
        &[],
        "  uses: [world.schema.yaml, a.schema.yaml, b.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  place: { members: [hall] }\n  haunt: { subsetOf: plase, members: [crypt] }\n",
    );
    write(
        &dir,
        "a.schema.yaml",
        "state:\n  run.score: { type: number, default: 0 }\n",
    );
    write(
        &dir,
        "b.schema.yaml",
        "state:\n  run.score: { type: number, default: 0 }\n",
    );
    for id in ["a", "b", "c"] {
        write(&dir, &format!("scenes/{id}.lute"), &scene(id, "", ""));
    }
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    for (code, at) in [
        ("E-ENTITY-KIND-SHAPE", "    world.schema.yaml:3:3:"),
        ("E-USES-DUP-STATE", "    b.schema.yaml:2:3:"),
    ] {
        let lines = top_lines(&s, code);
        assert_eq!(lines.len(), 1, "{code} once, not per document: {s}");
        assert!(lines[0].contains("(+2 more callers)"), "{s}");
        assert!(s.contains(at), "{code} anchored at {at}: {s}");
    }
}

// ── §8 (T3-7, T3-8): member did-you-mean and the clash hint ─────────────────

#[test]
fn a_non_member_at_use_and_assert_gets_a_did_you_mean() {
    let dir = project(
        "dym",
        &[],
        "  uses: [world.schema.yaml]\n  components: [components/meet.component.lute]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  rivalMeet: { members: [bridge, tower] }\nrelations:\n  met: { args: [rivalMeet], tier: run }\n",
    );
    write(
        &dir,
        "components/meet.component.lute",
        "---\ncomponent: meet\neffects: true\nparams:\n  spot: string\n---\n\n## Meet\n\n::assert{met(@spot)}\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "",
            "::use{component=\"meet\" spot=\"brdge\"}\n::assert{met(towr)}\n",
        ),
    );
    let s = text(&run(&dir, &["check-project", "."]));
    let lines = top_lines(&s, "E-FACT-DOMAIN");
    assert_eq!(lines.len(), 2, "{s}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("`brdge`") && l.contains("did you mean `bridge`?")),
        "{s}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("`towr`") && l.contains("did you mean `tower`?")),
        "{s}"
    );
}

#[test]
fn an_id_in_two_unrelated_kinds_offers_subset_or_rename() {
    let dir = project("clash", &[], "  uses: [world.schema.yaml]\n");
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  place: { members: [ashTower, hall] }\n  lair: { members: [ashTower] }\n",
    );
    write(&dir, "scenes/a.lute", &scene("a", "", ""));
    let s = text(&run(&dir, &["check-project", "."]));
    let lines = top_lines(&s, "E-ENTITY-KIND-CLASH");
    assert_eq!(lines.len(), 1, "{s}");
    assert!(
        lines[0].contains("subsetOf:") && lines[0].contains("rename one of them"),
        "{s}"
    );
}

// ── §2.4 (T2-6): defaults.uses globs, defaults.questTier ────────────────────

#[test]
fn defaults_uses_globs_import_every_matching_schema() {
    let dir = project("glob", &[], "  uses: [schema/*.schema.yaml]\n");
    write(
        &dir,
        "schema/east.schema.yaml",
        "state:\n  run.eastFlag: { type: bool, default: false }\n",
    );
    write(
        &dir,
        "schema/south.schema.yaml",
        "state:\n  run.southFlag: { type: bool, default: false }\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "",
            "::set{run.eastFlag = true}\n::set{run.southFlag = true}\n",
        ),
    );
    let out = run(&dir, &["check-project", "."]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
}

#[test]
fn defaults_quest_tier_applies_to_quests_without_one() {
    let dir = project("tier", &[], "  questTier: run\n");
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q.doc\n---\n\n<quest id=\"plain\" title=\"P\" start=\"true\">\n  \
         <objective id=\"o\" title=\"O\" done=\"true\"/>\n</quest>\n\n\
         <quest id=\"pinned\" title=\"R\" start=\"true\" tier=\"user\">\n  \
         <objective id=\"o\" title=\"O\" done=\"true\"/>\n</quest>\n",
    );
    let out = run(&dir, &["compile", "--project", ".", "quests/q.lute"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let ir: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    // The IR carries a quest's tier only when it is the non-default `run`.
    let mut tiers = Vec::new();
    collect_quest_tiers(&ir, &mut tiers);
    assert_eq!(tiers, [("plain".to_string(), "run".to_string())], "{ir:#}");
}

/// Every `{ "id": …, "tier": … }` object in the artifact whose id names a quest.
fn collect_quest_tiers(v: &serde_json::Value, out: &mut Vec<(String, String)>) {
    match v {
        serde_json::Value::Object(m) => {
            if let (Some(id), Some(tier)) = (
                m.get("id").and_then(|x| x.as_str()),
                m.get("tier").and_then(|x| x.as_str()),
            ) {
                if ["plain", "pinned"].contains(&id) && !out.iter().any(|(i, _)| i == id) {
                    out.push((id.to_string(), tier.to_string()));
                }
            }
            m.values().for_each(|x| collect_quest_tiers(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| collect_quest_tiers(x, out)),
        _ => {}
    }
}

// ── §2.5 (T1-8, T2-10): engine content ids ──────────────────────────────────

const ITEMS: &str = "entities:\n  bagItem: { members: [goodRod, oldRod, potion] }\n";

fn content_project(tag: &str) -> PathBuf {
    let dir = project(
        tag,
        &[
            (
                "directives",
                "give.yaml",
                "directives:\n  - name: give\n    attrs:\n      - { name: item, required: true, type: { entity: bagItem } }\n      - { name: tm, type: { entity: tmm } }\n",
            ),
            (
                "rewardkinds",
                "r.yaml",
                "rewardKinds:\n  ITEM: { target: { entity: bagItem, required: true } }\n",
            ),
        ],
        "  uses: [items.schema.yaml]\n",
    );
    write(&dir, "items.schema.yaml", ITEMS);
    dir
}

#[test]
fn an_entity_typed_attribute_checks_members_with_a_did_you_mean() {
    let dir = content_project("attr");
    write(
        &dir,
        "scenes/a.lute",
        &scene(
            "a",
            "",
            "::give{item=\"goodRod\"}\n::give{item=\"godRod\"}\n::give{item=\"potion\" tm=\"x\"}\n",
        ),
    );
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    let bad = top_lines(&s, "E-BAD-ENUM");
    assert_eq!(bad.len(), 1, "{s}");
    assert!(
        bad[0].contains("a.lute:10:") && bad[0].contains("did you mean `goodRod`?"),
        "{s}"
    );
    let unknown = top_lines(&s, "E-DOMAIN-UNKNOWN");
    assert_eq!(unknown.len(), 1, "{s}");
    assert!(
        unknown[0].contains("`tmm` is not a declared entity kind"),
        "{s}"
    );
    assert!(
        top_lines(&s, "W-DOMAIN-UNREAD").is_empty(),
        "an entity-typed attribute reads its kind: {s}"
    );
}

#[test]
fn reward_targets_are_checked_against_the_kind_contract() {
    let dir = content_project("reward");
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q.doc\n---\n\n<quest id=\"q\" title=\"Q\" start=\"true\" tier=\"run\">\n  \
         <objective id=\"o\" title=\"O\" done=\"true\"/>\n  \
         <reward kind=\"ITEM\" target=\"goodRod\"/>\n  \
         <reward kind=\"ITEM\" target=\"godRod\"/>\n  \
         <reward kind=\"ITEM\"/>\n</quest>\n",
    );
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    let lines = top_lines(&s, "E-REWARD-TARGET");
    assert_eq!(lines.len(), 2, "{s}");
    assert!(
        lines
            .iter()
            .any(|l| l.contains("q.lute:9:") && l.contains("did you mean `goodRod`?")),
        "{s}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("q.lute:10:") && l.contains("needs a `target=`")),
        "{s}"
    );
}

fn refs_project(tag: &str) -> PathBuf {
    let dir = content_project(tag);
    write(
        &dir,
        "scenes/east.lute",
        &scene("east", "", "::give{item=\"goodRod\"}\n<branch id=\"b\">\n  <choice id=\"c\" label=\"C\">\n    ::give{item=\"potion\"}\n  </choice>\n</branch>\n"),
    );
    write(
        &dir,
        "scenes/mid.lute",
        &scene("mid", "", "::give{item=\"goodRod\"}\n"),
    );
    write(
        &dir,
        "quests/q.lute",
        "---\nkind: quest\nid: q.doc\n---\n\n<quest id=\"q\" title=\"Q\" start=\"true\" tier=\"run\">\n  \
         <objective id=\"o\" title=\"O\" done=\"true\">\n    <reward kind=\"ITEM\" target=\"goodRod\"/>\n  </objective>\n  \
         <reward kind=\"ITEM\"/>\n</quest>\n",
    );
    dir
}

#[test]
fn refs_lists_every_value_with_the_documents_using_it() {
    let dir = refs_project("refs-list");
    let out = run(
        &dir,
        &["refs", ".", "--attr", "give.item", "--reward", "ITEM"],
    );
    let s = text(&out);
    assert_eq!(out.status.code(), Some(0), "{s}");
    assert!(s.contains("::give.item: 2 value(s)"), "{s}");
    assert!(s.contains("`goodRod` — 2 use(s) in 2 document(s)\n    scenes/east.lute:9\n    scenes/mid.lute:9\n"), "{s}");
    assert!(
        s.contains("`potion` — 1 use(s) in 1 document(s)\n    scenes/east.lute:12\n"),
        "nested bodies count: {s}"
    );
    assert!(s.contains("reward ITEM: 2 value(s)"), "{s}");
    assert!(
        s.contains("(no target) — 1 use(s) in 1 document(s)\n    quests/q.lute:10\n"),
        "{s}"
    );
    assert!(
        s.contains("`goodRod` — 1 use(s) in 1 document(s)\n    quests/q.lute:8\n"),
        "objective rewards count: {s}"
    );

    let json = run(&dir, &["refs", ".", "--reward", "ITEM", "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&json.stdout).expect(&text(&json));
    let q = &v["queries"][0];
    assert_eq!(
        (q["kind"].as_str(), q["name"].as_str()),
        (Some("reward"), Some("ITEM"))
    );
    assert_eq!(q["values"][0]["value"], serde_json::Value::Null, "{v:#}");
    assert_eq!(q["values"][1]["value"], "goodRod", "{v:#}");
    assert_eq!(q["values"][1]["uses"][0]["document"], "quests/q.lute");
    assert_eq!(q["values"][1]["uses"][0]["line"], 8);
}

#[test]
fn refs_needs_a_well_formed_query() {
    let dir = refs_project("refs-query");
    assert_eq!(run(&dir, &["refs", "."]).status.code(), Some(2));
    assert_eq!(
        run(&dir, &["refs", ".", "--attr", "give"]).status.code(),
        Some(2)
    );
}

// ── §2.8 (T3-13): display names ─────────────────────────────────────────────

fn names_project(tag: &str) -> PathBuf {
    let dir = project(
        tag,
        &[(
            "cast",
            "c.yaml",
            "cast:\n  trainer: { name: Trainer }\n  oldNed: { name: Old Ned }\n  fisherNed: { name: Old Ned }\n  sol: { name: Keeper Sol }\n",
        )],
        "  components: [components/battle.component.lute]\n",
    );
    write(
        &dir,
        "components/battle.component.lute",
        "---\ncomponent: battle\nparams:\n  who: string\n  name: string\n---\n\n## Battle\n\n@trainer{as=@name}: Fight!\n",
    );
    write(&dir, "scenes/south.lute", &scene(
        "south",
        "",
        "@oldNed: Hello.\n::use{component=\"battle\" who=\"r3Gus\" name=\"Hiker Gus\"}\n::use{component=\"battle\" who=\"r3Gus\" name=\"Hiker Gus\"}\n",
    ));
    dir
}

#[test]
fn equal_display_names_of_different_speakers_warn_once() {
    let dir = names_project("names");
    write(&dir, "scenes/north.lute", &scene(
        "north",
        "",
        "@fisherNed: Hi.\n::use{component=\"battle\" who=\"r16Gus\" name=\"Hiker Gus\"}\n::use{component=\"battle\" who=\"r16Keeper\" name=\"Keeper Sol\"}\n",
    ));
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(0), "advisory: {s}");
    let lines = top_lines(&s, "W-DISPLAY-NAME-DUP");
    assert_eq!(lines.len(), 3, "Old Ned, Hiker Gus, Keeper Sol: {s}");
    let gus = lines.iter().find(|l| l.contains("`Hiker Gus`")).expect(&s);
    assert!(
        gus.contains("2 different speakers") && gus.contains("`r16Gus`") && gus.contains("`r3Gus`"),
        "{s}"
    );
    assert!(
        lines.iter().any(|l| l.contains("`Old Ned`")
            && l.contains("`oldNed`")
            && l.contains("`fisherNed`")),
        "{s}"
    );
    assert!(
        lines.iter().any(|l| l.contains("`Keeper Sol`")
            && l.contains("`sol`")
            && l.contains("`r16Keeper`")),
        "cast vs component name=: {s}"
    );

    let denied = run(
        &dir,
        &["check-project", ".", "--deny", "W-DISPLAY-NAME-DUP"],
    );
    assert_eq!(denied.status.code(), Some(1), "{}", text(&denied));

    let lint = run(&dir, &["lint", "."]);
    let l = text(&lint);
    assert_eq!(top_lines(&l, "W-DISPLAY-NAME-DUP").len(), 3, "{l}");
    let lint_denied = run(&dir, &["lint", ".", "--deny", "W-DISPLAY-NAME-DUP"]);
    assert_eq!(lint_denied.status.code(), Some(1), "{}", text(&lint_denied));
}

#[test]
fn one_speaker_shown_twice_under_one_name_is_not_a_collision() {
    let dir = names_project("names-one");
    // `fisherNed` and `oldNed` share a name, but only `south` exists: the
    // cast pair still collides; the `r3Gus` battle and rematch do not.
    let s = text(&run(&dir, &["check-project", "."]));
    let lines = top_lines(&s, "W-DISPLAY-NAME-DUP");
    assert_eq!(lines.len(), 1, "{s}");
    assert!(lines[0].contains("`Old Ned`"), "{s}");
}

/// A schema that `extends:` a base and overrides its default refines it —
/// the language's own refinement, not two authors colliding.
#[test]
fn an_extends_refinement_is_not_a_conflict() {
    let dir = project("st-extends", &[], "");
    write(
        &dir,
        "base.schema.yaml",
        "state:\n  run.blessed: { type: bool, default: false }\n",
    );
    write(
        &dir,
        "child.schema.yaml",
        "extends: base.schema.yaml\nstate:\n  run.blessed: { type: bool, default: true }\n",
    );
    write(
        &dir,
        "scenes/a.lute",
        &scene("a", "uses: ../base.schema.yaml\n", ""),
    );
    write(
        &dir,
        "scenes/b.lute",
        &scene("b", "uses: ../child.schema.yaml\n", ""),
    );
    write(
        &dir,
        "scenes/c.lute",
        &scene(
            "c",
            "state:\n  run.blessed: { type: number, default: 0 }\n",
            "",
        ),
    );
    let s = text(&run(&dir, &["check-project", "."]));
    let lines = top_lines(&s, "E-STATE-DECL-CONFLICT");
    assert_eq!(lines.len(), 1, "only the unrelated number declaration: {s}");
    assert!(
        lines[0].contains("c.lute:5") && lines[0].contains("base.schema.yaml:2"),
        "{s}"
    );
}

// ── §2.6 (T2-7): checking a spine before the areas exist ───────────────────

/// A spine whose only `hasBadge` producer is a component `::assert` with an
/// unbound `@badge` — no area `::use`s it yet — gated by a required
/// objective (under a parent's `quest=` objective) and a scene beat.
/// `ribbon` is produced, but only as `ribbon(stone)`.
fn spine(tag: &str, ribbon_tide: bool) -> PathBuf {
    let dir = project(tag, &[], "  uses: [world.schema.yaml]\n");
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  badge: { members: [stone, tide] }\nrelations:\n  \
         hasBadge: { args: [badge], tier: run }\n  ribbon: { args: [badge], tier: run }\n",
    );
    write(
        &dir,
        "components/gym.component.lute",
        "---\ncomponent: gym\neffects: true\nparams:\n  badge: string\n---\n\n## Gym\n\n\
         @narrator: A badge.\n::assert{hasBadge(@badge)}\n",
    );
    let ribbon = if ribbon_tide {
        "  <objective id=\"r\" title=\"R\" done=\"holds(ribbon(tide))\"/>\n"
    } else {
        ""
    };
    write(
        &dir,
        "quests/league.lute",
        &format!(
            "---\nkind: quest\nid: spine.quests\n---\n\n\
             <quest id=\"league\" title=\"League\" start=\"true\" tier=\"run\">\n  \
             <objective id=\"badges\" title=\"Badges\" quest=\"badgeRoad\"/>\n</quest>\n\n\
             <quest id=\"badgeRoad\" title=\"Badges\" tier=\"run\">\n  \
             <objective id=\"stone\" title=\"Stone\" done=\"holds(hasBadge(stone))\"/>\n\
             {ribbon}</quest>\n"
        ),
    );
    write(
        &dir,
        "scenes/broadcast.lute",
        &scene(
            "broadcast",
            "on: go\nwhen: \"holds(hasBadge(tide))\"\n",
            "::assert{ribbon(stone)}\n",
        ),
    );
    dir
}

#[test]
fn wip_reads_a_component_only_producer_as_not_written_yet() {
    let dir = spine("wip-spine", false);
    let plain = run(&dir, &["check-project", "."]);
    let s = text(&plain);
    assert_eq!(plain.status.code(), Some(1), "{s}");
    for code in ["E-OBJECTIVE-UNSATISFIABLE", "E-BEAT-UNREACHABLE"] {
        assert!(s.contains(&format!("error [{code}]")), "{code}: {s}");
    }

    let wip = run(&dir, &["check-project", "--wip", "."]);
    let s = text(&wip);
    assert_eq!(wip.status.code(), Some(0), "{s}");
    let unsat = top_lines(&s, "E-OBJECTIVE-UNSATISFIABLE");
    // The dead `done` and the parent's `quest="badgeRoad"` objective.
    assert_eq!(unsat.len(), 2, "{s}");
    assert!(
        unsat
            .iter()
            .all(|l| l.contains("warning [") && l.contains("`--wip`")),
        "{s}"
    );
    assert!(
        unsat.iter().any(|l| l.contains("quest=\"badgeRoad\"")),
        "{s}"
    );
    let beat = top_lines(&s, "E-BEAT-UNREACHABLE");
    assert_eq!(beat.len(), 1, "{s}");
    assert!(
        beat[0].contains("warning [") && beat[0].contains("`--wip`"),
        "{s}"
    );
}

#[test]
fn wip_keeps_a_ground_producer_that_never_matches_an_error() {
    // `ribbon(tide)`: `ribbon` has a producer (`ribbon(stone)`), never a
    // component one, so the dead objective — and the parent objective on
    // the quest it kills — stays an error under `--wip`.
    let dir = spine("wip-ground", true);
    let wip = run(&dir, &["check-project", "--wip", "."]);
    let s = text(&wip);
    assert_eq!(wip.status.code(), Some(1), "{s}");
    let unsat = top_lines(&s, "E-OBJECTIVE-UNSATISFIABLE");
    assert!(
        unsat
            .iter()
            .any(|l| l.contains("error [") && l.contains("`ribbon(tide)`")),
        "{s}"
    );
    assert!(
        unsat
            .iter()
            .any(|l| l.contains("error [") && l.contains("quest=\"badgeRoad\"")),
        "{s}"
    );
    assert!(
        unsat
            .iter()
            .any(|l| l.contains("warning [") && l.contains("`hasBadge(stone)`")),
        "{s}"
    );
}

// ── 0.26 prerelease review (Monster League N1–N8) ──────────────────────────

/// A demo project: plugin `demo` exporting `directives`/`occasions`/`cast`
/// files as given, `uses` as `defaults.uses`.
fn demo(tag: &str, files: &[(&str, &str)], uses: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "plugins/demo/plugin.yaml",
        "id: demo\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n  directives: directives/\n",
    );
    write(
        &dir,
        "plugins/demo/occasions/o.yaml",
        "occasions:\n  talk: { select: first }\n",
    );
    write(
        &dir,
        "plugins/demo/directives/give.yaml",
        "directives:\n  - name: give\n    attrs:\n      - { name: item, required: true, type: { entity: bagItem } }\n",
    );
    write(
        &dir,
        "lute.project.yaml",
        &format!(
            "pluginsDir: plugins/\ndefaultProfile: base\nprofiles:\n  base: {{ plugins: {{ demo: true }} }}\ndefaults:\n  luteVersion: \"0.26.0\"\n  uses: [{uses}]\n"
        ),
    );
    for (rel, body) in files {
        write(&dir, rel, body);
    }
    dir
}

fn gift_project(tag: &str) -> PathBuf {
    demo(
        tag,
        &[
            (
                "world.schema.yaml",
                "state:\n  run.fish: { type: number, default: 0 }\nentities:\n  bagItem: { members: [potion, nugget] }\n",
            ),
            (
                "components/gift.component.lute",
                "---\ncomponent: gift\nparams:\n  item: string\n---\n\n## Gift\n\n@narrator: Here.\n::give{item=@item}\n",
            ),
            (
                "components/relay.component.lute",
                "---\ncomponent: relay\nparams:\n  thing: string\n---\n\n## Relay\n\n::use{component=\"gift\" item=@thing}\n",
            ),
            (
                "components/typed.component.lute",
                "---\ncomponent: typed\nparams:\n  item: { entity: bagItem }\n---\n\n## Typed\n\n@narrator: Here.\n",
            ),
            (
                "a.lute",
                "---\nkind: scene\nid: a\ncomponents: [components/gift.component.lute, components/relay.component.lute, components/typed.component.lute]\n---\n## A\n\n::give{item=\"potion\"}\n::use{component=\"gift\" item=\"potoin\"}\n::use{component=\"relay\" thing=\"nuget\"}\n::use{component=\"typed\" item=\"potoin\"}\n::use{component=\"gift\" item=\"nugget\"}\n",
            ),
        ],
        "world.schema.yaml",
    )
}

/// N1: an entity-typed attribute is judged through a component argument —
/// passed whole to the attribute, through a nested `::use`, or bound to a
/// param typed by the kind — at the argument, with a did-you-mean.
#[test]
fn entity_typed_attribute_is_checked_through_component_arguments() {
    let dir = gift_project("n1");
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(1), "{s}");
    let bad = top_lines(&s, "E-BAD-ENUM");
    assert_eq!(bad.len(), 3, "{s}");
    assert!(
        bad.iter()
            .any(|l| l.contains("a.lute:9:") && l.contains("did you mean `potion`?")),
        "{s}"
    );
    assert!(
        bad.iter().any(|l| l.contains("a.lute:10:")
            && l.contains("`nuget`")
            && l.contains("did you mean `nugget`?")),
        "{s}"
    );
    assert!(
        bad.iter()
            .any(|l| l.contains("a.lute:11:") && l.contains("`potoin`")),
        "{s}"
    );
}

/// N2: `lute refs --attr` lists a value passed through a component at the
/// `::use` binding it, naming the component.
#[test]
fn refs_lists_values_passed_through_components() {
    let dir = gift_project("n2");
    let out = run(&dir, &["refs", ".", "--attr", "give.item"]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(0), "{s}");
    assert!(s.contains("a.lute:8\n"), "direct: {s}");
    assert!(s.contains("a.lute:9 (via component `gift`)"), "{s}");
    assert!(
        s.contains("a.lute:10 (via component `relay`)"),
        "nested: {s}"
    );
    assert!(s.contains("a.lute:12 (via component `gift`)"), "{s}");
    assert!(
        !s.contains("a.lute:11"),
        "`typed` passes nothing to `::give`: {s}"
    );
}

/// N3: a scene ineligible only by its `after:` names that premise and the
/// mock it needs, not the (true) `when`; the stale "walk below" note is gone.
#[test]
fn ineligible_scene_failure_names_the_false_premise() {
    let dir = demo(
        "n3",
        &[
            ("world.schema.yaml", "state:\n  run.fish: { type: number, default: 0 }\n"),
            ("a.lute", "---\nkind: scene\nid: a\n---\n## A\n\n@narrator: A.\n"),
            (
                "b.lute",
                "---\nkind: scene\nid: b\non: talk\nafter: 'visited(\"a\")'\nwhen: \"run.fish == 0\"\n---\n## B\n\n@narrator: B.\n",
            ),
            (
                "c.lute",
                "---\nkind: scene\nid: c\non: talk\nwhen: \"run.fish == 1\"\n---\n## C\n\n@narrator: C.\n",
            ),
            ("tests/b.test.yaml", "file: ../b.lute\nexpect:\n  exit: complete\n"),
            ("tests/c.test.yaml", "file: ../c.lute\nexpect:\n  exit: complete\n"),
        ],
        "world.schema.yaml",
    );
    let s = text(&run(&dir, &["test", ".", "--project", "."]));
    assert!(
        s.contains("eligible b: not eligible under these mocks (its `after: visited(\"a\")` is false — mock `visited: [a]`)"),
        "{s}"
    );
    assert!(
        s.contains(
            "eligible c: not eligible under these mocks (its `when` (run.fish == 1) is false)"
        ),
        "{s}"
    );
    assert!(!s.contains("as if it had been presented"), "{s}");
}

/// N4: a member two `add:` lists (or `members:` and an `add:`) name is
/// anchored at the second member's own line, naming both lines.
#[test]
fn duplicate_add_member_is_anchored_at_both_member_lines() {
    let dir = demo(
        "n4",
        &[
            (
                "world.schema.yaml",
                "state:\n  run.fish: { type: number, default: 0 }\n",
            ),
            (
                "schema/a.schema.yaml",
                "entities:\n  person:\n    members:\n      - ada\n      - bo\n",
            ),
            (
                "schema/b.schema.yaml",
                "entities:\n  person:\n    add:\n      - cy\n      - bo\n",
            ),
            (
                "one.lute",
                "---\nkind: scene\nid: one\n---\n## S\n\n@narrator: x.\n",
            ),
        ],
        "world.schema.yaml, schema/a.schema.yaml, schema/b.schema.yaml",
    );
    let s = text(&run(&dir, &["check-project", "."]));
    assert!(
        s.contains("in `person`'s declaration in `schema/a.schema.yaml` (line 5) and in the `add:` of `schema/b.schema.yaml` (line 5)"),
        "{s}"
    );
    assert!(
        s.contains("b.schema.yaml:5:9: error [E-ENTITY-KIND-SHAPE]"),
        "anchored at the member: {s}"
    );
}

/// N5: an imported def whose type is not inferred is `E-DEF-DECL` once, at
/// the schema line; `visited(…)` infers `bool`.
#[test]
fn imported_def_decl_is_folded_at_the_schema_and_visited_is_bool() {
    let scenes: Vec<(String, String)> = ["a", "b", "c"]
        .iter()
        .map(|id| {
            (
                format!("{id}.lute"),
                format!("---\nkind: scene\nid: {id}\n---\n## S\n\n@narrator: x.\n"),
            )
        })
        .collect();
    let mut files: Vec<(&str, &str)> = scenes
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_str()))
        .collect();
    files.push((
        "world.schema.yaml",
        "state:\n  run.fish: { type: number, default: 0 }\ndefs:\n  metA: \"visited('a')\"\n  metB: \"run.fish + run.gone\"\n",
    ));
    let dir = demo("n5", &files, "world.schema.yaml");
    let s = text(&run(&dir, &["check-project", "."]));
    let lines = top_lines(&s, "E-DEF-DECL");
    assert_eq!(lines.len(), 1, "folded: {s}");
    assert!(
        lines[0].contains("`metB`") && lines[0].contains("(+2 more callers)"),
        "{s}"
    );
    assert!(
        s.contains("world.schema.yaml:5:3: error [E-DEF-DECL]"),
        "at the def line: {s}"
    );
    assert!(
        !s.contains("`metA` has no `type:`"),
        "visited() is bool: {s}"
    );
}

/// N6: a cast entry marked `sharedName: true` is not counted by
/// `W-DISPLAY-NAME-DUP`; two unmarked entries still are.
#[test]
fn shared_name_cast_entries_are_not_display_name_dups() {
    let dir = demo(
        "n6",
        &[
            (
                "world.schema.yaml",
                "state:\n  run.fish: { type: number, default: 0 }\ncast:\n  g1: { name: Eclipse Grunt, sharedName: true }\n  g2: { name: Eclipse Grunt, sharedName: true }\n  gus: { name: Hiker Gus }\n  gus2: { name: Hiker Gus }\n",
            ),
            ("one.lute", "---\nkind: scene\nid: one\n---\n## S\n\n@g1: a.\n@g2: b.\n@gus: c.\n@gus2: d.\n"),
        ],
        "world.schema.yaml",
    );
    let s = text(&run(&dir, &["check-project", "."]));
    let lines = top_lines(&s, "W-DISPLAY-NAME-DUP");
    assert_eq!(lines.len(), 1, "{s}");
    assert!(lines[0].contains("`Hiker Gus`"), "{s}");
}

/// N7: a `defaults.uses` glob over a directory that does not exist yet
/// matches nothing; the project checks.
#[test]
fn defaults_uses_glob_over_a_missing_directory_is_no_error() {
    let dir = demo(
        "n7",
        &[
            (
                "world.schema.yaml",
                "state:\n  run.fish: { type: number, default: 0 }\n",
            ),
            (
                "one.lute",
                "---\nkind: scene\nid: one\n---\n## S\n\n@narrator: x.\n",
            ),
        ],
        "world.schema.yaml, schema/areas/*.schema.yaml",
    );
    let out = run(&dir, &["check-project", "."]);
    let s = text(&out);
    assert_eq!(out.status.code(), Some(0), "{s}");
    assert!(!s.contains("E-DEFAULTS-KEY"), "{s}");
}
