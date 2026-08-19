//! Timeline resolver + resolved-table view (dsl §11.4).
//!
//! A `<timeline>` stages parallel `<track>`s of `<clip>`s onto a shared clock.
//! [`resolve_timeline`] flattens that into a [`ResolvedTimeline`] — one
//! [`ResolvedRow`] per clip with its absolute start `at`, the subject it drives,
//! a short `summary`, and its `duration` — plus every staging diagnostic the
//! layout produces. T4.9 renders the resolved table and surfaces the diagnostics.
//!
//! ## Cursor (§11.4 sequential-omission, dsl 0.10.0 §10.2)
//! Each track carries an independent cursor, and every cursor value, clip end,
//! maximum end and barrier inside [`resolve_timeline`] is an **integer number of
//! milliseconds** (**D-H**). A clip with an omitted `at` starts at `0` when it is
//! the track's first clip, otherwise immediately after the previous clip's END
//! (`prev.at + prev.duration`). An explicit `at` places the clip there and resets
//! the cursor to that clip's end. A clip's duration comes from its directive's
//! `duration` timing attr (§7.5), read with
//! [`parse_time_ms`](crate::time::parse_time_ms); absent/non-numeric ⇒ `0`.
//! `<set>` clips carry no duration ⇒ `0`.
//!
//! Integers, not an epsilon: `0.8 + 0.4` in IEEE-754 is `1.2000000000000002`,
//! so a clip handed off at the previous clip's exact end used to land one ULP
//! inside it and draw `E-CLIP-OVERLAP`. An epsilon would have replaced one false
//! positive with an unpredictable class of them.
//!
//! ## Diagnostics (all [`Layer::Staging`])
//! - **`E-DUP-TRACK`** — two `<track>`s share the same [`TrackKey`].
//! - **`E-CLIP-OVERLAP`** — two clips in the SAME track whose
//!   `[at, at+duration)` half-open intervals overlap.
//! - **`E-WRITE-CONFLICT`** — two clips on DIFFERENT tracks whose resolved
//!   state-write targets overlap at overlapping times (see model below).
//! - **`E-CLIP-TIMING`** — one clip carries BOTH `at` and `delay` (§7.5, §11.4).
//! - **`E-TIMELINE-DURATION`** — an explicit `duration` below the max resolved
//!   clip end (§11.4); `E-AT-CONTEXT` (`at` outside a track) is raised at the
//!   directive site in `directives.rs`, not here.
//! - **`E-TIME-RESOLUTION`** — an authored time value finer than a millisecond
//!   (0.10.0 §10.1). Clip `at` and `<timeline duration>` are raised here;
//!   `duration=`/`delay=` at the directive site in `directives.rs`.
//! - **`W-TIMELINE-TRACKS`** — more than 8 tracks.
//! - **`W-TIMELINE-CLIPS`** — more than 12 clips in a single track.
//! - **`W-TIMELINE-TOTAL`** — more than 40 clips across all tracks.
//!
//! ## Barrier (§11.4, 0.10.0 §10.2/§10.3)
//! `barrier_at` is the timeline's explicit `duration` when present (its
//! [`CelSlot`](lute_syntax::ast::CelSlot) `raw` read with
//! [`parse_time_ms`](crate::time::parse_time_ms)), otherwise the maximum clip
//! end across all tracks (`0` for an empty timeline).
//!
//! ## Seconds at the boundary (**D-T**, §10.3)
//! [`ResolvedRow::at`], [`ResolvedRow::duration`] and
//! [`ResolvedTimeline::barrier_at`] are `f64` **seconds**, converted once with
//! [`ms_to_seconds`](crate::time::ms_to_seconds). They are the rendered surface
//! (`check --json`'s `resolved.timeline_tables`) and the compiler's input
//! (`lute-compile`'s `schedule.rs` → `Stamp.at`), and the artifact keeps
//! seconds: changing those fields to milliseconds under the same names and the
//! same JSON type would be a break with no detection surface at all.
//!
//! ## `E-WRITE-CONFLICT` model
//! Compares each clip's resolved directive `effects.writes[]` state-write
//! targets: a `<set>` writes its path verbatim; a known effectless directive
//! writes nothing; an unknown directive or an unresolvable `fromAttr` falls
//! back to the coarse track subject as a single conservative target. Clips on
//! DIFFERENT tracks whose targets overlap (equal or dotted-boundary prefix) at
//! overlapping times are flagged. Exact-duplicate keys are left to
//! `E-DUP-TRACK`; clips that provably write nothing never conflict.

