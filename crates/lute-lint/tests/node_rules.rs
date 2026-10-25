//! dsl 0.50.0 §8 (#172, #177): the `node` target, through the public
//! [`lute_lint::lint`] entry and the [`lute_lint::nodes::walk_nodes`] rows.
//!
//! The §8.3 recipes are read from `docs/linting.md`, so the documented rules
//! are the ones tested.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_core_span::Span;
use lute_lint::eval::{Record, Value};
use lute_lint::model::diagnostic_code;
use lute_lint::nodes::{walk_nodes, NodeRow};
use lute_lint::{lint, LintConfig, LintDocInput, LintOutcome, LintRuleDecl, LintScope};
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

/// `(code, line, message)` of every finding of `rules` and every
/// `E-LINT-EXPR`, in the engine's order.
fn findings(src: &str, rules: &[LintRuleDecl]) -> Vec<(String, u32, String)> {
    let codes: BTreeSet<String> = rules.iter().map(|r| diagnostic_code(&r.id)).collect();
    run(src, rules)
        .diagnostics
        .into_iter()
        .filter(|(_, d)| codes.contains(&d.code) || d.code == "E-LINT-EXPR")
        .map(|(_, d)| (d.code, d.span.line, d.message))
        .collect()
}

fn found(list: &[(&str, u32, &str)]) -> Vec<(String, u32, String)> {
    list.iter()
        .map(|(c, l, m)| ((*c).to_string(), *l, (*m).to_string()))
        .collect()
}

/// The `lints:` block of `docs/linting.md` that holds the §8.3 recipes.
fn recipes() -> BTreeMap<String, LintRuleDecl> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/linting.md");
    let text = std::fs::read_to_string(&path).expect("docs/linting.md");
    let start = text
        .find("```yaml\nlints:\n  - id: signal-needs-menu")
        .expect("the recipe block")
        + "```yaml\n".len();
    let end = start + text[start..].find("```").expect("its closing fence");
    let file: yaml_serde::Value = yaml_serde::from_str(&text[start..end]).expect("recipe YAML");
    let lints: Vec<LintRuleDecl> =
        yaml_serde::from_value(file["lints"].clone()).expect("recipes are lint rules");
    lints.into_iter().map(|r| (r.id.clone(), r)).collect()
}

fn recipe(ids: &[&str]) -> Vec<LintRuleDecl> {
    let all = recipes();
    ids.iter()
        .map(|id| {
            all.get(*id)
                .unwrap_or_else(|| panic!("recipe {id}"))
                .clone()
        })
        .collect()
}

const SCENE: &str = "---\nid: s\nkind: scene\n---\n";

#[test]
fn the_documented_recipes_are_all_node_rules() {
    let all = recipes();
    assert_eq!(all.len(), 9);
    assert!(all
        .values()
        .all(|r| r.target == lute_lint::LintTarget::Node));
}

#[test]
fn signal_needs_menu() {
    let src = format!(
        "{SCENE}## A\n::bg{{id=\"room\"}}\n::signal{{value=\"puzzle\"}}\n@mira: Hm.\n\
         ::signal{{value=\"puzzle\"}}\n<branch id=\"b\">\n<choice id=\"c\" text=\"Go\">\n\
         @mira: Go.\n</choice>\n</branch>\n::signal{{value=\"calm\"}}\n::signal\n"
    );
    assert_eq!(
        findings(&src, &recipe(&["signal-needs-menu"])),
        found(&[(
            "L-SIGNAL-NEEDS-MENU",
            7,
            "a puzzle ::signal must be followed by a <branch>"
        )])
    );
}

#[test]
fn section_opens_with_bg() {
    let src = format!(
        "{SCENE}## A\n::bg{{id=\"r\"}}\n@a: x\n## B\n::music{{id=\"m\"}}\n<branch id=\"b\">\n\
         <choice id=\"c\" text=\"t\">\n::sfx{{id=\"s\"}}\n</choice>\n</branch>\n## C\n@a: hi\n"
    );
    assert_eq!(
        findings(&src, &recipe(&["section-opens-with-bg"])),
        found(&[
            ("L-SECTION-OPENS-WITH-BG", 9, "section B opens with music"),
            ("L-SECTION-OPENS-WITH-BG", 16, "section C opens with "),
        ])
    );
}

