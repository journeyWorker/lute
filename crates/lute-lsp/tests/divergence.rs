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
        components: Default::default(),
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
        components: Default::default(),
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

/// Plugin-def golden (dsl §8, Task P1.2): the divergence invariant must hold when
/// a scene uses a PLUGIN-EXPORTED def. A temp project's one plugin `demo.defs`
/// exports a bool def `warm` and a number def `tally` (`defs/defs.yaml`), modeled
/// on the committed `docs/examples/plugindef-project` fixture. Each def flows
/// load_project -> resolve_document_snapshot -> snapshot.defs -> check() through
/// the SAME resolver the CLI (Task 7.3) and the LSP backend (`snapshot_for`) call,
/// so a plugin-exported `@ref` is a declared def on BOTH surfaces. Two cases,
/// mirroring `divergence_holds_under_uses_import`: (a) `@warm` whole-slot in a
/// `<when test>` bool guard is declared + bool-compatible -> error-clean; (b)
/// NON-VACUOUS: `@tally` (number) in the same bool guard flags `E-REF-TYPE`, whose
/// headless projection and LSP-converted projection must agree byte-for-byte — the
/// real proof that a plugin-def-derived diagnostic reprojects identically.
#[test]
fn divergence_holds_under_plugin_defs() {
    use lute_manifest::project::{load_project, project_providers, resolve_document_snapshot};

    // A temp project modeled on docs/examples/plugindef-project (committed P1.1):
    // one plugin `demo.defs` exporting a bool def `warm` and a number def `tally`,
    // activated by the default profile — the same on-disk layout + defs the
    // disk-path integration test (`lute-check/tests/plugin_defs_disk.rs`) builds.
    let root = std::env::temp_dir().join(format!(
        "lute-divergence-plugindefs-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let plugin = root.join("plugins/demo.defs");
    std::fs::create_dir_all(plugin.join("defs")).unwrap();
    std::fs::write(
        root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: demo\nprofiles:\n  demo:\n    plugins: { demo.defs: true }\n",
    )
    .unwrap();
    std::fs::write(
        plugin.join("plugin.yaml"),
        "id: demo.defs\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  defs: defs/\n",
    )
    .unwrap();
    std::fs::write(
        plugin.join("defs/defs.yaml"),
        "defs:\n  - { name: warm, type: bool, cel: \"true\" }\n  - { name: tally, type: number, cel: \"1\" }\n",
    )
    .unwrap();

    let proj = load_project(&root)
        .expect("temp plugindef project loads")
        .expect("temp plugindef project has a lute.project.yaml");

    // A scene whose whole `<when test>` bool guard is a bare plugin-def `@ref`;
    // only the ref differs between the two cases.
    let scene = |guard: &str| {
        format!(
            "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\nstate:\n  scene.flag: {{ type: bool, default: false }}\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"{guard}\">@narrator: a\n</when>\n<otherwise>@narrator: b\n</otherwise>\n</match>\n"
        )
    };

    // Resolve+check a scene exactly as both surfaces do: parse frontmatter with a
    // default snapshot, assemble the activated snapshot via the shared resolver
    // (profile None -> project defaultProfile `demo` -> `demo.defs` plugin active),
    // then run the ONE headless `check`.
    let run = |text: &str| {
        let (doc, _) = lute_syntax::parse(text);
        let (meta0, _) = lute_check::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let (snapshot, _rd) =
            resolve_document_snapshot(Some(&proj), meta0.profile.as_deref(), &meta0.plugins);
        let providers = project_providers(Some(&proj));
        check(&CheckInput {
            text: text.to_string(),
            uri: "plugindef-scene".into(),
            snapshot,
            providers,
            mode: Mode::Author,
            imports: SchemaImports::default(),
            components: Default::default(),
        })
    };

    // (a) `@warm` (bool def) whole-slot in a bool guard: declared + type-compatible
    // via `snapshot.defs`, so the scene is error-clean. Its (possibly empty)
    // projection still agrees across surfaces.
    let text_a = scene("@warm");
    let res_a = run(&text_a);
    assert!(
        res_a
            .diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "`@warm` (bool def) in a bool guard must be error-clean; got {:?}",
        res_a
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
    let ia = idx(&text_a);
    let ha: Vec<Norm> = res_a
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &ia))
        .collect();
    let la: Vec<Norm> = res_a
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &ia)))
        .collect();
    assert_eq!(
        ha, la,
        "headless and LSP surfaces diverged for the clean plugin-def scene"
    );

    // (b) non-vacuous: `@tally` (number def) in the SAME bool guard flags
    // `E-REF-TYPE`; its headless and LSP projections must agree byte-for-byte —
    // the plugin-def-derived diagnostic reprojects identically on both surfaces.
    let text_b = scene("@tally");
    let res_b = run(&text_b);
    assert!(
        res_b.diagnostics.iter().any(|d| d.code == "E-REF-TYPE"),
        "`@tally` (number def) in a bool guard must flag E-REF-TYPE; got {:?}",
        res_b
            .diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
    let ib = idx(&text_b);
    let hb: Vec<Norm> = res_b
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &ib))
        .collect();
    let lb: Vec<Norm> = res_b
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &ib)))
        .collect();
    assert_eq!(
        hb, lb,
        "E-REF-TYPE projection diverged between surfaces under a plugin-exported def"
    );

    let _ = std::fs::remove_dir_all(&root);
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
    let imports = lute_check::resolve_imports(dir, &meta0.uses, &meta0.extends, doc.meta.span);
    let input = CheckInput {
        text: text.clone(),
        uri: "carry-ep".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
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
    let bad = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: __no_such_schema__.lute\n---\n## Shot 1.\n@x: hi\n";
    let (bdoc, _) = lute_syntax::parse(bad);
    let (bmeta, _) = lute_check::parse_meta(
        &bdoc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let bimports = lute_check::resolve_imports(dir, &bmeta.uses, &bmeta.extends, bdoc.meta.span);
    let binput = CheckInput {
        text: bad.to_string(),
        uri: "bad-import".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: bimports,
        components: Default::default(),
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

/// No-divergence under `components:` component imports (dsl §13). Two cases: (a)
/// an error-clean scene that imports + `::use`s a valid presentational component
/// (resolved via the SAME `resolve_components` both surfaces call) and (b) a scene
/// whose `::use` names an undeclared component, yielding an `E-COMPONENT-UNDECLARED`
/// whose headless and LSP projections must agree byte-for-byte — guarding the new
/// §13 diagnostic codes' projection.
#[test]
fn divergence_holds_under_components() {
    let dir = std::env::temp_dir().join(format!("lute_div_components_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("greet.lute"),
        "---\ncomponent: greet\nparams:\n  who: string\n---\n## Scene 1.\n::auto{character=@who}\n@narrator: hi\n",
    )
    .unwrap();

    // (a) happy path: import + ::use a valid component cleanly; projections agree.
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [greet.lute]\n---\n## Shot 1.\n::use{component=\"greet\" who=\"bianca\"}\n";
    let (doc, _) = lute_syntax::parse(text);
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let components = lute_check::resolve_components(&dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: text.into(),
        uri: "comp-scene".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
    };
    let res = check(&input);
    assert!(
        res.diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "a clean ::use of a valid component must be error-free; got {:?}",
        res.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
    let index = idx(text);
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
        "headless and LSP surfaces diverged under a components: import"
    );

    // (b) non-vacuous: an unknown ::use produces E-COMPONENT-UNDECLARED; its
    // headless and LSP projections must agree too (the new §13 codes' projection).
    let bad = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::use{component=\"ghost\" who=\"x\"}\n";
    let (bdoc, _) = lute_syntax::parse(bad);
    let (bmeta, _) = lute_check::parse_meta(
        &bdoc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let bcomponents = lute_check::resolve_components(&dir, &bmeta.components, bdoc.meta.span);
    let binput = CheckInput {
        text: bad.to_string(),
        uri: "ghost-scene".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: bcomponents,
    };
    let bres = check(&binput);
    assert!(
        bres.diagnostics
            .iter()
            .any(|d| d.code == "E-COMPONENT-UNDECLARED"),
        "an unknown ::use must yield E-COMPONENT-UNDECLARED; got {:?}",
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
        "E-COMPONENT-UNDECLARED projection diverged between surfaces"
    );
}

/// No-divergence for the 0.2.0 quest kind (Plan E Task 6): the invariant that
/// held for every scene-kind construct must also hold for `<quest>`/`<on>`/
/// `<objective>` — new AST variants, new diagnostic codes, but the SAME single
/// diagnostic surface. Two cases, mirroring `divergence_holds_under_components`:
/// (a) an error-clean quest (a declared `run.grove` state path read by an
/// objective's `done` guard and written by a `questComplete` `<on>` handler)
/// and (b) a quest whose objective omits `done`, which yields
/// `E-OBJECTIVE-MISSING-DONE` — its headless and LSP projections must agree
/// byte-for-byte, proving the quest-specific diagnostic (anchored at the
/// `<objective>` construct, not a content line) reprojects identically.
#[test]
fn divergence_holds_for_quest_docs() {
    // (a) happy path: a clean quest doc is error-free; projections agree.
    let text = "---\nkind: quest\nstate:\n  run.grove: { type: bool, default: false }\n---\n\
                <quest id=\"rescueHalsin\" title=\"Rescue Halsin\">\n\
                <objective id=\"reachGrove\" title=\"Reach the grove\" done=\"run.grove\"/>\n\
                <on event=\"questComplete\">\n::set{run.grove = true}\n</on>\n\
                </quest>\n";
    let res = check(&input_for(text));
    assert!(
        res.diagnostics.iter().all(|d| d.severity != Severity::Error),
        "a clean quest doc must be error-free; got {:?}",
        res.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>()
    );
    let index = idx(text);
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
        "headless and LSP surfaces diverged for a clean quest doc"
    );

    // (b) non-vacuous: an objective with no `done` flags E-OBJECTIVE-MISSING-DONE
    // (dsl 0.2.0 §6.4); its headless and LSP projections must agree too.
    let bad = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\"/>\n</quest>\n";
    let bres = check(&input_for(bad));
    assert!(
        bres.diagnostics.iter().any(|d| d.code == "E-OBJECTIVE-MISSING-DONE"),
        "an objective with no done= must yield E-OBJECTIVE-MISSING-DONE; got {:?}",
        bres.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>()
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
        "E-OBJECTIVE-MISSING-DONE projection diverged between surfaces"
    );

    // (c) non-vacuous, CEL-attribute anchor: an objective whose `done` reads an
    // undeclared state path flags a CEL-layer diagnostic anchored at the `done=`
    // attribute's CEL span (NOT the `<objective>` construct span
    // E-OBJECTIVE-MISSING-DONE uses above) -- the quest-CEL-attribute
    // reprojection path.
    let cel_bad = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"run.missing\"/>\n</quest>\n";
    let cel_res = check(&input_for(cel_bad));
    assert!(
        cel_res.diagnostics.iter().any(|d| d.code == "E-UNDECLARED"),
        "an objective done= reading an undeclared state path must yield E-UNDECLARED; got {:?}",
        cel_res.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>()
    );
    let cel_index = idx(cel_bad);
    let cel_headless: Vec<Norm> = cel_res
        .diagnostics
        .iter()
        .map(|d| normalize_headless(d, &cel_index))
        .collect();
    let cel_via_lsp: Vec<Norm> = cel_res
        .diagnostics
        .iter()
        .map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &cel_index)))
        .collect();
    assert_eq!(
        cel_headless, cel_via_lsp,
        "E-UNDECLARED projection diverged between surfaces on the quest done= CEL-attribute path"
    );
}
