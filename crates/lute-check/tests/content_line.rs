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
fn two_delivery_flags_conflict() {
    let cs = codes(&format!("{HDR}@x{{mono os}}: hi\n"));
    assert!(cs.contains(&"E-DELIVERY-CONFLICT".to_string()), "{cs:?}");
}

#[test]
fn single_delivery_flag_ok() {
    for f in ["mono", "os", "vo"] {
        let cs = codes(&format!("{HDR}@x{{{f}}}: hi\n"));
        assert!(!cs.iter().any(|c| c.starts_with("E-DELIVERY")), "{f}: {cs:?}");
    }
}

#[test]
fn valued_delivery_flag_is_error() {
    // dsl 0.2.2 §D7: `mono`/`os`/`vo` are BARE flags (`{ident}⇒true`); a
    // valued form (`mono="yes"`) is malformed, not a second delivery flag.
    let cs = codes(&format!("{HDR}@x{{mono=\"yes\"}}: hi\n"));
    assert!(cs.contains(&"E-DELIVERY-FLAG-VALUE".to_string()), "{cs:?}");
    assert!(!cs.iter().any(|c| c == "E-DELIVERY-CONFLICT"), "{cs:?}");
}

#[test]
fn delivery_flag_on_narrator_errors() {
    let cs = codes(&format!("{HDR}@narrator{{mono}}: hi\n"));
    assert!(cs.contains(&"E-DELIVERY-NARRATOR".to_string()), "{cs:?}");
}

#[test]
fn delivery_string_attr_is_unknown_not_a_value_domain() {
    // 0.2.2 retires the `delivery="…"` enum-valued form entirely — the key
    // itself is no longer in `KNOWN_ATTRS`, so it falls through to
    // `E-UNKNOWN-ATTR` (retiring 0.2.1's `E-DELIVERY-VALUE`).
    let cs = codes(&format!("{HDR}@x{{delivery=\"thought\"}}: hi\n"));
    assert!(cs.contains(&"E-UNKNOWN-ATTR".to_string()), "{cs:?}");
    assert!(!cs.iter().any(|c| c == "E-DELIVERY-VALUE"), "{cs:?}");
}

#[test]
fn unknown_content_attr_is_error() {
    let cs = codes(&format!("{HDR}@x{{bogus=\"1\"}}: hi\n"));
    assert!(cs.contains(&"E-UNKNOWN-ATTR".to_string()), "{cs:?}");
}

#[test]
fn known_content_attrs_are_clean() {
    let cs = codes(&format!(
        "{HDR}@x{{code=\"0010\" emotion=\"neutral\" variant=\"0\" action=\"wave\" dialogMotion=\"m\" mono as=\"???\"}}: hi\n"
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
