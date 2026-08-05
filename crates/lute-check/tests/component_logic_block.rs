//! A component body's PRESENTATIONAL contract (dsl 0.4.0 §6.2) must hold on
//! the standalone leg too — the sibling of `component_content_line.rs`'s
//! Task 7b finding, one construct class out.
//!
//! `walk_component_body` owns the prohibition (`<branch>`/`<hub>`/`<timeline>`/
//! `<on>`/`<objective>`/`::set`/`::assert`/`::retract` are `E-COMPONENT-BODY`;
//! a non-param `<match>` subject is `E-COMPONENT-STATE`/`E-COMPONENT-BODY`),
//! but it is reached ONLY from `validate_components`, which iterates the
//! IMPORTING document's `components.table`. A component file opened as its own
//! root degrades to `DocKind::Scene` (`check.rs`'s `resolve_doc_kind` `None`
//! arm) and walks through `Walker`, where every one of those constructs is
//! perfectly legal.
//!
//! The consequence is a FALSE GREEN, and it points the wrong way from Task 7f's
//! (which made the standalone leg too STRICT): `lute check c.component.lute`
//! reported `ok` for a body that fails at every single call site. The author
//! most likely to check a component file alone is the one writing it.
//!
//! The invariant here is NOT the three-way parity content lines get — a
//! `<branch>` is legal in a scene and forbidden in a component, so scene
//! deliberately diverges. What must hold is that the two COMPONENT legs agree:
//! standalone and `::use` classify the same body identically.
use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lute_clb_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const USE_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
## Shot 1.\n::use{component=\"c\"}\n";

fn check_codes(text: String, components: ComponentSet, snapshot: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput {
        text,
        uri: "scene".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
    };
    let mut v: Vec<String> = check(&input).diagnostics.into_iter().map(|d| d.code).collect();
    v.sort();
    v
}

/// A paramless component FILE carrying `body` — the same source text both legs
/// check, so the only variable is which root walks it.
fn component_file(body: &str) -> String {
    format!("---\ncomponent: c\n---\n## Scene 1.\n{body}\n")
}

/// The component file checked as its own root.
fn standalone_codes(body: &str) -> Vec<String> {
    check_codes(component_file(body), Default::default(), vocab_snapshot())
}

/// The same component body reached through a `::use` from a scene.
fn use_codes(body: &str) -> Vec<String> {
    let dir = unique_dir();
    std::fs::write(dir.join("c.lute"), component_file(body)).unwrap();
    let text = USE_SCENE.to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    check_codes(text, components, vocab_snapshot())
}

/// One body per construct `walk_component_body` rejects, each paired with the
/// code it must produce. Bodies are otherwise clean — no undeclared vocabulary,
/// no `code=` — so the prohibition is the ONLY thing either leg can report.
const FORBIDDEN: &[(&str, &str)] = &[
    // presenting a menu records the selection: a state write (§6.2)
    (
        "<branch id=\"b\">\n<choice id=\"c\" label=\"L\">\n@narrator: hi\n</choice>\n</branch>",
        "E-COMPONENT-BODY",
    ),
    (
        "<hub id=\"h\">\n<choice id=\"c\" label=\"L\">\n@narrator: hi\n</choice>\n</hub>",
        "E-COMPONENT-BODY",
    ),
    // a component body performs no state writes
    ("::set{run.x = 1}", "E-COMPONENT-BODY"),
    ("::assert{awake(vesna)}", "E-COMPONENT-BODY"),
    ("::retract{awake(vesna)}", "E-COMPONENT-BODY"),
    // choreography and quest structure are not presentational
    (
        "<timeline duration=\"1.0\">\n<track subject=\"x\">\n::camera{shake=\"0.2\"}\n</track>\n</timeline>",
        "E-COMPONENT-BODY",
    ),
    (
        "<on event=\"questComplete\">\n@narrator: hi\n</on>",
        "E-COMPONENT-BODY",
    ),
];

/// The load-bearing invariant: a component-contract violation the `::use` leg
/// reports is reported by the standalone leg too, code for code. A bypass on
/// either side fails here.
///
/// Compared over the `E-COMPONENT-*` codes rather than the whole multiset, and
/// the exclusion is measured, not assumed: a `<hub>` additionally draws
/// `E-HUB-NO-EXIT` on the standalone leg, because `fold_branches` — the
/// pre-pass that also folds the implicit `scene.choices.*`/`scene.visited.*`
/// declarations — runs over the ROOT document whatever its role, while the
/// `::use` leg's root is the importing scene, which has no hub of its own.
/// That residual is scene machinery meeting a construct already fatally
/// rejected here; suppressing it means gating a declaration-folding pass other
/// checks read from, which buys a cleaner second diagnostic on an
/// already-failing document and risks the env. Left deliberately.
#[test]
fn component_legs_agree_on_forbidden_constructs() {
    let contract = |codes: Vec<String>| -> Vec<String> {
        codes
            .into_iter()
            .filter(|c| c.starts_with("E-COMPONENT-"))
            .collect()
    };
    for (body, _) in FORBIDDEN {
        assert_eq!(
            contract(standalone_codes(body)),
            contract(use_codes(body)),
            "component-contract diagnostics diverge between a standalone \
             component file and the same body reached through a `::use` for:\n{body}"
        );
    }
}

/// Parity alone would be satisfied by both legs going silent, so pin the
/// verdict itself: each construct actually produces its prohibition code.
#[test]
fn standalone_component_rejects_logic_blocks() {
    for (body, code) in FORBIDDEN {
        let codes = standalone_codes(body);
        assert!(
            codes.iter().any(|c| c == code),
            "a standalone component file must report `{code}` for:\n{body}\ngot: {codes:?}"
        );
    }
}

/// The complement, so the fix cannot be "reject everything in a component
/// root": the forms §6.2 ADMITS must stay clean on the standalone leg —
/// content lines, staging directives, and a param-scoped `<match>`.
#[test]
fn standalone_component_admits_presentational_forms() {
    let admitted = [
        "@narrator: a line is presentational",
        "::auto{character=\"bianca\" action=\"fade-in-up\"}",
    ];
    for body in admitted {
        let codes = standalone_codes(body);
        assert!(
            codes.is_empty(),
            "a presentational component body must check clean standalone, got {codes:?} for:\n{body}"
        );
    }
    // A param-scoped `<match>` needs a declared param, so it does not fit the
    // paramless `component_file` shape above.
    let param_match = "---\ncomponent: c\nparams:\n  tier: string\n---\n## Scene 1.\n\
<match on=\"@tier\">\n<when is=\"gold\">\n@narrator: hi\n</when>\n<otherwise>\n\
@narrator: ho\n</otherwise>\n</match>\n";
    let codes = check_codes(param_match.to_string(), Default::default(), vocab_snapshot());
    assert!(
        codes.is_empty(),
        "a param-scoped `<match>` is admitted by dsl 0.4.0 §6.2 and must check \
         clean standalone, got {codes:?}"
    );
}
