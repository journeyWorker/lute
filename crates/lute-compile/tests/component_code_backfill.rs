//! ashen N7 / decision 10 — an untagged component line's code comes from the
//! COMPONENT SOURCE, before a param-scoped `<match>` folds. A call site with a
//! literal argument (the match folds to one arm) and one with a `@def`
//! argument (the match stays residual) mint the same code for the same source
//! line — the code `lute tag` would write into the component file — so
//! tagging never renames a lineId an untagged compile already shipped.
use std::path::Path;

use lute_check::{check, parse_meta, resolve_components, resolve_imports, CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;

fn input_for(dir: &Path, scene_text: &str) -> CheckInput {
    let (doc, _) = lute_syntax::parse(scene_text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let imports = resolve_imports(dir, &meta0.uses, &meta0.extends, doc.meta.span);
    CheckInput::new(
        scene_text.to_string(),
        "scene.lute".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            vocab_snapshot(),
            ProviderSet::default(),
            imports,
            components,
            Default::default(),
        ),
    )
}

/// Untagged, exactly as ashen-stair shipped `hearth-fire.component.lute`.
const FIRE: &str = "---\ncomponent: fire\nparams:\n  flare: { enum: [low, high] }\n---\n\
## The fire\n<match subject=\"@flare\">\n  <when is=\"high\">\n    @narrator: It leaps.\n  </when>\n\
  <when is=\"low\">\n    @narrator: It settles.\n  </when>\n</match>\n";

const SCENE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
components: [fire.component.lute]\n\
state:\n  scene.hot: { type: bool, default: false }\n\
defs:\n  flareNow: \"scene.hot ? 'high' : 'low'\"\n---\n\
## Shot 1.\n::use{component=\"fire\" flare=\"low\"}\n::use{component=\"fire\" flare=@flareNow}\n";

fn settles_ids(dir: &Path) -> Vec<String> {
    let input = input_for(dir, SCENE);
    let res = check(&input);
    assert!(
        !res.diagnostics.iter().any(|d| d.code.starts_with("E-")),
        "{:#?}",
        res.diagnostics
    );
    let artifact = compile(&input).unwrap_or_else(|e| panic!("{e:#?}"));
    artifact
        .commands
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .filter(|v| v["kind"] == "line" && v["text"] == "It settles.")
        .map(|v| v["lineId"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_component_line_keeps_its_source_code_whatever_the_argument_form() {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(dir.join("fire.component.lute"), FIRE).unwrap();
    let ids = settles_ids(&dir);
    assert_eq!(
        ids,
        [
            "x.s01ep01.fire#1.narrator_0020",
            "x.s01ep01.fire#2.narrator_0020"
        ],
        "the `low` arm's line is the component's second narrator line at both call sites"
    );

    // `lute tag` on the component writes that same code: tagging renames nothing.
    let tagged = lute_check::tag_document(FIRE);
    assert!(
        tagged
            .text
            .contains("@narrator{code=\"0020\"}: It settles."),
        "{}",
        tagged.text
    );
    std::fs::write(dir.join("fire.component.lute"), &tagged.text).unwrap();
    assert_eq!(settles_ids(&dir), ids);
}
