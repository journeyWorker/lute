//! The §4.2 rule-4 conformance assertion, reviewable as a test: NONE of the
//! quarantined crates may depend on `lute-trace`. `lute-cel` stays
//! parse-only (it holds no evaluator and MUST NOT gain one); `lute-check`
//! and `lute-compile` depending on `lute-trace` is a conformance violation.
//! This test fails only by crate absence today (before this crate existed)
//! and stays green forever after: it never inspects `lute-trace` itself,
//! only every sibling manifest's text.

#[test]
fn no_quarantined_crate_depends_on_lute_trace() {
    for krate in [
        "lute-core-span",
        "lute-syntax",
        "lute-cel",
        "lute-manifest",
        "lute-check",
        "lute-compile",
        "lute-lsp",
    ] {
        let manifest =
            std::fs::read_to_string(format!("{}/../{krate}/Cargo.toml", env!("CARGO_MANIFEST_DIR"))).unwrap();
        assert!(
            !manifest.contains("lute-trace"),
            "D1 QUARANTINE VIOLATION: {krate} must not reach lute-trace (dsl 0.4 \u{00a7}4.2)"
        );
    }
}
