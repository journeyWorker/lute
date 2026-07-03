//! The "No divergence" golden (Task 6.2) — the architecture's central invariant
//! made executable.
//!
//! The CLI/headless surface (`lute_check::check`) and the editor/LSP surface
//! (`check` -> `lute_lsp::convert::to_lsp_diagnostic`) MUST encode *identical*
//! information for every diagnostic: same code, same severity, same message, and
//! the same start/end position. There is exactly ONE diagnostic surface; the LSP
//! is a pure reprojection of the headless result, never a second source of truth.
//!
//! To compare on equal footing, each diagnostic is normalized to the same tuple
//! shape `(code, severity-discriminant, message, start (line0, utf16col), end)`:
//! - the **headless** side derives its positions from the diagnostic's own `span`
//!   bytes through a [`TextIndex`] over the document — exactly how the CLI reports
//!   them (`{line - 1, utf16_col}`, matching LSP's 0-based line / 0-based UTF-16
//!   character);
//! - the **LSP** side reads them back off the converted `Range`
//!   (`range.start`/`.end` `(line, character)`), unwraps the string `code`, and
//!   maps the LSP severity back to the same discriminant.
//!
//! Both sides map their severities to a shared discriminant (Error<->ERROR = 1,
//! Warning<->WARNING = 2, Info<->INFORMATION = 3, Hint<->HINT = 4) so the enums
//! line up. `check()` already dedups and sorts by `(span.byte_start, code)`, so
//! the two vectors must match in length, order, AND content — `assert_eq!` on the
//! whole `Vec` proves all three. Each golden first asserts its diagnostics vector
//! is NON-EMPTY, so a future refactor that makes `check()` silently emit nothing
//! can't turn the equality into a vacuous pass.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity, TextIndex};
use lute_manifest::provider::ProviderSet;
// v0.23 of `tower-lsp-server` re-exports the LSP type crate as `ls_types` (backed
// by `ls-types` 0.0.6), NOT `lsp_types`. We only ever *read* the converted type,
// produced by the single conversion path `lute_lsp::convert::to_lsp_diagnostic`.
use tower_lsp_server::ls_types;

/// The comparable projection of one diagnostic: `(code, severity-discriminant,
/// message, start (line0, utf16col), end (line0, utf16col))`. Both surfaces
/// normalize to this exact shape so `assert_eq!` compares like with like.
type Norm = (String, u8, String, (u32, u32), (u32, u32));

/// Build the same `CheckInput` the LSP backend uses: `Mode::Author` over the core
/// snapshot with the default (permissive) provider set. Mirrors
/// `lute-check/tests/examples.rs::input_for` so headless and LSP see identical
/// analysis conditions.
fn input_for(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
    }
}

/// A `TextIndex` over the exact document text the diagnostics' byte offsets refer
/// to — the same index the LSP backend builds in `analyze()`.
fn idx(text: &str) -> TextIndex<'_> {
    TextIndex::new(text)
}

/// Shared severity discriminant for the headless side (Error=1 .. Hint=4, aligned
/// with the LSP wire numbers so the two mappings collapse to the same `u8`).
fn headless_severity(sev: Severity) -> u8 {
    match sev {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Info => 3,
        Severity::Hint => 4,
    }
}

/// The LSP-side inverse of [`headless_severity`]: map `DiagnosticSeverity` back to
/// the same discriminant. `DiagnosticSeverity` is a newtype over `i32` exposing
/// the four LSP constants; we compare against them (it derives `PartialEq`).
fn lsp_severity(sev: ls_types::DiagnosticSeverity) -> u8 {
    use ls_types::DiagnosticSeverity as D;
    if sev == D::ERROR {
        1
    } else if sev == D::WARNING {
        2
    } else if sev == D::INFORMATION {
        3
    } else if sev == D::HINT {
        4
    } else {
        panic!("unexpected LSP diagnostic severity outside the four mapped values")
    }
}

