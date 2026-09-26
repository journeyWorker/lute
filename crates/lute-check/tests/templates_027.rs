//! dsl 0.27.0 §6 — beat templates. A component with a `beat:` header is a
//! template; `<beat use="…">` in a bundle document desugars (before any
//! check) into an ordinary bundle beat: the header keys it does not write,
//! `@param`-substituted and spanned at the use site, and a body that starts
//! with the template's `::use`. Misuse is `E-TEMPLATE`; the arguments are
//! checked like any `::use`'s.

use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{AttrValue, Document, Node};
use lute_test_vocab::vocab_snapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_tpl027_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const BOND: &str = "---\ncomponent: bondStory\n\
params:\n  who: { type: string }\n  need: { type: number, default: 0 }\n  prev: { type: string, default: \"\" }\n\
beat:\n  on: bond\n  once: user\n  when: \"user.bond >= @need\"\n  after: \"@prev\"\n  priority: 2\n---\n\
## Bond\n@narrator: A bond story begins.\n";

const LORE_HEAD: &str = "---\nkind: lore\nid: bonds\ncomponents: [bond.lute]\n\
state:\n  user.bond: { type: number, default: 0 }\n---\n\n";

/// `lore` checked against `bond.lute` (the `BOND` template unless
/// `component` overrides it), components resolved from disk. Returns the
/// input (for [`desugared`]) and the check's diagnostics.
fn run(component: &str, lore: &str) -> (CheckInput, Vec<Diagnostic>) {
    let dir = unique_dir();
    std::fs::write(dir.join("bond.lute"), component).unwrap();
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

fn codes(diags: &[Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_use_derives_the_header_and_opens_with_the_template() {
    let lore = format!(
        "{LORE_HEAD}<beat use=\"bondStory\" id=\"first\" who=\"aria\">\n@narrator: Hello.\n</beat>\n\n\
<beat use=\"bondStory\" id=\"second\" who=\"aria\" need=\"1\" prev=\"bonds.first\" once=\"run\">\n@narrator: Again.\n</beat>\n"
    );
    let (input, diags) = run(BOND, &lore);
    assert!(diags.is_empty(), "{diags:#?}");
    let doc = desugared(&input);
    let use_at = lore.find("bondStory").unwrap();

    let first = &doc.beats[0];
    assert_eq!(first.on.as_ref().unwrap().0, "bond");
    assert_eq!(first.once.as_ref().unwrap().0, "user");
    assert_eq!(first.priority.as_ref().unwrap().0, "2");
    assert_eq!(
        first.when.as_ref().unwrap().raw,
        "user.bond >= 0",
        "the default fills `@need`"
    );
    assert!(first.after.is_none(), "an empty `after:` is omitted");
    // Derived keys sit at the use site, not in the component file.
    assert_eq!(first.on.as_ref().unwrap().1.byte_start, use_at);
    // The template's body runs first, then the use's own.
    match &first.body[0] {
        Node::Directive(d) => {
            assert_eq!(d.tag, "use");
            assert!(d.attrs.iter().any(|a| a.key == "component"
                && matches!(&a.value, AttrValue::Str(s) if s == "bondStory")));
            assert!(d.attrs.iter().any(|a| a.key == "who"));
        }
        other => panic!("expected the template's ::use first, got {other:?}"),
    }
    assert!(matches!(&first.body[1], Node::Line(l) if l.text == "Hello."));
    assert!(first.attrs.is_empty(), "the arguments moved into the ::use");

    let second = &doc.beats[1];
    assert_eq!(second.when.as_ref().unwrap().raw, "user.bond >= 1");
    // A bare id in `after:` means `visited(id)`; an attribute on the use wins.
    assert_eq!(second.after.as_ref().unwrap().0, "visited(\"bonds.first\")");
    assert_eq!(second.once.as_ref().unwrap().0, "run");
}

#[test]
fn arguments_are_checked_like_use_args() {
    let lore = format!(
        "{LORE_HEAD}<beat use=\"bondStory\" id=\"a\" whom=\"aria\">\n@narrator: Hi.\n</beat>\n"
    );
    let (_, diags) = run(BOND, &lore);
    let args: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "E-COMPONENT-ARG")
        .collect();
    // The misspelt argument names the param it misses (ML-F6), which is then
    // not reported again as required.
    assert!(
        args.iter().any(|d| d
            .message
            .contains("no parameter `whom` — did you mean `who`?")),
        "{diags:#?}"
    );
    assert!(
        !args
            .iter()
            .any(|d| d.message.contains("requires argument `who`")),
        "{diags:#?}"
    );
}

#[test]
fn a_body_less_template_and_self_closing_use_make_a_one_line_beat() {
    let trainer = "---\ncomponent: trainer\nparams: { who: { type: string } }\n\
beat: { on: battle, once: user, title: \"Trainer @who\" }\n---\n";
    let lore = "---\nkind: lore\nid: route\ncomponents: [bond.lute]\n---\n\n\
<beat use=\"trainer\" id=\"r3Joey\" who=\"joey\"/>\n";
    let (input, diags) = run(trainer, lore);
    assert!(diags.is_empty(), "{diags:#?}");
    let doc = desugared(&input);
    assert_eq!(doc.beats[0].title.as_ref().unwrap().0, "Trainer joey");
    assert_eq!(doc.beats[0].on.as_ref().unwrap().0, "battle");
}

#[test]
fn body_places_the_use_body_and_closes_with_a_marker() {
    let wrap = "---\ncomponent: wrap\nbeat: { on: talk }\n---\n## W\n@narrator: Before.\n::body\n@narrator: After.\n";
    let lore = "---\nkind: lore\nid: t\ncomponents: [bond.lute]\n---\n\n\
<beat use=\"wrap\" id=\"a\">\n@narrator: Middle.\n</beat>\n";
    let (input, diags) = run(wrap, lore);
    assert!(diags.is_empty(), "{diags:#?}");
    let doc = desugared(&input);
    let body = &doc.beats[0].body;
    assert!(matches!(body.last(), Some(Node::Directive(d)) if d.tag == "body"));
}

#[test]
fn misuse_is_e_template() {
    // An unknown template, with a did-you-mean.
    let lore =
        format!("{LORE_HEAD}<beat use=\"bondStry\" id=\"a\" who=\"x\">\n@narrator: Hi.\n</beat>\n");
    let (_, diags) = run(BOND, &lore);
    let t: Vec<_> = diags.iter().filter(|d| d.code == "E-TEMPLATE").collect();
    assert_eq!(t.len(), 1, "{diags:#?}");
    assert!(
        t[0].message.contains("did you mean `bondStory`"),
        "{}",
        t[0].message
    );
    assert_eq!(t[0].span.byte_start, lore.find("bondStry").unwrap());

    // A component without a `beat:` header.
    let plain = "---\ncomponent: bondStory\n---\n## B\n@narrator: Hi.\n";
    let lore = format!("{LORE_HEAD}<beat use=\"bondStory\" id=\"a\">\n@narrator: Hi.\n</beat>\n");
    let (_, diags) = run(plain, &lore);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "E-TEMPLATE" && d.message.contains("declares no `beat:`")),
        "{diags:#?}"
    );

    // `::body` in an ordinary scene places nothing.
    let scene = "---\nkind: scene\nid: s\n---\n## S\n@narrator: Hi.\n::body\n";
    let (_, diags) = run(BOND, scene);
    assert_eq!(codes(&diags), vec!["E-TEMPLATE"], "{diags:#?}");
}