#[test]
fn beat_label_prefix() {
    let rules = recipe(&["beat-label-prefix"]);
    let beat = "---\nid: b\non: dawn\n---\n## A\n::label{name=\"x\"}\n::label{name=\"b_y\"}\n\
                ::label\n";
    assert_eq!(
        findings(beat, &rules),
        found(&[("L-BEAT-LABEL-PREFIX", 6, "a beat's labels start with b_")])
    );
    let scene = format!("{SCENE}## A\n::label{{name=\"x\"}}\n");
    assert_eq!(findings(&scene, &rules), found(&[]));
}

/// #177, same element: a missing key gives nothing, an unknown
/// discriminator is reported once by the `*-known` rule.
#[test]
fn move_mode_and_speed() {
    let src = format!(
        "{SCENE}## A\n::move{{mode=\"walk\" speed=\"fast\"}}\n::move{{mode=\"run\" speed=\"fast\"}}\n\
         ::move{{speed=\"fast\"}}\n::move{{mode=\"walk\"}}\n::move{{mode=\"swim\" speed=\"fast\"}}\n"
    );
    assert_eq!(
        findings(&src, &recipe(&["move-mode-known", "speed-fits-mode"])),
        found(&[
            (
                "L-SPEED-FITS-MODE",
                6,
                "speed fast is not legal in mode walk"
            ),
            ("L-MOVE-MODE-KNOWN", 10, "unknown move mode swim"),
        ])
    );
}

/// #177, a marker before the branch: `preceding` follows the path to the
/// node — the marker of an earlier sibling arm is not seen, the one before
/// the `<match>` is — and starts empty at each section.
#[test]
fn interaction_kind_and_option() {
    let src = format!(
        "{SCENE}## A\n::interaction{{kind=\"greet\"}}\n<branch id=\"b\">\n<choice id=\"c1\" text=\"1\">\n\
         ::option{{value=\"wave\"}}\n::option{{value=\"accept\"}}\n::option\n</choice>\n</branch>\n\
         <match subject=\"run.x\">\n<when is=\"1\">\n::interaction{{kind=\"gift\"}}\n</when>\n\
         <otherwise>\n::option{{value=\"accept\"}}\n</otherwise>\n</match>\n\
         ## B\n::option{{value=\"wave\"}}\n::interaction\n::option{{value=\"wave\"}}\n\
         ::interaction{{kind=\"trade\"}}\n::option{{value=\"wave\"}}\n\
         ## C\n<match subject=\"run.x\">\n<when is=\"1\">\n::interaction{{kind=\"gift\"}}\n</when>\n\
         <otherwise>\n::option{{value=\"wave\"}}\n</otherwise>\n</match>\n"
    );
    let fit = "option accept does not belong to this interaction";
    let known = "::interaction needs a known kind";
    assert_eq!(
        findings(
            &src,
            &recipe(&["interaction-kind-known", "option-fits-interaction"])
        ),
        found(&[
            ("L-OPTION-FITS-INTERACTION", 10, fit),
            ("L-OPTION-FITS-INTERACTION", 19, fit),
            ("L-INTERACTION-KIND-KNOWN", 24, known),
            ("L-INTERACTION-KIND-KNOWN", 26, known),
        ])
    );
}

/// #177 with #164 stamps: the parent `<branch>` carries the discriminator.
#[test]
fn menu_type_and_choice_key() {
    let src = format!(
        "{SCENE}## A\n<branch id=\"m1\" menuType=\"timed\">\n\
         <choice id=\"c1\" text=\"1\" optionKey=\"L1\">\n@a: x\n</choice>\n\
         <choice id=\"c2\" text=\"2\" optionKey=\"T1\">\n@a: y\n</choice>\n\
         <choice id=\"c3\" text=\"3\">\n@a: z\n</choice>\n</branch>\n\
         <branch id=\"m2\">\n<choice id=\"c4\" text=\"4\" optionKey=\"L1\">\n@a: w\n</choice>\n</branch>\n\
         <branch id=\"m3\" menuType=\"grid\">\n<choice id=\"c5\" text=\"5\" optionKey=\"L1\">\n@a: v\n\
         </choice>\n</branch>\n"
    );
    assert_eq!(
        findings(&src, &recipe(&["menu-type-known", "choice-fits-menu"])),
        found(&[
            ("L-CHOICE-FITS-MENU", 7, "option key L1 is not a timed key"),
            ("L-MENU-TYPE-KNOWN", 22, "unknown menu type grid"),
        ])
    );
}

