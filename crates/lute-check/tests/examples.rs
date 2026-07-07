//! Integration tests for the assembled `check()` entrypoint (Task 4.9).
//!
//! These exercise the full pipeline (parse -> fill_document -> parse_meta ->
//! validators -> resolved view -> dedup + determinism sort) over the real
//! `docs/examples/*.lute` fixtures and small hand-written snippets that pin the
//! binding carry-forwards: document-level definite-assignment, `E-UNDECLARED`
//! dedup, and the byte-offset determinism ordering the Phase-6 divergence golden
//! compares.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Severity;
use lute_manifest::provider::ProviderSet;

/// The example fixtures reference no `providerRef`-typed attributes (all
/// `assetId`s are plain strings in `lute.core`), so an empty set is fully
/// permissive: `check_provider_ref` is never consulted.
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
fn bianca_example_checks_clean() {
    let text = std::fs::read_to_string("../../docs/examples/bianca-s01ep02.lute").unwrap();
    let input = CheckInput {
        text,
        uri: "bianca".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: permissive_providers(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    let res = check(&input);
    let errors: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
    assert!(
        res.resolved.is_some(),
        "clean document must produce a resolved view"
    );
}

#[test]
fn undeclared_state_read_is_reported() {
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<match on=\"scene.nope\">\n<otherwise>\n:narrator: hi\n</otherwise>\n</match>\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().any(|d| d.code == "E-UNDECLARED"),
        "expected E-UNDECLARED for undeclared subject read, got: {:#?}",
        res.diagnostics
    );
}

#[test]
fn undeclared_set_target_reports_exactly_one_undeclared() {
    // A `::set` to an undeclared state path is flagged by BOTH `check_set`
    // (Layer::Staging) and `check_definite_assignment` (Layer::Logic). The
    // dedup carry-forward must collapse them to a single `E-UNDECLARED`.
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::set{scene.nope = 1}\n";
    let res = check(&input_for(text));
    let undeclared: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "E-UNDECLARED")
        .collect();
    assert_eq!(
        undeclared.len(),
        1,
        "undeclared `::set` target must surface exactly one E-UNDECLARED, got: {:#?}",
        undeclared
    );
}

#[test]
fn two_distinct_undeclared_paths_in_one_slot_both_survive() {
    // Regression (T4.9 review Important #1): a single CEL slot reading TWO
    // undeclared paths (`scene.a` and `scene.b`) gets one whole-slot fallback
    // span for both (cel-parser 0.10.1 has no per-node offsets). Path-aware
    // dedup must keep BOTH — collapsing only same-path+overlapping-span pairs —
    // so the author sees every undeclared path at once, not one at a time.
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<match on=\"scene.a == scene.b\">\n<otherwise>\n:narrator: hi\n</otherwise>\n</match>\n";
    let res = check(&input_for(text));
    let paths: Vec<&str> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "E-UNDECLARED")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        paths.iter().any(|m| m.contains("scene.a")) && paths.iter().any(|m| m.contains("scene.b")),
        "both undeclared paths must survive dedup, got: {paths:#?}"
    );
}

#[test]
fn diagnostics_are_sorted_by_byte_start() {
    // Two errors at different byte offsets (an undeclared `::set` in shot 1, an
    // unknown directive in shot 2) must come back ordered by `span.byte_start`.
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::set{scene.nope = 1}\n## Shot 2.\n::bogusdirective{}\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.len() >= 2,
        "expected at least two diagnostics: {:#?}",
        res.diagnostics
    );
    for pair in res.diagnostics.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            (a.span.byte_start, a.code.as_str()) <= (b.span.byte_start, b.code.as_str()),
            "diagnostics not sorted by (byte_start, code): {a:?} then {b:?}"
        );
    }
}

// --- E-PATH-IDENT: `-` forbidden in CEL-facing names (dsl §8.4, §4.4 CelIdent) ---

#[test]
fn hyphen_state_path_decl_rejected() {
    // A `state:` path segment is a `CelIdent` (dsl §9.3): `-` after the tier is
    // E-PATH-IDENT (dsl §8.4).
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.affect-total:\n    type: number\n---\n## Shot 1.\n:narrator: hi\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().any(|d| d.code == "E-PATH-IDENT"),
        "expected E-PATH-IDENT for hyphenated state-path segment, got: {:#?}",
        res.diagnostics
    );
}

#[test]
fn hyphen_set_target_rejected() {
    // A `::set` LHS is a CEL-facing state path (dsl §7.3.4/§8.4). The target is
    // NOT declared (so E-PATH-IDENT can only come from the `::set` path check).
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.total:\n    type: number\n---\n## Shot 1.\n::set{scene.affect-total = 1}\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().any(|d| d.code == "E-PATH-IDENT"),
        "expected E-PATH-IDENT for hyphenated `::set` target segment, got: {:#?}",
        res.diagnostics
    );
}

#[test]
fn hyphen_def_name_rejected() {
    // A `defs` name is a CEL-facing identifier (dsl §8.1/§8.4).
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\ndefs:\n  my-def:\n    type: number\n    cel: \"1\"\n---\n## Shot 1.\n:narrator: hi\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().any(|d| d.code == "E-PATH-IDENT"),
        "expected E-PATH-IDENT for hyphenated def name, got: {:#?}",
        res.diagnostics
    );
}

#[test]
fn hyphen_def_param_rejected() {
    // A def parameter name is CEL-facing (dsl §8.1/§8.4).
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\ndefs:\n  scale:\n    type: number\n    params:\n      a-b: number\n    cel: \"1\"\n---\n## Shot 1.\n:narrator: hi\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().any(|d| d.code == "E-PATH-IDENT"),
        "expected E-PATH-IDENT for hyphenated def param name, got: {:#?}",
        res.diagnostics
    );
}

#[test]
fn hyphen_directive_and_speaker_ok() {
    // `-` in an asset id, attribute value, and speaker id is legal `Ident`
    // (dsl §4.4): NOT CEL-facing, so no E-PATH-IDENT.
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::music{assetId=\"a-b\"}\n:some-speaker: hi\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().all(|d| d.code != "E-PATH-IDENT"),
        "unexpected E-PATH-IDENT for a legal hyphenated id, got: {:#?}",
        res.diagnostics
    );
}

#[test]
fn hyphen_path_ident_span_is_narrow() {
    // The meta-side E-PATH-IDENT span must point at the offending key, not the
    // whole frontmatter block (span-quality requirement).
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.affect-total:\n    type: number\n---\n## Shot 1.\n:narrator: hi\n";
    let res = check(&input_for(text));
    let d = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-PATH-IDENT")
        .unwrap_or_else(|| panic!("expected an E-PATH-IDENT diagnostic, got: {:#?}", res.diagnostics));
    let len = d.span.byte_end - d.span.byte_start;
    assert_eq!(
        len,
        "scene.affect-total".len(),
        "span should cover exactly the offending path segment, got {len} bytes: {d:#?}"
    );
    // Must start past the `---\n` frontmatter opener (i.e. not the whole block,
    // which begins at byte 0), and be far narrower than the frontmatter itself.
    assert!(
        d.span.byte_start > "---\n".len(),
        "span must point inside the frontmatter, not at its start: {d:#?}"
    );
    let block_end = text.find("\n---\n").map(|i| i + 5).expect("frontmatter close");
    assert!(
        len < block_end,
        "span ({len} bytes) must be narrower than the whole frontmatter block ({block_end} bytes)"
    );
}
