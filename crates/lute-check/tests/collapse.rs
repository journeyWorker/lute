//! Task 14 (dsl 0.4.0 §8.2/§8.3, D11/D12) — root-cause collapse. One mistake
//! MUST NOT read as N mistakes:
//! - **C1** (`collapse_same_root`, D11): same-code/same-root-subject
//!   diagnostics over `{E-UNDECLARED, E-UNDECLARED-REF, E-RELATION-UNKNOWN,
//!   E-CHOICELOG-READ, E-COMPONENT-UNDECLARED}` collapse to ONE primary at the
//!   first document-order occurrence, carrying every further occurrence's span
//!   in `primary.covered` (also document order). Site-specific codes
//!   (`E-MAYBE-UNSET`) are exempt.
//! - **C3** (`suppress_unproven_absence`, D12): a failed `uses:`/`extends:`/
//!   `components:` import suppresses dependent absence diagnostics whose claim
//!   depends on the merge that failed to build.
//!
//! Fed through the assembled `check()` (mirrors `interp.rs`'s inline harness
//! for C1, `datalog.rs`'s/`components_use.rs`'s temp-dir resolver harness for
//! C3), exactly like every other whole-`check()` test in this crate.

use lute_check::{check, resolve_components, resolve_imports, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n";

fn diagnose(text: &str) -> Vec<Diagnostic> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "collapse".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics
}

static UNIQ: AtomicU64 = AtomicU64::new(0);

/// A fresh temp dir per call; import/component files are written into it.
fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "lute_collapse_{}_{}_{}",
        std::process::id(),
        n,
        nanos
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

// One typo'd path (`run.metHelpfuly`) read 3x on 3 distinct content lines: the
// Appendix A C1 fixture. Must collapse to exactly 1 `E-UNDECLARED`, kept at
// the FIRST occurrence, carrying the other 2 spans as `covered` in document
// order.
#[test]
fn three_reads_one_primary() {
    let t = format!(
        "{HDR}---\n## Shot 1.\n\
         @bianca: I sense a {{{{run.metHelpfuly}}}}\n\
         @bianca: again, {{{{run.metHelpfuly}}}}\n\
         @bianca: still, {{{{run.metHelpfuly}}}}\n"
    );
    let diags = diagnose(&t);
    let undeclared: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == "E-UNDECLARED").collect();
    assert_eq!(
        undeclared.len(),
        1,
        "3 reads of one typo must collapse to 1 primary, got {diags:?}"
    );
    let primary = undeclared[0];
    assert_eq!(
        primary.covered.len(),
        2,
        "2 follower occurrences expected, got {:?}",
        primary.covered
    );
    // Document order: primary at the first site, followers after it and
    // increasing.
    assert!(primary.span.byte_start < primary.covered[0].byte_start);
    assert!(primary.covered[0].byte_start < primary.covered[1].byte_start);
}

// Two DIFFERENT typo'd paths, each read once -> 2 distinct primaries, neither
// carrying covered occurrences.
#[test]
fn distinct_subjects_do_not_collapse() {
    let t = format!(
        "{HDR}---\n## Shot 1.\n\
         @bianca: I sense a {{{{run.metHelpfuly}}}}\n\
         @bianca: and also {{{{run.otherTypo}}}}\n"
    );
    let diags = diagnose(&t);
    let undeclared: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == "E-UNDECLARED").collect();
    assert_eq!(
        undeclared.len(),
        2,
        "two distinct root subjects must stay 2 primaries, got {diags:?}"
    );
    assert!(
        undeclared.iter().all(|d| d.covered.is_empty()),
        "neither primary has a follower here: {undeclared:?}"
    );
}

// Finding 5 (C1 over-collapse): `collapse_same_root` keyed on the FIRST
// backtick token, but a `::set` undeclared-target message quotes the
// DIRECTIVE first (`` `::set` target `run.x` … ``, set_op.rs) — so two
// DISTINCT bad writes (`run.x`, `run.y`) wrongly shared the collapse key
// `("E-UNDECLARED", "::set")` and merged into one primary (the second
// hidden in `covered[]`). The real root subject — the undeclared PATH, not
// the leading `::set` token — must be the collapse key.
#[test]
fn distinct_undeclared_set_targets_do_not_collapse() {
    let t = format!(
        "{HDR}---\n## Shot 1.\n\
         ::set{{run.x = 1}}\n\
         ::set{{run.y = 2}}\n"
    );
    let diags = diagnose(&t);
    let undeclared: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == "E-UNDECLARED").collect();
    assert_eq!(
        undeclared.len(),
        2,
        "two distinct ::set undeclared targets must stay 2 primaries, got {diags:?}"
    );
    assert!(
        undeclared.iter().all(|d| d.covered.is_empty()),
        "neither primary should carry the other as covered[]: {undeclared:?}"
    );
}

