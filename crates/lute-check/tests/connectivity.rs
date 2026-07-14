//! Integration tests for the connectivity layer's `after` surface (Task 2):
//! the scene `after:` frontmatter key and the `<quest after="…">` attribute,
//! each validated LOCALLY (grammar only, via `prereq::parse_prereq`) by the
//! single-file `check()` entrypoint. Node existence (whether the referenced
//! `visited`/`completed` id actually exists in the project) is NOT resolved
//! here — that is `check-project`'s job (Task 3+).

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn permissive_providers() -> ProviderSet {
    ProviderSet::default()
}

fn input_for(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: permissive_providers(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    }
}

#[test]
fn scene_after_key_is_parsed_and_validated() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: 'visited(\"y.s01ep01\")'\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    // A well-formed after against an (unknown, single-file) node must NOT raise E-CONN-PROFILE.
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn scene_after_malformed_raises_profile_error() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: '!visited(\"y\")'\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_after_attribute_is_parsed_and_validated() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"visited('y.s01ep01')\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_after_malformed_raises_profile_error() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"!visited('x')\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_frontmatter_after_key_is_not_a_prereq_surface() {
    // §2.1: frontmatter `after:` is a SCENE-ONLY prerequisite surface. A
    // quest pack declares its prerequisite via the per-`<quest>` `after`
    // attribute instead — a malformed `after:` in a quest doc's FRONTMATTER
    // must NOT be fed to `prereq::parse_prereq` (it may still raise the
    // ordinary unknown-meta-key diagnostic; we assert only on the absence
    // of the spurious E-CONN-PROFILE).
    let text = "---\nkind: quest\nafter: '!visited(\"x\")'\n---\n<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}
