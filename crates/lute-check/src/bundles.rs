//! dsl 0.23.0 §4 beat bundles: scene-like `<beat id on target when priority
//! once also title>` blocks declared in a lore document (dsl 0.25.0: `share`
//! and `after`).
//!
//! A bundle beat is a beat of the project exactly as a scene beat is — it
//! answers an occasion, is spent by presentation (`once`), and presenting it
//! marks its canonical id `<document id>.<beat id>` visited — but its body
//! lives beside other beats in one lore file instead of in a scene document
//! of its own. This module owns the shape rules ([`check_bundle_beats`],
//! `E-BEAT-ATTR`) and the resolution helpers every later layer reads
//! ([`bundle_beat_key`], [`bundle_beat_once`], [`bundle_beat_priority`]).

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::OccasionDecl;
use lute_syntax::ast::{AttrValue, BundleBeat};

use crate::beats::{parse_beat_priority, BeatOnce, E_BEAT_ATTR};
use crate::lore::{is_entry_ident, is_entry_target};

/// `<beat>`'s permitted attribute keys (dsl 0.23.0 §4). The parser extracts
/// each into a typed field, so a permitted key reaches the residual list only
/// with a value of the wrong shape (a bare `on`, `also="maybe"`) — that is
/// [`E_BEAT_ATTR`]; every OTHER key is `E-UNKNOWN-ATTR`.
pub const BUNDLE_BEAT_ATTRS: &[&str] = &[
    "id", "on", "target", "title", "when", "priority", "once", "also", "share", "after",
];

/// The canonical id of bundle beat `beat_id` in the lore document whose
/// authored `id:` is `doc_id` (dsl 0.23.0 §4): `<document id>.<beat id>`.
pub fn bundle_beat_key(doc_id: &str, beat_id: &str) -> String {
    format!("{doc_id}.{beat_id}")
}

/// A bundle beat's repetition policy, as a scene beat's (dsl 0.21.0 §3.1,
/// 0.24.0 §1): `once="user"` / `"false"` / `"day"` / `"slot"`; anything
/// else — absent, `run`, or a malformed value `E-BEAT-ATTR` already
/// reports — is the default `run`.
pub fn bundle_beat_once(beat: &BundleBeat) -> BeatOnce {
    match beat.once.as_ref().map(|(o, _)| o.as_str()) {
        Some("user") => BeatOnce::User,
        Some("false") => BeatOnce::None,
        Some("day") => BeatOnce::Day,
        Some("slot") => BeatOnce::Slot,
        _ => BeatOnce::Run,
    }
}

/// A bundle beat's resolved priority: the authored integer, else `0`.
pub fn bundle_beat_priority(beat: &BundleBeat) -> i64 {
    beat.priority
        .as_ref()
        .and_then(|(p, _)| parse_beat_priority(p))
        .unwrap_or(0)
}

/// Whether a bundle beat rides along after the `select: first` winner
/// (dsl 0.23.0 §3): `also` / `also="true"`.
pub fn bundle_beat_also(beat: &BundleBeat) -> bool {
    matches!(beat.also, Some((true, _)))
}

