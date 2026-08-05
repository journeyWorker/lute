use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

/// The default harness: the shared test vocabulary, so a fixture writing
/// `emotion="neutral"` declares that vocabulary the way a real project does.
fn codes(text: &str) -> Vec<String> {
    codes_with(text, lute_test_vocab::vocab_snapshot())
}

/// Same harness against an explicit snapshot — for the one test whose subject
/// IS the absence of a vocabulary declaration.
fn codes_with(text: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
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

/// dsl 0.9.0 D-C: a domain slot with no declared domain is an ERROR. Before
/// 0.9.0 `action` was silently skipped when undeclared (this test's ancestor,
/// `action_is_open_by_default`, asserted exactly that), which is why a typo in
/// a 9,880-row action vocabulary shipped unchecked.
///
/// DELIBERATELY runs against the BARE CORE (`load_core_snapshot()`), not the
/// shared test vocabulary: the claim is about NOTHING declaring an `action`
/// domain. Against `vocab_snapshot()` it would only prove membership.
#[test]
fn undeclared_action_domain_is_an_error() {
    let cs = codes_with(
        &format!("{HDR}@x{{action=\"wave\"}}: hi\n"),
        lute_manifest::core::load_core_snapshot(),
    );
    assert!(
        cs.contains(&"E-DOMAIN-UNKNOWN".to_string()),
        "undeclared `action` must error: {cs:?}"
    );
}

/// Declared → membership is checked, exactly like `emotion`.
#[test]
fn declared_action_domain_is_membership_checked() {
    let clean = codes(&format!("{HDR}@x{{action=\"wave\"}}: hi\n"));
    assert!(!clean.iter().any(|c| c == "E-DOMAIN-UNKNOWN"), "{clean:?}");
    assert!(!clean.iter().any(|c| c == "E-BAD-ENUM"), "{clean:?}");
    let bad = codes(&format!("{HDR}@x{{action=\"zzz\"}}: hi\n"));
    assert!(bad.contains(&"E-BAD-ENUM".to_string()), "{bad:?}");
}
