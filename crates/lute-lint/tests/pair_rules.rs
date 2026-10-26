//! dsl 0.51.0 §6 (#172): the `pair` target, through the public
//! [`lute_lint::lint`] entry and `lute.lint.yaml` parsing.
//!
//! The §6.3 recipes are read from `docs/linting.md`, so the documented rules
//! are the ones tested.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lute_core_span::{Severity, Span};
use lute_lint::model::diagnostic_code;
use lute_lint::{
    lint, parse_config, LintConfig, LintDocInput, LintLevel, LintOutcome, LintRuleBody,
    LintRuleDecl, LintScope, LintTarget, PairScope, PairSpec,
};
use lute_manifest::provider::ProviderSet;

fn span0() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

fn run(src: &str, rules: &[LintRuleDecl]) -> LintOutcome {
    let (doc, _) = lute_syntax::parse(src);
    let input = LintDocInput {
        path: PathBuf::from("d.lute"),
        doc,
        text: src.to_string(),
    };
    let plugin: Vec<(String, LintRuleDecl)> =
        rules.iter().map(|r| (r.id.clone(), r.clone())).collect();
    lint(
        &[input],
        &LintConfig::default(),
        &plugin,
        &ProviderSet::default(),
        None,
        span0(),
        LintScope::Full,
    )
}

/// `(code, line, message, related lines)` of every finding of `rules` and
/// every `E-LINT-EXPR`, in the engine's order.
type Found = (String, u32, String, Vec<u32>);

fn findings(src: &str, rules: &[LintRuleDecl]) -> Vec<Found> {
    let codes: Vec<String> = rules.iter().map(|r| diagnostic_code(&r.id)).collect();
    run(src, rules)
        .diagnostics
        .into_iter()
        .filter(|(_, d)| codes.contains(&d.code) || d.code == "E-LINT-EXPR")
        .map(|(_, d)| {
            let related = d.related.iter().map(|r| r.diagnostic.span.line).collect();
            (d.code, d.span.line, d.message, related)
        })
        .collect()
}

fn found(list: &[(&str, u32, &str, &[u32])]) -> Vec<Found> {
    list.iter()
        .map(|(c, l, m, r)| ((*c).to_string(), *l, (*m).to_string(), r.to_vec()))
        .collect()
}

/// The `lints:` block of `docs/linting.md` that holds the §6.3 recipes.
fn recipes() -> BTreeMap<String, LintRuleDecl> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/linting.md");
    let text = std::fs::read_to_string(&path).expect("docs/linting.md");
    let start = text
        .find("```yaml\nlints:\n  - id: group-balanced")
        .expect("the pair recipe block")
        + "```yaml\n".len();
    let end = start + text[start..].find("```").expect("its closing fence");
    let file: yaml_serde::Value = yaml_serde::from_str(&text[start..end]).expect("recipe YAML");
    let lints: Vec<LintRuleDecl> =
        yaml_serde::from_value(file["lints"].clone()).expect("recipes are lint rules");
    lints.into_iter().map(|r| (r.id.clone(), r)).collect()
}

fn recipe(id: &str) -> LintRuleDecl {
    recipes()
        .remove(id)
        .unwrap_or_else(|| panic!("recipe {id}"))
}

/// `rule` with its scope replaced.
fn scoped(mut rule: LintRuleDecl, scope: PairScope) -> LintRuleDecl {
    let LintRuleBody::Pair(pair) = &mut rule.body else {
        panic!("a pair rule")
    };
    pair.scope = scope;
    rule
}

fn pair_rule(id: &str, open: &str, close: &str, inside: Option<&str>) -> LintRuleDecl {
    LintRuleDecl {
        id: id.to_string(),
        target: LintTarget::Pair,
        body: LintRuleBody::Pair(PairSpec {
            open: open.to_string(),
            close: close.to_string(),
            inside: inside.map(str::to_string),
            scope: PairScope::Body,
        }),
        level: None,
        message: "{node.tag}".to_string(),
        options: yaml_serde::Mapping::new(),
    }
}

const SCENE: &str = "---\nid: s\nkind: scene\n---\n";

const GROUP: &str = "L-GROUP-BALANCED";

