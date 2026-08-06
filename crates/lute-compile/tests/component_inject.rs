//! Task 7g, compile side — the de-duplication invariant behind
//! `lib.rs`'s `state.diags.clear()`.
//!
//! `compile()`'s own CFG walk folds the NORMALIZED tree, so an imported
//! component body's staging is already inlined and whatever the injection
//! fold raises for it IS derived there. The walk's `StageState.diags` is
//! nonetheless cleared: `check()` is the single diagnostic surface (D6).
//!
//! Until Task 7g that clear was justified by a premise that was FALSE for a
//! component body — `check()` did NOT re-derive the body's diagnostics,
//! because `fold_injections` treated a `::use` as opaque — so they were
//! reported by no tool at all. `check()` now folds through the `::use` with
//! the stage state inherited at that site, which is exactly the context this
//! walk folds it in, and the clear is honest again.
//!
//! dsl 0.10.0 §12.3 (**D-U**) removed `W-INJECT-CONFLICT`, the code this suite
//! was written around. The explicit-anchor shape below is now SILENT, which
//! makes the observable property stronger rather than weaker: a real
//! `compile()` emits no diagnostics of its own for either shape, `check()`
//! emits none either, and the ARTIFACT stays correct — the author's explicit
//! anchor is honoured and no second anchor is injected beside it.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lute_check::{check, parse_meta, resolve_components, resolve_imports, CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir()
        .join(format!("lute_cinj_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Assemble a `CheckInput` for `scene_text` against on-disk component fixtures
/// in `dir`, exactly as the CLI does (mirrors `tests/component_fold.rs`).
fn input_for(dir: &Path, scene_text: &str) -> CheckInput {
    let (doc, _) = lute_syntax::parse(scene_text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let imports = resolve_imports(dir, &meta0.uses, &meta0.extends, doc.meta.span);
    CheckInput {
        text: scene_text.to_string(),
        uri: "scene.lute".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
        defaults: Default::default(),
    }
}

/// `::auto` whose explicit `anchor` equals the `anchor` domain's declared
/// `default:` — until 0.10.0 §12.3 the one shape `auto-anchor-on-show` warned
/// about, and now silent on every leg.
const CONFLICT_BODY: &str = "::auto{character=\"bianca\" anchor=\"center\"}\n@bianca: Hello.\n";

const SCENE_INLINE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\
## Shot 1.\n::auto{character=\"bianca\" anchor=\"center\"}\n@bianca: Hello.\n";

const SCENE_USE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
components: [c.lute]\n---\n## Shot 1.\n::use{component=\"c\"}\n";

/// The `sprite` records — `::auto` lowers to ONE of them carrying the resolved
/// `anchor`, whether the author wrote it or `auto-anchor-on-show` injected it.
/// A double injection would show up here as a second record or a lost anchor.
fn sprite_records(artifact: &lute_compile::Artifact) -> Vec<serde_json::Value> {
    artifact
        .commands
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .filter(|v| v["kind"] == "sprite")
        .collect()
}

/// Both shapes: `check()` reports NOTHING (0.10.0 §12.3) and `compile()`
/// succeeds having emitted nothing of its own, with the author's anchor the
/// only anchor in the artifact.
#[test]
fn compile_adds_no_diagnostics_of_its_own_for_an_explicit_anchor() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        format!("---\ncomponent: c\n---\n## Scene 1.\n{CONFLICT_BODY}"),
    )
    .unwrap();

    for (label, text) in [("scene level", SCENE_INLINE), ("via ::use", SCENE_USE)] {
        let input = input_for(&dir, text);
        let res = check(&input);
        assert!(
            res.diagnostics.is_empty(),
            "{label}: an explicit anchor equal to the declared default is silent \
             as of 0.10.0 §12.3: {:#?}",
            res.diagnostics
        );

        let artifact = compile(&input)
            .unwrap_or_else(|e| panic!("{label}: this shape must never gate: {e:#?}"));
        let sprites = sprite_records(&artifact);
        assert_eq!(
            sprites.len(),
            1,
            "{label}: the author's anchor is honored, none injected beside it: {sprites:#?}"
        );
        assert_eq!(sprites[0]["anchor"], "center", "{label}");
        assert_eq!(sprites[0]["character"], "bianca", "{label}");
    }
}

/// The artifact a `::use` produces is the artifact the same content produces
/// inline — the property that made this a warning-only loss. Compared on the
/// command stream, so a divergence in what got injected fails here.
#[test]
fn use_and_inline_lower_to_the_same_command_stream() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        format!("---\ncomponent: c\n---\n## Scene 1.\n{CONFLICT_BODY}"),
    )
    .unwrap();
    let inline = compile(&input_for(&dir, SCENE_INLINE)).expect("inline compiles");
    let via_use = compile(&input_for(&dir, SCENE_USE)).expect("::use compiles");
    let kinds = |a: &lute_compile::Artifact| -> Vec<String> {
        a.commands
            .iter()
            .map(|c| serde_json::to_value(c).unwrap()["kind"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(kinds(&inline), kinds(&via_use));
}
