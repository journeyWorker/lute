//! Timeline resolver + resolved-table view (dsl §11.4).
//!
//! A `<timeline>` stages parallel `<track>`s of `<clip>`s onto a shared clock.
//! [`resolve_timeline`] flattens that into a [`ResolvedTimeline`] — one
//! [`ResolvedRow`] per clip with its absolute start `at`, the subject it drives,
//! a short `summary`, and its `duration` — plus every staging diagnostic the
//! layout produces. T4.9 renders the resolved table and surfaces the diagnostics.
//!
//! ## Cursor (§11.4 sequential-omission)
//! Each track carries an independent cursor. A clip with an omitted `at` starts
//! at `0.0` when it is the track's first clip, otherwise immediately after the
//! previous clip's END (`prev.at + prev.duration`). An explicit `at` places the
//! clip there and resets the cursor to that clip's end. A clip's duration comes
//! from its directive's `duration` timing attr (§7.5), best-effort parsed;
//! absent/non-numeric ⇒ `0.0`. `<set>` clips carry no duration ⇒ `0.0`.
//!
//! ## Diagnostics (all [`Layer::Staging`])
//! - **`E-DUP-TRACK`** — two `<track>`s share the same [`TrackKey`].
//! - **`E-CLIP-OVERLAP`** — two clips in the SAME track whose
//!   `[at, at+duration)` half-open intervals overlap.
//! - **`E-WRITE-CONFLICT`** — two clips in DIFFERENT tracks that write the same
//!   subject at overlapping times (see scope note below).
//! - **`W-TIMELINE-TRACKS`** — more than 8 tracks.
//! - **`W-TIMELINE-CLIPS`** — more than 12 clips in a single track.
//! - **`W-TIMELINE-TOTAL`** — more than 40 clips across all tracks.
//!
//! ## Barrier (§11.4)
//! `barrier_at` is the timeline's explicit `duration` when present (its
//! [`CelSlot`](lute_syntax::ast::CelSlot) `raw` parsed best-effort as `f64`),
//! otherwise the maximum clip end across all tracks (`0.0` for an empty
//! timeline).
//!
//! ## `E-WRITE-CONFLICT` scope / limitation
//! Precise write-target resolution needs each directive's declared `writes[]`
//! from the resolved [`CapabilitySnapshot`], which is NOT threaded into
//! `resolve_timeline` (only [`Ctx`] is). So the check is scoped to the subject
//! derivable from the [`TrackKey`] itself: two clips conflict when their tracks
//! resolve to the SAME subject (`Subject(s)` or `Property { subject, .. }`;
//! `Channel` tracks have no subject and never conflict) via DIFFERENT keys and
//! their intervals overlap. Exact-duplicate keys are left to `E-DUP-TRACK` to
//! avoid piling two errors on one cause. Property-level and directive-level
//! (`writes[]`) precision is deferred until the snapshot is available here.

use std::collections::BTreeSet;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{AttrValue, ClipNode, Timeline, TrackKey};

use crate::ctx::Ctx;

/// One resolved clip: its absolute start, the subject it drives, a short
/// human-readable summary, and its duration (seconds, best-effort).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ResolvedRow {
    pub at: f64,
    pub subject: String,
    pub summary: String,
    pub duration: f64,
}

/// The flattened timeline: one [`ResolvedRow`] per clip in document order plus
/// the final barrier time.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ResolvedTimeline {
    pub rows: Vec<ResolvedRow>,
    pub barrier_at: f64,
}

