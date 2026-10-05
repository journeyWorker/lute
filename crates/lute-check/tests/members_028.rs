//! `occasion.target` in kind and `for=` beats: each beat reads it typed by
//! its own members, and a member its `when` can never hold for — with the
//! project's facts in scope — needs no `<match>` arm and keeps no beat alive.

use std::path::{Path, PathBuf};

use lute_check::connectivity::{
    ambiguous_quest_ids, assemble_graph, check_reachability, live_assert_sites, quest_id_set,
    scene_key_set, unreachable_quest_ids,
};
use lute_check::{
    check, check_fact_guards, compute_must, fold_env, stable_seeds, CheckInput, FactEnv, FoldedEnv,
    MaySet, Mode, ProjectDocs, RootVocab, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};


fn input(text: &str) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    for (name, entity) in [("talk", "villager"), ("landed", "fish"), ("visit", "place")] {
        snapshot.occasions.insert(
            name.into(),
            OccasionDecl {
                name: name.into(),
                select: OccasionSelect::First,
                target: OccasionTarget::Domain {
                    prefix: entity.into(),
                    entity: entity.into(),
                    members: None,
                },
                ..Default::default()
            },
        );
    }
    snapshot.occasions.insert(
        "hour".into(),
        OccasionDecl {
            name: "hour".into(),
            select: OccasionSelect::Sequence,
            ..Default::default()
        },
    );
    CheckInput {
        text: text.to_string(),
        uri: "members".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

/// `mara` is seeded on the deck and the roof, and nothing asserts `at` or
/// `found`: `at(mara, radio)` and every `found(…)` can never hold.
const VOCAB: &str = "entities:\n  villager: { members: [mara, tomas] }\n  \
                     fish: { members: [cod, eel] }\n  \
                     place: { members: [deck, roof, radio] }\n\
                     relations:\n  at: { args: [villager, place], tier: run }\n  \
                     found: { args: [villager], tier: run }\n\
                     facts:\n  - \"at(mara, deck)\"\n  - \"at(mara, roof)\"\n";

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: town\ntitle: Town\n{VOCAB}---\n{body}")
}