use std::collections::BTreeSet;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::PathSegment;
use lute_syntax::ast::{AttrValue, Clip, ClipNode, Timeline, TrackKey};

use crate::ctx::Ctx;
use crate::time::{
    fmt_seconds, ms_to_seconds, parse_time_ms, TimeParse, TIME_MAX_FRACTIONAL_DIGITS,
};

/// `E-CLIP-TIMING`: a single `<track>` clip carrying BOTH `at` (an absolute
/// timeline position) and `delay` (a relative nudge) — mutually exclusive on one
/// clip (dsl §7.5, §11.4).
pub const E_CLIP_TIMING: &str = "E-CLIP-TIMING";

/// `E-TIMELINE-DURATION`: an explicit `<timeline duration>` that is LESS THAN the
/// maximum resolved clip end across all tracks — a timeline may not truncate its
/// own content (dsl §11.4).
pub const E_TIMELINE_DURATION: &str = "E-TIMELINE-DURATION";

/// `E-TIME-RESOLUTION`: an authored time value carrying more fractional
/// precision than a millisecond (dsl 0.10.0 §10.1). There is no rounding —
/// a timeline the author cannot see the difference in is a timeline whose
/// diagnostics they cannot predict, and `1.2000001` (the epsilon superstition
/// T7.2 measured an author acquiring) must be rejected rather than quietly
/// honoured.
pub const E_TIME_RESOLUTION: &str = "E-TIME-RESOLUTION";

