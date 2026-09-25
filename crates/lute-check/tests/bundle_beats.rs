//! dsl 0.23.0 §4 beat bundles: a lore document's `<beat>` blocks are checked
//! like scene beats — shape (`E-BEAT-ATTR`), scene-body admission, the beat
//! `when` rules, reachability, the project beat passes, and `visited()`
//! resolution of the canonical `<document id>.<beat id>`.

use std::path::PathBuf;

use lute_check::connectivity::{quest_id_set, resolve_nodes, scene_key_set};
use lute_check::{
    check, check_project_beats, fold_env, CheckInput, CheckResult, Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "lore".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn run(text: &str) -> CheckResult {
    check(&input(text))
}

fn with_code(text: &str, code: &str) -> Vec<Diagnostic> {
    run(text)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == code)
        .collect()
}

const HDR: &str =
    "---\nkind: lore\nid: interviews\nstate:\n  run.trust: { type: number, default: 0 }\n---\n";

fn doc(body: &str) -> String {
    format!("{HDR}{body}")
}

const PORTER: &str =
    "<beat id=\"porter\" on=\"talk\" target=\"npc.porter\" when=\"run.trust >= 0\">\n\
     @porter: You again.\n\
     <branch id=\"talk\">\n\
     <choice id=\"ask\" label=\"Ask\">\n@porter: Nothing.\n::set{run.trust += 1}\n</choice>\n\
     <choice id=\"leave\" label=\"Leave\">\n@porter: Good.\n</choice>\n\
     </branch>\n\
     <hub id=\"look\" prompt=\"Where do you look?\">\n\
     <choice id=\"lamp\" label=\"The lamp\" exit>\n::bg{location=\"station\"}\n</choice>\n\
     </hub>\n\
     </beat>\n";

#[test]
fn a_bundle_with_a_scene_body_checks_clean() {
    let r = run(&doc(PORTER));
    assert!(r.ok, "{:#?}", r.diagnostics);
}

#[test]
fn beat_shape_faults_are_beat_attr() {
    for (open, needle) in [
        ("<beat on=\"talk\">", "no `id`"),
        ("<beat id=\"a-b\" on=\"talk\">", "without `-`"),
        ("<beat id=\"a\">", "names no occasion"),
        (
            "<beat id=\"a\" on=\"talk\" priority=\"high\">",
            "must be an integer",
        ),
        (
            "<beat id=\"a\" on=\"talk\" once=\"never\">",
            "`once=\"never\"`",
        ),
        ("<beat id=\"a\" on=\"talk\" also=\"maybe\">", "is a flag"),
        ("<beat id=\"a\" on=\"talk\" target=\"npc..x\">", "malformed"),
    ] {
        let ds = with_code(&doc(&format!("{open}\n@n: hi\n</beat>\n")), "E-BEAT-ATTR");
        assert!(
            ds.iter().any(|d| d.message.contains(needle)),
            "{open}: {ds:#?}"
        );
    }
    let ds = with_code(
        "---\nkind: lore\n---\n<beat id=\"a\" on=\"talk\">\n@n: hi\n</beat>\n",
        "E-BEAT-ATTR",
    );
    assert!(
        ds.iter().any(|d| d.message.contains("document `id:`")),
        "{ds:#?}"
    );
    let dup = doc("<beat id=\"a\" on=\"talk\">\n@n: 1\n</beat>\n<beat id=\"a\" on=\"talk\">\n@n: 2\n</beat>\n");
    assert!(with_code(&dup, "E-BEAT-ATTR")
        .iter()
        .any(|d| d.message.contains("duplicate")));
    let unknown = doc("<beat id=\"a\" on=\"talk\" series=\"s\">\n@n: hi\n</beat>\n");
    assert_eq!(with_code(&unknown, "E-UNKNOWN-ATTR").len(), 1);
}

#[test]
fn beat_body_is_a_scene_body_and_beats_live_only_in_lore() {
    // A body construct only a quest admits is rejected; scene constructs pass.
    let on =
        doc("<beat id=\"a\" on=\"talk\">\n<on event=\"questStarted\">\n@n: x\n</on>\n</beat>\n");
    assert_eq!(with_code(&on, "E-GRAMMAR-NOT-ADMITTED").len(), 1);
    let scene = "---\nkind: scene\nid: s\n---\n<beat id=\"a\" on=\"talk\">\n@n: x\n</beat>\n## Shot 1.\n@n: y\n";
    assert!(with_code(scene, "E-GRAMMAR-NOT-ADMITTED")
        .iter()
        .any(|d| d.message.contains("<beat id=\"a\">")));
}