// `E-MAYBE-UNSET` is a site-specific analysis (each site's own dominators/
// guards) and is EXEMPT from C1 — two unset-read sites of the SAME declared
// path both survive collapse.
#[test]
fn maybe_unset_is_exempt() {
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: number }}\n---\n## Shot 1.\n\
         @bianca: you have {{{{run.x}}}}\n\
         @bianca: still {{{{run.x}}}}\n"
    );
    let diags = diagnose(&t);
    let unset: Vec<&Diagnostic> = diags.iter().filter(|d| d.code == "E-MAYBE-UNSET").collect();
    assert_eq!(
        unset.len(),
        2,
        "C1 exemption: each maybe-unset site is independently meaningful, got {diags:?}"
    );
    assert!(unset.iter().all(|d| d.covered.is_empty()));
}

// C3 (D12): a missing `uses:` schema import + one dependent `run.*` read ->
// EXACTLY one error, the import failure itself. The dependent E-UNDECLARED
// (whose claim depends on the merge the failed import never built) is
// suppressed.
#[test]
fn missing_uses_suppresses_dependents() {
    let dir = unique_dir();
    let imports = resolve_imports(&dir, &["nonexistent.yaml".to_string()], &[], zero_span());
    let t = format!("{HDR}---\n## Shot 1.\n@bianca: {{{{run.x}}}}\n");
    let input = CheckInput {
        text: t,
        uri: "collapse".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
    };
    let diags = check(&input).diagnostics;
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
    assert_eq!(
        errors.len(),
        1,
        "an import failure must suppress its dependents, got {diags:?}"
    );
    assert_eq!(errors[0].code, "E-USES-NOT-FOUND");
}

// C3's component-side rule: a broken component file (`E-COMPONENT-PARSE`,
// never enters the resolved table) + a `::use` of that name -> only
// `E-COMPONENT-PARSE` survives; the dependent `E-COMPONENT-UNDECLARED` (whose
// claim depends on the table the parse failure never built) is suppressed.
#[test]
fn component_parse_suppresses_undeclared_component() {
    let dir = unique_dir();
    // No `component:` name declared -> E-COMPONENT-PARSE; never enters the table.
    write_lute(&dir, "broken.lute", "---\nparams:\n  who: string\n---\n## G.\n@x: hi\n");
    let scene = format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [broken.lute]\n---\n\
         ## Shot 1.\n::use{{component=\"broken\" who=\"x\"}}\n"
    );
    let (doc, _) = lute_syntax::parse(&scene);
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let components = resolve_components(&dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: scene,
        uri: "collapse".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: SchemaImports::default(),
        components,
    };
    let diags = check(&input).diagnostics;
    let errors: Vec<&Diagnostic> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
    assert!(
        errors.iter().any(|d| d.code == "E-COMPONENT-PARSE"),
        "the broken component file must still be reported, got {diags:?}"
    );
    assert!(
        !errors.iter().any(|d| d.code == "E-COMPONENT-UNDECLARED"),
        "a dependent E-COMPONENT-UNDECLARED must be suppressed, got {diags:?}"
    );
}

// Collapse (and suppression) must be a pure function of the input: running
// `check()` twice over the identical document yields identical diagnostic
// vectors (`§8.2 C5`: "collapse never changes ordering or determinism").
#[test]
fn collapse_is_deterministic() {
    let t = format!(
        "{HDR}---\n## Shot 1.\n\
         @bianca: I sense a {{{{run.metHelpfuly}}}}\n\
         @bianca: again, {{{{run.metHelpfuly}}}}\n"
    );
    let d1 = diagnose(&t);
    let d2 = diagnose(&t);
    assert_eq!(d1, d2, "check() must be deterministic across runs");
}
