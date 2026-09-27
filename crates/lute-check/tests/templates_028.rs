//! dsl 0.28.0 §1, §3 — beat templates: reserved param names refused where
//! they are declared, a use's `when=` that replaces the template's warns,
//! `F[@param]` in a header reads the member, and one mistake in one argument
//! is one report.

use lute_check::{
    check, parse_meta, parse_meta_kind, resolve_components, CheckInput, ComponentSet, MetaKind,
    Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;
use lute_test_vocab::vocab_snapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_tpl028_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const LORE_HEAD: &str = "---\nkind: lore\nid: bonds\ncomponents: [t.lute]\n\
state:\n  user.bond: { type: number, default: 0 }\n---\n\n";

/// `lore` checked against the template `component` (written to `t.lute`).
fn run(component: &str, lore: &str) -> (CheckInput, Vec<Diagnostic>) {
    let dir = unique_dir();
    std::fs::write(dir.join("t.lute"), component).unwrap();
    let (doc, _) = lute_syntax::parse(lore);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components: ComponentSet = resolve_components(&dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: lore.to_string(),
        uri: "bonds.lute".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: SchemaImports::default(),
        components,
        defaults: Default::default(),
    };
    let diags = check(&input).diagnostics;
    (input, diags)
}

fn desugared(input: &CheckInput) -> Document {
    let (mut doc, _) = lute_syntax::parse(&input.text);
    lute_check::desugar_document(&mut doc, input);
    doc
}

/// A component file's own frontmatter diagnostics.
fn component_diags(src: &str) -> Vec<Diagnostic> {
    let (doc, _) = lute_syntax::parse(src);
    parse_meta_kind(
        &doc.meta,
        &CapabilitySnapshot::default(),
        MetaKind::Component,
    )
    .1
}

#[test]
fn a_template_param_named_like_a_header_key_is_refused_where_declared() {
    let src = "---\ncomponent: topic\nparams:\n  who: string\n  title: string\n  once: { type: string, default: \"\" }\n\
beat:\n  on: examine\n  title: \"About @title\"\n---\n\n## T\n\n@narrator: {{@title}}.\n";
    let diags = component_diags(src);
    let refused: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == "E-TEMPLATE").collect();
    assert_eq!(refused.len(), 2, "{diags:#?}");
    assert!(refused[0].message.contains("param `title`"), "{refused:#?}");
    assert!(refused[0].message.contains("rename it (e.g. `heading`)"));
    // Anchored at the param's own entry under `params:`, not the header key.
    let at = refused[0].span.byte_start;
    assert_eq!(&src[at..at + 5], "title");
    assert!(
        src[..at].ends_with("string\n  "),
        "the param entry, got {at}"
    );

    // The use that writes `title=` gets no second report about the param.
    let lore =
        format!("{LORE_HEAD}<beat use=\"topic\" id=\"a\" who=\"watch\" title=\"the watch\"/>\n");
    let (_, diags) = run(src, &lore);
    assert!(
        !diags.iter().any(|d| d.code == "E-COMPONENT-ARG"),
        "{diags:#?}"
    );
}

#[test]
fn a_component_param_named_when_or_component_is_refused_and_title_is_not() {
    let plain = "---\ncomponent: greet\nparams:\n  when: string\n  component: string\n  title: string\n---\n\n## A\n\n@narrator: {{@title}}.\n";
    let diags = component_diags(plain);
    let refused: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == "E-TEMPLATE")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(refused.len(), 2, "{diags:#?}");
    assert!(refused
        .iter()
        .any(|m| m.contains("param `when`") && m.contains("guard")));
    assert!(refused.iter().any(|m| m.contains("param `component`")));
    // `title` names nothing of a plain `::use`.
    assert!(!refused.iter().any(|m| m.contains("param `title`")));
}

const GIFT: &str = "---\ncomponent: gift\nparams:\n  who: string\n  need: number\n\
beat:\n  on: talk\n  once: user\n  when: \"user.bond >= @need && @who != ''\"\n---\n\n## Gift\n\n@narrator: Here.\n";

#[test]
fn a_use_when_that_replaces_the_template_when_warns() {
    let lore = format!(
        "{LORE_HEAD}<beat use=\"gift\" id=\"maud\" who=\"maud\" need=\"4\" when=\"user.bond >= 0\"/>\n"
    );
    let (_, diags) = run(GIFT, &lore);
    let warn: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.code == "W-TEMPLATE-OVERRIDE")
        .collect();
    assert_eq!(warn.len(), 1, "{diags:#?}");
    let m = &warn[0].message;
    assert!(m.contains("template `gift`"), "{m}");
    assert!(
        m.contains("`when: user.bond >= @need && @who != ''`"),
        "{m}"
    );
    // `need` (and `who`) fed only the dropped condition: named as unused.
    assert!(m.contains("`need=\"4\"`"), "{m}");
    assert!(
        m.contains("user.bond >= 4 && maud != '' && user.bond >= 0"),
        "{m}"
    );
    // At the use's own `when=` value.
    let at = warn[0].span.byte_start;
    assert!(lore[at..].starts_with("user.bond >= 0"), "{at}");

    // A use without `when=` draws nothing.
    let lore = format!("{LORE_HEAD}<beat use=\"gift\" id=\"maud\" who=\"maud\" need=\"4\"/>\n");
    let (_, diags) = run(GIFT, &lore);
    assert!(
        !diags.iter().any(|d| d.code == "W-TEMPLATE-OVERRIDE"),
        "{diags:#?}"
    );
}