/// Normalize a headless (core) diagnostic. Positions come from the diagnostic's
/// own `span` bytes through `idx` — de-1-indexing the line and using the 0-based
/// UTF-16 column, exactly as the LSP conversion does, so the two surfaces are
/// compared on equal footing.
fn normalize_headless(d: &Diagnostic, idx: &TextIndex) -> Norm {
    let start = idx.position(d.span.byte_start);
    let end = idx.position(d.span.byte_end);
    (
        d.code.clone(),
        headless_severity(d.severity),
        d.message.clone(),
        (start.line - 1, start.utf16_col),
        (end.line - 1, end.utf16_col),
    )
}

/// Normalize an LSP diagnostic (the output of `to_lsp_diagnostic`): unwrap the
/// string `code` (our codes are always `NumberOrString::String`), map the severity
/// back to the shared discriminant, and read the range's 0-based
/// `(line, character)` endpoints.
fn normalize_lsp(d: &ls_types::Diagnostic) -> Norm {
    let code = match d.code.as_ref() {
        Some(ls_types::NumberOrString::String(s)) => s.clone(),
        other => panic!("expected a string diagnostic code, got {other:?}"),
    };
    let severity = lsp_severity(
        d.severity
            .expect("converted diagnostic always carries a severity"),
    );
    (
        code,
        severity,
        d.message.clone(),
        (d.range.start.line, d.range.start.character),
        (d.range.end.line, d.range.end.character),
    )
}

/// Error-bearing golden: `date-minigame.lute` yields real diagnostics (ledger
/// errors + a warning). The headless projection and the LSP-converted-then-
/// normalized projection must be byte-for-byte identical.
#[test]
fn headless_and_lsp_diagnostics_match() {
    let text = std::fs::read_to_string("../../docs/examples/date-minigame.lute").unwrap();
    let res = check(&input_for(&text));

    // Sanity: a non-empty vector, so the equality below is meaningful, not vacuous.
    assert!(
        !res.diagnostics.is_empty(),
        "date-minigame.lute must produce diagnostics; an empty vector would make the golden trivially pass"
    );

    let index = idx(&text);
    let headless: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &index))
        .collect();
    let via_lsp: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &index)))
        .collect();

    // Same length, same order (check() already sorts), same content.
    assert_eq!(
        headless, via_lsp,
        "headless and LSP diagnostic surfaces diverged"
    );
}

/// Warning-bearing golden: `bianca-s01ep02.lute` is error-clean but carries a
/// `W-INJECT-CONFLICT` warning, so the golden also covers the Warning severity
/// round-trip. Same equality invariant.
#[test]
fn headless_and_lsp_diagnostics_match_warning_bearing() {
    let text = std::fs::read_to_string("../../docs/examples/bianca-s01ep02.lute").unwrap();
    let res = check(&input_for(&text));

    assert!(
        !res.diagnostics.is_empty(),
        "bianca-s01ep02.lute must produce diagnostics; an empty vector would make the golden trivially pass"
    );
    assert!(
        res.diagnostics.iter().any(|d| d.severity == Severity::Warning),
        "bianca-s01ep02.lute should carry at least one warning-severity diagnostic (covers the Warning round-trip)"
    );

    let index = idx(&text);
    let headless: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &index))
        .collect();
    let via_lsp: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &index)))
        .collect();

    assert_eq!(
        headless, via_lsp,
        "headless and LSP diagnostic surfaces diverged"
    );
}

