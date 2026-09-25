//! dsl 0.24.0 §4: component `effects: true` (the body may write state; each
//! write is judged at every `::use` against the HOST's schema, anchored at the
//! `::use`) and `speaker` params (a literal cast id). Scenes + component files
//! are written to a temp dir and resolved via `resolve_components`, then run
//! through the assembled `check()` — the `components_use.rs` harness.
use lute_check::{check, parse_meta, resolve_components, CheckInput, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::CapabilitySnapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("lute_c024_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn member(id: &str, name: &str) -> CastMember {
    CastMember {
        id: id.into(),
        name: Some(name.into()),
        ..Default::default()
    }
}

/// `check()` over `scene` with `component` written as `c.lute` next to it and
/// `cast` as the snapshot's declared cast.
fn run(component: &str, scene: &str, cast: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = unique_dir();
    std::fs::write(dir.join("c.lute"), component).unwrap();
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let mut snapshot = load_core_snapshot();
    for (id, name) in cast {
        snapshot.cast.insert((*id).into(), member(id, name));
    }
    let input = CheckInput {
        text: scene.to_string(),
        uri: "scene".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components,
        defaults: Default::default(),
    };
    check(&input).diagnostics
}

fn codes(ds: &[Diagnostic]) -> Vec<&str> {
    ds.iter().map(|d| d.code.as_str()).collect()
}

fn scene(front: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n{front}---\n## Shot 1.\n{body}\n"
    )
}

/// 1-based line of the first line of `text` containing `needle`.
fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
}

const APPROVAL: &str = "---\ncomponent: approval\neffects: true\nparams:\n  delta: number\n---\n\
## Scene 1.\n@narrator: Noted.\n::set{run.approval.isolde += @delta}\n";

const DECLARED: &str = "state:\n  run.approval.isolde: { type: number, default: 0 }\n";

#[test]
fn set_in_effects_body_is_admitted_and_clean_where_the_host_declares_it() {
    let s = scene(DECLARED, "::use{component=\"approval\" delta=\"2\"}");
    let ds = run(APPROVAL, &s, &[]);
    assert!(
        !ds.iter().any(|d| d.code.starts_with("E-")),
        "an effects component writing a host-declared path is clean: {ds:#?}"
    );
}

#[test]
fn set_without_effects_is_still_component_body() {
    let presentational = APPROVAL.replace("effects: true\n", "");
    let s = scene(DECLARED, "::use{component=\"approval\" delta=\"2\"}");
    let ds = run(&presentational, &s, &[]);
    assert!(codes(&ds).contains(&"E-COMPONENT-BODY"), "{ds:#?}");
}

#[test]
fn write_to_a_path_the_host_does_not_declare_is_undeclared_at_the_use() {
    let s = scene("", "@narrator: Before.\n::use{component=\"approval\" delta=\"2\"}");
    let ds = run(APPROVAL, &s, &[]);
    let d = ds
        .iter()
        .find(|d| d.code == "E-UNDECLARED")
        .unwrap_or_else(|| panic!("the host has no `run.approval.isolde`: {ds:#?}"));
    assert_eq!(d.span.line, line_of(&s, "::use{"), "anchored at the `::use`: {d:#?}");
    assert!(d.message.contains("run.approval.isolde"), "{}", d.message);
    assert!(
        !codes(&ds).contains(&"E-COMPONENT-BODY"),
        "the effects body itself is admitted: {ds:#?}"
    );
}

#[test]
fn write_to_an_engine_owned_host_path_is_engine_owned_write_at_the_use() {
    let front = "state:\n  run.approval.isolde: { type: number, default: 0, owner: engine }\n";
    let s = scene(front, "::use{component=\"approval\" delta=\"2\"}");
    let ds = run(APPROVAL, &s, &[]);
    let d = ds
        .iter()
        .find(|d| d.code == "E-ENGINE-OWNED-WRITE")
        .unwrap_or_else(|| panic!("{ds:#?}"));
    assert_eq!(d.span.line, line_of(&s, "::use{"));
}

#[test]
fn set_type_is_judged_against_the_host_decl() {
    let front = "state:\n  run.approval.isolde: { type: bool, default: false }\n";
    let s = scene(front, "::use{component=\"approval\" delta=\"2\"}");
    let ds = run(APPROVAL, &s, &[]);
    assert!(
        ds.iter().any(|d| d.code.starts_with("E-") && d.span.line == line_of(&s, "::use{")),
        "`+=` on a bool path is a type error at the `::use`: {ds:#?}"
    );
}

const VOCAB: &str = "entities:\n  c: { members: [ana, bo] }\n  f: { members: [reds] }\nrelations:\n  inParty: { args: [c] }\n";

