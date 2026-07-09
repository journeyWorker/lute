//! Timeline cross-cutting timing diagnostics (dsl §7.5, §11.4):
//! - `E-AT-CONTEXT`: `at` on a directive OUTSIDE a `<track>` clip (dedicated,
//!   NOT an `E-UNKNOWN-ATTR` fallthrough).
//! - `E-CLIP-TIMING`: a single clip carrying BOTH `at` and `delay`.
//! - `E-TIMELINE-DURATION`: an explicit `<timeline duration>` below the max
//!   resolved clip end.
//! Fed through the assembled `check()` over inline `state:` frontmatter so the
//! parser's `at`-stripping (track context) and the walker are both exercised.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "timeline_timing".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn at_outside_track_rejected() {
    // A content-context ::camera with `at` (not inside a <track>) → E-AT-CONTEXT,
    // and NOT the generic E-UNKNOWN-ATTR fallthrough.
    let t = format!("{HDR}::camera{{at=\"1.0\"}}\n");
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-AT-CONTEXT"),
        "expected E-AT-CONTEXT for `at` outside a track; got {cs:?}"
    );
    assert!(
        !cs.iter().any(|c| c == "E-UNKNOWN-ATTR"),
        "`at` outside a track is E-AT-CONTEXT, not E-UNKNOWN-ATTR; got {cs:?}"
    );
}

#[test]
fn at_and_delay_same_clip() {
    // A track clip carrying both `at` and `delay` → E-CLIP-TIMING.
    let t = format!(
        "{HDR}<timeline>\n<track subject=\"camera\">\n\
         ::camera{{at=\"1\" delay=\"0.5\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-CLIP-TIMING"),
        "expected E-CLIP-TIMING for at+delay on one clip; got {cs:?}"
    );
}

#[test]
fn duration_below_content_rejected() {
    // <timeline duration="0.3"> whose camera clip ends at 1.0 → E-TIMELINE-DURATION.
    let t = format!(
        "{HDR}<timeline duration=\"0.3\">\n<track subject=\"camera\">\n\
         ::camera{{focus=\"x\" duration=\"1.0\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-TIMELINE-DURATION"),
        "expected E-TIMELINE-DURATION for duration 0.3 < clip end 1.0; got {cs:?}"
    );
}

#[test]
fn at_inside_track_is_clean() {
    // `at` INSIDE a <track> is valid — no E-AT-CONTEXT.
    let t = format!(
        "{HDR}<timeline>\n<track subject=\"camera\">\n\
         ::camera{{focus=\"x\" at=\"0.5\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        !cs.iter().any(|c| c == "E-AT-CONTEXT"),
        "`at` inside a <track> must not flag E-AT-CONTEXT; got {cs:?}"
    );
}

#[test]
fn use_directive_at_outside_track_rejected() {
    // The reserved `::use` directive form is dispatched to component validation
    // BEFORE the generic directive check; a content-context `::use{… at=…}` must
    // still flag E-AT-CONTEXT (not just E-COMPONENT-UNDECLARED / a bogus arg).
    let t = format!("{HDR}::use{{component=\"x\" at=\"1\"}}\n");
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-AT-CONTEXT"),
        "expected E-AT-CONTEXT for `at` on a ::use outside a track; got {cs:?}"
    );
}