/// Every `<beat>` of one lore document (dsl 0.23.0 §4): attribute shape and
/// closure, per-document id uniqueness, the document `id:` the canonical ids
/// hang off, and each beat's occasion against the resolved vocabulary
/// (`E-OCCASION-UNKNOWN`, the untargeted-occasion `target` rule) exactly as a
/// scene beat's. `doc_id` is the document's authored `id:`.
pub fn check_bundle_beats(
    doc_id: Option<&str>,
    beats: &[BundleBeat],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    if let (None, Some(first)) = (doc_id, beats.first()) {
        diags.push(beat_attr(
            "a lore document that bundles `<beat>`s needs a document `id:` — each beat's \
             canonical id is `<document id>.<beat id>` (the key `visited()` and the project \
             index use) (dsl 0.23.0 §4)"
                .to_string(),
            first.id_span,
        ));
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for beat in beats {
        check_shape(beat, &mut diags);
        if is_entry_ident(&beat.id) && !seen.insert(beat.id.as_str()) {
            diags.push(beat_attr(
                format!(
                    "duplicate `<beat id=\"{}\">` in this document; a bundle beat's id names \
                     one beat (dsl 0.23.0 §4)",
                    beat.id
                ),
                beat.id_span,
            ));
        }
        let Some((on, on_span)) = beat.on.as_ref().filter(|(on, _)| is_entry_ident(on)) else {
            continue;
        };
        let target_span = beat
            .target
            .as_ref()
            .filter(|(t, _)| is_entry_target(t))
            .map(|(_, span)| *span);
        crate::beats::check_occasion(
            on,
            *on_span,
            target_span,
            occasions,
            Layer::Logic,
            &mut diags,
        );
    }
    diags
}

/// One beat's attribute shape ([`E_BEAT_ATTR`]) and closure (`E-UNKNOWN-ATTR`).
fn check_shape(beat: &BundleBeat, diags: &mut Vec<Diagnostic>) {
    let mut residual: BTreeSet<&str> = BTreeSet::new();
    for attr in &beat.attrs {
        let key = attr.key.as_str();
        if !BUNDLE_BEAT_ATTRS.contains(&key) || key == "when" {
            continue;
        }
        residual.insert(key);
        let message = if key == "also" {
            "`<beat>` `also` is a flag: write it bare (`<beat … also>`) or as `also=\"true\"` / \
             `also=\"false\"` (dsl 0.23.0 §3)"
                .to_string()
        } else if matches!(attr.value, AttrValue::Str(_)) {
            // A repeated key: the parser took the first occurrence.
            continue;
        } else {
            format!("`<beat>` attribute `{key}` must be a quoted string (dsl 0.23.0 §4)")
        };
        diags.push(beat_attr(message, attr.span));
    }
    crate::logic_attrs::check_bundle_beat_attrs(beat, diags);

    let id = beat.id.as_str();
    if id.is_empty() {
        if !residual.contains("id") {
            diags.push(beat_attr(
                "`<beat>` has no `id`; a bundle beat's id is required — its canonical id is \
                 `<document id>.<id>` (dsl 0.23.0 §4)"
                    .to_string(),
                beat.id_span,
            ));
        }
    } else if !is_entry_ident(id) || id.contains('-') {
        diags.push(beat_attr(
            format!(
                "`<beat id=\"{id}\">`: `id` must be an identifier without `-` \
                 (`[A-Za-z][A-Za-z0-9_]*`) — it is a segment of the canonical id \
                 `<document id>.{id}` that `visited()` reads (dsl 0.23.0 §4)"
            ),
            beat.id_span,
        ));
    }
    match &beat.on {
        None if !residual.contains("on") => diags.push(beat_attr(
            format!(
                "`<beat id=\"{id}\">` names no occasion; a bundle beat answers one — add \
                 `on=\"<occasion>\"` (dsl 0.23.0 §4)"
            ),
            beat.id_span,
        )),
        Some((on, span)) if !is_entry_ident(on) => diags.push(beat_attr(
            format!(
                "`<beat>` `on=\"{on}\"` must name an occasion — an identifier \
                 (`[A-Za-z][A-Za-z0-9_-]*`) (dsl 0.23.0 §4)"
            ),
            *span,
        )),
        _ => {}
    }
    if let Some((target, span)) = &beat.target {
        if !is_entry_target(target) {
            diags.push(beat_attr(
                format!(
                    "`<beat>` `target=\"{target}\"` is malformed; a target is a dotted id \
                     `Ident (\".\" Segment)*` with `Segment ::= [A-Za-z0-9_-]+`, e.g. \
                     `npc.porter` (dsl 0.23.0 §4)"
                ),
                *span,
            ));
        }
    }
    if let Some((raw, span)) = &beat.priority {
        if parse_beat_priority(raw).is_none() {
            diags.push(beat_attr(
                format!("`<beat>` `priority=\"{raw}\"` must be an integer (dsl 0.23.0 §4)"),
                *span,
            ));
        }
    }
    if let Some((raw, span)) = &beat.once {
        if !matches!(raw.as_str(), "run" | "user" | "false" | "day" | "slot") {
            diags.push(beat_attr(
                format!(
                    "`<beat>` `once=\"{raw}\"` must be `run` (once per run, the default), `user` \
                     (once ever), `day` / `slot` (once per clock day / slot), or `false` \
                     (repeatable) (dsl 0.23.0 §4, 0.24.0 §1)"
                ),
                *span,
            ));
        }
    }
    // dsl 0.25.0 §2: a shared spend needs a spend to share.
    if let Some((key, span)) = &beat.share {
        let once = beat.once.as_ref().map(|(o, _)| o.as_str());
        if !is_entry_ident(key) {
            diags.push(beat_attr(
                crate::beats::share_malformed("`<beat>`", key),
                *span,
            ));
        } else if once == Some("false") || (once.is_none() && !residual.contains("once")) {
            diags.push(beat_attr(crate::beats::share_without_once(key), *span));
        }
    }
    // dsl 0.25.0 §3: `after=` under the scene `after:` grammar
    // (`E-CONN-PROFILE`); an exact empty value declares no prerequisite.
    if let Some((after, span)) = beat.after.as_ref().filter(|(a, _)| !a.is_empty()) {
        diags.extend(crate::prereq::parse_prereq(after, *span).1);
    }
}

fn beat_attr(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_BEAT_ATTR.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