#[test]
fn the_documented_recipes_are_pair_rules() {
    let all = recipes();
    assert_eq!(
        all.keys().map(String::as_str).collect::<Vec<_>>(),
        ["group-balanced", "letterbox-balanced"]
    );
    let scopes: Vec<PairScope> = all
        .values()
        .map(|r| {
            assert_eq!(r.target, LintTarget::Pair);
            match &r.body {
                LintRuleBody::Pair(p) => p.scope,
                LintRuleBody::Predicate { .. } => panic!("{r:?}"),
            }
        })
        .collect();
    assert_eq!(scopes, [PairScope::Body, PairScope::Section]);
}

#[test]
fn balanced_groups_report_nothing() {
    let src = format!(
        "{SCENE}## A\n::group{{mode=\"begin\"}}\n@mira: Stay.\n::sfx{{id=\"s\"}}\n\
         ::actor{{id=\"m\"}}\n::group{{mode=\"end\"}}\n::group{{mode=\"begin\"}}\n\
         ::group{{mode=\"end\"}}\n::bg{{id=\"after\"}}\n"
    );
    assert_eq!(findings(&src, &[recipe("group-balanced")]), []);
}

/// Each fault once, with its phrase, anchor, level and related open row.
#[test]
fn every_fault_in_a_body() {
    let src = format!(
        "{SCENE}## A\n::group{{mode=\"end\"}}\n::group{{mode=\"begin\"}}\n::bg{{id=\"hall\"}}\n\
         ::group{{mode=\"begin\"}}\n::group{{mode=\"end\"}}\n::group{{mode=\"begin\"}}\n@mira: hi\n"
    );
    let out = run(&src, &[recipe("group-balanced")]);
    assert!(out
        .diagnostics
        .iter()
        .filter(|(_, d)| d.code == GROUP)
        .all(|(path, d)| d.severity == Severity::Error
            && d.related
                .iter()
                .all(|r| r.file == path.display().to_string())));
    assert_eq!(
        findings(&src, &[recipe("group-balanced")]),
        found(&[
            (
                GROUP,
                6,
                "::group begin/end — a close with no open pair in this body",
                &[]
            ),
            (
                GROUP,
                8,
                "::group begin/end — not allowed inside the pair opened at line 7",
                &[7]
            ),
            (
                GROUP,
                9,
                "::group begin/end — opened again while the pair from line 7 is still open",
                &[7]
            ),
            (
                GROUP,
                11,
                "::group begin/end — opened here, with no close in this body",
                &[]
            ),
        ])
    );
}

/// The message is rendered at the row it is reported at; an unclosed open's
/// at the open, the rows after it being gone by the end of the body.
#[test]
fn the_message_is_rendered_at_the_reported_row() {
    let mut rule = recipe("group-balanced");
    rule.message = "{node.tag} {node.index}".to_string();
    let src = format!("{SCENE}## A\n::group{{mode=\"begin\"}}\n::bg{{id=\"x\"}}\n@a: b\n");
    assert_eq!(
        findings(&src, &[rule]),
        found(&[
            (
                GROUP,
                6,
                "group 0 — opened here, with no close in this body",
                &[]
            ),
            (
                GROUP,
                7,
                "bg 1 — not allowed inside the pair opened at line 6",
                &[6]
            ),
        ])
    );
}

/// `body` scope: `inside` reads the siblings between the two, not their
/// descendants, and a nested body balances on its own. `section` scope
/// reads every row between them.
#[test]
fn inside_reads_siblings_only_in_body_scope() {
    let src = format!(
        "{SCENE}## A\n::group{{mode=\"begin\"}}\n<branch id=\"b\">\n<choice id=\"c\" text=\"t\">\n\
         ::bg{{id=\"x\"}}\n</choice>\n</branch>\n::group{{mode=\"end\"}}\n"
    );
    let inside = |line, open| {
        (
            GROUP,
            line,
            "::group begin/end — not allowed inside the pair opened at line 6",
            open,
        )
    };
    assert_eq!(
        findings(&src, &[recipe("group-balanced")]),
        found(&[inside(7, &[6][..])])
    );
    assert_eq!(
        findings(
            &src,
            &[scoped(recipe("group-balanced"), PairScope::Section)]
        ),
        found(&[
            inside(7, &[6][..]),
            inside(8, &[6][..]),
            inside(9, &[6][..])
        ])
    );
}

