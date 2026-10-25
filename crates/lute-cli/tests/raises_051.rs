//! 0.51.0 §4.3 (#170): `E-RAISES-TARGET` — each declared raise's resolved
//! target, judged by whole-project `check` against its occasion's domain and
//! the project's answering scenes and beats.
//!
//! On 0.50.0 `effects.raises` is an unknown key: the plugin fails to load
//! (`E-PLUGIN-PARSE`, then `E-PLUGIN-MISSING-ACTIVE`), so every test below,
//! which asserts the project's exact diagnostics, fails there.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  \
                      occasions: occasions/\n  rewardKinds: rewardkinds/\n";

const OCCASIONS: &str = "occasions:\n  \
     cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }\n  \
     finale: { target: { prefix: scene, entity: cutsceneId, members: [ending] } }\n  \
     visit: { target: { prefix: scene, entity: cutsceneId } }\n  \
     bell: { target: true }\n";

const DIRECTIVES: &str = concat!(
    "directives:\n",
    "  - name: playScene\n",
    "    attrs:\n",
    "      - { name: scene, required: true, type: { entity: cutsceneId } }\n",
    "    effects: { raises: [{ occasion: cutscene, target: \"@scene\" }] }\n",
    "  - name: cue\n",
    "    attrs:\n",
    "      - { name: scene, type: { entity: cutsceneId } }\n",
    "    effects: { raises: [{ occasion: cutscene, target: \"@scene\" }] }\n",
    "  - name: replay\n",
    "    attrs:\n",
    "      - { name: scene, type: { entity: cutsceneId }, default: chapter2 }\n",
    "    effects: { raises: [{ occasion: cutscene, target: \"@scene\" }] }\n",
    "  - name: rerun\n",
    "    attrs:\n",
    "      - { name: scene, type: { entity: cutsceneId }, default: chapter3 }\n",
    "    effects: { raises: [{ occasion: cutscene, target: \"@scene\" }] }\n",
    "  - name: playFinale\n",
    "    attrs:\n",
    "      - { name: scene, required: true, type: { entity: cutsceneId } }\n",
    "    effects: { raises: [{ occasion: finale, target: \"@scene\" }] }\n",
    "  - name: showLost\n",
    "    effects: { raises: [{ occasion: cutscene, target: scene.lost }] }\n",
    "  - name: showTwo\n",
    "    effects: { raises: [{ occasion: cutscene, target: scene.chapter2 }] }\n",
    "  - name: ring\n",
    "    attrs:\n",
    "      - { name: times, type: int, default: 7 }\n",
    "    effects: { raises: [{ occasion: bell, target: \"@times\" }] }\n",
);

const KINDS: &str = "rewardKinds:\n  CUTSCENE: { target: { entity: cutsceneId, required: true }, \
                     effects: { raises: [{ occasion: cutscene, target: \"@target\" }] } }\n";

const SCHEMA: &str = "entities:\n  cutsceneId: { members: [intro, chapter2, chapter3, ending, \
                      epilogue] }\nstate:\n  run.flag: { type: bool, default: false }\n";

/// The scene every raise is checked against: `chapter2` answers `cutscene`
/// for `scene.chapter2`, `ending` answers `finale` for `scene.ending`.
const ANSWERERS: &[(&str, &str)] = &[
    (
        "scenes/chapter2.lute",
        "---\nkind: scene\nid: chapter2\non: cutscene\ntarget: scene.chapter2\n---\n\n## Shot 1.\n\n@narrator: Two.\n",
    ),
    (
        "scenes/ending.lute",
        "---\nkind: scene\nid: ending\non: finale\ntarget: scene.ending\n---\n\n## Shot 1.\n\n@narrator: End.\n",
    ),
];

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// A project with the plugin above, the answering scenes, and `intro` (the
/// body of `scenes/intro.lute`, after `frontmatter`), plus `extra` files.
fn project(
    tag: &str,
    frontmatter: &str,
    intro: &str,
    extra: &[(&str, &str)],
) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.stage/occasions/o.yaml", OCCASIONS);
    write(&dir, "plugins/acme.stage/directives/d.yaml", DIRECTIVES);
    write(&dir, "plugins/acme.stage/rewardkinds/r.yaml", KINDS);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    for (rel, text) in ANSWERERS.iter().chain(extra) {
        write(&dir, rel, text);
    }
    write(
        &dir,
        "scenes/intro.lute",
        &format!("---\nkind: scene\nid: intro\n{frontmatter}---\n\n## Shot 1.\n\n{intro}"),
    );
    dir
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    let out = lute(dir, &["check", "--project", "."]);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.lines()
        .filter_map(|l| {
            let l = l.strip_prefix("./")?;
            let (at, rest) = l.split_once(": ")?;
            let (_, d) = rest.split_once(' ')?;
            d.starts_with('[').then(|| format!("{at} {d}"))
        })
        .collect()
}