/// Why every recipe guards its indexes: without the membership test, a node
/// without `mode` is one `E-LINT-EXPR`, and the rule stops for the document.
#[test]
fn an_unguarded_index_is_a_lint_expression_error() {
    let mut rule = recipe(&["speed-fits-mode"]).remove(0);
    rule.id = "speed-fits-mode-unguarded".to_string();
    rule.when =
        r#"node.tag == "move" && !(node.attrs["speed"] in options.speeds[node.attrs["mode"]])"#
            .to_string();
    let src = format!(
        "{SCENE}## A\n::move{{speed=\"fast\"}}\n::move{{speed=\"fast\"}}\n::move{{mode=\"walk\" speed=\"fast\"}}\n"
    );
    assert_eq!(
        findings(&src, &[rule]),
        found(&[(
            "E-LINT-EXPR",
            1,
            "rule `speed-fits-mode-unguarded`: missing key `mode`"
        )])
    );
}

fn node_rule(id: &str, when: &str, message: &str) -> LintRuleDecl {
    LintRuleDecl {
        id: id.to_string(),
        target: lute_lint::LintTarget::Node,
        when: when.to_string(),
        level: None,
        message: message.to_string(),
        options: yaml_serde::Mapping::new(),
    }
}

/// §8.2 through a rule, including a non-string operand.
#[test]
fn string_members_in_a_node_rule() {
    let src = format!("{SCENE}## A\n::label{{name=\"b_one\"}}\n::label{{name=\"two_end\"}}\n");
    let rules = [
        node_rule(
            "starts",
            r#"node.tag == "label" && node.attrs["name"].startsWith("b_")"#,
            "{node.attrs.name}",
        ),
        node_rule(
            "ends",
            r#"node.tag == "label" && node.attrs["name"].endsWith("_end")"#,
            "{node.attrs.name}",
        ),
        node_rule(
            "has",
            r#"node.tag == "label" && node.attrs["name"].contains("o_e")"#,
            "{node.attrs.name}",
        ),
        node_rule("bad", "node.index.startsWith(\"0\")", "never"),
    ];
    assert_eq!(
        findings(&src, &rules),
        found(&[
            (
                "E-LINT-EXPR",
                1,
                "rule `bad`: `.startsWith` needs string operands, got int and string"
            ),
            ("L-STARTS", 6, "b_one"),
            ("L-ENDS", 7, "two_end"),
            ("L-HAS", 7, "two_end"),
        ])
    );
}

/// Authored attributes on rows: a lifted value keeps its text, an
/// unauthored one is absent.
#[test]
fn reward_amount_is_read_as_written() {
    let src = "---\nid: q\nkind: quest\n---\n<quest id=\"q1\">\n\
               <objective id=\"o\" done=\"true\"/>\n<reward kind=\"XP\" amount=\"+005\"/>\n\
               <reward kind=\"XP\" amount=\"5\"/>\n<reward kind=\"XP\"/>\n</quest>\n";
    let rules = [
        node_rule(
            "amount-text",
            r#"node.kind == "reward" && "amount" in node.attrs"#,
            "{node.attrs.amount}",
        ),
        node_rule(
            "no-amount",
            r#"node.kind == "reward" && !("amount" in node.attrs)"#,
            "none",
        ),
    ];
    assert_eq!(
        findings(src, &rules),
        found(&[
            ("L-AMOUNT-TEXT", 7, "+005"),
            ("L-AMOUNT-TEXT", 8, "5"),
            ("L-NO-AMOUNT", 9, "none"),
        ])
    );
}