#[test]
fn a_nested_body_has_its_own_slot_in_body_scope() {
    let src = format!(
        "{SCENE}## A\n<branch id=\"b\">\n<choice id=\"c\" text=\"t\">\n::group{{mode=\"begin\"}}\n\
         </choice>\n</branch>\n::group{{mode=\"end\"}}\n"
    );
    assert_eq!(
        findings(&src, &[recipe("group-balanced")]),
        found(&[
            (
                GROUP,
                8,
                "::group begin/end — opened here, with no close in this body",
                &[]
            ),
            (
                GROUP,
                11,
                "::group begin/end — a close with no open pair in this body",
                &[]
            ),
        ])
    );
    let section = scoped(recipe("group-balanced"), PairScope::Section);
    assert_eq!(findings(&src, &[section]), []);
}

/// `section` scope is document order per section: an open before a branch
/// and a close in its choice balance, an open in one choice and a close in
/// its sibling pair up, a second sibling close has no open pair, and each
/// section starts with an empty slot.
#[test]
fn section_scope_spans_branches_in_document_order() {
    let src = format!(
        "{SCENE}## A\n::letterbox{{on}}\n<branch id=\"b\">\n<choice id=\"c\" text=\"t\">\n\
         ::letterbox{{off}}\n</choice>\n</branch>\n\
         ## B\n<branch id=\"b2\">\n<choice id=\"c1\" text=\"1\">\n::letterbox{{on}}\n</choice>\n\
         <choice id=\"c2\" text=\"2\">\n::letterbox{{off}}\n</choice>\n\
         <choice id=\"c3\" text=\"3\">\n::letterbox{{off}}\n</choice>\n</branch>\n\
         ## C\n::letterbox{{on}}\n## D\n::letterbox{{off}}\n"
    );
    const BOX: &str = "L-LETTERBOX-BALANCED";
    assert_eq!(
        findings(&src, &[recipe("letterbox-balanced")]),
        found(&[
            (
                BOX,
                21,
                "::letterbox on/off — a close with no open pair in this section",
                &[]
            ),
            (
                BOX,
                25,
                "::letterbox on/off — opened here, with no close in this section",
                &[]
            ),
            (
                BOX,
                27,
                "::letterbox on/off — a close with no open pair in this section",
                &[]
            ),
        ])
    );
    // As a `body` rule, section A's two rows are in different bodies.
    let body = scoped(recipe("letterbox-balanced"), PairScope::Body);
    let lines: Vec<(u32, String)> = findings(&src, &[body])
        .into_iter()
        .map(|(_, line, message, _)| (line, message))
        .filter(|(line, _)| *line < 12)
        .collect();
    assert_eq!(
        lines,
        [
            (
                6,
                "::letterbox on/off — opened here, with no close in this body".to_string()
            ),
            (
                9,
                "::letterbox on/off — a close with no open pair in this body".to_string()
            ),
        ]
    );
}

/// Each top-level declaration is its own `section`-scope unit.
#[test]
fn section_scope_resets_per_declaration() {
    let src = "---\nid: q\nkind: quest\n---\n<quest id=\"q1\">\n\
               <objective id=\"o\" done=\"true\">\n::group{mode=\"begin\"}\n</objective>\n</quest>\n\
               <quest id=\"q2\">\n<objective id=\"p\" done=\"true\">\n::group{mode=\"end\"}\n\
               </objective>\n</quest>\n";
    let rule = scoped(recipe("group-balanced"), PairScope::Section);
    assert_eq!(
        findings(src, &[rule]),
        found(&[
            (
                GROUP,
                7,
                "::group begin/end — opened here, with no close in this section",
                &[]
            ),
            (
                GROUP,
                12,
                "::group begin/end — a close with no open pair in this section",
                &[]
            ),
        ])
    );
}