#[test]
fn a_bracketed_param_index_in_a_header_reads_the_member_and_the_dot_form_hints() {
    let bracket =
        "---\ncomponent: chat\nparams:\n  who: string\n  need: { type: number, default: 0 }\n\
beat:\n  on: talk\n  when: \"user.bond[@who] >= @need\"\n---\n\n## Chat\n\n@narrator: Oh.\n";
    let lore = format!("{LORE_HEAD}<beat use=\"chat\" id=\"maraOne\" who=\"mara\"/>\n");
    let (input, _) = run(bracket, &lore);
    let doc = desugared(&input);
    assert_eq!(
        doc.beats[0].when.as_ref().unwrap().raw,
        "user.bond.mara >= 0",
        "`[@who]` reads the member the argument names"
    );

    let dot = bracket.replace("user.bond[@who]", "user.bond.@who");
    let (doc, _) = lute_syntax::parse(&dot);
    let (typed, _) = parse_meta_kind(
        &doc.meta,
        &CapabilitySnapshot::default(),
        MetaKind::Component,
    );
    let template = typed.beat_template.expect("a template");
    let hints = lute_check::templates::check_template_header(&template, &Default::default());
    assert_eq!(hints.len(), 1, "{hints:#?}");
    assert_eq!(hints[0].code, "W-TEMPLATE-DOT-PARAM");
    assert!(
        hints[0].message.contains("did you mean `user.bond[@who]`?"),
        "{}",
        hints[0].message
    );
}

#[test]
fn one_bad_argument_is_one_report() {
    // A missing argument: the header key reading it is not derived, so no
    // `E-UNDECLARED-REF @need` beside the missing-argument error.
    let lore = format!("{LORE_HEAD}<beat use=\"gift\" id=\"a\" who=\"brann\"/>\n");
    let (_, diags) = run(GIFT, &lore);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(errors[0].message.contains("requires argument `need`"));

    // A mistyped argument: the type error alone, no `E-CEL-PROFILE lots`.
    let lore = format!("{LORE_HEAD}<beat use=\"gift\" id=\"b\" who=\"sefa\" need=\"lots\"/>\n");
    let (_, diags) = run(GIFT, &lore);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].code, "E-COMPONENT-ARG");
    assert!(errors[0].message.contains("does not fit `number`"));
}

#[test]
fn a_header_may_declare_also_and_a_title_renders_its_params() {
    let src = "---\ncomponent: idle\nparams:\n  who: speaker\n\
beat:\n  on: talk\n  title: \"{{@who}} idles\"\n  also: true\n---\n";
    let lore = "---\nkind: lore\nid: talks\ncomponents: [t.lute]\n\
cast:\n  isolde: { name: Isolde }\n---\n\n<beat use=\"idle\" id=\"isoldeIdle\" who=\"isolde\">\n@isolde: Posted.\n</beat>\n";
    let (input, diags) = run(src, lore);
    assert!(
        !diags.iter().any(|d| d.code == "E-TEMPLATE"),
        "`also` is a header key: {diags:#?}"
    );
    let doc = desugared(&input);
    let beat = &doc.beats[0];
    assert_eq!(beat.title.as_ref().unwrap().0, "Isolde idles");
    assert!(lute_check::bundle_beat_also(beat));
}

#[test]
fn an_entry_use_is_one_error_naming_beat() {
    let lore = format!(
        "{LORE_HEAD}<entry use=\"gift\" id=\"e\" who=\"maud\" need=\"2\">\n@narrator: x\n</entry>\n"
    );
    let (_, diags) = run(GIFT, &lore);
    let errors: Vec<&Diagnostic> = diags
        .iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].code, "E-TEMPLATE");
    assert!(errors[0]
        .message
        .contains("beat templates apply to `<beat>` only"));
}

/// `src` checked on its own, with `components` (files in `dir`) imported.
fn check_in(dir: &std::path::Path, uri: &str, src: &str) -> Vec<Diagnostic> {
    let (doc, _) = lute_syntax::parse(src);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: src.to_string(),
        uri: dir.join(uri).display().to_string(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: SchemaImports::default(),
        components,
        defaults: Default::default(),
    };
    check(&input).diagnostics
}

fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .collect()
}