/// The `preceding` view is never copied: a rule reading it by key builds
/// no map, and a whole-map read builds one per row that reads it.
#[test]
fn preceding_is_materialized_only_by_a_whole_map_read() {
    let src = format!(
        "{SCENE}## A\n::interaction{{kind=\"greet\"}}\n<branch id=\"b\">\n<choice id=\"c1\" text=\"1\">\n\
         ::option{{value=\"accept\"}}\n</choice>\n</branch>\n"
    );
    let keyed = run(
        &src,
        &recipe(&["interaction-kind-known", "option-fits-interaction"]),
    );
    assert_eq!(keyed.node_stats.rows, 4);
    assert!(keyed.node_stats.preceding_lookups > 0);
    assert_eq!(keyed.node_stats.preceding_materialized, 0);
    let whole = run(
        &src,
        &[node_rule(
            "scope-size",
            "size(node.preceding) > 1",
            "{node.tag}",
        )],
    );
    assert_eq!(whole.node_stats.preceding_materialized, 4);
    let sizes: Vec<_> = whole
        .diagnostics
        .iter()
        .filter(|(_, d)| d.code == "L-SCOPE-SIZE")
        .map(|(_, d)| (d.span.line, d.message.as_str()))
        .collect();
    assert_eq!(sizes, [(8, "choice"), (9, "option")]);
    assert_eq!(run(&src, &[]).node_stats, Default::default());
}

// ---------------------------------------------------------------------------
// Row fields
// ---------------------------------------------------------------------------

fn text(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => format!("{other:?}"),
    }
}

fn map(v: &Value) -> &BTreeMap<String, Value> {
    match v {
        Value::Map(m) => m,
        other => panic!("map expected, got {other:?}"),
    }
}

/// One row as `kind:tag sp=… i=<index>[f][l] d=… parent=… prev=… next=…
/// pre=[tags] sec=index:title:id attrs={k=v,…}`.
fn row_line(row: &NodeRow<'_>) -> String {
    let whole = row.materialize();
    let m = map(&whole);
    let summary = |key: &str| {
        let s = map(&m[key]);
        format!("{}:{}", text(&s["kind"]), text(&s["tag"]))
    };
    let attrs = map(&m["attrs"])
        .iter()
        .map(|(k, v)| format!("{k}={}", text(v)))
        .collect::<Vec<_>>()
        .join(",");
    let pre = map(&m["preceding"])
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    let sec = map(&m["section"]);
    format!(
        "{}:{} sp={} i={}{}{} d={} parent={} prev={} next={} pre=[{pre}] sec={}:{}:{} attrs={{{attrs}}}",
        text(&m["kind"]),
        text(&m["tag"]),
        text(&m["speaker"]),
        text(&m["index"]),
        if m["first"] == Value::Bool(true) { "f" } else { "" },
        if m["last"] == Value::Bool(true) { "l" } else { "" },
        text(&m["depth"]),
        summary("parent"),
        summary("prev"),
        summary("next"),
        text(&sec["index"]),
        text(&sec["title"]),
        text(&sec["id"]),
    )
}

fn rows(src: &str) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(src);
    let mut out = Vec::new();
    walk_nodes(&doc, src, &mut |row| out.push(row_line(row)));
    out
}

const FIELDS_SCENE: &str = "---\nid: s\nkind: scene\n---\n## Intro {#intro}\n\
::bg{id=\"room\"}\n@mira{emotion=\"fond\" vo}: Hello.\n::set{run.x = 1}\n\
<branch id=\"b\">\n<choice id=\"c1\" text=\"One\">\n::sfx{id=\"a\"}\n</choice>\n\
<choice id=\"c2\" text=\"Two\">\n@narrator: Narrated.\n</choice>\n</branch>\n\
<hub id=\"h\">\n<choice id=\"h1\" text=\"Ask\">\n::label{name=\"l\"}\n</choice>\n\
<return>\n::jump{to=\"l\"}\n</return>\n</hub>\n\
<match subject=\"run.x\">\n<when is=\"1\">\n::assert{met(a, b)}\n</when>\n\
<otherwise>\n::retract{met(a, b)}\n</otherwise>\n</match>\n\
<timeline duration=\"1\">\n<track subject=\"camera\">\n::camera{focus=\"m\"}\n</track>\n</timeline>\n\
## Two\n::end\n";

