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
        defaults: Default::default(),
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

/// 0.10.0 §10.1: a time value finer than a millisecond is an error naming the
/// resolution limit. No rounding — a timeline the author cannot see the
/// difference in is a timeline whose diagnostics they cannot predict.
#[test]
fn clip_at_finer_than_a_millisecond_is_rejected() {
    let t = format!(
        "{HDR}<timeline>\n<track channel=\"vfx\">\n\
         ::vfx{{type=\"shed\" at=\"1.2000001\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-TIME-RESOLUTION"),
        "expected E-TIME-RESOLUTION for at=\"1.2000001\"; got {cs:?}"
    );
}

#[test]
fn duration_finer_than_a_millisecond_is_rejected() {
    let t = format!(
        "{HDR}<timeline>\n<track subject=\"camera\">\n\
         ::camera{{focus=\"x\" duration=\"0.12345\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-TIME-RESOLUTION"),
        "expected E-TIME-RESOLUTION for duration=\"0.12345\"; got {cs:?}"
    );
}

#[test]
fn delay_finer_than_a_millisecond_is_rejected() {
    let t = format!("{HDR}::camera{{focus=\"x\" delay=\"0.00005\"}}\n");
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-TIME-RESOLUTION"),
        "expected E-TIME-RESOLUTION for delay=\"0.00005\"; got {cs:?}"
    );
}

#[test]
fn timeline_duration_finer_than_a_millisecond_is_rejected() {
    let t = format!(
        "{HDR}<timeline duration=\"1.23456\">\n<track subject=\"camera\">\n\
         ::camera{{focus=\"x\" duration=\"0.5\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        cs.iter().any(|c| c == "E-TIME-RESOLUTION"),
        "expected E-TIME-RESOLUTION for <timeline duration=\"1.23456\">; got {cs:?}"
    );
}

/// Three fractional digits are exactly legal — the boundary, not one side of it.
#[test]
fn three_fractional_digits_are_legal() {
    let t = format!(
        "{HDR}<timeline>\n<track channel=\"vfx\">\n\
         ::vfx{{type=\"shed\" at=\"1.005\" duration=\"0.125\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        !cs.iter().any(|c| c == "E-TIME-RESOLUTION"),
        "1 ms is the limit, not 0.1 ms; got {cs:?}"
    );
}

/// §10.2's last paragraph: a value that does not parse as a decimal at all
/// keeps its CURRENT behaviour (a clip `at` is treated as absent). That is
/// pre-existing and underspecified; #26 does not change it, and it must not
/// become `E-TIME-RESOLUTION`.
#[test]
fn a_non_numeric_time_is_not_a_resolution_error() {
    let t = format!(
        "{HDR}<timeline>\n<track channel=\"vfx\">\n\
         ::vfx{{type=\"shed\" at=\"soon\"}}\n\
         </track>\n</timeline>\n"
    );
    let cs = codes(&t);
    assert!(
        !cs.iter().any(|c| c == "E-TIME-RESOLUTION"),
        "an unparseable value keeps its pre-existing fallback (§10.2); got {cs:?}"
    );
}