/// Resolve a `<timeline>` into its table view + staging diagnostics (dsl §11.4).
pub fn resolve_timeline(tl: &Timeline, _ctx: &Ctx) -> (ResolvedTimeline, Vec<Diagnostic>) {
    let mut diags = Vec::new();

    // Size warnings (arch LSP feature map).
    if tl.tracks.len() > 8 {
        diags.push(diag(
            "W-TIMELINE-TRACKS",
            Severity::Warning,
            format!("timeline has {} tracks (>8); consider splitting", tl.tracks.len()),
            tl.span,
        ));
    }
    let total_clips: usize = tl.tracks.iter().map(|t| t.clips.len()).sum();
    if total_clips > 40 {
        diags.push(diag(
            "W-TIMELINE-TOTAL",
            Severity::Warning,
            format!("timeline has {total_clips} clips (>40); consider splitting"),
            tl.span,
        ));
    }

    // Duplicate track key (E-DUP-TRACK). Keys are normalized to a canonical
    // string since `TrackKey` does not derive `Eq`.
    let mut seen_keys: BTreeSet<String> = BTreeSet::new();
    for track in &tl.tracks {
        let canon = canon_key(&track.key);
        if !seen_keys.insert(canon.clone()) {
            diags.push(diag(
                "E-DUP-TRACK",
                Severity::Error,
                format!("duplicate track key `{canon}` in timeline"),
                track.span,
            ));
        }
    }

    // Per-track resolution: sequential-omission cursor + track-local overlap.
    // `placed` records each clip's absolute interval + subject for the
    // cross-track write-conflict pass.
    struct Placed {
        at: f64,
        end: f64,
        subject: Option<String>,
        key: String,
        span: Span,
    }
    let mut rows = Vec::new();
    let mut placed: Vec<Placed> = Vec::new();
    let mut max_end = 0.0_f64;

    for track in &tl.tracks {
        if track.clips.len() > 12 {
            diags.push(diag(
                "W-TIMELINE-CLIPS",
                Severity::Warning,
                format!(
                    "track `{}` has {} clips (>12); consider splitting",
                    canon_key(&track.key),
                    track.clips.len()
                ),
                track.span,
            ));
        }
        let subject = subject_of(&track.key);
        let canon = canon_key(&track.key);
        // Intervals placed within THIS track, for track-local overlap.
        let mut track_ivals: Vec<(f64, f64)> = Vec::new();
        let mut cursor = 0.0_f64;
        for clip in &track.clips {
            let at = clip.at.unwrap_or(cursor);
            let duration = clip_duration(&clip.node);
            let end = at + duration;

            // Track-local overlap against every earlier clip in this track.
            for &(o_at, o_end) in &track_ivals {
                if at < o_end && o_at < end {
                    diags.push(diag(
                        "E-CLIP-OVERLAP",
                        Severity::Error,
                        format!("clip at {at} overlaps another clip in track `{canon}`"),
                        clip.span,
                    ));
                    break;
                }
            }

            rows.push(ResolvedRow {
                at,
                subject: canon.clone(),
                summary: summary_of(&clip.node),
                duration,
            });
            placed.push(Placed { at, end, subject: subject.clone(), key: canon.clone(), span: clip.span });
            track_ivals.push((at, end));
            if end > max_end {
                max_end = end;
            }
            cursor = end;
        }
    }

    // Cross-track write conflict (E-WRITE-CONFLICT): clips in DIFFERENT tracks
    // resolving to the SAME subject with overlapping intervals. Exact-duplicate
    // keys are excluded (already flagged E-DUP-TRACK).
    for (i, a) in placed.iter().enumerate() {
        let Some(a_subj) = &a.subject else { continue };
        for b in placed.iter().skip(i + 1) {
            let Some(b_subj) = &b.subject else { continue };
            if a.key == b.key || a_subj != b_subj {
                continue;
            }
            if a.at < b.end && b.at < a.end {
                diags.push(diag(
                    "E-WRITE-CONFLICT",
                    Severity::Error,
                    format!("cross-track write conflict on subject `{a_subj}` at overlapping times"),
                    b.span,
                ));
            }
        }
    }

    // Final barrier: explicit `duration` if parseable, else max clip end.
    let barrier_at = tl
        .duration
        .as_ref()
        .and_then(|slot| parse_f64(&slot.raw))
        .unwrap_or(max_end);

    (ResolvedTimeline { rows, barrier_at }, diags)
}

/// Canonical string for a [`TrackKey`], used for equality/dedup and display.
fn canon_key(key: &TrackKey) -> String {
    match key {
        TrackKey::Subject(s) => s.clone(),
        TrackKey::Channel(c) => format!("#{c}"),
        TrackKey::Property { subject, property } => format!("{subject}.{property}"),
    }
}

