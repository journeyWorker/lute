//! dsl 0.23.0 §5 (lamplight F2/F30) — a component `string` param MAY be
//! interpolated (`{{@p}}`) inside the component body: expansion splices the
//! `::use` site's literal into the line text. Outside a component body a
//! string-typed `{{@def}}` stays `E-REF-TYPE` (dsl §7.6), and so does a `::use`
//! that binds an interpolated string param to a string `@def` (the rebound
//! `{{@def}}` could not render).
use lute_check::{check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lute_csp_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check_with(text: &str, components: ComponentSet) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "doc".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components,
        defaults: Default::default(),
    })
    .diagnostics
}

/// Check `scene` with its `components:` resolved from `dir` (the CLI/LSP path).
fn check_scene(dir: &Path, scene: &str) -> Vec<Diagnostic> {
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    check_with(scene, components)
}

fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
    diags
        .iter()
        .filter(|d| d.code.starts_with("E-"))
        .collect()
}

/// The lamplight F2 component: an enum-dispatched staging block whose last
/// line is the inspector's thought, different at each call site.
const LAMPS_OUT: &str = "---\ncomponent: lampsOut\nparams:\n  memory: string\n  \
depth: { enum: [brief, long] }\n---\n## Lamps out\n\
<match on=\"@depth\">\n  <when is=\"brief\">\n    @narrator: The lamps gutter.\n  </when>\n  \
<when is=\"long\">\n    @narrator: The tunnel swallows the train.\n  </when>\n</match>\n\
@narrator: {{@memory}}\n";

/// `memory` is passed on whole-slot to `lampsOut`, which interpolates it.
const WRAPPER: &str = "---\ncomponent: wrapper\nparams:\n  line: string\n---\n## W\n\
::use{component=\"lampsOut\" memory=@line depth=\"brief\"}\n";

fn scene(defs: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         components: [lamps-out.component.lute, wrapper.component.lute]\n{defs}---\n\
         ## Shot 1.\n@narrator: before.\n{body}\n"
    )
}

fn project() -> PathBuf {
    let dir = unique_dir();
    std::fs::write(dir.join("lamps-out.component.lute"), LAMPS_OUT).unwrap();
    std::fs::write(dir.join("wrapper.component.lute"), WRAPPER).unwrap();
    dir
}

#[test]
fn string_param_interpolation_checks_clean_standalone() {
    let diags = check_with(LAMPS_OUT, ComponentSet::default());
    assert!(
        errors(&diags).is_empty(),
        "`{{{{@memory}}}}` over a string param renders in a component body: {diags:#?}"
    );
}

#[test]
fn string_param_interpolation_checks_clean_through_a_literal_use() {
    let dir = project();
    let s = scene(
        "",
        "::use{component=\"lampsOut\" memory=\"Four minutes.\" depth=\"long\"}\n\
         ::use{component=\"wrapper\" line=\"Anyone can walk anywhere.\"}",
    );
    let diags = check_scene(&dir, &s);
    assert!(
        errors(&diags).is_empty(),
        "literal args to an interpolated string param are clean: {diags:#?}"
    );
}

#[test]
fn string_def_interpolated_in_a_scene_stays_non_renderable() {
    let s = scene(
        "defs:\n  thought: { type: string, cel: \"'Four minutes.'\" }\n",
        "@narrator: {{@thought}}",
    );
    let diags = check_with(&s, ComponentSet::default());
    assert!(
        diags.iter().any(|d| d.code == "E-REF-TYPE"),
        "a string def outside a component body is still E-REF-TYPE: {diags:#?}"
    );
}

/// Byte offset of the `@thought` value of the `::use` arg `key` in `text`.
fn arg_value_start(text: &str, key: &str) -> usize {
    text.find(&format!("{key}=@thought")).unwrap() + key.len() + 1
}

#[test]
fn string_def_bound_to_an_interpolated_param_is_ref_type_at_the_arg() {
    let dir = project();
    let defs = "defs:\n  thought: { type: string, cel: \"'Four minutes.'\" }\n";
    // Directly, and through `wrapper`, which hands `line` on to `lampsOut`'s
    // interpolated `memory`.
    for (key, body) in [
        (
            "memory",
            "::use{component=\"lampsOut\" memory=@thought depth=\"long\"}",
        ),
        ("line", "::use{component=\"wrapper\" line=@thought}"),
    ] {
        let s = scene(defs, body);
        let diags = check_scene(&dir, &s);
        let d = diags
            .iter()
            .find(|d| d.code == "E-REF-TYPE")
            .unwrap_or_else(|| panic!("{key}: a string def cannot render: {diags:#?}"));
        assert_eq!(
            d.span.byte_start,
            arg_value_start(&s, key),
            "{key}: anchored at the `::use` arg: {d:#?}"
        );
        assert!(
            d.message.contains("must be a literal"),
            "{key}: names the fix: {}",
            d.message
        );
    }
}

#[test]
fn string_def_bound_to_a_param_the_body_does_not_interpolate_is_clean() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("lamps-out.component.lute"),
        "---\ncomponent: lampsOut\nparams:\n  memory: string\n---\n## L\n\
         ::sfx{sound=@memory}\n@narrator: The lamps gutter.\n",
    )
    .unwrap();
    std::fs::write(dir.join("wrapper.component.lute"), WRAPPER).unwrap();
    let s = scene(
        "defs:\n  thought: { type: string, cel: \"'train whistle'\" }\n",
        "::use{component=\"lampsOut\" memory=@thought}",
    );
    let diags = check_scene(&dir, &s);
    assert!(
        !diags.iter().any(|d| d.code == "E-REF-TYPE"),
        "a string def feeding a string attr slot renders nothing: {diags:#?}"
    );
}
