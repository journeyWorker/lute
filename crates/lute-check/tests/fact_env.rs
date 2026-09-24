//! dsl 0.20.0 fact envelopes: the project-wide may set (§3), the must set
//! (§4), the verdict plumbing into `decide()` (§5), and the relational guard
//! pass (`check_fact_guards`) — run through the same pipeline `lute-cli`'s
//! `check-project` runs (per-file `check()` -> reachability ->
//! `live_assert_sites` -> `MaySet` -> `compute_must` -> `check_fact_guards`).

use std::path::{Path, PathBuf};

use lute_check::connectivity::{
    ambiguous_quest_ids, assemble_graph, check_reachability, live_assert_sites, quest_id_set,
    scene_key_set, unreachable_quest_ids,
};
use lute_check::fact_env::{MustFact, Provenance};
use lute_check::{
    check, check_fact_guards, compute_must, fold_env, CheckInput, FactEnv, FoldedEnv, GroundFact,
    MaySet, Mode, MustMap, RootVocab, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_syntax::ast::{Document, Node};

fn input_for(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

/// The inline vocabulary every fixture document declares: `found` is
/// declared and asserted nowhere unless a test asserts it, `sealed` is
/// engine-populated, `can_halt` derives from `awake` + `knows`, `at` is keyed
/// on its first argument, and `near` is scene-tier.
const VOCAB: &str = "entities:\n  crew: { members: [vesna, toma, ilsabet] }\n  \
    topic: { members: [heading, manifest, shed_sequence] }\n  \
    place: { members: [bridge, hold] }\n\
    relations:\n  awake: { args: [crew], tier: run }\n  knows: { args: [crew, topic], tier: run }\n  \
    found: { args: [crew], tier: run }\n  sealed: { args: [crew], reserved: true }\n  \
    can_halt: { args: [crew], derive: true }\n  \
    at: { args: [crew, place], tier: run, key: [0] }\n  near: { args: [crew], tier: scene }\n\
    facts:\n  - \"awake(vesna)\"\n\
    rules:\n  - \"can_halt(C) :- awake(C), knows(C, shed_sequence)\"\n";

fn scene(episode: u32, body: &str) -> String {
    format!("---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: {episode}\n{VOCAB}---\n## Shot 1.\n{body}\n")
}

/// A scene whose `after:` is `after` (single-quoted YAML).
fn scene_after(episode: u32, after: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: {episode}\nafter: '{after}'\n\
         {VOCAB}---\n## Shot 1.\n{body}\n"
    )
}

/// The 1-based line of the first occurrence of `needle` in `text`.
fn line_of(text: &str, needle: &str) -> usize {
    let at = text.find(needle).expect("needle in fixture");
    text[..at].matches('\n').count() + 1
}

fn quest(body: &str) -> String {
    format!("---\nkind: quest\n{VOCAB}---\n{body}\n")
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\ntitle: Records\n{VOCAB}---\n{body}\n")
}

/// One resolved root, analyzed the way `check-project` analyzes it.
struct Root {
    docs: Vec<(PathBuf, Document)>,
    foldeds: Vec<FoldedEnv>,
    per_file: Vec<Vec<Diagnostic>>,
    env: FactEnv,
    scene_entry: std::collections::BTreeMap<String, Vec<MustFact>>,
}

fn root(texts: &[(&str, &str)]) -> Root {
    let mut docs = Vec::new();
    let mut foldeds = Vec::new();
    let mut per_file = Vec::new();
    let mut results = Vec::new();
    for (path, text) in texts {
        let input = input_for(text);
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, &input);
        let result = check(&input);
        per_file.push(result.diagnostics.clone());
        results.push((PathBuf::from(path), result));
        foldeds.push(folded);
        docs.push((PathBuf::from(path), doc));
    }
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (graph, _) = assemble_graph(&docs, &key_set, &quest_ids);
    let lifecycle = unreachable_quest_ids(&docs, &results);
    let ambiguous = ambiguous_quest_ids(&docs);
    let (reach, _) = check_reachability(&graph, &quest_ids, &ambiguous, &lifecycle);
    let mut vocab = RootVocab::default();
    for folded in &foldeds {
        vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    }
    let facts = live_assert_sites(&docs, &reach, &ambiguous, &lifecycle)
        .into_iter()
        .filter_map(|(_, a)| GroundFact::from_pattern(&a.pattern));
    let may = MaySet::build(&vocab, facts);
    let folded_refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    let must = compute_must(&docs, &folded_refs, &graph, &vocab, &may);
    let scene_entry = must.scene_entry;
    let env = FactEnv::new(may, must.slots);
    Root {
        docs,
        foldeds,
        per_file,
        env,
        scene_entry,
    }
}