#[test]
fn every_row_field_of_a_scene() {
    let s1 = "sec=1:Intro:intro";
    let want = [
        format!("directive:bg sp= i=0f d=0 parent=section: prev=: next=line: pre=[] {s1} attrs={{id=room}}"),
        format!("line: sp=mira i=1 d=0 parent=section: prev=directive:bg next=directive:set pre=[bg] {s1} attrs={{emotion=fond,vo=true}}"),
        format!("directive:set sp= i=2 d=0 parent=section: prev=line: next=branch:branch pre=[bg] {s1} attrs={{}}"),
        format!("branch:branch sp= i=3 d=0 parent=section: prev=directive:set next=hub:hub pre=[bg,set] {s1} attrs={{id=b}}"),
        format!("choice:choice sp= i=0f d=1 parent=branch:branch prev=: next=choice:choice pre=[bg,branch,set] {s1} attrs={{id=c1,text=One}}"),
        format!("directive:sfx sp= i=0fl d=2 parent=choice:choice prev=: next=: pre=[bg,branch,choice,set] {s1} attrs={{id=a}}"),
        format!("choice:choice sp= i=1l d=1 parent=branch:branch prev=choice:choice next=: pre=[bg,branch,choice,set] {s1} attrs={{id=c2,text=Two}}"),
        format!("line: sp=narrator i=0fl d=2 parent=choice:choice prev=: next=: pre=[bg,branch,choice,set] {s1} attrs={{}}"),
        format!("hub:hub sp= i=4 d=0 parent=section: prev=branch:branch next=match:match pre=[bg,branch,set] {s1} attrs={{id=h}}"),
        format!("choice:choice sp= i=0f d=1 parent=hub:hub prev=: next=return:return pre=[bg,branch,hub,set] {s1} attrs={{id=h1,text=Ask}}"),
        format!("directive:label sp= i=0fl d=2 parent=choice:choice prev=: next=: pre=[bg,branch,choice,hub,set] {s1} attrs={{name=l}}"),
        format!("return:return sp= i=1l d=1 parent=hub:hub prev=choice:choice next=: pre=[bg,branch,choice,hub,set] {s1} attrs={{}}"),
        format!("directive:jump sp= i=0fl d=2 parent=return:return prev=: next=: pre=[bg,branch,choice,hub,return,set] {s1} attrs={{to=l}}"),
        format!("match:match sp= i=5 d=0 parent=section: prev=hub:hub next=timeline:timeline pre=[bg,branch,hub,set] {s1} attrs={{subject=run.x}}"),
        format!("when:when sp= i=0f d=1 parent=match:match prev=: next=otherwise:otherwise pre=[bg,branch,hub,match,set] {s1} attrs={{is=1}}"),
        format!("directive:assert sp= i=0fl d=2 parent=when:when prev=: next=: pre=[bg,branch,hub,match,set,when] {s1} attrs={{}}"),
        format!("otherwise:otherwise sp= i=1l d=1 parent=match:match prev=when:when next=: pre=[bg,branch,hub,match,set,when] {s1} attrs={{}}"),
        format!("directive:retract sp= i=0fl d=2 parent=otherwise:otherwise prev=: next=: pre=[bg,branch,hub,match,otherwise,set,when] {s1} attrs={{}}"),
        format!("timeline:timeline sp= i=6l d=0 parent=section: prev=match:match next=: pre=[bg,branch,hub,match,set] {s1} attrs={{duration=1}}"),
        "directive:end sp= i=0fl d=0 parent=section: prev=: next=: pre=[] sec=2:Two: attrs={}".to_string(),
    ];
    assert_eq!(rows(FIELDS_SCENE), want);
}

/// CRLF line ends and multi-byte text give the same rows.
#[test]
fn crlf_and_multibyte_rows_are_the_same() {
    let lf = FIELDS_SCENE
        .replace("Hello.", "안녕, 별빛.")
        .replace("name=\"l\"", "name=\"별\"");
    let crlf = lf.replace('\n', "\r\n");
    let (a, b) = (rows(&lf), rows(&crlf));
    assert_eq!(a, b);
    assert!(a.iter().any(|r| r.ends_with("attrs={name=별}")), "{a:?}");
}