/// Subject a track drives, for cross-track conflict scoping. `Channel` tracks
/// have no subject.
fn subject_of(key: &TrackKey) -> Option<String> {
    match key {
        TrackKey::Subject(s) => Some(s.clone()),
        TrackKey::Property { subject, .. } => Some(subject.clone()),
        TrackKey::Channel(_) => None,
    }
}

/// Best-effort clip duration from a directive's `duration` timing attr (§7.5).
/// `<set>` clips and directives without a numeric `duration` ⇒ `0.0`.
fn clip_duration(node: &ClipNode) -> f64 {
    match node {
        ClipNode::Directive(d) => d
            .attrs
            .iter()
            .find(|a| a.key == "duration")
            .and_then(|a| match &a.value {
                AttrValue::Str(s) => parse_f64(s),
                AttrValue::Ref(slot) => parse_f64(&slot.raw),
                AttrValue::BoolTrue => None,
            })
            .unwrap_or(0.0),
        ClipNode::Set(_) => 0.0,
    }
}

/// Short human-readable summary of a clip's node for the resolved table.
fn summary_of(node: &ClipNode) -> String {
    match node {
        ClipNode::Directive(d) => format!("<{}>", d.tag),
        ClipNode::Set(s) => format!("{} {} {}", s.path, s.op, s.expr.raw),
    }
}

/// Parse a best-effort `f64` from a timing string. Accepts a bare number or a
/// number with a trailing `s`/`ms` unit; anything else ⇒ `None`.
fn parse_f64(raw: &str) -> Option<f64> {
    let t = raw.trim();
    if let Ok(v) = t.parse::<f64>() {
        return Some(v);
    }
    if let Some(ms) = t.strip_suffix("ms") {
        return ms.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    }
    if let Some(s) = t.strip_suffix('s') {
        return s.trim().parse::<f64>().ok();
    }
    None
}

/// Build a staging-layer diagnostic at `span`.
fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::ast::{Attr, Clip, Directive, Track};

    fn span() -> Span {
        Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }

    fn ctx() -> Ctx {
        Ctx::default()
    }

    fn dir(tag: &str, duration: &str) -> Directive {
        Directive {
            tag: tag.to_string(),
            attrs: vec![Attr {
                key: "duration".to_string(),
                value: AttrValue::Str(duration.to_string()),
                value_span: span(),
                span: span(),
            }],
            span: span(),
        }
    }

    fn clip(at: Option<f64>, duration: &str) -> Clip {
        Clip { node: ClipNode::Directive(dir("camera", duration)), at, span: span() }
    }

    fn timeline_camera_two_clips() -> Timeline {
        Timeline {
            duration: None,
            tracks: vec![Track {
                key: TrackKey::Subject("camera".to_string()),
                clips: vec![clip(None, "0.4"), clip(None, "0.4")],
                span: span(),
            }],
            span: span(),
        }
    }

    fn timeline_two_camera_tracks() -> Timeline {
        Timeline {
            duration: None,
            tracks: vec![
                Track {
                    key: TrackKey::Subject("camera".to_string()),
                    clips: vec![clip(None, "0.4")],
                    span: span(),
                },
                Track {
                    key: TrackKey::Subject("camera".to_string()),
                    clips: vec![clip(None, "0.4")],
                    span: span(),
                },
            ],
            span: span(),
        }
    }

    #[test]
    fn omitted_at_follows_previous_clip_end() {
        // track camera: clip A dur 0.4 (at omitted=>0.0), clip B dur 0.4 (at omitted=>0.4)
        let tl = timeline_camera_two_clips();
        let (res, diags) = resolve_timeline(&tl, &ctx());
        assert!(diags.is_empty());
        assert_eq!(res.rows[1].at, 0.4);
    }

    #[test]
    fn duplicate_track_key_errors() {
        let tl = timeline_two_camera_tracks();
        let (_res, diags) = resolve_timeline(&tl, &ctx());
        assert!(diags.iter().any(|d| d.code == "E-DUP-TRACK"));
    }

    #[test]
    fn barrier_is_max_end_when_no_duration() {
        let tl = timeline_camera_two_clips(); // ends at 0.8, no explicit duration
        let (res, _d) = resolve_timeline(&tl, &ctx());
        assert_eq!(res.barrier_at, 0.8);
    }
}
