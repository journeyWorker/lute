//! dsl 0.24 — rule guards and rule syntax (round-3 triage T1-1, T3-6, T3-8,
//! T3-9): `@def`s expand inside a rule `cel()` guard, a relation cannot take
//! a reserved CEL name, `_` is an anonymous rule variable, `countDistinct`
//! joins the profile, and a schema's own problems are reported at the
//! schema's line, once, without cascading.
use lute_check::{check, fold_env, resolve_imports, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Span};
use lute_manifest::provider::ProviderSet;
use lute_syntax::datalog::BodyLiteral;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
state:\n  scene.n: { type: int, default: 0 }\n\
entities:\n  npc: { members: [mara, tomas] }\n  item: { members: [lamp] }\n";

fn input(text: &str, imports: SchemaImports) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "check_024_rules".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            ProviderSet::default(),
            imports,
            Default::default(),
            Default::default(),
        ),
    )
}

/// A scene whose frontmatter adds `extra` (relations/rules/defs) and whose
/// one line is guarded by `when`.
fn scene(extra: &str, when: &str) -> String {
    format!("{HDR}{extra}---\n## Shot 1.\n@narrator{{when=\"{when}\"}}: a\n")
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&input(text, SchemaImports::default())).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<&str> {
    ds.iter().map(|d| d.code.as_str()).collect()
}

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

const LIT: &str = "relations:\n  lit: { args: [item], derive: true }\n";

// --- T1-1: `@def` in a rule guard ---

/// The expanded body is what every consumer (checker, IR, trace) reads.
#[test]
fn a_def_in_a_rule_guard_is_expanded_and_checks_clean() {
    let text = scene(
        &format!("{LIT}rules:\n  - \"lit(lamp) :- cel(\\\"@first\\\")\"\ndefs:\n  first: \"scene.n == 0\"\n"),
        "holds('lit', ['lamp'])",
    );
    let ds = diags(&text);
    assert!(ds.is_empty(), "{ds:#?}");
    let (doc, _) = lute_syntax::parse(&text);
    let (folded, _, _) = fold_env(&doc, &input(&text, SchemaImports::default()));
    let guard = folded.env.rel_vocab.rules[0]
        .rule
        .body
        .iter()
        .find_map(|l| match l {
            BodyLiteral::Guard { cel, .. } => Some(cel.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        !guard.contains('@') && guard.contains("scene.n == 0"),
        "{guard}"
    );
}

#[test]
fn an_undefined_def_in_a_rule_guard_is_rule_guard_def() {
    let text = scene(
        &format!("{LIT}rules:\n  - \"lit(lamp) :- cel(\\\"@nope\\\")\"\n"),
        "holds('lit', ['lamp'])",
    );
    let ds = diags(&text);
    assert!(codes(&ds).contains(&"E-RULE-GUARD-DEF"), "{ds:#?}");
}

/// The expanded body still passes the rule-guard firewall.
#[test]
fn a_def_reading_facts_in_a_rule_guard_hits_the_firewall() {
    let text = scene(
        &format!(
            "{LIT}rules:\n  - \"lit(lamp) :- cel(\\\"@seen\\\")\"\ndefs:\n  seen: \"holds('lit', ['lamp'])\"\n"
        ),
        "holds('lit', ['lamp'])",
    );
    let ds = diags(&text);
    assert!(codes(&ds).contains(&"E-DATALOG-GUARD-FACT"), "{ds:#?}");
}

// --- T3-8: reserved relation names ---

#[test]
fn a_relation_named_like_a_cel_macro_is_reserved_name() {
    let text = scene(
        "relations:\n  has: { args: [item], tier: run }\n",
        "holds('has', ['lamp'])",
    );
    let ds = diags(&text);
    assert!(codes(&ds).contains(&"E-RESERVED-NAME"), "{ds:#?}");
    let parse = ds
        .iter()
        .find(|d| d.code == "E-CEL-PARSE")
        .expect("the use cannot parse");
    assert!(
        parse.message.contains("`has` is a reserved CEL name"),
        "{}",
        parse.message
    );
}

// --- T3-9: `_` in rule bodies, `countDistinct` ---

#[test]
fn anonymous_rule_variables_check_clean_positive_and_negated() {
    let text = scene(
        "relations:\n  seen: { args: [npc, item], tier: run }\n  testified: { args: [npc], derive: true }\n  \
         silent: { args: [npc], derive: true }\n\
         rules:\n  - \"testified(W) :- seen(W, _)\"\n  - \"silent(W) :- npc(W), not seen(W, _)\"\n",
        "holds('testified', ['mara']) || holds('silent', ['tomas'])",
    );
    let ds = diags(&text);
    assert!(ds.is_empty(), "{ds:#?}");
}

#[test]
fn count_distinct_is_in_profile_and_its_column_is_an_index() {
    let rel = "relations:\n  seen: { args: [npc, item], tier: run }\n";
    let ok = diags(&scene(rel, "countDistinct('seen', ['_', '_'], 0) >= 2"));
    assert!(ok.is_empty(), "{ok:#?}");
    // The counted column must be an in-range zero-based index whose pattern
    // position is the wildcard.
    let bad = diags(&scene(rel, "countDistinct('seen', ['_', '_'], 2) >= 2"));
    assert!(codes(&bad).contains(&"E-FACT-QUERY"), "{bad:#?}");
}

// --- T3-6: schema-level diagnostics ---

/// An imported relation's `W-DERIVE-NO-RULES` names the schema and carries
/// the schema line as `related` (so whole-project check folds the importers into
/// one); a relation whose only rule failed to parse draws none at all.
#[test]
fn imported_schema_problems_point_at_the_schema_and_do_not_cascade() {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "entities:\n  npc: { members: [mara] }\nrelations:\n  empty: { args: [npc], derive: true }\n  \
         broken: { args: [npc], derive: true }\nrules:\n  - \"broken(W) :- seen(W, f(x))\"\n",
    )
    .unwrap();
    let imports = resolve_imports(&dir, &["world.schema.yaml".to_string()], &[], zero_span());
    let text =
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: a\n";
    let res = check(&input(text, imports));
    let no_rules: Vec<&Diagnostic> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "W-DERIVE-NO-RULES")
        .collect();
    assert_eq!(no_rules.len(), 1, "{:#?}", res.diagnostics);
    let d = no_rules[0];
    assert!(
        d.related[0].file.ends_with("world.schema.yaml"),
        "{:#?}",
        d.related
    );
    let at = &d.related[0];
    assert!(
        d.message.contains("`empty`")
            && d.message
                .ends_with("(declared in schema import `world.schema.yaml`)"),
        "{}",
        d.message
    );
    // `  empty:` is on line 4 of the schema.
    assert_eq!(at.diagnostic.span.line, 4, "{at:#?}");
}