#[test]
fn an_answered_target_is_clean() {
    let dir = project(
        "raises-ok",
        "",
        "::playScene{scene=\"chapter2\"}\n::showTwo\n::playFinale{scene=\"ending\"}\n",
        &[],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
}

#[test]
fn a_target_no_scene_answers_is_refused_with_a_did_you_mean() {
    let dir = project(
        "raises-unanswered",
        "",
        "::playScene{scene=\"chapter3\"}\n",
        &[],
    );
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:8:20 [E-RAISES-TARGET] `::playScene{scene=\"chapter3\"}` raises \
             `cutscene` for `scene.chapter3`, but no scene or beat answers it — did you mean \
             `scene.chapter2`?"
                .to_string()
        ]
    );
}

/// Review051b F6: an omitted attribute's declared `default:` is a static
/// target, judged like an authored one.
#[test]
fn a_defaulted_target_is_judged_like_an_authored_one() {
    let dir = project("raises-default", "", "::replay\n::rerun\n", &[]);
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:9:1 [E-RAISES-TARGET] `::rerun` raises `cutscene` for \
             `scene.chapter3` (the attribute's declared `default:`), but no scene or beat \
             answers it — did you mean `scene.chapter2`?"
                .to_string()
        ]
    );
}

/// Review051b F7: an `@def` value folds to its constant (a static target).
#[test]
fn a_def_value_folds_to_a_static_target() {
    let dir = project(
        "raises-def",
        "",
        "::playScene{scene=@nextScene}\n::playScene{scene=@lateScene}\n",
        &[(
            "schema/world.schema.yaml",
            &format!("{SCHEMA}defs:\n  lateScene: \"'chapter3'\"\n  nextScene: \"'chapter2'\"\n"),
        )],
    );
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:9:19 [E-RAISES-TARGET] `::playScene{scene=@lateScene}` raises \
             `cutscene` for `scene.chapter3`, but no scene or beat answers it — did you mean \
             `scene.chapter2`?"
                .to_string()
        ]
    );
}

/// An attribute with no value and no `default:` raises nothing: no
/// diagnostic beyond what the call already gets.
#[test]
fn an_unbound_target_raises_nothing() {
    let dir = project("raises-unbound", "", "::cue\n", &[]);
    assert_eq!(check(&dir), Vec::<String>::new());
}

/// `occasion.target` is the one dynamic target: the runtime binds it, so
/// the check does not judge it.
#[test]
fn an_occasion_target_value_is_not_judged() {
    let dir = project(
        "raises-dynamic",
        "",
        "@narrator: Hi.\n",
        &[(
            "scenes/visit.lute",
            "---\nkind: scene\nid: visitAny\non: visit\ntarget: \"kind:cutsceneId\"\n---\n\n## \
             Shot 1.\n\n::playScene{scene=occasion.target}\n",
        )],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
}

/// A value inside the kind but outside the occasion's `members:`, and a
/// literal outside the kind.
#[test]
fn a_target_outside_the_domain_is_refused() {
    let dir = project(
        "raises-domain",
        "",
        "::playFinale{scene=\"epilogue\"}\n::showLost\n",
        &[],
    );
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:8:21 [E-RAISES-TARGET] `::playFinale{scene=\"epilogue\"}` raises \
             `finale` for `scene.epilogue`, but `finale` is raised only for `scene.ending` (its \
             `members:`)"
                .to_string(),
            "scenes/intro.lute:9:1 [E-RAISES-TARGET] `::showLost` raises `cutscene` for \
             `scene.lost`, but `lost` is no `cutsceneId` member"
                .to_string(),
        ]
    );
}