fn errors(text: &str) -> Vec<(String, String)> {
    check(&input(text))
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

/// One lore document analyzed the way `check-project` analyzes it: the
/// per-file report (reconciled with the facts) and the project pass's.
struct Project {
    per_file: Vec<Diagnostic>,
    project: Vec<Diagnostic>,
}

fn project(text: &str) -> Project {
    let path = PathBuf::from("lore/town.lute");
    let input = input(text);
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = fold_env(&doc, &input);
    let result = check(&input);
    let mut per_file = result.diagnostics.clone();
    let docs = ProjectDocs::parse(vec![(path.clone(), doc)], &input.snapshot);
    let views = docs.views();
    let results = vec![(path.clone(), result)];
    let key_set = scene_key_set(&views);
    let quest_ids = quest_id_set(&views);
    let (graph, _) = assemble_graph(&views, &key_set, &quest_ids);
    let lifecycle = unreachable_quest_ids(&views, &results);
    let ambiguous = ambiguous_quest_ids(&views);
    let (reach, _) = check_reachability(&graph, &quest_ids, &ambiguous, &lifecycle);
    let mut vocab = RootVocab::default();
    vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    vocab.note_unreadable_documents(&views);
    let facts = live_assert_sites(&views, &reach, &ambiguous, &lifecycle, &Default::default())
        .into_iter()
        .flat_map(|(_, a)| vocab.asserted_facts(&a));
    let may = MaySet::build(&vocab, facts, &stable_seeds(&views, &vocab));
    let foldeds: Vec<&FoldedEnv> = vec![&folded];
    let must = compute_must(&views, &foldeds, &graph, &vocab, &may);
    let env = FactEnv::new(may, must.slots);
    let doc = &docs.documents()[0].1;
    let project = check_fact_guards(Path::new(&path), doc, &folded, &env, &per_file);
    lute_check::fact_check::reconcile_member_matches(&mut per_file, &path, doc, &folded, &env);
    Project { per_file, project }
}

fn coded<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

#[test]
fn each_kind_beat_matches_only_its_own_members() {
    let errs = errors(&lore(
        "<beat id=\"nod\" on=\"talk\" target=\"kind:villager\" once=\"false\">\n  \
         @narrator: {{occasion.target}} nods.\n</beat>\n\n\
         <beat id=\"land\" on=\"landed\" target=\"kind:fish\" once=\"false\">\n  \
         <match on=\"occasion.target\">\n    <when is=\"cod\">\n      @narrator: A cod.\n    \
         </when>\n    <when is=\"eel\">\n      @narrator: An eel.\n    </when>\n  </match>\n\
         </beat>\n",
    ));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn a_member_of_another_kind_beat_is_outside_the_domain() {
    let errs = errors(&lore(
        "<beat id=\"nod\" on=\"talk\" target=\"kind:villager\" once=\"false\">\n  \
         @narrator: {{occasion.target}} nods.\n</beat>\n\n\
         <beat id=\"land\" on=\"landed\" target=\"kind:fish\" once=\"false\" \
         when=\"occasion.target == 'mara'\">\n  @narrator: A catch.\n</beat>\n",
    ));
    let domain: Vec<_> = errs
        .iter()
        .filter(|(c, _)| c == "E-WHEN-LITERAL-DOMAIN")
        .collect();
    assert_eq!(domain.len(), 1, "{errs:?}");
    assert!(
        domain[0].1.contains("[cod, eel]") && !domain[0].1.contains("tomas"),
        "{errs:?}"
    );
    assert!(
        !errs.iter().any(|(c, _)| c == "E-BEAT-UNREACHABLE"),
        "{errs:?}"
    );
}

#[test]
fn a_match_needs_no_arm_for_a_member_the_when_rules_out() {
    let errs = errors(&lore(
        "<beat id=\"land\" on=\"landed\" target=\"kind:fish\" once=\"false\" \
         when=\"occasion.target != 'eel'\">\n  <match on=\"occasion.target\">\n    \
         <when is=\"cod\">\n      @narrator: A cod.\n    </when>\n  </match>\n</beat>\n",
    ));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn a_match_needs_no_arm_for_a_member_the_facts_rule_out() {
    let body = |arms: &str| {
        lore(&format!(
            "<beat id=\"warm\" on=\"visit\" target=\"kind:place\" once=\"false\" \
             when=\"holds('at', ['mara', occasion.target])\">\n  <match on=\"occasion.target\">\n\
             {arms}  </match>\n</beat>\n"
        ))
    };
    let arm = |m: &str| format!("    <when is=\"{m}\">\n      @narrator: {m}.\n    </when>\n");
    // `radio` is never where `mara` is: its arm is not needed…
    let p = project(&body(&(arm("deck") + &arm("roof"))));
    assert!(
        coded(&p.per_file, "E-NONEXHAUSTIVE").is_empty(),
        "{:?}",
        p.per_file
    );
    assert!(p.project.is_empty(), "{:?}", p.project);
    // …and an arm for it can never fire.
    let p = project(&body(&(arm("deck") + &arm("roof") + &arm("radio"))));
    let dead = coded(&p.project, "E-ARM-DEAD");
    assert_eq!(dead.len(), 1, "{:?}", p.project);
    assert!(dead[0].message.contains("`radio`"), "{:?}", dead[0]);
    // A member the facts leave possible still needs its arm.
    let p = project(&body(&arm("deck")));
    let open = coded(&p.per_file, "E-NONEXHAUSTIVE");
    assert_eq!(open.len(), 1, "{:?}", p.per_file);
    assert!(
        open[0].message.contains("`roof`") && !open[0].message.contains("radio"),
        "{:?}",
        open[0]
    );
}

#[test]
fn a_for_beat_whose_when_holds_for_no_member_is_unreachable() {
    let p = project(&lore(
        "<beat id=\"lost\" on=\"hour\" for=\"kind:villager\" once=\"false\" \
         when=\"holds('found', [occasion.target])\">\n  @narrator: {{occasion.target}} is found.\n\
         </beat>\n",
    ));
    let dead = coded(&p.project, "E-BEAT-UNREACHABLE");
    assert_eq!(dead.len(), 1, "{:?}", p.project);
    let m = &dead[0].message;
    assert!(
        m.contains("town.lost") && m.contains("`mara`, `tomas`") && !m.contains("terminal"),
        "{m}"
    );
}
