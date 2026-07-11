use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

#[test]
fn delivery_typo_is_error() {
    let cs = codes(&format!("{HDR}@x{{delivery=\"thouhgt\"}}: hi\n"));
    assert!(cs.contains(&"E-DELIVERY-VALUE".to_string()), "{cs:?}");
}

#[test]
fn delivery_on_narrator_is_error() {
    let cs = codes(&format!("{HDR}@narrator{{delivery=\"thought\"}}: hi\n"));
    assert!(cs.contains(&"E-DELIVERY-NARRATOR".to_string()), "{cs:?}");
}

#[test]
fn every_valid_delivery_is_clean() {
    for v in ["spoken", "thought", "voiceover"] {
        let cs = codes(&format!("{HDR}@x{{delivery=\"{v}\"}}: hi\n"));
        assert!(!cs.iter().any(|c| c.starts_with("E-DELIVERY")), "{v}: {cs:?}");
    }
}

#[test]
fn unknown_content_attr_is_error() {
    let cs = codes(&format!("{HDR}@x{{bogus=\"1\"}}: hi\n"));
    assert!(cs.contains(&"E-UNKNOWN-ATTR".to_string()), "{cs:?}");
}

#[test]
fn known_content_attrs_are_clean() {
    let cs = codes(&format!(
        "{HDR}@x{{code=\"0010\" emotion=\"neutral\" variant=\"0\" action=\"wave\" dialogMotion=\"m\" as=\"???\"}}: hi\n"
    ));
    assert!(!cs.iter().any(|c| c == "E-UNKNOWN-ATTR"), "{cs:?}");
}

#[test]
fn emotion_member_clean_nonmember_errors() {
    // uses the HDR + codes() harness already in content_line.rs tests
    assert!(!codes(&format!("{HDR}@x{{emotion=\"neutral\"}}: hi\n")).iter().any(|c| c == "E-BAD-ENUM"));
    assert!(codes(&format!("{HDR}@x{{emotion=\"zzz\"}}: hi\n")).contains(&"E-BAD-ENUM".to_string()));
}

#[test]
fn action_is_open_by_default() {
    // action stays free-form in a core-only context (no project action domain)
    let cs = codes(&format!("{HDR}@x{{action=\"wave\"}}: hi\n"));
    assert!(!cs.iter().any(|c| c == "E-DOMAIN-UNKNOWN" || c == "E-BAD-ENUM"), "{cs:?}");
}