#[test]
fn a_bad_header_is_e_template_in_the_component() {
    let bad = "---\ncomponent: c\nparams: { who: { type: string } }\n\
beat:\n  on: talk\n  id: nope\n  target: \"npc.@whom\"\n---\n";
    let (doc, _) = lute_syntax::parse(bad);
    let (_, diags) = lute_check::meta::parse_meta_kind(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Component,
    );
    let t: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == "E-TEMPLATE")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(t.len(), 2, "{diags:#?}");
    assert!(t.iter().any(|m| m.contains("names its own `id=`")));
    assert!(t.iter().any(|m| m.contains("`@whom`")));
}

/// `text` (a component file) checked on its own, as `lute check` does.
fn check_component(text: &str) -> Vec<Diagnostic> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "bond.lute".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: SchemaImports::default(),
        components: ComponentSet::default(),
        defaults: Default::default(),
    };
    check(&input).diagnostics
}

/// ML-F1: a header value no argument changes is judged once, at its key in
/// the component; the uses derive nothing for it and report nothing.
#[test]
fn a_fixed_header_fault_is_reported_once_at_the_header() {
    let bad = BOND.replace("once: user", "once: sometimes");
    let lore = format!(
        "{LORE_HEAD}<beat use=\"bondStory\" id=\"a\" who=\"x\">\n@narrator: Hi.\n</beat>\n\n\
<beat use=\"bondStory\" id=\"b\" who=\"y\">\n@narrator: Hi.\n</beat>\n"
    );
    let (input, diags) = run(&bad, &lore);
    assert!(diags.is_empty(), "the uses report nothing: {diags:#?}");
    assert!(
        desugared(&input).beats[0].once.is_none(),
        "the bad value is not derived"
    );

    let own = check_component(&bad);
    let at: Vec<_> = own.iter().filter(|d| d.code == "E-BEAT-ATTR").collect();
    assert_eq!(at.len(), 1, "{own:#?}");
    assert_eq!(at[0].span.byte_start, bad.find("once: sometimes").unwrap());
    assert!(at[0].message.contains("`beat.once`"), "{}", at[0].message);
}

