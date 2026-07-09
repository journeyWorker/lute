//! `<on>` ECA trigger semantics (dsl 0.2.0 §4): `event` name validation
//! (`E-ON-NO-EVENT`/`E-UNKNOWN-EVENT`) + the `when` guard's shared CEL-profile
//! gate.

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "on".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn on_without_event_errors() {
    // <on> with no event= -> E-ON-NO-EVENT.
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n\
                    <on>\n:x: hi\n</on>\n</quest>\n");
    assert!(cs.contains(&"E-ON-NO-EVENT".to_string()), "{cs:?}");
}

#[test]
fn on_unknown_event_errors() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n\
                    <on event=\"noSuchEvent\">\n:x: hi\n</on>\n</quest>\n");
    assert!(cs.contains(&"E-UNKNOWN-EVENT".to_string()), "{cs:?}");
}

#[test]
fn on_builtin_lifecycle_event_is_clean() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.d\"/>\n\
                    <on event=\"questComplete\">\n::set{run.x = 1}\n</on>\n</quest>\n");
    assert!(!cs.iter().any(|c| c == "E-ON-NO-EVENT" || c == "E-UNKNOWN-EVENT"), "{cs:?}");
}