#[test]
fn a_beat_template_body_needs_no_heading_and_an_ordinary_component_does() {
    let headless = GIFT
        .replace("## Gift\n\n", "")
        .replace(" && @who != ''", "");
    let (doc, diags) = lute_syntax::parse(&headless);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(doc.shots.len(), 1, "the body is the template's one shot");
    assert_eq!(doc.shots[0].body.len(), 1);

    let lore = format!("{LORE_HEAD}<beat use=\"gift\" id=\"maud\" who=\"maud\" need=\"4\"/>\n");
    let (input, diags) = run(&headless, &lore);
    assert!(errors(&diags).is_empty(), "{diags:#?}");
    // The use still expands to the template's body.
    let doc = desugared(&input);
    assert!(
        matches!(&doc.beats[0].body[0], lute_syntax::ast::Node::Directive(d) if d.tag == "use"),
        "{:#?}",
        doc.beats[0].body
    );

    // Without a `beat:` header the heading is still required.
    let plain = "---\ncomponent: nod\n---\n\n@narrator: A nod.\n";
    let (_, diags) = lute_syntax::parse(plain);
    assert!(
        diags.iter().any(|d| d.code == "E-CONTENT-OUTSIDE-SHOT"),
        "{diags:#?}"
    );
}

#[test]
fn a_speaker_param_passes_through_a_nested_component() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("inner.lute"),
        "---\ncomponent: inner\neffects: true\nparams:\n  who: speaker\n---\n\n## Inner\n\n\
::set{run.aff[@who] += 1}\n@narrator: {{@who}} smiles.\n",
    )
    .unwrap();
    let outer = "---\ncomponent: outer\neffects: true\ncomponents: [inner.lute]\nparams:\n  who: speaker\n\
state:\n  run.aff: { type: number, default: 0, per: suitor }\nentities:\n  suitor: { members: [ren, kai] }\n\
---\n\n## Outer\n\n@@who: Hello.\n::use{component=\"inner\" who=@who}\n";
    std::fs::write(dir.join("outer.lute"), outer).unwrap();
    // The outer component's own check: `who=@who` forwards its speaker.
    let diags = check_in(&dir, "outer.lute", outer);
    assert!(
        !diags.iter().any(|d| d.code == "E-COMPONENT-ARG"),
        "{diags:#?}"
    );

    let host = |who: &str| {
        format!(
            "---\nkind: lore\nid: talk\ncomponents: [outer.lute, inner.lute]\n\
state:\n  run.aff: {{ type: number, default: 0, per: suitor }}\nentities:\n  suitor: {{ members: [ren, kai] }}\n\
---\n\n\
<beat id=\"hi\" on=\"talk\">\n  ::use{{component=\"outer\" who=\"{who}\"}}\n</beat>\n"
        )
    };
    let diags = check_in(&dir, "talk.lute", &host("ren"));
    assert!(errors(&diags).is_empty(), "{diags:#?}");
    // The host judges the forwarded id where it binds it, through `inner`.
    let diags = check_in(&dir, "talk.lute", &host("narrator"));
    assert!(
        diags.iter().any(|d| d.code == "E-COMPONENT-ARG"
            && d.message
                .contains("argument `who` to component `outer` picks the member")),
        "{diags:#?}"
    );
}

#[test]
fn a_component_body_reading_a_def_is_one_report_naming_the_param() {
    let dir = unique_dir();
    let comp = "---\ncomponent: lit\nstate:\n  run.hour: { type: number, default: 8 }\n\
defs:\n  late: \"run.hour >= 20\"\n---\n\n## Lit\n\n@narrator{when=\"@late\"}: It is late.\n";
    std::fs::write(dir.join("lit.lute"), comp).unwrap();
    let advice = "declare the param `late: { type: bool, default: \"@late\" }` under `params:`";

    // The component's own check: at the read, not "not a declared def".
    let own = check_in(&dir, "lit.lute", comp);
    let own: Vec<&Diagnostic> = errors(&own);
    assert_eq!(own.len(), 1, "{own:#?}");
    assert_eq!(own[0].code, "E-COMPONENT-STATE");
    assert!(own[0].message.contains(advice), "{}", own[0].message);
    let at = own[0].span.byte_start;
    assert!(comp[at..].starts_with("@late"), "{at}");

    // A host reports the same diagnostic (at its `::use`, the component's
    // copy related) — identical, so `check-project` keeps only one.
    let host = "---\nkind: scene\nid: s\ntitle: S\ncomponents: [lit.lute]\n\
state:\n  run.hour: { type: number, default: 8 }\ndefs:\n  late: \"run.hour >= 20\"\n---\n\n\
## S\n\n::use{component=\"lit\"}\n";
    let diags = check_in(&dir, "s.lute", host);
    let errs = errors(&diags);
    assert_eq!(errs.len(), 1, "{errs:#?}");
    assert!(
        !diags.iter().any(|d| d.code == "E-UNDECLARED-REF"),
        "{diags:#?}"
    );
    let inner = &errs[0].related[0].diagnostic;
    assert_eq!(
        (&inner.code, inner.span.byte_start, &inner.message),
        (&own[0].code, own[0].span.byte_start, &own[0].message)
    );
}