/// A row for which both predicates hold closes an open pair and opens one
/// otherwise.
#[test]
fn a_row_satisfying_both_predicates_closes_first() {
    let rule = pair_rule("sep", r#"node.tag == "sep""#, r#"node.tag == "sep""#, None);
    let two = format!("{SCENE}## A\n::sep\n@a: x\n::sep\n");
    assert_eq!(findings(&two, std::slice::from_ref(&rule)), []);
    let three = format!("{SCENE}## A\n::sep\n::sep\n::sep\n");
    assert_eq!(
        findings(&three, &[rule]),
        found(&[(
            "L-SEP",
            8,
            "sep — opened here, with no close in this body",
            &[]
        )])
    );
}

/// The recipe guards its index; without the guard, the first row missing
/// the key is one `E-LINT-EXPR`, and the rule stops for the document (no
/// unclosed report follows).
#[test]
fn a_missing_key_is_an_error_only_unguarded() {
    let src = format!(
        "{SCENE}## A\n::group{{mode=\"begin\"}}\n::group\n@a: x\n::group{{mode=\"end\"}}\n\
         ::group{{mode=\"begin\"}}\n"
    );
    assert_eq!(
        findings(&src, &[recipe("group-balanced")]),
        found(&[
            (
                GROUP,
                7,
                "::group begin/end — not allowed inside the pair opened at line 6",
                &[6]
            ),
            (
                GROUP,
                10,
                "::group begin/end — opened here, with no close in this body",
                &[]
            ),
        ])
    );
    let mut rule = recipe("group-balanced");
    rule.id = "group-unguarded".to_string();
    let LintRuleBody::Pair(pair) = &mut rule.body else {
        panic!("a pair rule")
    };
    pair.open = r#"node.tag == "group" && node.attrs["mode"] == "begin""#.to_string();
    assert_eq!(
        findings(&src, &[rule]),
        found(&[(
            "E-LINT-EXPR",
            1,
            "rule `group-unguarded`: `open`: missing key `mode`",
            &[]
        )])
    );
}

#[test]
fn predicate_errors_name_the_predicate() {
    let src = format!("{SCENE}## A\n::a\n::b\n");
    let rules = [
        pair_rule("unparsed", "true", "false", Some("node.tag ==")),
        pair_rule("not-bool", "node.tag", "false", None),
    ];
    let out = findings(&src, &rules);
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(
        out[1]
            .2
            .starts_with("rule `not-bool`: `open` returned non-bool: Str("),
        "{out:?}"
    );
    assert!(
        out[0]
            .2
            .starts_with("rule `unparsed`: `inside` failed to parse: "),
        "{out:?}"
    );
}

/// A rule whose body does not fit its target, built in Rust, is refused at
/// resolution as a config shape fault.
#[test]
fn a_mismatched_body_built_in_rust_is_a_config_error() {
    let mut node = pair_rule("as-node", "true", "true", None);
    node.target = LintTarget::Node;
    let mut pair = pair_rule("as-pair", "true", "true", None);
    pair.body = LintRuleBody::Predicate {
        when: "true".to_string(),
    };
    let out = run(&format!("{SCENE}## A\n::a\n"), &[node, pair]);
    let messages: Vec<&str> = out
        .config_diagnostics
        .iter()
        .map(|(_, d)| {
            assert_eq!(d.code, "E-LINT-CONFIG");
            d.message.as_str()
        })
        .collect();
    assert_eq!(
        messages,
        [
            "rule `as-node`: `open` belongs to `target: pair`, not `target: node`; this target \
             states `when`",
            "rule `as-pair`: `when` is refused on `target: pair`, which states `open` and `close` \
             instead",
        ]
    );
    assert!(out
        .diagnostics
        .iter()
        .all(|(_, d)| !d.code.starts_with("L-AS-")));
}

/// Every shape fault of a `custom:` rule is `E-LINT-CONFIG`, and the rule is
/// dropped; a well-formed pair rule parses with its default scope.
#[test]
fn every_custom_shape_fault_is_a_config_error() {
    let rule = |keys: &str| format!("custom:\n  - id: r\n    message: m\n{keys}");
    let (cfg, diags) = parse_config(
        &rule("    target: pair\n    open: 'true'\n    close: 'false'\n    level: warn\n"),
        span0(),
    )
    .unwrap();
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(
        cfg.custom[0].body,
        LintRuleBody::Pair(PairSpec {
            open: "true".to_string(),
            close: "false".to_string(),
            inside: None,
            scope: PairScope::Body,
        })
    );
    assert_eq!(cfg.custom[0].level, Some(LintLevel::Warn));
    let cases = [
        (
            "    target: pair\n    when: 'true'\n    open: 'true'\n    close: 'true'\n",
            "`when` is refused on `target: pair`",
        ),
        (
            "    target: pair\n    close: 'true'\n",
            "a `target: pair` rule needs `open`",
        ),
        (
            "    target: pair\n    open: 'true'\n",
            "a `target: pair` rule needs `close`",
        ),
        (
            "    target: pair\n    open: 'true'\n    close: 'true'\n    scope: shot\n",
            "unknown variant `shot`, expected `body` or `section`",
        ),
        (
            "    target: node\n    when: 'true'\n    open: 'true'\n",
            "`open` belongs to `target: pair`, not `target: node`",
        ),
        (
            "    target: scene\n    when: 'true'\n    close: 'true'\n",
            "`close` belongs to `target: pair`, not `target: scene`",
        ),
        (
            "    target: line\n    when: 'true'\n    inside: 'true'\n",
            "`inside` belongs to `target: pair`, not `target: line`",
        ),
        (
            "    target: node\n    when: 'true'\n    scope: body\n",
            "`scope` belongs to `target: pair`, not `target: node`",
        ),
        ("    target: node\n", "rule `r`: missing field `when`"),
    ];
    for (keys, want) in cases {
        let (cfg, diags) = parse_config(&rule(keys), span0()).unwrap();
        assert!(cfg.custom.is_empty(), "{keys}");
        assert_eq!(diags.len(), 1, "{keys}: {diags:?}");
        assert_eq!(diags[0].code, "E-LINT-CONFIG");
        assert!(
            diags[0].message.starts_with("bad custom rule: ") && diags[0].message.contains(want),
            "{keys}: {}",
            diags[0].message
        );
    }
}

// ---------------------------------------------------------------------------
// Cost counters (dsl 0.51.0 §6.2)
// ---------------------------------------------------------------------------

/// The `pair` counters on a small document: every frame pushed, every open
/// and close, every predicate invocation; `eval_chars` counts what the
/// predicates scan.
#[test]
fn pair_counters_count_slots_invocations_and_scanned_characters() {
    // Rows: four markers in the section body. Frames: the section body and
    // the (empty) declaration body.
    let src = format!("{SCENE}## A\n::b\n::x{{t=\"abcd\"}}\n::e\n::b\n");
    let rule = pair_rule(
        "sized",
        r#"node.tag == "b""#,
        r#"node.tag == "e""#,
        Some(r#""t" in node.attrs && size(node.attrs["t"]) > 0"#),
    );
    let stats = run(&src, &[rule]).node_stats;
    assert_eq!(stats.rows, 4);
    // 2 frames + 2 opens + 1 close.
    assert_eq!(stats.pair_slots, 5);
    // ::b open (1) · ::x close, open, inside (3) · ::e close (1) · ::b open (1).
    assert_eq!(stats.pair_evals, 6);
    // Every tag is one character, so each tag comparison scans 1: `b`
    // (open) 1, `x` (close, open) 2 + `size` 4, `e` (close) 1, `b` (open) 1.
    assert_eq!(stats.eval_chars, 9);
    assert_eq!(
        stats.work(),
        stats.rows
            + stats.head_bytes
            + stats.attrs
            + stats.preceding_steps
            + stats.preceding_copied
            + stats.eval_chars
            + stats.pair_slots
    );
}

/// No `pair` rule, no `pair` work; and a `node` rule's string operations are
/// counted the same way.
#[test]
fn no_pair_work_without_a_pair_rule() {
    let src = format!("{SCENE}## A\n::b\n::x{{t=\"abcd\"}}\n");
    let node = LintRuleDecl {
        id: "n".to_string(),
        target: LintTarget::Node,
        body: LintRuleBody::Predicate {
            when: r#""t" in node.attrs && size(node.attrs["t"]) > 10"#.to_string(),
        },
        level: None,
        message: "m".to_string(),
        options: yaml_serde::Mapping::new(),
    };
    let stats = run(&src, &[node]).node_stats;
    assert_eq!((stats.pair_evals, stats.pair_slots), (0, 0));
    assert_eq!(stats.eval_chars, 4);
    assert_eq!(run(&src, &[]).node_stats, Default::default());
}
