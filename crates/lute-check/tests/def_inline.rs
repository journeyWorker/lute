//! 0.21.1 T1-3/T1-4 — a def reference that must become artifact data:
//! an attribute `@def` must fold to a constant (`E-ATTR-DEF-DYNAMIC`) and is
//! then validated as the literal it folds to; a `{{@def}}` must inline into
//! one standalone expression (`E-INTERP-DEF`). A def arg reaching an attribute
//! through a component param is held to the same rule.
use lute_check::{check, parse_meta, resolve_components, CheckInput, Mode};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use std::path::{Path, PathBuf};

fn codes_in(dir: Option<&Path>, scene: &str) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = match dir {
        Some(dir) => resolve_components(dir, &meta0.components, doc.meta.span),
        None => Default::default(),
    };
    let input = CheckInput {
        text: scene.to_string(),
        uri: "scene".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components,
        defaults: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn scene(extra_meta: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\n\
         state:\n  run.n: {{ type: number, default: 3 }}\n\
         defs:\n  closeUp: \"1.3\"\n  zoomDyn: \"run.n > 2 ? 1.3 : 1.1\"\n  \
         twice: \"run.n * 2\"\n  blink: \"'blink'\"\n  loopA: {{ type: number, cel: \"@loopB + 1\" }}\n  \
         loopB: {{ type: number, cel: \"@loopA + 1\" }}\n{extra_meta}---\n\n## One\n\n{body}"
    )
}

fn count(c: &[String], code: &str) -> usize {
    c.iter().filter(|x| x.as_str() == code).count()
}

#[test]
fn constant_def_in_attr_is_accepted() {
    let c = codes_in(None, &scene("", "::camera{focus=\"demo\" zoom=@closeUp}\n"));
    assert_eq!(count(&c, "E-ATTR-DEF-DYNAMIC"), 0, "got {c:?}");
}

#[test]
fn state_dependent_def_in_attr_is_rejected() {
    let c = codes_in(None, &scene("", "::camera{focus=\"demo\" zoom=@zoomDyn}\n"));
    assert_eq!(count(&c, "E-ATTR-DEF-DYNAMIC"), 1, "got {c:?}");
}

#[test]
fn folded_literal_is_validated_like_an_authored_one() {
    // `action` is `enum: [show, hide]`; `@blink` folds to `blink`.
    let c = codes_in(None, &scene("", "::cut{assetId=\"x\" action=@blink}\n"));
    assert_eq!(count(&c, "E-BAD-ENUM"), 1, "got {c:?}");
    assert_eq!(count(&c, "E-ATTR-DEF-DYNAMIC"), 0, "got {c:?}");
}

#[test]
fn renderable_def_interpolation_is_accepted() {
    let c = codes_in(None, &scene("", "@narrator: Def {{@twice}}.\n"));
    assert_eq!(count(&c, "E-INTERP-DEF"), 0, "got {c:?}");
}

#[test]
fn def_interpolation_that_cannot_expand_is_rejected() {
    let c = codes_in(None, &scene("", "@narrator: Loop {{@loopA}}.\n"));
    assert_eq!(count(&c, "E-INTERP-DEF"), 1, "got {c:?}");
}

/// A fresh component directory per call. The name carries a process-wide
/// counter: two tests running in parallel can read the same clock value
/// (macOS reports microseconds), and a shared directory lets one test's
/// component body overwrite the other's.
fn component_dir(body: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_def_inline_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("shot.component.lute"),
        format!("---\ncomponent: shot\nparams:\n  z: number\n---\n\n## Shot\n\n{body}"),
    )
    .unwrap();
    dir
}

#[test]
fn dynamic_def_arg_reaching_a_component_attr_is_rejected() {
    let dir = component_dir("::camera{focus=\"demo\" zoom=@z}\n");
    let text = scene(
        "components: [shot.component.lute]\n",
        "::use{component=\"shot\" z=@zoomDyn}\n",
    );
    let c = codes_in(Some(&dir), &text);
    assert_eq!(count(&c, "E-ATTR-DEF-DYNAMIC"), 1, "got {c:?}");
    // A constant arg folds after binding.
    let text = scene(
        "components: [shot.component.lute]\n",
        "::use{component=\"shot\" z=@closeUp}\n",
    );
    let c = codes_in(Some(&dir), &text);
    assert_eq!(count(&c, "E-ATTR-DEF-DYNAMIC"), 0, "got {c:?}");
}

#[test]
fn dynamic_def_arg_rendered_as_text_is_accepted() {
    let dir = component_dir("@narrator: Zoom {{@z}}.\n");
    let text = scene(
        "components: [shot.component.lute]\n",
        "::use{component=\"shot\" z=@zoomDyn}\n",
    );
    let c = codes_in(Some(&dir), &text);
    assert_eq!(count(&c, "E-ATTR-DEF-DYNAMIC"), 0, "got {c:?}");
}
