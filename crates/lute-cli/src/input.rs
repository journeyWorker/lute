//! Per-document input assembly: the `CheckInput` `check`, `compile`,
//! `trace` and every project pass resolve a `.lute` file into.

use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::project::{resolve_permissions, ResolveDiag};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

use crate::input_cache::InputCache;

/// A `CheckInput` plus the verdict of the capability-resolution step that
/// produced it.
pub(crate) struct BuiltInput {
    pub input: CheckInput,
    /// `true` when resolving the project/plugin snapshot emitted an
    /// `E-`-severity diagnostic (plugin 0.0.2 §2 option validation,
    /// `E-PLUGIN-MISSING-ACTIVE`, `E-IDENTITY-TEMPLATE`, …). These describe the
    /// PROJECT rather than a span in the document, so they print through the
    /// `lute:` channel instead of the per-document diagnostic list — but they
    /// are errors, and every gating command MUST fold this into its exit code.
    pub resolve_error: bool,
    /// The project-level problems resolution surfaced, in emission order: a
    /// `lute.project.yaml` that failed to load, then each
    /// [`resolve_document_snapshot`] diagnostic as `<code>: <message>`. Each is
    /// the BODY of one `lute: …` stderr line.
    ///
    /// RETURNED rather than printed because they describe the PROJECT, not this
    /// document: a caller that resolves many documents under one project would
    /// print the identical line once per file, and a caller that reports through
    /// a structured model (`lute doctor`) could not capture them at all. Every
    /// gating command calls [`BuiltInput::report_project_diags`] immediately, so
    /// its stderr is byte-identical to when `build_input` printed them itself.
    ///
    /// [`resolve_document_snapshot`]: lute_manifest::project::resolve_document_snapshot
    pub project_diags: Vec<String>,
    /// The document's own lifted frontmatter, as `build_input` already parsed it
    /// to resolve the snapshot. Carried because the domain vocabulary a document
    /// resolves includes its OWN inline `enums:`/`entities:` projection
    /// (`TypedMeta::domains`), which `merge_domains` needs alongside
    /// `input.imports` — see `doctor::resolved_domains`.
    pub meta: lute_check::TypedMeta,
    /// The governing manifest's `defaults:` (0.10.0 §6), as applied to this
    /// document's frontmatter. Carried so future consumers of `BuiltInput`
    /// can inspect the applied defaults without re-loading the project.
    #[allow(dead_code)]
    pub defaults: lute_manifest::project::MetaDefaults,
    /// Frozen project identity templates resolved by the same manifest load as
    /// the capability snapshot and defaults.
    pub identity: lute_manifest::project::IdentityTemplates,
}

impl BuiltInput {
    /// Print [`BuiltInput::project_diags`] on the `lute:` stderr channel — the
    /// exact lines `build_input` used to emit inline.
    pub fn report_project_diags(&self) {
        for m in &self.project_diags {
            eprintln!("lute: {m}");
        }
    }
}

/// Assemble the `CheckInput` for `file` exactly as `check` does: project
/// snapshot resolution (plugin §4/§11), provider-catalog precedence (plugin
/// §10), and `uses:`/`components:` imports resolved against the file's own
/// directory. `None` => the file could not be read (caller exits 2).
pub(crate) fn build_input(
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BuiltInput> {
    build_input_with(
        &InputCache::default(),
        file,
        providers,
        project,
        permission_profile,
    )
}

/// [`build_input`] against a per-run [`InputCache`], for a caller that
/// assembles many documents' inputs in one invocation.
pub(crate) fn build_input_with(
    cache: &InputCache,
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BuiltInput> {
    match read_document(file) {
        Ok(text) => {
            Some(assemble_input(cache, file, text, providers, project, permission_profile).0)
        }
        Err(message) => {
            eprintln!("{message}");
            None
        }
    }
}

/// `file`'s text, or the `lute: cannot read …` line [`build_input`] prints.
pub(crate) fn read_document(file: &Path) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("lute: cannot read {}: {e}", file.display()))
}

