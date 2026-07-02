//! Per-directive golden-snapshot suite (Task 5.2 — plugin §12 acceptance gate).
//!
//! One `tests/golden/<name>.lute` fixture per baseline directive plus the four
//! block constructs (`::set`, `<branch>`, `<match>`, `<timeline>`). Each test
//! runs the assembled `check()` (Task 4.9) over its fixture and pins the
//! resulting diagnostics via `insta`; the block cases additionally pin the
//! `resolved` view (timeline tables + auto-injections), the surface Phase-6's
//! divergence golden and the CLI both wrap. Every fixture is a small, VALID
//! `lute.core` document that checks clean — the snapshot is the executable
//! record of "what `check()` emits for a correct document of this shape".

use lute_check::{check, CheckInput, Mode};

/// Canonical `CheckInput` for a fixture: the core snapshot, an empty (fully
/// permissive) provider set, and interactive `Author` mode — the same shape the
/// example integration tests use (`tests/examples.rs`).
fn input_for(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Author,
    }
}

/// A golden that pins only the diagnostics stream.
macro_rules! golden_diags {
    ($test:ident, $fixture:literal) => {
        #[test]
        fn $test() {
            let text = include_str!(concat!("golden/", $fixture, ".lute"));
            let res = check(&input_for(text));
            insta::assert_yaml_snapshot!(res.diagnostics);
        }
    };
}

/// A golden that pins diagnostics AND the resolved view (a second, named
/// snapshot) — used for the block constructs whose value is the resolved shape
/// (timeline tables, auto-injections, command preview), not just diagnostics.
macro_rules! golden_diags_resolved {
    ($test:ident, $fixture:literal, $resolved:literal) => {
        #[test]
        fn $test() {
            let text = include_str!(concat!("golden/", $fixture, ".lute"));
            let res = check(&input_for(text));
            insta::assert_yaml_snapshot!(res.diagnostics);
            insta::assert_yaml_snapshot!($resolved, res.resolved);
        }
    };
}

// --- baseline directives (dsl Appendix A) --------------------------------
golden_diags!(golden_bg_ok, "bg_ok");
golden_diags!(golden_music_ok, "music_ok");
golden_diags!(golden_sfx_ok, "sfx_ok");
golden_diags!(golden_auto_ok, "auto_ok");
golden_diags!(golden_vfx_ok, "vfx_ok");
golden_diags!(golden_cut_ok, "cut_ok");
golden_diags!(golden_video_ok, "video_ok");
golden_diags!(golden_camera_ok, "camera_ok");

// --- `::set` (dsl §7.3.4) ------------------------------------------------
golden_diags!(golden_set_ok, "set_ok");

// --- block constructs: diagnostics + pinned resolved view ----------------
golden_diags_resolved!(golden_branch_ok, "branch_ok", "branch_ok_resolved");
golden_diags_resolved!(golden_match_ok, "match_ok", "match_ok_resolved");
golden_diags_resolved!(golden_timeline_ok, "timeline_ok", "timeline_ok_resolved");