#[test]
fn beat_when_follows_the_scene_beat_rules() {
    let dead = doc(
        "<beat id=\"a\" on=\"talk\" when=\"run.trust > 5 && run.trust < 3\">\n@n: hi\n</beat>\n",
    );
    let ds = with_code(&dead, "E-BEAT-UNREACHABLE");
    assert_eq!(ds.len(), 1, "{:#?}", run(&dead).diagnostics);
    assert!(ds[0].message.contains("interviews.a"), "{}", ds[0].message);
    // A numeric disjunction whose gap holds the conjunction's only value.
    let gap = doc("<beat id=\"a\" on=\"talk\" when=\"run.trust == 3 && (run.trust > 3 || run.trust < 3)\">\n@n: hi\n</beat>\n");
    assert_eq!(
        with_code(&gap, "E-BEAT-UNREACHABLE").len(),
        1,
        "{:#?}",
        run(&gap).diagnostics
    );
    let scene_read = doc("<beat id=\"a\" on=\"talk\" when=\"scene.x\">\n@n: hi\n</beat>\n");
    assert!(!with_code(&scene_read, "E-BEAT-ATTR").is_empty());
    let undeclared = doc("<beat id=\"a\" on=\"talk\" when=\"run.ghost\">\n@n: hi\n</beat>\n");
    assert!(!with_code(&undeclared, "E-UNDECLARED").is_empty());
}

fn project(files: &[(&str, &str)]) -> Vec<(PathBuf, lute_syntax::ast::Document)> {
    files
        .iter()
        .map(|(p, t)| (PathBuf::from(p), lute_syntax::parse(t).0))
        .collect()
}

#[test]
fn an_always_eligible_repeatable_beat_shadows_a_later_one() {
    let text = doc(
        "<beat id=\"first\" on=\"talk\" once=\"false\">\n@n: a\n</beat>\n\
         <beat id=\"second\" on=\"talk\">\n@n: b\n</beat>\n",
    );
    let docs = project(&[("interviews.lute", &text)]);
    let folded = fold_env(&docs[0].1, &input(&text)).0;
    let out = check_project_beats(&docs, &[&folded]);
    let shadowed: Vec<_> = out
        .iter()
        .filter(|(_, d)| d.code == "W-BEAT-SHADOWED")
        .collect();
    assert_eq!(shadowed.len(), 1, "{out:#?}");
    assert!(
        shadowed[0].1.message.contains("beat `interviews.second`"),
        "{}",
        shadowed[0].1.message
    );
}

/// dsl 0.24.0 §2: a bundle beat is a graph node, so `visited()` resolves it
/// in an `after:` exactly as in a condition slot.
#[test]
fn visited_resolves_a_bundle_beat_in_a_condition_and_in_after() {
    let lore = doc(PORTER);
    let when = "---\nkind: scene\nid: next\non: talk\nwhen: \"visited('interviews.porter')\"\n---\n## Shot 1.\n@n: hi\n";
    let after = "---\nkind: scene\nid: later\nafter: \"visited('interviews.porter')\"\n---\n## Shot 1.\n@n: hi\n";
    let typo = "---\nkind: scene\nid: typo\non: talk\nwhen: \"visited('interviews.portr')\"\n---\n## Shot 1.\n@n: hi\n";
    let docs = project(&[
        ("i.lute", &lore),
        ("n.lute", when),
        ("l.lute", after),
        ("t.lute", typo),
    ]);
    let out = resolve_nodes(&docs, &scene_key_set(&docs), &quest_id_set(&docs));
    let at = |file: &str| -> Vec<&Diagnostic> {
        out.iter()
            .filter(|(p, _)| p == &PathBuf::from(file))
            .map(|(_, d)| d)
            .collect()
    };
    assert!(at("n.lute").is_empty(), "{out:#?}");
    assert!(at("l.lute").is_empty(), "{out:#?}");
    assert!(
        at("t.lute")
            .iter()
            .any(|d| d.message.contains("did you mean `interviews.porter`")),
        "{out:#?}"
    );
}
