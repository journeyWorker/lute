//! E-BRANCH-ALL-GUARDED (dsl §11.1, S5): a `<branch>` MUST contain at least one
//! UNGUARDED (`when`-less) `<choice>`; a branch whose every choice carries a
//! `when` could present an empty menu. Fed through the assembled `check()` over
//! inline `state:` frontmatter (mirrors `choice_persist.rs`'s harness).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "branch_all_guarded".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn every_choice_guarded_flags_all_guarded() {
    // Both choices carry `when=…` → E-BRANCH-ALL-GUARDED.
    let t = format!(
        "{HDR}state:\n  scene.x: {{ type: bool }}\n  scene.y: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"approach\">\n\
         <choice id=\"soft\" label=\"Soft\" when=\"scene.x\">\n\
         </choice>\n\
         <choice id=\"blunt\" label=\"Blunt\" when=\"scene.y\">\n\
         </choice>\n\
         </branch>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-BRANCH-ALL-GUARDED"),
        "expected E-BRANCH-ALL-GUARDED; got {cs:?}"
    );
}

#[test]
fn one_unguarded_choice_is_clean() {
    // The second choice is `when`-less → no E-BRANCH-ALL-GUARDED.
    let t = format!(
        "{HDR}state:\n  scene.x: {{ type: bool }}\n---\n## Shot 1.\n\
         <branch id=\"approach\">\n\
         <choice id=\"soft\" label=\"Soft\" when=\"scene.x\">\n\
         </choice>\n\
         <choice id=\"blunt\" label=\"Blunt\">\n\
         </choice>\n\
         </branch>\n"
    );
    let cs = codes(&t);
    assert!(
        !cs.iter().any(|c| c == "E-BRANCH-ALL-GUARDED"),
        "unguarded choice present; got {cs:?}"
    );
}

#[test]
fn empty_branch_is_empty_not_all_guarded() {
    // Empty branch → E-BRANCH-EMPTY only, never double-flagged as all-guarded.
    let t = format!(
        "{HDR}---\n## Shot 1.\n\
         <branch id=\"dead\">\n\
         </branch>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-BRANCH-EMPTY"),
        "expected E-BRANCH-EMPTY; got {cs:?}"
    );
    assert!(
        !cs.iter().any(|c| c == "E-BRANCH-ALL-GUARDED"),
        "empty branch must not be double-flagged; got {cs:?}"
    );
}