impl Root {
    /// The project pass's diagnostics for the document at `path`.
    fn guards(&self, path: &str) -> Vec<Diagnostic> {
        let i = self.index(path);
        check_fact_guards(
            &self.docs[i].0,
            &self.docs[i].1,
            &self.foldeds[i],
            &self.env,
            &self.per_file[i],
        )
    }

    fn index(&self, path: &str) -> usize {
        self.docs
            .iter()
            .position(|(p, _)| p == Path::new(path))
            .expect("fixture path")
    }
}

/// A silent verdict must come from the analysis, not from a query the
/// documents never managed to declare.
fn assert_vocab_clean(r: &Root) {
    for ds in &r.per_file {
        assert!(
            !ds.iter()
                .any(|d| d.code.starts_with("E-RELATION") || d.code == "E-FACT-DOMAIN"),
            "fixture vocabulary must resolve: {ds:?}"
        );
    }
}

fn codes(ds: &[Diagnostic]) -> Vec<&str> {
    ds.iter().map(|d| d.code.as_str()).collect()
}

fn only<'a>(ds: &'a [Diagnostic], code: &str) -> &'a Diagnostic {
    let hits: Vec<&Diagnostic> = ds.iter().filter(|d| d.code == code).collect();
    assert_eq!(hits.len(), 1, "exactly one {code}: {ds:?}");
    hits[0]
}

// --- the verified gap: a line guard over a fact nobody asserts --------------

#[test]
fn line_guard_over_never_asserted_fact_is_arm_dead() {
    let body = "::assert{knows(toma, manifest)}\n\
                @vesna{when=\"holds(knows(toma, heading))\"}: So you read the log.";
    let text = scene(1, body);
    let r = root(&[("a.lute", &text)]);
    let ds = r.guards("a.lute");
    let d = only(&ds, "E-ARM-DEAD");
    assert_eq!(d.severity, Severity::Error);
    assert!(
        d.message.contains(
            "no seed, assert, rule, or engine relation produces `knows(toma, heading)`"
        ),
        "{}",
        d.message
    );
    assert!(d.message.contains("under your declared routes"), "{}", d.message);
    assert_eq!(&text[d.span.byte_start..d.span.byte_end], "holds(knows(toma, heading))");
}

#[test]
fn line_guard_over_relation_asserted_nowhere_is_arm_dead() {
    let r = root(&[(
        "a.lute",
        &scene(1, "@vesna{when=\"holds(found(toma))\"}: You found him."),
    )]);
    assert_eq!(codes(&r.guards("a.lute")), ["E-ARM-DEAD"]);
}

#[test]
fn single_file_check_stays_undecided_for_relational_guards() {
    let text = scene(
        1,
        "@vesna{when=\"holds(knows(toma, heading))\"}: a.\n\
         @vesna{when=\"count(knows(_, _)) >= 5\"}: b.",
    );
    let ds = check(&input_for(&text)).diagnostics;
    assert!(
        !ds.iter()
            .any(|d| d.code == "E-ARM-DEAD"),
        "single-file check has no fact envelope: {ds:?}"
    );
}

// --- producers: asserts anywhere in the root, lore entries, seeds, rules ----

#[test]
fn fact_asserted_in_a_sibling_scene_is_silent() {
    let r = root(&[
        ("a.lute", &scene(1, "::assert{knows(vesna, manifest)}\n@vesna: noted.")),
        (
            "b.lute",
            &scene(2, "@vesna{when=\"holds(knows(vesna, manifest))\"}: Two pods."),
        ),
    ]);
    assert_vocab_clean(&r);
    assert!(r.guards("b.lute").is_empty(), "{:?}", r.guards("b.lute"));
}