/// Plugin-loaded golden (Task 7.4): the divergence invariant must hold under a
/// project that activates a plugin. The document `date-minigame.lute` is *dirty*
/// core-only (its `::minigame` directive and provider id are unknown) but *clean*
/// once the `idola.minigame` plugin is resolved through the shared project
/// resolver — the SAME `resolve_document_snapshot` the CLI (Task 7.3) and the LSP
/// backend (`snapshot_for`) call. This guards no-divergence end to end: one
/// snapshot, one position path, on both surfaces.
#[test]
fn divergence_holds_under_plugin_project() {
    use lute_manifest::project::{load_project, resolve_document_snapshot};

    let text = std::fs::read_to_string("../../docs/examples/date-minigame.lute").unwrap();
    let proj = load_project(std::path::Path::new("../../docs/examples/idola-project"))
        .expect("idola-project loads")
        .expect("idola-project has a lute.project.yaml");

    // Lift the scene's frontmatter `profile`/`plugins` exactly as the surfaces do:
    // a default snapshot types those built-in keys (they are not capability-gated).
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );

    // The ONE resolver both CLI and LSP call — assemble the activated snapshot.
    let (snapshot, _rd) =
        resolve_document_snapshot(Some(&proj), meta0.profile.as_deref(), &meta0.plugins);
    // The plugin's provider catalog (same set both surfaces would use), so the
    // `providerRef` id `bianca_service_01` resolves and positions match.
    let providers = ProviderSet::load("../../docs/examples/idola-project/catalog");

    let input = CheckInput {
        text: text.clone(),
        uri: "date-minigame".into(),
        snapshot,
        providers,
        mode: Mode::Author,
        imports: SchemaImports::default(),
    };
    let res = check(&input);

    // With the plugin loaded, the scene is error-clean (the point of the fixture).
    let errs: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errs.is_empty(),
        "plugin-loaded date-minigame must be error-clean; got {errs:#?}"
    );

    // No-divergence: whatever diagnostics remain (warnings/hints), the headless
    // projection and the LSP-converted projection agree byte-for-byte — the same
    // equality the core goldens assert, now under an activated plugin snapshot.
    let index = idx(&text);
    let headless: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &index))
        .collect();
    let via_lsp: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &index)))
        .collect();
    assert_eq!(
        headless, via_lsp,
        "headless and LSP diagnostic surfaces diverged under the plugin project"
    );
}

/// No-divergence under a `uses:` schema import (dsl §9.2). Two cases: (a) the
/// error-clean `carry-ep.lute` (imported `run.choseHelp` resolves via the SAME
/// `resolve_imports` both surfaces call) and (b) a scene whose import is missing,
/// which yields an `E-USES-NOT-FOUND` whose headless and LSP projections must
/// agree byte-for-byte — guarding the new §9.2 diagnostic codes' projection.
#[test]
fn divergence_holds_under_uses_import() {
    let dir = std::path::Path::new("../../docs/examples");

    // (a) happy path: carry-ep is error-clean via its import; projections agree.
    let text = std::fs::read_to_string(dir.join("carry-ep.lute")).unwrap();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let imports = lute_check::resolve_imports(dir, &meta0.uses, doc.meta.span);
    let input = CheckInput {
        text: text.clone(),
        uri: "carry-ep".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
    };
    let res = check(&input);
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "carry-ep.lute must be error-clean under its import; got {:?}",
        res.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
    let index = idx(&text);
    let headless: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &index))
        .collect();
    let via_lsp: Vec<Norm> = res
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &index)))
        .collect();
    assert_eq!(
        headless, via_lsp,
        "headless and LSP surfaces diverged under a uses: import"
    );

    // (b) non-vacuous: a missing import produces E-USES-NOT-FOUND; its headless
    // and LSP projections must agree too (the new §9.2 codes' projection).
    let bad = "---\ncharacter: x\nseason: 1\nepisode: 1\nuses: __no_such_schema__.lute\n---\n## Shot 1.\n:line[x]: hi\n";
    let (bdoc, _) = lute_syntax::parse(bad);
    let (bmeta, _) = lute_check::parse_meta(
        &bdoc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let bimports = lute_check::resolve_imports(dir, &bmeta.uses, bdoc.meta.span);
    let binput = CheckInput {
        text: bad.to_string(),
        uri: "bad-import".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: bimports,
    };
    let bres = check(&binput);
    assert!(
        bres.diagnostics
            .iter()
            .any(|d| d.code == "E-USES-NOT-FOUND"),
        "a missing uses: import must yield E-USES-NOT-FOUND; got {:?}",
        bres.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
    let bindex = idx(bad);
    let bheadless: Vec<Norm> = bres
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &bindex))
        .collect();
    let bvia_lsp: Vec<Norm> = bres
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &bindex)))
        .collect();
    assert_eq!(
        bheadless, bvia_lsp,
        "E-USES-NOT-FOUND projection diverged between surfaces"
    );
}