#[test]
fn every_row_field_of_quest_and_lore_documents() {
    let quest = "---\nid: q\nkind: quest\n---\n<quest id=\"q1\" title=\"One\">\n\
                 <reward kind=\"XP\" amount=\"+005\"/>\n<objective id=\"o\" done=\"true\">\n\
                 @a: Done.\n<reward kind=\"COINS\"/>\n</objective>\n\
                 <on event=\"questComplete\">\n::accept{quest=\"q2\"}\n</on>\n</quest>\n\
                 <quest id=\"q2\">\n<objective id=\"p\" done=\"true\"/>\n</quest>\n";
    let out = "sec=0::";
    assert_eq!(
        rows(quest),
        [
            format!("quest:quest sp= i=0f d=0 parent=document: prev=: next=quest:quest pre=[] {out} attrs={{id=q1,title=One}}"),
            format!("reward:reward sp= i=0f d=1 parent=quest:quest prev=: next=objective:objective pre=[quest] {out} attrs={{amount=+005,kind=XP}}"),
            format!("objective:objective sp= i=1 d=1 parent=quest:quest prev=reward:reward next=on:on pre=[quest,reward] {out} attrs={{done=true,id=o}}"),
            format!("line: sp=a i=0f d=2 parent=objective:objective prev=: next=reward:reward pre=[objective,quest,reward] {out} attrs={{}}"),
            format!("reward:reward sp= i=1l d=2 parent=objective:objective prev=line: next=: pre=[objective,quest,reward] {out} attrs={{kind=COINS}}"),
            format!("on:on sp= i=2l d=1 parent=quest:quest prev=objective:objective next=: pre=[objective,quest,reward] {out} attrs={{event=questComplete}}"),
            format!("directive:accept sp= i=0fl d=2 parent=on:on prev=: next=: pre=[objective,on,quest,reward] {out} attrs={{quest=q2}}"),
            format!("quest:quest sp= i=1l d=0 parent=document: prev=quest:quest next=: pre=[] {out} attrs={{id=q2}}"),
            format!("objective:objective sp= i=0fl d=1 parent=quest:quest prev=: next=: pre=[quest] {out} attrs={{done=true,id=p}}"),
        ]
    );
    let lore = "---\nid: l\nkind: lore\n---\n<entry id=\"e1\">\n@a: One.\n</entry>\n\
                <entry id=\"e2\">\n@a: Two.\n</entry>\n";
    assert_eq!(
        rows(lore),
        [
            format!("entry:entry sp= i=0f d=0 parent=document: prev=: next=entry:entry pre=[] {out} attrs={{id=e1}}"),
            format!("line: sp=a i=0fl d=1 parent=entry:entry prev=: next=: pre=[entry] {out} attrs={{}}"),
            format!("entry:entry sp= i=1l d=0 parent=document: prev=entry:entry next=: pre=[] {out} attrs={{id=e2}}"),
            format!("line: sp=a i=0fl d=1 parent=entry:entry prev=: next=: pre=[entry] {out} attrs={{}}"),
        ]
    );
}

/// A `preceding` entry is `{ attrs }` of the nearest node on the path: the
/// quest-level reward, not the objective's later one.
#[test]
fn preceding_entries_hold_the_nearest_attrs() {
    let src = "---\nid: q\nkind: quest\n---\n<quest id=\"q1\">\n\
               <reward kind=\"XP\"/>\n<objective id=\"o\" done=\"true\">\n@a: Done.\n\
               <reward kind=\"COINS\"/>\n@a: After.\n</objective>\n</quest>\n";
    let rule = node_rule(
        "nearest-reward",
        r#"node.kind == "line" && "reward" in node.preceding"#,
        "{node.preceding.reward.attrs.kind}",
    );
    assert_eq!(
        findings(src, &[rule]),
        found(&[
            ("L-NEAREST-REWARD", 8, "XP"),
            ("L-NEAREST-REWARD", 10, "COINS"),
        ])
    );
}

/// A marker a nested body overwrites is the nested one inside it and the
/// outer one again after it; a sibling's later marker replaces it.
#[test]
fn preceding_is_restored_when_a_body_is_left() {
    let src = format!(
        "{SCENE}## A\n::m{{v=\"1\"}}\n@a: one\n<branch id=\"b\">\n<choice id=\"c\" text=\"t\">\n\
         ::m{{v=\"2\"}}\n@a: two\n</choice>\n</branch>\n@a: one again\n::m{{v=\"3\"}}\n@a: three\n"
    );
    let rule = node_rule(
        "marker",
        r#"node.kind == "line" && "m" in node.preceding"#,
        "{node.preceding.m.attrs.v}",
    );
    assert_eq!(
        findings(&src, &[rule]),
        found(&[
            ("L-MARKER", 7, "1"),
            ("L-MARKER", 11, "2"),
            ("L-MARKER", 14, "1"),
            ("L-MARKER", 16, "3"),
        ])
    );
}
