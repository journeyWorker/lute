//! `E-STATE-COLLECTION` (dsl 0.8.0 §4): author `state:` is SCALAR
//! (`number|bool|string|enum`).
//!
//! Before 0.8.0 three sources contradicted each other — the normative text
//! (`docs/proposals/scenario-dsl/0.2.0.md`/`0.3.0.md`) said scalar-only, the
//! shape validator (`meta.rs`'s `StateDeclRaw`) deserialized the full `Type`
//! union and only special-cased `narrativeTime`, and
//! `docs/runtime/state-lifecycle.md` documented `list<…>`/`map<…>`/`record`
//! as valid `StateEntry.type` values. 0.8.0 §4 resolves it in favour of the
//! normative text: a `list`/`record`/`map` author declaration is
//! `E-STATE-COLLECTION` and — like the `narrativeTime` rejection (dsl 0.3.0
//! D11) — is NOT installed, so a later read is plain `E-UNDECLARED` rather
//! than a phantom collection-typed slot.
//!
//! Collection-shaped `StateEntry` types still reach the compiled artifact
//! through a plugin `state_shapes` expansion — that surface is untouched
//! here; only the AUTHOR `state:` block narrows.

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

/// A scene carrying `state_block` verbatim in its frontmatter whose first
/// `<choice when=…>` reads `cond`. The second, unguarded `<choice>` keeps the
/// branch out of the unrelated `E-BRANCH-ALL-GUARDED` diagnostic (same
/// discipline as `tests/fact_query.rs::scene_when`).
fn scene(state_block: &str, cond: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{state_block}---\n\
         ## Shot 1.\n<branch>\n<choice id=\"a\" label=\"a\" when=\"{cond}\">\n@narrator: hi\n\
         </choice>\n<choice id=\"b\" label=\"b\">\n@narrator: bye\n</choice>\n</branch>\n",
    )
}

#[test]
fn list_typed_author_state_is_rejected_and_not_installed() {
    let text = scene("state:\n  run.bag: { type: { list: string } }\n", "run.bag");
    let diags = run(&text).diagnostics;
    let collection: Vec<_> = diags.iter().filter(|d| d.code == "E-STATE-COLLECTION").collect();
    assert_eq!(collection.len(), 1, "exactly one E-STATE-COLLECTION: {diags:?}");
    assert_eq!(
        collection[0].message,
        "state path `run.bag` cannot declare a collection type (`list`/`record`/`map`); \
         author state is scalar (number|bool|string|enum) — model collections as \
         `relations:` (dsl 0.3.0 §3) or a plugin `state_shapes` slot"
    );

    // The decl is SKIPPED, so the later read is plain `E-UNDECLARED` — never a
    // phantom collection-typed path (which would instead resolve, and let
    // `set_op::descend` invent field types for everything under `run.bag.*`).
    let cs: Vec<String> = diags.into_iter().map(|d| d.code).collect();
    assert!(cs.contains(&"E-UNDECLARED".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-MAYBE-UNSET".to_string()), "phantom decl: {cs:?}");
}

#[test]
fn record_and_map_typed_author_state_are_rejected() {
    let record = scene(
        "state:\n  run.player: { type: { record: [ { name: hp, type: number } ] } }\n",
        "run.hp",
    );
    assert!(
        codes(&record).contains(&"E-STATE-COLLECTION".to_string()),
        "{:?}",
        codes(&record)
    );

    let map = scene(
        "state:\n  run.tally: { type: { map: { key: string, value: number } } }\n",
        "run.other",
    );
    assert!(codes(&map).contains(&"E-STATE-COLLECTION".to_string()), "{:?}", codes(&map));
}

#[test]
fn scalar_author_state_is_unaffected() {
    // Regression: every scalar the normative text admits still folds, reads
    // clean, and never trips the new code.
    for (decl, cond) in [
        ("run.n: { type: number, default: 0 }", "run.n > 0"),
        ("run.b: { type: bool, default: false }", "run.b"),
        ("run.s: { type: string, default: \"\" }", "run.s == 'x'"),
        ("run.e: { type: { enum: [a, b] }, default: a }", "run.e == 'a'"),
    ] {
        let cs = codes(&scene(&format!("state:\n  {decl}\n"), cond));
        assert!(!cs.contains(&"E-STATE-COLLECTION".to_string()), "{decl}: {cs:?}");
        assert!(!cs.contains(&"E-UNDECLARED".to_string()), "{decl}: {cs:?}");
        assert!(!cs.iter().any(|c| c.starts_with("E-")), "{decl}: {cs:?}");
    }
}