#[test]
fn lore_entry_asserts_are_producers() {
    let r = root(&[
        (
            "notes.lute",
            &lore("<entry id=\"ledger\">\n  @vesna: The heading was a lie.\n  ::assert{knows(toma, heading)}\n</entry>"),
        ),
        (
            "a.lute",
            &scene(1, "@vesna{when=\"holds(knows(toma, heading))\"}: So you read the log."),
        ),
    ]);
    assert_vocab_clean(&r);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn unretracted_seed_is_guaranteed_and_a_retracted_one_is_possible() {
    let guard = scene(1, "@vesna{when=\"holds(awake(vesna))\"}: Awake.");
    let r = root(&[("a.lute", &guard)]);
    assert_vocab_clean(&r);
    let ds = r.guards("a.lute");
    let w = only(&ds, "W-FACT-GUARANTEED");
    assert!(
        w.message.contains("`awake(vesna)` is a `facts:` seed that nothing retracts"),
        "{}",
        w.message
    );
    let r = root(&[
        ("a.lute", &guard),
        ("b.lute", &scene(2, "::retract{awake(vesna)}\n@vesna: Asleep.")),
    ]);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn reserved_relation_queries_stay_silent() {
    let r = root(&[(
        "a.lute",
        &scene(
            1,
            "@vesna{when=\"holds(sealed(toma))\"}: a.\n\
             @vesna{when=\"count(sealed(_)) >= 40\"}: b.",
        ),
    )]);
    assert_vocab_clean(&r);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn derived_relation_through_rule_over_asserted_facts_is_silent() {
    let r = root(&[
        (
            "a.lute",
            &scene(1, "::assert{awake(toma)}\n::assert{knows(toma, shed_sequence)}\n@toma: ready."),
        ),
        (
            "b.lute",
            &scene(2, "@vesna{when=\"holds(can_halt(toma))\"}: Toma can halt it."),
        ),
    ]);
    assert_vocab_clean(&r);
    assert!(r.guards("b.lute").is_empty(), "{:?}", r.guards("b.lute"));
}

#[test]
fn derived_relation_over_never_asserted_facts_is_dead() {
    // `awake(toma)` is asserted, `knows(toma, shed_sequence)` never is — and
    // `ilsabet` knows it but is never awake: no binding satisfies the rule.
    let r = root(&[
        (
            "a.lute",
            &scene(1, "::assert{awake(toma)}\n::assert{knows(ilsabet, shed_sequence)}\n@toma: hi."),
        ),
        (
            "b.lute",
            &scene(
                2,
                "@vesna{when=\"holds(can_halt(toma))\"}: a.\n\
                 @vesna{when=\"holds(can_halt(ilsabet))\"}: b.",
            ),
        ),
    ]);
    let ds = r.guards("b.lute");
    assert_eq!(codes(&ds), ["E-ARM-DEAD", "E-ARM-DEAD"], "{ds:?}");
    assert!(ds[0].message.contains("`can_halt(toma)`"), "{}", ds[0].message);
}

#[test]
fn assert_in_a_provably_unreachable_scene_is_not_a_producer() {
    // `b` is gated on a quest whose `start` is literally false, so its
    // assert never runs; `c`'s guard over that fact is dead.
    let r = root(&[
        (
            "q.lute",
            &quest("<quest id=\"never\" start=\"false\">\n<objective id=\"o\" done=\"true\"/>\n</quest>"),
        ),
        (
            "b.lute",
            &format!(
                "---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: 2\n\
                 after: 'completed(\"never\")'\n{VOCAB}---\n## Shot 1.\n\
                 ::assert{{found(toma)}}\n@vesna: found.\n"
            ),
        ),
        ("c.lute", &scene(3, "@vesna{when=\"holds(found(toma))\"}: hi.")),
    ]);
    assert_eq!(codes(&r.guards("c.lute")), ["E-ARM-DEAD"]);
}

// --- composition through decide() -------------------------------------------

#[test]
fn negated_impossible_fact_decides_true_not_dead() {
    let r = root(&[(
        "a.lute",
        &scene(1, "@vesna{when=\"!holds(knows(toma, heading))\"}: You don't know yet."),
    )]);
    assert_vocab_clean(&r);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn count_above_the_may_upper_bound_is_dead() {
    let r = root(&[
        (
            "a.lute",
            &scene(1, "::assert{knows(vesna, manifest)}\n::assert{knows(toma, heading)}\n@vesna: hi."),
        ),
        (
            "b.lute",
            &scene(
                2,
                "@vesna{when=\"count(knows(_, _)) >= 5\"}: dead.\n\
                 @vesna{when=\"count(knows(_, _)) >= 2\"}: possible.\n\
                 @vesna{when=\"3 <= count(knows(_, _))\"}: dead, flipped.",
            ),
        ),
    ]);
    let ds = r.guards("b.lute");
    assert_eq!(codes(&ds), ["E-ARM-DEAD", "E-ARM-DEAD"], "{ds:?}");
    assert!(
        ds[0].message.contains("`count(knows(_, _))` is at most 2"),
        "{}",
        ds[0].message
    );
}

#[test]
fn or_with_one_possible_arm_is_not_dead() {
    let r = root(&[
        ("a.lute", &scene(1, "::assert{knows(vesna, manifest)}\n@vesna: hi.")),
        (
            "b.lute",
            &scene(
                2,
                "@vesna{when=\"holds(found(toma)) || holds(knows(vesna, manifest))\"}: a.\n\
                 @vesna{when=\"holds(found(toma)) && holds(knows(vesna, manifest))\"}: b.",
            ),
        ),
    ]);
    assert_eq!(codes(&r.guards("b.lute")), ["E-ARM-DEAD"]);
}

// --- each slot reports through its own code ---------------------------------

#[test]
fn choice_when_over_impossible_fact_is_arm_dead() {
    let text = scene(
        1,
        "<branch id=\"b\">\n<choice id=\"ask\" label=\"Ask\" when=\"holds(knows(toma, heading))\">\n\
         @toma: fine.\n</choice>\n<choice id=\"go\" label=\"Go\">\n@toma: bye.\n</choice>\n</branch>",
    );
    let r = root(&[("a.lute", &text)]);
    let ds = r.guards("a.lute");
    let d = only(&ds, "E-ARM-DEAD");
    assert!(d.message.starts_with("choice can never fire"), "{}", d.message);
}

#[test]
fn when_test_arm_over_impossible_fact_is_arm_dead() {
    let text = scene(
        1,
        "<match on=\"run.mood\">\n<when test=\"holds(found(toma))\">\n@toma: here.\n</when>\n\
         <otherwise>\n@toma: gone.\n</otherwise>\n</match>",
    )
    .replace("---\n## Shot", "state:\n  run.mood: { type: number, default: 0 }\n---\n## Shot");
    let r = root(&[("a.lute", &text)]);
    assert_eq!(codes(&r.guards("a.lute")), ["E-ARM-DEAD"], "{:?}", r.guards("a.lute"));
}

#[test]
fn objective_done_over_impossible_fact_is_unsatisfiable_once() {
    let text = quest(
        "<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"holds(knows(toma, heading))\"/>\n</quest>",
    );
    let r = root(&[("q.lute", &text)]);
    let ds = r.guards("q.lute");
    let d = only(&ds, "E-OBJECTIVE-UNSATISFIABLE");
    assert!(d.message.contains("`knows(toma, heading)`"), "{}", d.message);
    assert!(
        !r.per_file[r.index("q.lute")]
            .iter()
            .any(|d| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "the per-file pass cannot decide it, so it is reported exactly once"
    );
    let dead = lute_check::fact_check::dead_required_objective_quests(
        &r.docs[0].0,
        &r.docs[0].1,
        &r.foldeds[0],
        &r.env,
        &Default::default(),
    );
    assert!(dead.contains("q"), "connectivity sees the same verdict: {dead:?}");
}

#[test]
fn scalar_dead_objective_is_left_to_the_per_file_pass() {
    let r = root(&[(
        "q.lute",
        &quest("<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"false && holds(found(toma))\"/>\n</quest>"),
    )]);
    assert!(r.guards("q.lute").is_empty(), "{:?}", r.guards("q.lute"));
    assert!(r.per_file[0]
        .iter()
        .any(|d| d.code == "E-OBJECTIVE-UNSATISFIABLE"));
}

#[test]
fn quest_start_over_impossible_fact_is_unreachable() {
    let r = root(&[(
        "q.lute",
        &quest("<quest id=\"q\" start=\"holds(found(toma))\">\n<objective id=\"o\" done=\"true\"/>\n</quest>"),
    )]);
    let ds = r.guards("q.lute");
    let d = only(&ds, "E-QUEST-UNREACHABLE");
    assert_eq!(d.span, r.docs[0].1.quests[0].span, "anchored at the quest");
    let dead = lute_check::fact_check::dead_lifecycle_quests(
        &r.docs[0].0,
        &r.docs[0].1,
        &r.foldeds[0],
        &r.env,
        &Default::default(),
    );
    assert!(dead.contains("q"));
}

#[test]
fn quest_fail_over_impossible_fact_is_silent() {
    // A `fail` that can never hold means the quest can never FAIL — not a
    // defect (dsl 0.10.0 D-N).
    let r = root(&[(
        "q.lute",
        &quest("<quest id=\"q\" start=\"true\" fail=\"holds(found(toma))\">\n<objective id=\"o\" done=\"true\"/>\n</quest>"),
    )]);
    assert!(r.guards("q.lute").is_empty(), "{:?}", r.guards("q.lute"));
}

#[test]
fn entry_when_over_impossible_fact_is_entry_unreachable() {
    let r = root(&[(
        "notes.lute",
        &lore("<entry id=\"bark\" when=\"holds(found(toma))\">\n  @vesna: Found him.\n</entry>"),
    )]);
    let ds = r.guards("notes.lute");
    let d = only(&ds, "E-ENTRY-UNREACHABLE");
    assert!(d.message.starts_with("entry `bark` is never eligible"), "{}", d.message);
}

// --- never a cascade onto a slot another code owns ---------------------------

#[test]
fn query_outside_the_documents_vocabulary_is_never_decided() {
    // `b` declares no relations: its query is `E-RELATION-UNKNOWN`, and the
    // sibling's vocabulary must not decide it.
    let r = root(&[
        ("a.lute", &scene(1, "@vesna: hi.")),
        (
            "b.lute",
            "---\nkind: scene\ncharacter: haven\nseason: 1\nepisode: 2\n---\n## Shot 1.\n\
             @vesna{when=\"holds(found(toma))\"}: hi.\n",
        ),
    ]);
    assert!(r.guards("b.lute").is_empty(), "{:?}", r.guards("b.lute"));
}

#[test]
fn non_member_argument_is_left_to_fact_domain() {
    let r = root(&[(
        "a.lute",
        &scene(1, "@vesna{when=\"holds(knows(toma, heding))\"}: typo."),
    )]);
    assert!(r.per_file[0].iter().any(|d| d.code == "E-FACT-DOMAIN"));
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

// --- guaranteed verdicts (must set supplied directly) ------------------------

/// The `when` slot of the `n`-th top-level line of the first shot.
fn line_when(doc: &Document, n: usize) -> lute_core_span::Span {
    let lines: Vec<_> = doc.shots[0]
        .body
        .iter()
        .filter_map(|node| match node {
            Node::Line(l) => l.when.as_ref(),
            _ => None,
        })
        .collect();
    lines[n].span
}

#[test]
fn guaranteed_fact_in_a_line_guard_is_fact_guaranteed() {
    let text = scene(
        2,
        "@vesna{when=\"holds(knows(vesna, manifest))\"}: redundant.\n\
         @vesna{when=\"!holds(knows(vesna, manifest))\"}: dead.",
    );
    let mut r = root(&[
        ("a.lute", &scene(1, "::assert{knows(vesna, manifest)}\n@vesna: hi.")),
        ("b.lute", &text),
    ]);
    let i = r.index("b.lute");
    let fact = MustFact {
        fact: GroundFact {
            relation: "knows".into(),
            args: vec!["vesna".into(), "manifest".into()],
        },
        provenance: Provenance::Assert {
            path: PathBuf::from("scenes/archive.lute"),
            line: 21,
        },
    };
    let mut must = MustMap::default();
    for n in 0..2 {
        must.insert(&r.docs[i].0, line_when(&r.docs[i].1, n), [fact.clone()]);
    }
    r.env.must = must;
    let ds = r.guards("b.lute");
    let w = only(&ds, "W-FACT-GUARANTEED");
    assert_eq!(w.severity, Severity::Warning);
    assert!(
        w.message
            .contains("`knows(vesna, manifest)` is asserted on every route to here (scenes/archive.lute:21)"),
        "{}",
        w.message
    );
    let e = only(&ds, "E-ARM-DEAD");
    assert!(e.message.contains("scenes/archive.lute:21"), "{}", e.message);
}

#[test]
fn guaranteed_count_lower_bound_decides_comparisons() {
    let mut r = root(&[(
        "a.lute",
        &scene(
            1,
            "::assert{knows(vesna, manifest)}\n\
             @vesna{when=\"count(knows(_, _)) >= 1\"}: redundant.\n\
             @vesna{when=\"count(knows(_, _)) == 0\"}: dead.",
        ),
    )]);
    let fact = MustFact {
        fact: GroundFact {
            relation: "knows".into(),
            args: vec!["vesna".into(), "manifest".into()],
        },
        provenance: Provenance::Assert {
            path: PathBuf::from("a.lute"),
            line: 1,
        },
    };
    let mut must = MustMap::default();
    for n in 0..2 {
        must.insert(&r.docs[0].0, line_when(&r.docs[0].1, n), [fact.clone()]);
    }
    r.env.must = must;
    let ds = r.guards("a.lute");
    assert_eq!(codes(&ds), ["W-FACT-GUARANTEED", "E-ARM-DEAD"], "{ds:?}");
}

// --- the must set, computed (§4) ---------------------------------------------

const KNOWS: &str = "@vesna{when=\"holds(knows(vesna, manifest))\"}: So you read it.";
const ASSERT_KNOWS: &str = "::assert{knows(vesna, manifest)}";
const MOOD: &str = "state:\n  run.mood: { type: number, default: 0 }\n---\n## Shot";

fn with_mood(text: String) -> String {
    text.replace("---\n## Shot", MOOD)
}

fn branch(arm_x: &str, arm_y: &str) -> String {
    format!(
        "<branch id=\"b\">\n<choice id=\"x\" label=\"X\">\n{arm_x}\n@vesna: x.\n</choice>\n\
         <choice id=\"y\" label=\"Y\">\n{arm_y}\n@vesna: y.\n</choice>\n</branch>\n"
    )
}

#[test]
fn assert_then_guard_in_the_same_scene_is_fact_guaranteed() {
    let text = scene(1, &format!("{ASSERT_KNOWS}\n{KNOWS}"));
    let r = root(&[("a.lute", &text)]);
    assert_vocab_clean(&r);
    let ds = r.guards("a.lute");
    assert_eq!(codes(&ds), ["W-FACT-GUARANTEED"], "{ds:?}");
    let line = line_of(&text, ASSERT_KNOWS);
    assert!(
        ds[0].message.contains(&format!(
            "`knows(vesna, manifest)` is asserted on every route to here (a.lute:{line})"
        )),
        "{}",
        ds[0].message
    );
    assert_eq!(
        &text[ds[0].span.byte_start..ds[0].span.byte_end],
        "holds(knows(vesna, manifest))"
    );
}

#[test]
fn assert_in_one_branch_arm_is_only_possible_after_the_join() {
    let text = scene(1, &format!("{}{KNOWS}", branch(ASSERT_KNOWS, "")));
    let r = root(&[("a.lute", &text)]);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn assert_in_every_branch_arm_is_guaranteed_after_the_join() {
    let text = scene(1, &format!("{}{KNOWS}", branch(ASSERT_KNOWS, ASSERT_KNOWS)));
    let r = root(&[("a.lute", &text)]);
    let ds = r.guards("a.lute");
    let w = only(&ds, "W-FACT-GUARANTEED");
    let line = line_of(&text, ASSERT_KNOWS);
    assert!(w.message.contains(&format!("(a.lute:{line})")), "{}", w.message);
}

#[test]
fn retract_after_assert_is_not_guaranteed() {
    let text = scene(1, &format!("{ASSERT_KNOWS}\n::retract{{knows(vesna, _)}}\n{KNOWS}"));
    let r = root(&[("a.lute", &text)]);
    assert_vocab_clean(&r);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn keyed_assert_displaces_the_previous_value() {
    let text = scene(
        1,
        "::assert{at(vesna, bridge)}\n::assert{at(vesna, hold)}\n\
         @vesna{when=\"holds(at(vesna, bridge))\"}: still on the bridge?\n\
         @vesna{when=\"holds(at(vesna, hold))\"}: in the hold.",
    );
    let r = root(&[("a.lute", &text)]);
    assert_vocab_clean(&r);
    let ds = r.guards("a.lute");
    assert_eq!(codes(&ds), ["W-FACT-GUARANTEED"], "{ds:?}");
    assert!(ds[0].message.contains("`at(vesna, hold)`"), "{}", ds[0].message);
}

#[test]
fn a_skipping_next_keeps_the_skipped_assert_out_of_the_label() {
    let text = with_mood(scene(
        1,
        &format!(
            "::next{{to=\"skip\" when=\"run.mood > 0\"}}\n{ASSERT_KNOWS}\n::mark{{id=\"skip\"}}\n{KNOWS}"
        ),
    ));
    let r = root(&[("a.lute", &text)]);
    assert!(r.per_file[0].iter().all(|d| d.severity != Severity::Error), "{:?}", r.per_file[0]);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn a_guard_is_an_assumption_inside_its_region() {
    let producer = scene(2, &format!("{ASSERT_KNOWS}\n@vesna: logged."));
    let in_when = with_mood(scene(
        1,
        &format!(
            "<match on=\"run.mood\">\n<when test=\"holds(knows(vesna, manifest))\">\n{KNOWS}\n</when>\n\
             <otherwise>\n@vesna: no.\n</otherwise>\n</match>"
        ),
    ));
    let in_choice = scene(
        1,
        &format!(
            "<branch id=\"b\">\n<choice id=\"ask\" label=\"Ask\" when=\"holds(knows(vesna, manifest)) && true\">\n\
             {KNOWS}\n</choice>\n<choice id=\"go\" label=\"Go\">\n@vesna: bye.\n</choice>\n</branch>"
        ),
    );
    for (text, guard) in [(&in_when, "<when test"), (&in_choice, "<choice id=\"ask\"")] {
        let r = root(&[("a.lute", text), ("b.lute", &producer)]);
        assert_vocab_clean(&r);
        let ds = r.guards("a.lute");
        let w = only(&ds, "W-FACT-GUARANTEED");
        assert_eq!(w.span.line as usize, line_of(text, KNOWS), "the inner guard, not the outer");
        assert!(
            w.message.contains(&format!(
                "already holds here: the enclosing guard at a.lute:{} requires it",
                line_of(text, guard)
            )),
            "{}",
            w.message
        );
    }
}

#[test]
fn fact_asserted_in_a_prerequisite_scene_is_guaranteed_downstream() {
    let a = scene(1, &format!("@vesna: reading.\n{ASSERT_KNOWS}"));
    let b = scene_after(2, "visited(\"haven.s01ep01\")", KNOWS);
    let r = root(&[("a.lute", &a), ("b.lute", &b)]);
    assert_vocab_clean(&r);
    let ds = r.guards("b.lute");
    let w = only(&ds, "W-FACT-GUARANTEED");
    let line = line_of(&a, ASSERT_KNOWS);
    assert!(
        w.message.contains(&format!("asserted on every route to here (a.lute:{line})")),
        "{}",
        w.message
    );
}

#[test]
fn a_retract_anywhere_makes_a_prerequisite_fact_non_monotone() {
    let a = scene(1, ASSERT_KNOWS);
    let b = scene_after(2, "visited(\"haven.s01ep01\")", KNOWS);
    let lore_retract = lore("<entry id=\"e\">\n  @vesna: Forget it.\n  ::retract{knows(vesna, manifest)}\n</entry>");
    let quest_retract = quest(
        "<quest id=\"q\" start=\"true\">\n<on event=\"questComplete\">\n::retract{knows(vesna, _)}\n</on>\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>",
    );
    for retractor in [&lore_retract, &quest_retract] {
        let r = root(&[("a.lute", &a), ("b.lute", &b), ("r.lute", retractor)]);
        assert!(r.guards("b.lute").is_empty(), "{:?}", r.guards("b.lute"));
    }
}

#[test]
fn or_of_prerequisites_guarantees_only_their_common_facts() {
    let a = scene(1, ASSERT_KNOWS);
    let c = scene(3, "@vesna: elsewhere.");
    let either = scene_after(2, "visited(\"haven.s01ep01\") || visited(\"haven.s01ep03\")", KNOWS);
    let r = root(&[("a.lute", &a), ("b.lute", &either), ("c.lute", &c)]);
    assert!(r.guards("b.lute").is_empty(), "{:?}", r.guards("b.lute"));
    let both = scene_after(2, "visited(\"haven.s01ep01\") && visited(\"haven.s01ep03\")", KNOWS);
    let r = root(&[("a.lute", &a), ("b.lute", &both), ("c.lute", &c)]);
    assert_eq!(codes(&r.guards("b.lute")), ["W-FACT-GUARANTEED"]);
}

#[test]
fn a_hub_body_assert_is_not_guaranteed_after_the_hub() {
    let text = scene(
        1,
        &format!(
            "<hub id=\"h\">\n<choice id=\"t\" label=\"Talk\">\n{ASSERT_KNOWS}\n@vesna: told.\n</choice>\n\
             <choice id=\"leave\" label=\"Leave\" exit>\n@vesna: bye.\n</choice>\n</hub>\n{KNOWS}"
        ),
    );
    let r = root(&[("a.lute", &text)]);
    assert!(r.guards("a.lute").is_empty(), "{:?}", r.guards("a.lute"));
}

#[test]
fn scene_tier_facts_do_not_cross_a_document_boundary() {
    let near = "@vesna{when=\"holds(near(toma))\"}: Toma is here.";
    let a = scene(1, &format!("::assert{{near(toma)}}\n{near}"));
    let b = scene_after(2, "visited(\"haven.s01ep01\")", near);
    let r = root(&[("a.lute", &a), ("b.lute", &b)]);
    assert_vocab_clean(&r);
    assert_eq!(codes(&r.guards("a.lute")), ["W-FACT-GUARANTEED"]);
    assert!(r.guards("b.lute").is_empty(), "{:?}", r.guards("b.lute"));
}

#[test]
fn derived_relation_over_guaranteed_facts_is_guaranteed() {
    let text = scene(
        1,
        "::assert{awake(toma)}\n::assert{knows(toma, shed_sequence)}\n\
         @vesna{when=\"holds(can_halt(toma))\"}: Toma can halt it.",
    );
    let r = root(&[("a.lute", &text)]);
    assert_vocab_clean(&r);
    let ds = r.guards("a.lute");
    let w = only(&ds, "W-FACT-GUARANTEED");
    let line = line_of(&text, "::assert{awake(toma)}");
    assert!(
        w.message.contains(&format!(
            "`can_halt(toma)` follows by rule from facts that hold on every route to here (a.lute:{line})"
        )),
        "{}",
        w.message
    );
}

#[test]
fn scene_entry_lists_the_facts_guaranteed_on_arrival() {
    let a = scene(1, ASSERT_KNOWS);
    let b = scene_after(2, "visited(\"haven.s01ep01\")", "@vesna: hi.");
    let r = root(&[("a.lute", &a), ("b.lute", &b)]);
    let entry: Vec<(String, String)> = r.scene_entry["haven.s01ep02"]
        .iter()
        .map(|m| (m.fact.to_string(), m.provenance.to_string()))
        .collect();
    let line = line_of(&a, ASSERT_KNOWS);
    assert_eq!(
        entry,
        [
            ("awake(vesna)".to_string(), "`facts:` seed".to_string()),
            ("knows(vesna, manifest)".to_string(), format!("a.lute:{line}")),
        ]
    );
    assert_eq!(r.scene_entry["haven.s01ep01"].len(), 1, "seeds only on the entry scene");
}

#[test]
fn entry_when_that_decides_false_is_entry_unreachable_per_file() {
    let text = lore("<entry id=\"bark\" when=\"false\">\n  @vesna: Never.\n</entry>");
    let per_file = check(&input_for(&text)).diagnostics;
    let d = only(&per_file, "E-ENTRY-UNREACHABLE");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(&text[d.span.byte_start..d.span.byte_end], "false");
    let r = root(&[("notes.lute", &text)]);
    assert!(r.guards("notes.lute").is_empty(), "reported once, per file: {:?}", r.guards("notes.lute"));
}
