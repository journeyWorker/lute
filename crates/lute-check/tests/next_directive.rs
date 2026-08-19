//! dsl 0.12.0 forward jump (`::mark`/line `id=`/`::next`) — from the
//! checker's side: `W-CODE-AFTER-NEXT` (mirrors `tests/end_directive.rs`'s
//! `W-CODE-AFTER-END` exactly), plus `E-NEXT-UNDEFINED`/`E-NEXT-BACKWARD`/
//! `E-MARK-DUP` through the FULL `check()` pipeline (unit-level coverage of
//! the same three codes lives in `lute_check::next_labels`'s own tests;
//! this file exercises them via the real `CheckInput` entry point instead).

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "next_directive".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

fn count(text: &str, code: &str) -> usize {
    codes(text).iter().filter(|c| c.as_str() == code).count()
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

const W: &str = "W-CODE-AFTER-NEXT";

// --- the directive itself is admitted ---------------------------------------

#[test]
fn next_is_a_known_core_directive() {
    // The mark lives in a SEPARATE shot body — placed in the SAME
    // straight-line body as the unconditional `::next` it would be
    // unreachable code (see `content_after_unguarded_next_in_the_same_shot_warns_once`).
    let out = codes(&format!("{HDR}::next{{to=\"x\"}}\n## Shot 2.\n::mark{{id=\"x\"}}\n"));
    assert!(out.is_empty(), "a clean forward jump must be clean: {out:?}");
}

#[test]
fn next_rejects_an_undeclared_attr() {
    let out = codes(&format!("{HDR}::next{{to=\"x\" bogus=\"1\"}}\n::mark{{id=\"x\"}}\n"));
    assert!(
        out.contains(&"E-UNKNOWN-ATTR".to_string()),
        "`::next` declares only `to`/`when`: {out:?}"
    );
}

#[test]
fn next_without_to_is_missing_attr() {
    let out = codes(&format!("{HDR}::next\n"));
    assert!(
        out.contains(&"E-MISSING-ATTR".to_string()),
        "`to` is required: {out:?}"
    );
}

#[test]
fn mark_without_id_is_missing_attr() {
    let out = codes(&format!("{HDR}::mark\n"));
    assert!(
        out.contains(&"E-MISSING-ATTR".to_string()),
        "`id` is required: {out:?}"
    );
}

// --- W-CODE-AFTER-NEXT: the straight-line rule, mirrors W-CODE-AFTER-END ---

#[test]
fn content_after_unguarded_next_in_the_same_shot_warns_once() {
    let text = format!("{HDR}::next{{to=\"x\"}}\n@narrator: never\n::mark{{id=\"x\"}}\n");
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn the_warning_is_a_warning_with_the_spec_message() {
    let text = format!("{HDR}::next{{to=\"x\"}}\n@narrator: never\n::mark{{id=\"x\"}}\n");
    let result = run(&text);
    let d = result
        .diagnostics
        .iter()
        .find(|d| d.code == W)
        .unwrap_or_else(|| panic!("expected {W}: {:?}", result.diagnostics));
    assert_eq!(d.severity, lute_core_span::Severity::Warning);
    assert_eq!(d.message, "unreachable content after `::next` (the walk jumps away here)");
    // Anchored at the FIRST unreachable node, not at the `::next`.
    assert_eq!(&text[d.span.byte_start..d.span.byte_end], "@narrator: never");
}

#[test]
fn only_the_first_unreachable_node_warns() {
    let text = format!(
        "{HDR}::next{{to=\"x\"}}\n@narrator: a\n@narrator: b\n::sfx{{sound=\"s\"}}\n::mark{{id=\"x\"}}\n"
    );
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn unguarded_next_as_the_last_node_is_clean() {
    let text = format!("{HDR}@narrator: a\n::next{{to=\"x\"}}\n## Shot 2.\n::mark{{id=\"x\"}}\n");
    assert_eq!(count(&text, W), 0, "{:?}", codes(&text));
}

#[test]
fn guarded_next_never_warns_fallthrough_exists() {
    // A GUARDED `::next` does not qualify — fall-through exists (dsl
    // 0.12.0's own dual-branch rule), so content after it is reachable via
    // the `false` arm.
    let text = format!(
        "{HDR}::next{{to=\"x\" when=\"true\"}}\n@narrator: still reachable on false\n::mark{{id=\"x\"}}\n"
    );
    assert_eq!(count(&text, W), 0, "{:?}", codes(&text));
}

// --- E-NEXT-UNDEFINED / E-NEXT-BACKWARD / E-MARK-DUP, full pipeline --------

#[test]
fn next_targeting_an_undefined_label_errors() {
    let out = codes(&format!("{HDR}::next{{to=\"nope\"}}\n"));
    assert!(out.contains(&"E-NEXT-UNDEFINED".to_string()), "{out:?}");
}

#[test]
fn next_targeting_a_backward_label_errors() {
    let text = format!("{HDR}::mark{{id=\"x\"}}\n@narrator: hi\n::next{{to=\"x\"}}\n");
    assert!(codes(&text).contains(&"E-NEXT-BACKWARD".to_string()), "{:?}", codes(&text));
}

#[test]
fn duplicate_mark_ids_error() {
    let text = format!("{HDR}::mark{{id=\"x\"}}\n::mark{{id=\"x\"}}\n");
    assert!(codes(&text).contains(&"E-MARK-DUP".to_string()), "{:?}", codes(&text));
}

#[test]
fn mark_and_line_id_collision_errors() {
    let text = format!("{HDR}::mark{{id=\"x\"}}\n@narrator{{id=\"x\"}}: hi\n");
    assert!(codes(&text).contains(&"E-MARK-DUP".to_string()), "{:?}", codes(&text));
}

// --- guarded `::next{when}` gets the SAME CEL validation a line `when=` does -

#[test]
fn malformed_when_cel_is_reported() {
    let text = format!("{HDR}::next{{to=\"x\" when=\"(\"}}\n::mark{{id=\"x\"}}\n");
    let out = codes(&text);
    assert!(
        out.iter().any(|c| c.starts_with("E-CEL") || c == "E-UNCLASSIFIED"),
        "a malformed guard must be reported through the SAME CEL validation path a line `when=` uses: {out:?}"
    );
}

// --- `::mark`/`::next` are not staging leaves -------------------------------

#[test]
fn next_inside_a_track_clip_is_rejected() {
    let text = format!(
        "{HDR}<timeline>\n  <track key=\"fg\">\n    ::next{{to=\"x\" at=\"0\"}}\n  </track>\n</timeline>\n::mark{{id=\"x\"}}\n"
    );
    let out = codes(&text);
    assert!(
        out.iter().any(|c| c == "E-TIMELINE-CONTENT"),
        "`::next` must not be admitted as a staging clip: {out:?}"
    );
}

#[test]
fn mark_inside_a_track_clip_is_rejected() {
    let text = format!(
        "{HDR}<timeline>\n  <track key=\"fg\">\n    ::mark{{id=\"x\" at=\"0\"}}\n  </track>\n</timeline>\n"
    );
    let out = codes(&text);
    assert!(
        out.iter().any(|c| c == "E-TIMELINE-CONTENT"),
        "`::mark` must not be admitted as a staging clip: {out:?}"
    );
}