/// ML-F1 / ML-F5: a use of a template that cannot be applied whole — a
/// misspelt header key, an unknown template — says nothing about the `on=`
/// the template would have supplied; one `@param` no param declares is not
/// derived, so it cannot fail to parse at every use.
#[test]
fn a_failed_template_does_not_cascade_into_its_uses() {
    let lore = format!(
        "{LORE_HEAD}<beat use=\"bondStory\" id=\"a\" who=\"x\">\n@narrator: Hi.\n</beat>\n"
    );
    // The importer carries the component's own fault as one related
    // `E-COMPONENT-PARSE` (check-project drops it for the component's line).
    let typo = BOND.replace("  on: bond", "  onn: bond");
    let (_, diags) = run(&typo, &lore);
    assert_eq!(codes(&diags), vec!["E-COMPONENT-PARSE"], "{diags:#?}");

    let undeclared = BOND.replace("after: \"@prev\"", "after: \"@previous\"");
    let (_, diags) = run(&undeclared, &lore);
    assert_eq!(codes(&diags), vec!["E-COMPONENT-PARSE"], "{diags:#?}");

    let unknown = lore.replace("bondStory", "bondStry");
    let (_, diags) = run(BOND, &unknown);
    assert_eq!(codes(&diags), vec!["E-TEMPLATE"], "{diags:#?}");
}

/// ML-F7: an optional extra condition passed as a param and left empty or
/// `true` drops out of the derived `when`, never `… && (true)`.
#[test]
fn an_empty_condition_param_drops_its_conjunct() {
    let only = BOND
        .replace(
            "prev: { type: string, default: \"\" }",
            "prev: { type: string, default: \"\" }\n  only: { type: string, default: \"true\" }",
        )
        .replace(
            "when: \"user.bond >= @need\"",
            "when: \"user.bond >= @need && (@only)\"",
        );
    let lore = format!(
        "{LORE_HEAD}<beat use=\"bondStory\" id=\"a\" who=\"x\">\n@narrator: Hi.\n</beat>\n\n\
<beat use=\"bondStory\" id=\"b\" who=\"x\" only=\"user.bond > 5\">\n@narrator: Hi.\n</beat>\n"
    );
    let (input, diags) = run(&only, &lore);
    assert!(diags.is_empty(), "{diags:#?}");
    let doc = desugared(&input);
    assert_eq!(doc.beats[0].when.as_ref().unwrap().raw, "user.bond >= 0");
    assert_eq!(
        doc.beats[1].when.as_ref().unwrap().raw,
        "user.bond >= 0 && (user.bond > 5)"
    );
}