#[test]
fn fact_writes_are_judged_against_the_host_vocabulary() {
    let comp = "---\ncomponent: recruit\neffects: true\n---\n## Scene 1.\n::assert{inParty(reds)}\n::retract{knows(ana)}\n";
    let s = scene(VOCAB, "::use{component=\"recruit\"}");
    let ds = run(comp, &s, &[]);
    let at_use = |code: &str| {
        ds.iter()
            .any(|d| d.code == code && d.span.line == line_of(&s, "::use{"))
    };
    assert!(at_use("E-FACT-DOMAIN"), "`reds` is not a `c` in the host: {ds:#?}");
    assert!(at_use("E-RELATION-UNKNOWN"), "the host declares no `knows`: {ds:#?}");

    let ok = "---\ncomponent: recruit\neffects: true\n---\n## Scene 1.\n::assert{inParty(ana)}\n";
    let ds = run(ok, &s, &[]);
    assert!(!ds.iter().any(|d| d.code.starts_with("E-")), "{ds:#?}");
}

#[test]
fn a_presentational_body_may_not_use_an_effects_component() {
    // `c.lute` holds both; the outer one is presentational.
    let comp = "---\ncomponent: outer\ncomponents: [inner.lute]\n---\n## Scene 1.\n::use{component=\"inner\"}\n";
    let dir = unique_dir();
    std::fs::write(
        dir.join("inner.lute"),
        "---\ncomponent: inner\neffects: true\n---\n## Scene 1.\n::set{run.x = 1}\n",
    )
    .unwrap();
    std::fs::write(dir.join("c.lute"), comp).unwrap();
    let s = scene("state:\n  run.x: { type: number, default: 0 }\n", "::use{component=\"outer\"}");
    let (doc, _) = lute_syntax::parse(&s);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let input = CheckInput {
        text: s.clone(),
        uri: "scene".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: resolve_components(&dir, &meta0.components, doc.meta.span),
        defaults: Default::default(),
    };
    let ds = check(&input).diagnostics;
    assert!(
        ds.iter()
            .any(|d| d.code == "E-COMPONENT-BODY" && d.message.contains("effects: true")),
        "{ds:#?}"
    );
}

#[test]
fn an_effects_guard_still_may_not_read_ambient_state() {
    let comp = "---\ncomponent: approval\neffects: true\n---\n## Scene 1.\n@narrator{when=\"run.approval.isolde > 2\"}: Warm.\n";
    let s = scene(DECLARED, "::use{component=\"approval\"}");
    let ds = run(comp, &s, &[]);
    assert!(codes(&ds).contains(&"E-COMPONENT-STATE"), "{ds:#?}");
}

const REACTION: &str = "---\ncomponent: reaction\neffects: true\nparams:\n  who: speaker\n---\n\
## Scene 1.\n@narrator: {{@who}} approves.\n<match on=\"@who\">\n<when is=\"isolde\">\n::set{run.approval.isolde += 1}\n</when>\n<otherwise>\n@narrator: Nobody minds.\n</otherwise>\n</match>\n";

const PARTY: &[(&str, &str)] = &[("isolde", "Isolde Vane"), ("corvin", "Corvin")];

#[test]
fn speaker_arg_must_be_a_cast_member() {
    let s = scene(DECLARED, "::use{component=\"reaction\" who=\"isolda\"}");
    let ds = run(REACTION, &s, PARTY);
    let d = ds
        .iter()
        .find(|d| d.code == "E-CAST-UNKNOWN")
        .unwrap_or_else(|| panic!("{ds:#?}"));
    assert!(d.message.contains("did you mean `isolde`"), "{}", d.message);
    assert!(!codes(&ds).contains(&"E-COMPONENT-ARG"), "{ds:#?}");

    let s = scene(DECLARED, "::use{component=\"reaction\" who=\"isolde\"}");
    let ds = run(REACTION, &s, PARTY);
    assert!(!ds.iter().any(|d| d.code.starts_with("E-")), "{ds:#?}");
}

#[test]
fn speaker_arg_is_any_identifier_without_a_cast_but_never_a_def() {
    let s = scene(DECLARED, "::use{component=\"reaction\" who=\"anyone\"}");
    let ds = run(REACTION, &s, &[]);
    assert!(!ds.iter().any(|d| d.code.starts_with("E-")), "{ds:#?}");

    let front = format!("{DECLARED}defs:\n  lead: {{ type: string, cel: \"'isolde'\" }}\n");
    let s = scene(&front, "::use{component=\"reaction\" who=@lead}");
    let ds = run(REACTION, &s, PARTY);
    assert!(codes(&ds).contains(&"E-COMPONENT-ARG"), "{ds:#?}");
}

#[test]
fn speaker_match_dispatches_over_the_cast() {
    // Every cast id and `narrator` covered without `<otherwise>`: exhaustive.
    let comp = "---\ncomponent: reaction\nparams:\n  who: speaker\n---\n## Scene 1.\n<match on=\"@who\">\n<when is=\"isolde\">\n@narrator: A.\n</when>\n<when is=\"corvin|narrator\">\n@narrator: B.\n</when>\n</match>\n";
    let s = scene("", "::use{component=\"reaction\" who=\"corvin\"}");
    let ds = run(comp, &s, PARTY);
    assert!(!ds.iter().any(|d| d.code.starts_with("E-")), "{ds:#?}");

    // An arm naming a non-member is outside the param's domain.
    let bad = comp.replace("is=\"isolde\"", "is=\"oda\"");
    let ds = run(&bad, &s, PARTY);
    assert!(ds.iter().any(|d| d.code.starts_with("E-")), "{ds:#?}");
}