/// The one `E-TIME-RESOLUTION` message, shared by all four authorable time
/// surfaces (clip `at`, `duration`, `delay`, `<timeline duration>`) so they
/// cannot drift. `what` names the surface; `raw` is the authored text verbatim.
pub fn time_resolution_diag(what: &str, raw: &str, span: Span) -> Diagnostic {
    diag(
        E_TIME_RESOLUTION,
        Severity::Error,
        format!(
            "{what} `{raw}` is finer than a millisecond; a time value is a decimal number of \
             seconds with at most {TIME_MAX_FRACTIONAL_DIGITS} fractional digits \
             (dsl 0.10.0 §10.1)"
        ),
        span,
    )
}

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
pub fn resolve_timeline(
    tl: &Timeline,
    _ctx: &Ctx<'_>,
    snapshot: &CapabilitySnapshot,
) -> (ResolvedTimeline, Vec<Diagnostic>) {
    let mut diags = Vec::new();

    // Size warnings (arch LSP feature map).
    if tl.tracks.len() > 8 {
        diags.push(diag(
            "W-TIMELINE-TRACKS",
            Severity::Warning,
            format!(
                "timeline has {} tracks (>8); consider splitting",
                tl.tracks.len()
            ),
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

    // Track-key validation (E-TRACK-KEY) + duplicate track key (E-DUP-TRACK).
    // Keys are normalized to a canonical string since `TrackKey` does not
    // derive `Eq`. A keyless track (or `property=` without `subject=`) collapses
    // to an empty canonical key and is reported once as E-TRACK-KEY; it is not
    // also flagged as a duplicate.
    let mut seen_keys: BTreeSet<String> = BTreeSet::new();
    for track in &tl.tracks {
        let canon = canon_key(&track.key);
        if canon.is_empty() {
            diags.push(diag(
                "E-TRACK-KEY",
                Severity::Error,
                "a <track> requires `subject`, `channel`, or `subject`+`property` (dsl §7.4)"
                    .to_string(),
                track.span,
            ));
            continue; // don't also flag empty keys as duplicates
        }
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
    //
    // dsl 0.10.0 §10.2 (**D-H**): ALL cursor arithmetic here is integer
    // MILLISECONDS. The comparison below was never the bug — `at < o_end &&
    // o_at < end` is a correct symmetric half-open intersection — the bug was
    // that `0.8 + 0.4` in IEEE-754 is `1.2000000000000002`, so a hand-off at
    // the previous clip's exact end landed one ULP inside it. An epsilon would
    // have replaced one false positive with an unpredictable class of them;
    // integers remove the class.
    //
    // `placed` records each clip's absolute interval + resolved write targets
    // for the cross-track write-conflict pass.
    struct Placed {
        at_ms: i64,
        end_ms: i64,
        targets: WriteTargets,
        key: String,
        span: Span,
    }
    let mut rows = Vec::new();
    let mut placed: Vec<Placed> = Vec::new();
    let mut max_end_ms = 0_i64;

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
        let mut track_ivals: Vec<(i64, i64)> = Vec::new();
        let mut cursor_ms = 0_i64;
        for clip in &track.clips {
            let explicit_at_ms = clip_at_ms(clip, &mut diags);
            let at_ms = explicit_at_ms.unwrap_or(cursor_ms);
            let duration_ms = clip_duration_ms(&clip.node);
            let end_ms = at_ms + duration_ms;

            // Track-local overlap against every earlier clip in this track.
            // Integer comparison in the millisecond domain: two clips whose
            // starts differ by less than a millisecond are the same instant
            // (§10.2's resolution limit), which is also where
            // `E-WRITE-CONFLICT`'s "same instant" is now defined.
            for &(o_at, o_end) in &track_ivals {
                if at_ms < o_end && o_at < end_ms {
                    diags.push(diag(
                        "E-CLIP-OVERLAP",
                        Severity::Error,
                        format!(
                            "clip at {} overlaps another clip in track `{canon}`",
                            fmt_seconds(at_ms)
                        ),
                        clip.span,
                    ));
                    break;
                }
            }

            // E-CLIP-TIMING (dsl §7.5, §11.4): `at` (absolute position) and
            // `delay` (relative nudge) are mutually exclusive on one clip. Keyed
            // off whether the `at` RESOLVED to a position: §10.2 keeps a
            // non-decimal `at`'s pre-existing "treated as absent" behaviour, and
            // a green document may not go red for it.
            if explicit_at_ms.is_some() && clip_has_delay(&clip.node) {
                diags.push(diag(
                    E_CLIP_TIMING,
                    Severity::Error,
                    format!(
                        "clip in track `{canon}` carries both `at` and `delay`; they are mutually exclusive (dsl §7.5)"
                    ),
                    clip.span,
                ));
            }

            // §10.3 (**D-T**): the row is the RENDERED surface and the
            // compiler's input, and both keep SECONDS. One conversion, here.
            rows.push(ResolvedRow {
                at: ms_to_seconds(at_ms),
                subject: canon.clone(),
                summary: summary_of(&clip.node),
                duration: ms_to_seconds(duration_ms),
            });
            placed.push(Placed {
                at_ms,
                end_ms,
                targets: clip_write_targets(&clip.node, snapshot, subject.as_deref()),
                key: canon.clone(),
                span: clip.span,
            });
            track_ivals.push((at_ms, end_ms));
            if end_ms > max_end_ms {
                max_end_ms = end_ms;
            }
            cursor_ms = end_ms;
        }
    }

    // Cross-track write conflict (E-WRITE-CONFLICT): clips on DIFFERENT tracks whose
    // resolved state-write targets overlap at overlapping times. Exact-duplicate
    // keys are excluded (already flagged E-DUP-TRACK). Clips that provably write
    // nothing (WriteTargets::None) never conflict.
    for (i, a) in placed.iter().enumerate() {
        for b in placed.iter().skip(i + 1) {
            if a.key == b.key {
                continue;
            }
            // Half-open interval overlap, in milliseconds.
            if a.at_ms < b.end_ms && b.at_ms < a.end_ms {
                if let Some(target) = targets_overlap(&a.targets, &b.targets) {
                    diags.push(diag(
                        "E-WRITE-CONFLICT",
                        Severity::Error,
                        format!("cross-track write conflict on `{target}` at overlapping times"),
                        b.span,
                    ));
                }
            }
        }
    }

    // dsl 0.10.0 §10.1: the timeline's own `duration` is a time value like any
    // other. Diagnosed here rather than at the `check_cel_slot` call in
    // `check.rs`, so all four surfaces share `time_resolution_diag`.
    if let Some(slot) = tl.duration.as_ref() {
        if parse_time_ms(&slot.raw) == TimeParse::TooFine {
            diags.push(time_resolution_diag(
                "`<timeline duration>`",
                &slot.raw,
                slot.span,
            ));
        }
    }

    // Final barrier: explicit `duration` if parseable, else max clip end. An
    // explicit duration BELOW the max resolved clip end truncates the timeline's
    // own content (dsl §11.4) → E-TIMELINE-DURATION (reported at the duration
    // slot span); the barrier still records the authored value.
    //
    // §10.2: the comparison is integer and the message prints the authored
    // decimal — it used to print `1.2000000000000002` at the author, which is
    // the same leak in its most confusing form.
    let explicit_barrier_ms = tl.duration.as_ref().and_then(|slot| match parse_time_ms(&slot.raw) {
        TimeParse::Ms(ms) => Some(ms),
        TimeParse::TooFine | TimeParse::NotANumber => None,
    });
    let barrier_ms = match explicit_barrier_ms {
        Some(explicit_ms) => {
            if explicit_ms < max_end_ms {
                let span = tl.duration.as_ref().map_or(tl.span, |s| s.span);
                diags.push(diag(
                    E_TIMELINE_DURATION,
                    Severity::Error,
                    format!(
                        "timeline duration {} is below the max resolved clip end {}; a timeline may not truncate its own content (dsl §11.4)",
                        fmt_seconds(explicit_ms),
                        fmt_seconds(max_end_ms)
                    ),
                    span,
                ));
            }
            explicit_ms
        }
        None => max_end_ms,
    };

    (
        ResolvedTimeline {
            rows,
            barrier_at: ms_to_seconds(barrier_ms),
        },
        diags,
    )
}

/// A clip's authored absolute start in milliseconds, or `None` when it has no
/// `at` at all — or one that is not a decimal, which §10.2 keeps treating as
/// absent. A sub-millisecond value is `E-TIME-RESOLUTION` at the attribute
/// value's own column and also resolves to `None`, so the clip falls back to the
/// track cursor (§11.4 sequential omission) rather than being placed at a
/// position the author cannot express.
///
/// dsl 0.10.0 §10.2: the conversion shifts the authored decimal three places as
/// TEXT. There is no `f64` on this path.
fn clip_at_ms(clip: &Clip, diags: &mut Vec<Diagnostic>) -> Option<i64> {
    let a = clip.at.as_ref()?;
    match parse_time_ms(&a.raw) {
        TimeParse::Ms(ms) => Some(ms),
        TimeParse::TooFine => {
            diags.push(time_resolution_diag("clip `at`", &a.raw, a.span));
            None
        }
        TimeParse::NotANumber => None,
    }
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

/// What a clip writes, for cross-track conflict detection.
#[derive(Clone, Debug, PartialEq)]
enum WriteTargets {
    /// Fully-resolved concrete state-write paths (e.g. "scene.minigame.service01.score").
    Paths(BTreeSet<String>),
    /// Writes are unresolvable to concrete paths (unknown directive, or a
    /// `fromAttr` path segment with no matching clip attr) — fall back to the
    /// coarse track subject as a single conservative target.
    Coarse(String),
    /// The clip provably writes no state (known directive, empty `effects.writes[]`).
    None,
}

/// Resolve what `node` writes. `track_subject` is the clip's track subject
/// (`subject_of(&track.key)`), used only for the `Coarse` fallback; a `None`
/// track subject with unresolvable writes ⇒ [`WriteTargets::None`] (a Channel
/// track with an unknown directive cannot be scoped, so it never conflicts).
///
/// Pure and total: no panic path. Unresolvable `fromAttr` segments never emit
/// partial paths — the whole clip falls back to `Coarse`/`None`.
fn clip_write_targets(
    node: &ClipNode,
    snapshot: &CapabilitySnapshot,
    track_subject: Option<&str>,
) -> WriteTargets {
    let coarse = || match track_subject {
        Some(s) => WriteTargets::Coarse(s.to_string()),
        None => WriteTargets::None,
    };
    match node {
        // A `<set>` writes its target path verbatim.
        ClipNode::Set(s) => {
            let mut paths = BTreeSet::new();
            paths.insert(s.path.clone());
            WriteTargets::Paths(paths)
        }
        ClipNode::Directive(d) => {
            let Some(decl) = snapshot.directive(&d.tag) else {
                // Unknown directive: cannot resolve writes.
                return coarse();
            };
            let Some(eff) = &decl.effects else {
                // Known directive that declares no effects: provably writes nothing.
                return WriteTargets::None;
            };
            if eff.writes.is_empty() {
                return WriteTargets::None;
            }
            let mut paths = BTreeSet::new();
            for w in &eff.writes {
                let mut path = w.scope.clone();
                for seg in &w.path {
                    let part = match seg {
                        PathSegment::Literal(seg) => seg,
                        PathSegment::FromAttr { from_attr } => {
                            match d.attrs.iter().find(|a| a.key == from_attr.name) {
                                Some(attr) => match &attr.value {
                                    AttrValue::Str(v) => v,
                                    // Non-string attr value can't scope the path.
                                    _ => return coarse(),
                                },
                                // No matching clip attr: segment is unresolvable.
                                None => return coarse(),
                            }
                        }
                    };
                    path.push('.');
                    path.push_str(part);
                }
                paths.insert(path);
            }
            WriteTargets::Paths(paths)
        }
    }
}

/// Materialize a `WriteTargets` into a comparable set, or `None` when the clip
/// writes nothing (a `None` clip never conflicts).
fn targets_as_set(t: &WriteTargets) -> Option<BTreeSet<String>> {
    match t {
        WriteTargets::Paths(p) => Some(p.clone()),
        WriteTargets::Coarse(s) => Some(std::iter::once(s.clone()).collect()),
        WriteTargets::None => None,
    }
}

/// The overlapping state target between two clips, if any. Two paths overlap
/// when equal, or one is a dotted-boundary prefix of the other
/// (`scene.box.k` prefixes `scene.box.k.a` but NOT `scene.box.kk`).
fn targets_overlap(a: &WriteTargets, b: &WriteTargets) -> Option<String> {
    let a_set = targets_as_set(a)?;
    let b_set = targets_as_set(b)?;
    for x in &a_set {
        for y in &b_set {
            if x == y
                || y.strip_prefix(x.as_str())
                    .is_some_and(|r| r.starts_with('.'))
                || x.strip_prefix(y.as_str())
                    .is_some_and(|r| r.starts_with('.'))
            {
                return Some(x.clone().min(y.clone())); // deterministic: report the lower
            }
        }
    }
    None
}

/// A clip's duration in milliseconds, from its directive's `duration` timing
/// attr (§7.5). `<set>` clips and directives without a numeric `duration` ⇒ `0`.
///
/// §10.1's `E-TIME-RESOLUTION` for `duration=` is emitted by
/// `directives::check_directive`, which sees every directive including these,
/// so a sub-millisecond duration resolves to `0` here without a second, derived
/// complaint about the layout.
fn clip_duration_ms(node: &ClipNode) -> i64 {
    match node {
        ClipNode::Directive(d) => d
            .attrs
            .iter()
            .find(|a| a.key == "duration")
            .and_then(|a| match &a.value {
                AttrValue::Str(s) => Some(s.as_str()),
                AttrValue::Ref(slot) => Some(slot.raw.as_str()),
                AttrValue::BoolTrue => None,
            })
            .and_then(|raw| match parse_time_ms(raw) {
                TimeParse::Ms(ms) => Some(ms),
                TimeParse::TooFine | TimeParse::NotANumber => None,
            })
            .unwrap_or(0),
        ClipNode::Set(_) => 0,
    }
}

/// True when a clip's directive carries a `delay` timing attr (§7.5). `<set>`
/// clips carry no timing, so they never carry a `delay`.
fn clip_has_delay(node: &ClipNode) -> bool {
    match node {
        ClipNode::Directive(d) => d.attrs.iter().any(|a| a.key == "delay"),
        ClipNode::Set(_) => false,
    }
}

/// Short human-readable summary of a clip's node for the resolved table.
fn summary_of(node: &ClipNode) -> String {
    match node {
        ClipNode::Directive(d) => format!("<{}>", d.tag),
        ClipNode::Set(s) => format!("{} {} {}", s.path, s.op, s.expr.raw),
    }
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
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Env;
    use lute_manifest::schema::{
        AttrDecl, DirectiveDecl, DirectiveEffects, Lowering, WriteDecl, WriteValue,
    };
    use lute_manifest::types::{FromAttr, Literal, PathSegment, Type};
    use lute_syntax::ast::Set;
    use lute_syntax::ast::{Attr, Clip, ClipAt, Directive, Track};
    use std::sync::LazyLock;

    fn span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn ctx() -> Ctx<'static> {
        static ENV: LazyLock<Env> = LazyLock::new(Env::default);
        Ctx {
            env: &ENV,
            in_match: false,
            match_subject: None,
        }
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
            when: None,
            span: span(),
        }
    }

    fn clip(at: Option<&str>, duration: &str) -> Clip {
        Clip {
            node: ClipNode::Directive(dir("camera", duration)),
            at: at.map(|raw| ClipAt {
                raw: raw.to_string(),
                span: span(),
            }),
            span: span(),
        }
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
        let (res, diags) =
            resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(diags.is_empty());
        assert_eq!(res.rows[1].at, 0.4);
    }

    #[test]
    fn duplicate_track_key_errors() {
        let tl = timeline_two_camera_tracks();
        let (_res, diags) =
            resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(diags.iter().any(|d| d.code == "E-DUP-TRACK"));
    }

    #[test]
    fn barrier_is_max_end_when_no_duration() {
        let tl = timeline_camera_two_clips(); // ends at 0.8, no explicit duration
        let (res, _d) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert_eq!(res.barrier_at, 0.8);
    }

    /// §10.3, D-T: the barrier is the max resolved clip end, in SECONDS, exact.
    #[test]
    fn barrier_is_exact_at_a_leaking_boundary() {
        let tl = Timeline {
            duration: None,
            tracks: vec![Track {
                key: TrackKey::Channel("vfx".into()),
                clips: vec![clip(Some("0.8"), "0.4")],
                span: span(),
            }],
            span: span(),
        };
        let (res, diags) =
            resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(res.barrier_at, 1.2, "0.8 + 0.4 is exactly 1.2");
        assert_eq!(
            serde_json::to_string(&res.barrier_at).unwrap(),
            "1.2",
            "and it serializes as 1.2, not 1.2000000000000002"
        );
    }

    fn snapshot_with_writer() -> CapabilitySnapshot {
        let mut snap = lute_manifest::core::load_core_snapshot();
        let write = |last: &str| WriteDecl {
            scope: "scene".into(),
            path: vec![
                PathSegment::Literal("box".into()),
                PathSegment::FromAttr {
                    from_attr: FromAttr {
                        name: "key".into(),
                        slot_type: None,
                    },
                },
                PathSegment::Literal(last.into()),
            ],
            value: WriteValue::Literal(Literal::Num(0.0)),
        };
        snap.directives.insert(
            "writer".into(),
            DirectiveDecl {
                name: "writer".into(),
                layer: None,
                attrs: vec![AttrDecl {
                    name: "key".into(),
                    required: true,
                    ty: Type::Str,
                    default: None,
                }],
                semantics: vec![],
                state: None,
                effects: Some(DirectiveEffects {
                    writes: vec![write("a"), write("b")],
                }),
                bridge: None,
                lower: Lowering::Builtin {
                    kind: "builtin".into(),
                    name: "writer".into(),
                },
            },
        );
        snap
    }

    fn writer_clip(key: &str) -> ClipNode {
        ClipNode::Directive(Directive {
            tag: "writer".into(),
            attrs: vec![Attr {
                key: "key".into(),
                value: AttrValue::Str(key.into()),
                value_span: span(),
                span: span(),
            }],
            when: None,
            span: span(),
        })
    }

    fn set_clip(path: &str) -> ClipNode {
        ClipNode::Set(Set {
            path: path.into(),
            path_span: span(),
            op: "=".into(),
            expr: lute_syntax::ast::CelSlot::raw(
                lute_syntax::ast::CelKind::SetExpr,
                "0".into(),
                span(),
            ),
            span: span(),
        })
    }

    fn directive_clip(tag: &str) -> ClipNode {
        ClipNode::Directive(Directive {
            tag: tag.into(),
            attrs: vec![],
            when: None,
            span: span(),
        })
    }

    #[test]
    fn write_targets_resolve_fromattr() {
        let snap = snapshot_with_writer();
        let node = writer_clip("k1");
        assert_eq!(
            clip_write_targets(&node, &snap, Some("box")),
            WriteTargets::Paths(
                ["scene.box.k1.a".to_string(), "scene.box.k1.b".to_string()]
                    .into_iter()
                    .collect()
            )
        );
    }

    #[test]
    fn write_targets_set_clip_is_its_path() {
        let snap = lute_manifest::core::load_core_snapshot();
        let node = set_clip("scene.affect.bianca");
        assert_eq!(
            clip_write_targets(&node, &snap, Some("bianca")),
            WriteTargets::Paths(["scene.affect.bianca".to_string()].into_iter().collect())
        );
    }

    #[test]
    fn write_targets_unknown_directive_is_coarse() {
        let snap = lute_manifest::core::load_core_snapshot();
        let node = directive_clip("nosuchdir");
        assert_eq!(
            clip_write_targets(&node, &snap, Some("cam")),
            WriteTargets::Coarse("cam".into())
        );
    }

    #[test]
    fn write_targets_effectless_directive_is_none() {
        // core ::vfx has no effects.writes -> None (provably writes nothing)
        let snap = lute_manifest::core::load_core_snapshot();
        let node = directive_clip("vfx");
        assert_eq!(
            clip_write_targets(&node, &snap, Some("x")),
            WriteTargets::None
        );
    }

    fn writer_clip_dur(key: &str, dur: &str) -> Clip {
        Clip {
            node: ClipNode::Directive(Directive {
                tag: "writer".into(),
                attrs: vec![
                    Attr {
                        key: "key".into(),
                        value: AttrValue::Str(key.into()),
                        value_span: span(),
                        span: span(),
                    },
                    Attr {
                        key: "duration".into(),
                        value: AttrValue::Str(dur.into()),
                        value_span: span(),
                        span: span(),
                    },
                ],
                when: None,
                span: span(),
            }),
            at: None,
            span: span(),
        }
    }

    fn dir_clip_dur(tag: &str, dur: &str) -> Clip {
        Clip {
            node: ClipNode::Directive(Directive {
                tag: tag.into(),
                attrs: vec![Attr {
                    key: "duration".into(),
                    value: AttrValue::Str(dur.into()),
                    value_span: span(),
                    span: span(),
                }],
                when: None,
                span: span(),
            }),
            at: None,
            span: span(),
        }
    }

    // Same subject "box", different property keys -> canon keys "box.pa"/"box.pb" differ.
    fn two_writer_tracks(key_a: &str, key_b: &str) -> Timeline {
        Timeline {
            duration: None,
            span: span(),
            tracks: vec![
                Track {
                    key: TrackKey::Property {
                        subject: "box".into(),
                        property: "pa".into(),
                    },
                    clips: vec![writer_clip_dur(key_a, "1.0")],
                    span: span(),
                },
                Track {
                    key: TrackKey::Property {
                        subject: "box".into(),
                        property: "pb".into(),
                    },
                    clips: vec![writer_clip_dur(key_b, "1.0")],
                    span: span(),
                },
            ],
        }
    }

    #[test]
    fn no_conflict_when_different_properties() {
        // track A ::writer key=k1 -> scene.box.k1.{a,b}; track B ::writer key=k2 -> scene.box.k2.{a,b}
        // same subject "box", DIFFERENT resolved paths, overlapping times -> NO conflict
        // (this is the false-positive the old subject-based rule raised for property tracks)
        let snap = snapshot_with_writer();
        let tl = two_writer_tracks("k1", "k2");
        let (_t, diags) = resolve_timeline(&tl, &ctx(), &snap);
        assert!(
            !diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"),
            "different resolved write paths must not conflict; got {:?}",
            diags.iter().map(|d| d.code.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn conflict_when_same_target() {
        // both tracks write scene.box.k.{a,b} at overlapping times, different tracks -> conflict
        let snap = snapshot_with_writer();
        let tl = two_writer_tracks("k", "k");
        let (_t, diags) = resolve_timeline(&tl, &ctx(), &snap);
        assert!(diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"));
    }

    #[test]
    fn conflict_when_subject_prefixes_property() {
        // track A unknown directive on subject "scene.box.k" -> Coarse("scene.box.k")
        // track B ::writer key=k -> scene.box.k.{a,b}; Coarse prefixes B -> conflict
        let snap = snapshot_with_writer();
        let tl = Timeline {
            duration: None,
            span: span(),
            tracks: vec![
                Track {
                    key: TrackKey::Subject("scene.box.k".into()),
                    clips: vec![dir_clip_dur("nosuchdir", "1.0")],
                    span: span(),
                },
                Track {
                    key: TrackKey::Property {
                        subject: "box".into(),
                        property: "pb".into(),
                    },
                    clips: vec![writer_clip_dur("k", "1.0")],
                    span: span(),
                },
            ],
        };
        let (_t, diags) = resolve_timeline(&tl, &ctx(), &snap);
        assert!(diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"));
    }

    #[test]
    fn effectless_directives_never_conflict() {
        // two ::vfx clips (core, no writes -> None) on different tracks, overlapping -> NO conflict
        let snap = lute_manifest::core::load_core_snapshot();
        let tl = Timeline {
            duration: None,
            span: span(),
            tracks: vec![
                Track {
                    key: TrackKey::Subject("t1".into()),
                    clips: vec![dir_clip_dur("vfx", "1.0")],
                    span: span(),
                },
                Track {
                    key: TrackKey::Subject("t2".into()),
                    clips: vec![dir_clip_dur("vfx", "1.0")],
                    span: span(),
                },
            ],
        };
        let (_t, diags) = resolve_timeline(&tl, &ctx(), &snap);
        assert!(!diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"));
    }

    #[test]
    fn keyless_track_errors() {
        // A <track> with no subject/channel/property collapses to Subject("") -> E-TRACK-KEY.
        let tl = Timeline {
            duration: None,
            span: span(),
            tracks: vec![Track {
                key: TrackKey::Subject(String::new()),
                clips: vec![],
                span: span(),
            }],
        };
        let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(
            diags.iter().any(|d| d.code == "E-TRACK-KEY"),
            "got {:?}",
            diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn property_track_pair_is_clean() {
        // Two property tracks on the SAME subject, DISTINCT properties -> no E-TRACK-KEY, no E-DUP-TRACK.
        let tl = Timeline {
            duration: None,
            span: span(),
            tracks: vec![
                Track {
                    key: TrackKey::Property {
                        subject: "bianca".into(),
                        property: "pos".into(),
                    },
                    clips: vec![],
                    span: span(),
                },
                Track {
                    key: TrackKey::Property {
                        subject: "bianca".into(),
                        property: "opacity".into(),
                    },
                    clips: vec![],
                    span: span(),
                },
            ],
        };
        let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(
            !diags
                .iter()
                .any(|d| d.code == "E-TRACK-KEY" || d.code == "E-DUP-TRACK"),
            "got {:?}",
            diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// A clip carrying BOTH `at` (absolute position) and `delay` (relative nudge)
    /// is contradictory (dsl §7.5, §11.4) -> E-CLIP-TIMING at the clip span.
    #[test]
    fn at_and_delay_same_clip() {
        let tl = Timeline {
            duration: None,
            span: span(),
            tracks: vec![Track {
                key: TrackKey::Subject("camera".to_string()),
                clips: vec![Clip {
                    node: ClipNode::Directive(Directive {
                        tag: "camera".to_string(),
                        attrs: vec![Attr {
                            key: "delay".to_string(),
                            value: AttrValue::Str("0.5".to_string()),
                            value_span: span(),
                            span: span(),
                        }],
                        when: None,
                        span: span(),
                    }),
                    at: Some(ClipAt {
                        raw: "1".to_string(),
                        span: span(),
                    }),
                    span: span(),
                }],
                span: span(),
            }],
        };
        let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(
            diags.iter().any(|d| d.code == "E-CLIP-TIMING"),
            "at+delay on one clip must flag E-CLIP-TIMING; got {:?}",
            diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// An explicit `<timeline duration>` below the max resolved clip end truncates
    /// its own content (dsl §11.4) -> E-TIMELINE-DURATION.
    #[test]
    fn duration_below_content_rejected() {
        // camera track: one clip dur 1.0 (at omitted => 0.0) -> resolved end 1.0.
        let tl = Timeline {
            duration: Some(lute_syntax::ast::CelSlot::raw(
                lute_syntax::ast::CelKind::AttrValue,
                "0.3".into(),
                span(),
            )),
            span: span(),
            tracks: vec![Track {
                key: TrackKey::Subject("camera".to_string()),
                clips: vec![clip(None, "1.0")],
                span: span(),
            }],
        };
        let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(
            diags.iter().any(|d| d.code == "E-TIMELINE-DURATION"),
            "duration 0.3 < max clip end 1.0 must flag E-TIMELINE-DURATION; got {:?}",
            diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// An explicit duration >= the max resolved clip end is clean (the showcase
    /// shape: duration 1.4, max clip end below it).
    #[test]
    fn duration_above_content_is_clean() {
        let tl = Timeline {
            duration: Some(lute_syntax::ast::CelSlot::raw(
                lute_syntax::ast::CelKind::AttrValue,
                "1.4".into(),
                span(),
            )),
            span: span(),
            tracks: vec![Track {
                key: TrackKey::Subject("camera".to_string()),
                clips: vec![clip(None, "1.0")],
                span: span(),
            }],
        };
        let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
        assert!(
            !diags.iter().any(|d| d.code == "E-TIMELINE-DURATION"),
            "duration 1.4 >= max clip end 1.0 must be clean; got {:?}",
            diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }
}
