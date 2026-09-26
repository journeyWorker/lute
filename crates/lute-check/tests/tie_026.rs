//! dsl 0.26.0 §8 (T3-2): `W-BEAT-PRIORITY-TIE` normalizes negations before
//! the exclusivity check (De Morgan), reads a beat's own asserts under its
//! `once`, and says why two `when`s are not exclusive.

use std::path::PathBuf;

use lute_check::{check_project_beats, fold_env, CheckInput, FoldedEnv, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_syntax::ast::Document;

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "tie026".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn vocab(tier: &str) -> String {
    format!(
        "entities:\n  foe: {{ members: [thief, cook] }}\n  item: {{ members: [key] }}\n\
         relations:\n  beaten: {{ args: [foe], tier: run }}\n  hasItem: {{ args: [item], tier: {tier} }}\n\
         state:\n  run.day: {{ type: number, default: 0 }}\n"
    )
}

fn lore(tier: &str, body: &str) -> String {
    format!("---\nkind: lore\nid: barks\n{}---\n{body}", vocab(tier))
}

fn scene(tier: &str, id: &str, fm: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\n{fm}{}---\n## Shot 1.\n@narrator: Hi.\n{body}",
        vocab(tier)
    )
}

fn entry(id: &str, when: &str) -> String {
    format!("<entry id=\"{id}\" on=\"talk\" priority=\"10\" when=\"{when}\">\n@narrator: {id}.\n</entry>\n")
}

fn ties(texts: &[&str]) -> Vec<Diagnostic> {
    let mut docs: Vec<(PathBuf, Document)> = Vec::new();
    let mut foldeds: Vec<FoldedEnv> = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let input = input(text);
        let (doc, _) = lute_syntax::parse(&input.text);
        foldeds.push(fold_env(&doc, &input).0);
        docs.push((PathBuf::from(format!("{i}.lute")), doc));
    }
    let refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    check_project_beats(&docs, &refs, &lute_check::cast::fact_producers(&docs), None)
        .into_iter()
        .map(|(_, d)| d)
        .filter(|d| d.code == "W-BEAT-PRIORITY-TIE")
        .collect()
}

const BOTH: &str = "holds(beaten(thief)) && holds(beaten(cook))";

#[test]
fn negations_are_normalized_before_the_exclusivity_check() {
    for negated in [
        "!(holds(beaten(thief)) && holds(beaten(cook)))",
        "!holds(beaten(thief)) || !holds(beaten(cook))",
        "!(!(!holds(beaten(thief)) || !holds(beaten(cook)))) || (run.day < 0 && run.day > 0)",
        "!(run.day >= 3) && !holds(beaten(cook))",
    ] {
        let when = if negated.contains("run.day >= 3") {
            "run.day > 5 || holds(beaten(cook))"
        } else {
            BOTH
        };
        let body = [entry("neg", negated), entry("pos", when)].concat();
        let out = ties(&[&lore("run", &body)]);
        assert!(out.is_empty(), "{negated}: {out:?}");
    }
    // A disjunct that overlaps keeps the tie, and says on which path.
    let body = [
        entry("neg", "!holds(beaten(thief)) || run.day > 2"),
        entry("pos", BOTH),
    ]
    .concat();
    let out = ties(&[&lore("run", &body)]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].message.contains("entry `neg` reads `run.day`"),
        "{}",
        out[0].message
    );
}

#[test]
fn a_fact_only_the_beats_own_unplayed_presentation_asserts_excludes_it() {
    // The hand-off scene (`once: run`, no `when`) asserts `hasItem(key)`;
    // the bark needs it. In a run they never compete.
    let giver = scene(
        "run",
        "giver",
        "on: talk\npriority: 10\n",
        "::assert{hasItem(key)}\n",
    );
    let bark = lore("run", &entry("after", "holds(hasItem(key))"));
    assert!(ties(&[&giver, &bark]).is_empty());

    // A user-tier fact outlives the run the scene is spent for.
    let giver_user = scene(
        "user",
        "giver",
        "on: talk\npriority: 10\n",
        "::assert{hasItem(key)}\n",
    );
    let bark_user = lore("user", &entry("after", "holds(hasItem(key))"));
    let out = ties(&[&giver_user, &bark_user]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0]
            .message
            .contains("`hasItem(key)` is `tier: user`, which persists across runs"),
        "{}",
        out[0].message
    );

    // Another producer can assert it before the scene plays.
    let other = scene("run", "other", "on: shop\n", "::assert{hasItem(key)}\n");
    let out = ties(&[&giver, &bark, &other]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0]
            .message
            .contains("`hasItem(key)` is also asserted in 2.lute"),
        "{}",
        out[0].message
    );

    // A repeatable scene plays again after asserting it.
    let again = scene(
        "run",
        "giver",
        "on: talk\npriority: 10\nonce: false\nwhen: 'run.day > 1'\n",
        "::assert{hasItem(key)}\n",
    );
    let out = ties(&[&again, &bark]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(out[0].message.contains("never spent"), "{}", out[0].message);
}

#[test]
fn a_flag_that_outlives_a_run_spend_is_named() {
    let rod = scene("run", "nedRod", "on: talk\npriority: 10\nonce: run\n", "");
    let bark = lore("run", &entry("nedAfter", "visited('nedRod')"));
    let out = ties(&[&rod, &bark]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0]
            .message
            .contains("`visited('nedRod')` persists across runs; `once: run` does not"),
        "{}",
        out[0].message
    );
    // No `when` at all is said as such.
    let plain = scene("run", "plain", "on: talk\npriority: 10\n", "");
    let out = ties(&[&plain, &lore("run", &entry("any", "run.day > 1"))]);
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].message.contains("scene `plain` has no `when`"),
        "{}",
        out[0].message
    );
}