/// The body of [`build_input`] over already-read `text`, also handing back
/// `lute_syntax::parse(&text)` — parsed here to lift the frontmatter — so a
/// caller can reuse it (`lute_check::check_parsed`) instead of re-parsing.
pub(crate) fn assemble_input(
    cache: &InputCache,
    file: &Path,
    text: String,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> (BuiltInput, (lute_syntax::ast::Document, Vec<Diagnostic>)) {
    // Resolve the capability snapshot the document is validated against. With
    // `--project`, load the project and assemble the scene's activated snapshot
    // (plugin §4/§11); without it, `resolve_document_snapshot(None, ..)` returns
    // the core-only `lute.core` baseline — behavior identical to before.
    let mut project_diags: Vec<String> = Vec::new();
    let root = project;
    let loaded = root.map(|dir| cache.project(dir));
    let project = match loaded.as_deref() {
        Some(Ok(p)) => p.as_ref(),
        Some(Err(e)) => {
            // A malformed project must not silently mis-validate: surface it
            // and fall back to core-only rather than pretending it loaded.
            project_diags.push(e.clone());
            None
        }
        None => None,
    };

    // Provider catalog precedence (plugin §10): an explicit `--providers <dir>`
    // wins; otherwise auto-discover the project's pinned catalog through the
    // SAME shared helper the LSP uses, so the two surfaces resolve the same ids
    // for the same project; with neither, an empty set.
    let providers = ProviderSet::clone(&cache.providers(providers, root, project));

    // 0.10.0 §6: the governing manifest's `defaults:`, already canonicalised
    // at load (D-Z). Lifted BEFORE the frontmatter parse, because a defaulted
    // `uses:` has to reach `resolve_imports` below.
    let defaults = project.map(|p| p.defaults.clone()).unwrap_or_default();

    // Lift the scene's frontmatter `profile`/`plugins` — both built-in keys, so a
    // default snapshot suffices to type them (they are not capability-gated).
    let mut parsed = lute_syntax::parse(&text);
    lute_check::meta::apply_quest_tier_default(&mut parsed.0, &defaults);
    lute_check::sequence::apply_sequence(&mut parsed.0, &defaults);
    let doc = &parsed.0;
    let (meta0, _) = lute_check::meta::parse_meta_kind_with_defaults(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Scene,
        &defaults,
    );

    let resolved = cache.snapshot(root, project, meta0.profile.as_deref(), &meta0.plugins);
    let (mut snapshot, mut rdiags) = (resolved.0.clone(), resolved.1.clone());
    if let Some(name) = permission_profile {
        match project.as_ref() {
            Some(config) => match resolve_permissions(config, name) {
                Ok(permissions) => snapshot.restrict_permissions(&permissions),
                Err(error) => rdiags.push(ResolveDiag {
                    code: error.code().to_string(),
                    message: error.to_string(),
                }),
            },
            None => rdiags.push(ResolveDiag {
                code: "E-PERMISSION-PROFILE".to_string(),
                message: format!(
                    "`--permission-profile {name}` requires a loaded `lute.project.yaml` from `--project <DIR>`"
                ),
            }),
        }
    }
    let mut resolve_error = !project_diags.is_empty();
    for d in &rdiags {
        project_diags.push(format!(
            "{}: {}",
            d.code,
            lute_core_span::plain_message(&d.message)
        ));
        // An `E-` resolve diagnostic is a build-failing error like any other
        // (dsl 0.1.0 Appendix E: severity is binary, `E-` gates). It travels
        // the `lute:` channel instead of the per-document diagnostic list
        // because it describes the PROJECT, not a span in this file — but it
        // must still set the exit code, or `E-PLUGIN-OPTION-TYPE` and friends
        // would print and pass.
        resolve_error |= d.code.starts_with("E-");
    }
    let identity = project
        .as_ref()
        .map(|p| p.identity.clone())
        .unwrap_or_default();

    // Resolve the scene's `uses:` schema imports (dsl §9.2) and `components:`
    // component imports (dsl §13) relative to the scene's own directory; the LSP
    // resolves identically -> no divergence.
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let imports = cache
        .imports
        .resolve(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = cache
        .imports
        .resolve_components(base, &meta0.components, doc.meta.span);

    let built = BuiltInput {
        input: CheckInput {
            text,
            uri: file.display().to_string(),
            snapshot,
            providers,
            // Batch/build analysis, not the interactive LSP default (both behave
            // identically today; the checker does not branch on mode yet).
            mode: Mode::Ci,
            imports,
            components,
            defaults: defaults.clone(),
        },
        resolve_error,
        project_diags,
        meta: meta0,
        defaults,
        identity,
    };
    (built, parsed)
}
