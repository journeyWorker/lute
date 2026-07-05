//! `fold_env` accessor (compile-spec §11 reuse-input exposure): the FOLDED
//! state schema (inline + implicit `scene.choices.*`) and the merged def
//! tables (types, params, CEL bodies) surface through one public call.

use lute_check::{fold_env, CheckInput, Mode};
use lute_core_span::Severity;
use lute_manifest::types::Type;

const SCENE: &str = r#"---
character: bianca
season: 1
episode: 2
state:
  scene.affect.bianca: { type: number, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.bianca >= 1" }
---

## Shot 1.

<branch id="number">
  <choice id="blunt" label="Flat">
    :line[fixer]: a
  </choice>
  <choice id="soft" label="Gentle">
    :line[fixer]: b
  </choice>
</branch>
"#;

#[test]
fn fold_env_exposes_folded_schema_and_def_bodies() {
    let input = CheckInput {
        text: SCENE.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    };
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, diags) = fold_env(&doc, &input);
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    // Inline decl folded.
    assert!(folded.env.state.decls.contains_key("scene.affect.bianca"));
    // Implicit branch decl folded (§11.1).
    let choice = folded
        .env
        .state
        .decls
        .get("scene.choices.number")
        .expect("implicit branch decl folded");
    assert_eq!(
        choice.ty,
        Type::Enum(vec!["blunt".to_string(), "soft".to_string()])
    );
    // Def body exposed for the D4 expander.
    assert_eq!(
        folded.def_bodies.get("fond").map(String::as_str),
        Some("scene.affect.bianca >= 1")
    );
    assert_eq!(folded.env.def_types.get("fond"), Some(&Type::Bool));
    // Typed frontmatter rides along for the envelope.
    assert_eq!(folded.typed.character.as_deref(), Some("bianca"));
    assert_eq!(folded.typed.season, Some(1));
    assert_eq!(folded.typed.episode, Some(2));
}