/// A call in a component body is judged at each `::use`, with that use's
/// arguments bound.
#[test]
fn a_component_call_is_judged_at_each_use() {
    let dir = project(
        "raises-component",
        "components: [../components/stage.component.lute]\n",
        "::use{component=\"stage\" which=\"chapter2\"}\n::use{component=\"stage\" which=\"chapter3\"}\n",
        &[(
            "components/stage.component.lute",
            "---\ncomponent: stage\nparams:\n  which: { entity: cutsceneId }\n---\n\n## Shot\n\n\
             ::playScene{scene=@which}\n",
        )],
    );
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:10:1 [E-RAISES-TARGET] `::playScene{scene=\"chapter3\"}` (in \
             component `stage`, at this `::use`) raises `cutscene` for `scene.chapter3`, but no scene or \
             beat answers it — did you mean `scene.chapter2`?"
                .to_string()
        ]
    );
}

/// A reward's raise binds its `target=`; a lore entry's call is a raise too.
#[test]
fn rewards_and_lore_entries_are_judged() {
    let dir = project(
        "raises-reward",
        "",
        "@narrator: Hi.\n",
        &[
            (
                "quests/q.lute",
                "---\nkind: quest\nid: acme.quests\n---\n\n<quest id=\"first\" tier=\"run\" \
                 title=\"First\" start=\"true\">\n  <objective id=\"c1\" title=\"Do it\" \
                 done=\"run.flag\"/>\n  <reward id=\"r1\" kind=\"CUTSCENE\" amount=\"1\" \
                 target=\"chapter3\"/>\n  <reward id=\"r2\" kind=\"CUTSCENE\" amount=\"1\" \
                 target=\"chapter2\"/>\n</quest>\n",
            ),
            (
                "lore/codex.lute",
                "---\nkind: lore\nid: codex\n---\n\n<entry id=\"e1\" title=\"One\">\n  \
                 ::playScene{scene=\"chapter3\"}\n</entry>\n",
            ),
        ],
    );
    assert_eq!(
        check(&dir),
        vec![
            "lore/codex.lute:7:22 [E-RAISES-TARGET] `::playScene{scene=\"chapter3\"}` raises \
             `cutscene` for `scene.chapter3`, but no scene or beat answers it — did you mean \
             `scene.chapter2`?"
                .to_string(),
            "quests/q.lute:8:54 [E-RAISES-TARGET] `<reward kind=\"CUTSCENE\" target=\"chapter3\">` \
             raises `cutscene` for `scene.chapter3`, but no scene or beat answers it — did you \
             mean `scene.chapter2`?"
                .to_string(),
        ]
    );
}

/// Review fix: two documents with a raise at the same offsets are two
/// sites (the splice dedupe is per document).
#[test]
fn equal_layout_documents_each_get_their_diagnostic() {
    let dir = project(
        "raises-two-docs",
        "",
        "::playScene{scene=\"chapter3\"}\n",
        &[(
            "scenes/outro.lute",
            "---\nkind: scene\nid: outro\n---\n\n## Shot 1.\n\n::playScene{scene=\"chapter3\"}\n",
        )],
    );
    let message = "[E-RAISES-TARGET] `::playScene{scene=\"chapter3\"}` raises `cutscene` for \
                   `scene.chapter3`, but no scene or beat answers it — did you mean \
                   `scene.chapter2`?";
    assert_eq!(
        check(&dir),
        vec![
            format!("scenes/intro.lute:8:20 {message}"),
            format!("scenes/outro.lute:8:20 {message}"),
        ]
    );
}

/// Review fix: a component's raises come from the body its expansion
/// builds — a param-scoped `<match>` the arguments decide keeps only the
/// selected arm.
#[test]
fn a_component_raise_in_an_unselected_arm_is_not_counted() {
    let dir = project(
        "raises-component-match",
        "components: [../components/pick.component.lute]\n",
        "::use{component=\"pick\" which=\"chapter2\"}\n",
        &[(
            "components/pick.component.lute",
            "---\ncomponent: pick\nparams:\n  which: { entity: cutsceneId }\n---\n\n## Shot\n\n\
             <match subject=\"@which\">\n<when is=\"chapter2\">\n::playScene{scene=\"chapter2\"}\n\
             </when>\n<otherwise>\n::playScene{scene=\"chapter3\"}\n</otherwise>\n</match>\n",
        )],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
}

/// Review fix: a non-string `default:` is the target too, spelled as an
/// authored value (`7`), not an unbound one.
#[test]
fn a_typed_default_is_a_static_target() {
    let dir = project("raises-typed-default", "", "::ring\n", &[]);
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:8:1 [E-RAISES-TARGET] `::ring` raises `bell` for `7` (the \
             attribute's declared `default:`), but no scene or beat answers it"
                .to_string()
        ]
    );
}
