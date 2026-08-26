//! LSP opt-in lint publishing — unit tests for the pure
//! [`lute_lsp::lint::lint_document`] entry (design §2, §3).
//!
//! `Backend::analyze` is a thin async wrapper that plumbs `lint_document`'s
//! `Vec<Diagnostic>` into the same publish channel the check surface uses;
//! exercising the pure function here pins the gating and scope contract
//! without spinning a live server (mirrors `tests/code_action.rs`'s pattern).

use std::path::PathBuf;

use lute_core_span::Diagnostic;
use lute_lsp::lint::lint_document;
use lute_manifest::provider::ProviderSet;

/// A scene document whose second dialogue line is 41 whitespace-tokens long,
/// tripping the built-in `dialogue-length` rule at its default `maxWords: 40`
/// (design §7). One shot + one narration line keeps unrelated rules quiet
/// (`dialogue-ratio` requires `bodyNodes >= 10` before it fires; only one
/// document ⇒ `scene-length-spread` skipped by `scenes >= 2`).
const DIALOGUE_LENGTH_SCENE: &str = "---\nkind: scene\n---\n## Shot 1.\n\
    @alice: hello\n\
    @bob: one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven twenty-eight twenty-nine thirty thirty-one thirty-two thirty-three thirty-four thirty-five thirty-six thirty-seven thirty-eight thirty-nine forty forty-one\n";

/// Build a scratch project dir plus a scene file under it. Returns `(root
/// dir, scene path)`. A `lute.project.yaml` is written so
/// [`Backend::analyze`]'s `find_project_root` discovery seam locates the
/// root; no plugin is activated. Follows the workspace convention (see
/// `crates/lute-manifest/tests/loader.rs`): a per-test `std::env::temp_dir()`
/// path uniqued by tag + PID, wiped up-front.
fn scaffold(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("lute_lsp_lint_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: base\nprofiles:\n  base:\n    plugins: {}\n",
    )
    .unwrap();
    let scene = dir.join("scene.lute");
    std::fs::write(&scene, DIALOGUE_LENGTH_SCENE).unwrap();
    (dir, scene)
}

fn parse_doc(text: &str) -> lute_syntax::ast::Document {
    let (doc, _diags) = lute_syntax::parse(text);
    doc
}

fn codes(diags: &[Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.code.as_str()).collect()
}

/// No `lute.lint.yaml` sitting next to the manifest ⇒ silent no-op: the LSP
/// publishes exactly the check surface, never any `L-*` diagnostic (design §2:
/// lint is opt-in).
#[test]
fn no_config_yields_no_lint_diagnostics() {
    let (root, scene) = scaffold("no_config");
    let doc = parse_doc(DIALOGUE_LENGTH_SCENE);
    let out = lint_document(
        &scene,
        &root,
        None,
        &ProviderSet::default(),
        &doc,
        DIALOGUE_LENGTH_SCENE,
    );
    assert!(
        out.is_empty(),
        "opt-in gating: without a config file, no L-* should surface (got {:?})",
        codes(&out)
    );
}

/// `lsp: true` on the config file ⇒ the document's 41-word line surfaces
/// `L-DIALOGUE-LENGTH` (design §2 opt-in, §7 default `maxWords: 40`).
#[test]
fn lsp_true_publishes_dialogue_length() {
    let (root, scene) = scaffold("lsp_true");
    std::fs::write(root.join("lute.lint.yaml"), "lsp: true\n").unwrap();
    let doc = parse_doc(DIALOGUE_LENGTH_SCENE);
    let out = lint_document(
        &scene,
        &root,
        None,
        &ProviderSet::default(),
        &doc,
        DIALOGUE_LENGTH_SCENE,
    );
    assert!(
        out.iter().any(|d| d.code == "L-DIALOGUE-LENGTH"),
        "expected L-DIALOGUE-LENGTH under lsp: true; got {:?}",
        codes(&out)
    );
}

/// `lsp: false` (or the key absent — same default) ⇒ silent no-op even with a
/// config file present, so an opt-out user is never surprised by lint noise in
/// the editor (design §2, §3: `lsp` defaults false).
#[test]
fn lsp_false_yields_no_lint_diagnostics() {
    let (root, scene) = scaffold("lsp_false");
    std::fs::write(root.join("lute.lint.yaml"), "lsp: false\n").unwrap();
    let doc = parse_doc(DIALOGUE_LENGTH_SCENE);
    let out = lint_document(
        &scene,
        &root,
        None,
        &ProviderSet::default(),
        &doc,
        DIALOGUE_LENGTH_SCENE,
    );
    assert!(
        out.is_empty(),
        "lsp: false is a silent no-op; got {:?}",
        codes(&out)
    );
}

/// Malformed YAML ⇒ silent no-op in the editor: the CLI (`lute lint`, exit 2)
/// is the authoritative reporter for config errors (design §3). Publishing an
/// `E-LINT-CONFIG` here would anchor it on a file the editor never opened.
#[test]
fn malformed_config_is_silent() {
    let (root, scene) = scaffold("malformed");
    // A YAML flow-sequence at the root is not a mapping — `parse_config`'s
    // wrong-root-type check turns this into a hard `ConfigError`.
    std::fs::write(root.join("lute.lint.yaml"), "[not, a, mapping]\n").unwrap();
    let doc = parse_doc(DIALOGUE_LENGTH_SCENE);
    let out = lint_document(
        &scene,
        &root,
        None,
        &ProviderSet::default(),
        &doc,
        DIALOGUE_LENGTH_SCENE,
    );
    assert!(
        out.is_empty(),
        "malformed config is silent in the editor; got {:?}",
        codes(&out)
    );
}

/// A `project`-target custom rule that would fire under `LintScope::Full`
/// (`project.scenes > 0` — trivially true for any input document) MUST NOT
/// surface in the LSP: the editor lints one open document at a time and the
/// engine drops project-target rules under `Document` scope (design §2, §7).
#[test]
fn project_target_rule_absent_under_document_scope() {
    let (root, scene) = scaffold("project_scope");
    std::fs::write(
        root.join("lute.lint.yaml"),
        "lsp: true\n\
         custom:\n\
         - id: my-project-rule\n\
         \x20\x20target: project\n\
         \x20\x20when: \"project.scenes > 0\"\n\
         \x20\x20level: warn\n\
         \x20\x20message: \"would fire in Full scope\"\n",
    )
    .unwrap();
    let doc = parse_doc(DIALOGUE_LENGTH_SCENE);
    let out = lint_document(
        &scene,
        &root,
        None,
        &ProviderSet::default(),
        &doc,
        DIALOGUE_LENGTH_SCENE,
    );
    assert!(
        !out.iter().any(|d| d.code == "L-MY-PROJECT-RULE"),
        "Document scope must skip project-target rules; got {:?}",
        codes(&out)
    );
    // Sanity: the document-scoped built-in still fires, so the assertion
    // above is not vacuously true (the engine actually ran).
    assert!(
        out.iter().any(|d| d.code == "L-DIALOGUE-LENGTH"),
        "engine did not run: expected L-DIALOGUE-LENGTH from the built-ins; got {:?}",
        codes(&out)
    );
}
