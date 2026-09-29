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
use crate::lore::is_beat_target;
use lute_manifest::ident::is_name;

/// `<beat>`'s permitted attribute keys (dsl 0.23.0 §4). The parser extracts
/// each into a typed field, so a permitted key reaches the residual list only
/// with a value of the wrong shape (a bare `on`) — that is [`E_BEAT_ATTR`],
/// or `E-FLAG-VALUE` for `also="maybe"`; every OTHER key is `E-UNKNOWN-ATTR`.
pub const BUNDLE_BEAT_ATTRS: &[&str] = &[
    "id", "on", "target", "title", "when", "priority", "once", "also", "share", "after", "spentBy",
    "use", "for", "advances",
];

/// The canonical id of bundle beat `beat_id` in the lore document whose
/// authored `id:` is `doc_id` (dsl 0.23.0 §4): `<document id>.<beat id>`.
pub fn bundle_beat_key(doc_id: &str, beat_id: &str) -> String {
    format!("{doc_id}.{beat_id}")
}

/// A bundle beat's repetition policy, as a scene beat's (dsl 0.21.0 §3.1,
/// 0.24.0 §1, 0.27.0 §5): `once="user"` / `"false"` / `"day"` / `"slot"` /
/// `"week"` / `"season:<name>"`; anything else — absent, `run`, or a
/// malformed value `E-BEAT-ATTR` already reports — is the default `run`.
/// On a `spentBy=` beat it is how long the beat stays spent once the
/// condition has held.
pub fn bundle_beat_once(beat: &BundleBeat) -> BeatOnce {
    match beat.once.as_ref().map(|(o, _)| o.as_str()) {
        Some("false") => BeatOnce::None,
        Some(raw) => BeatOnce::parse(raw).unwrap_or(BeatOnce::Run),
        None => BeatOnce::Run,
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
/// closure, the document `id:` the canonical ids hang off, and each beat's
/// occasion against the resolved vocabulary (`E-OCCASION-UNKNOWN`, the
/// untargeted-occasion `target` rule) exactly as a scene beat's. `doc_id` is
/// the document's authored `id:`; `id_written` says the document writes one
/// (a rejected id is its own `E-META-ID`, never also "missing"). Id
/// uniqueness is [`check_beat_ids`]'.
pub fn check_bundle_beats(
    doc_id: Option<&str>,
    id_written: bool,
    beats: &[BundleBeat],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    if let (None, false, Some(first)) = (doc_id, id_written, beats.first()) {
        diags.push(beat_attr(
            "a lore document that bundles `<beat>`s needs a document `id:` — each beat's \
             canonical id is `<document id>.<beat id>` (the key `visited()` and the project \
             index use) (dsl 0.23.0 §4)"
                .to_string(),
            first.id_span,
        ));
    }
    for beat in beats {
        check_shape(beat, &mut diags);
        let Some((on, on_span)) = beat.on.as_ref().filter(|(on, _)| is_name(on)) else {
            continue;
        };
        let target = beat
            .target
            .as_ref()
            .filter(|(t, _)| is_beat_target(t))
            .map(|(t, span)| (t.as_str(), *span));
        crate::beats::check_occasion(
            on,
            *on_span,
            target,
            true,
            occasions,
            Layer::Logic,
            &mut diags,
        );
        if let Some((true, also_span)) = beat.also {
            diags.extend(crate::beats::also_fault(
                on,
                also_span,
                false,
                occasions,
                Layer::Logic,
            ));
        }
    }
    diags
}

/// Two declarations of one lore document share an id: a
/// bundle `<beat>` id repeated, or an `<entry>` whose id is a `<beat>`'s —
/// the beat's canonical id `<document id>.<id>` is then also the entry's
/// alias, so one name (`visited()`, a play's `expect.winner`) means two
/// beats.
pub const E_BEAT_ID_DUP: &str = "E-BEAT-ID-DUP";

/// [`E_BEAT_ID_DUP`] for one document, at every declaration past the first
/// of an id its `<beat>`s and `<entry>`s share, in document order. `doc_id`
/// is the document's authored `id:` (without one no canonical id exists and
/// only a repeated beat id is reported).
pub fn check_beat_ids(
    doc_id: Option<&str>,
    entries: &[lute_syntax::ast::Entry],
    beats: &[BundleBeat],
) -> Vec<Diagnostic> {
    let mut decls: Vec<(bool, &str, Span)> = beats
        .iter()
        .map(|b| (true, b.id.as_str(), b.id_span))
        .chain(
            entries
                .iter()
                .filter(|_| doc_id.is_some() && !beats.is_empty())
                .map(|e| (false, e.id.as_str(), e.id_span)),
        )
        .filter(|(_, id, _)| is_name(id))
        .collect();
    decls.sort_by_key(|(_, _, span)| span.byte_start);
    let mut first: BTreeMap<&str, (bool, Span)> = BTreeMap::new();
    let mut out = Vec::new();
    for (is_beat, id, span) in decls {
        let Some(&(first_beat, first_span)) = first.get(id) else {
            first.insert(id, (is_beat, span));
            continue;
        };
        // Two entries sharing an id are `E-ENTRY-ID-DUP`'s.
        if !is_beat && !first_beat {
            continue;
        }
        let tag = |beat: bool| if beat { "beat" } else { "entry" };
        let at = if first_span.line > 0 {
            format!(" (line {})", first_span.line)
        } else {
            String::new()
        };
        let message = match doc_id {
            Some(doc) if is_beat != first_beat => format!(
                "`<{}>` id `{id}` is already declared by `<{} id=\"{id}\">`{at}: the beat's \
                 canonical id `{doc}.{id}` is also the entry's alias, so `{doc}.{id}` (in \
                 `visited()` or a play's `expect: {{ winner: {doc}.{id} }}`) would name both — \
                 rename one",
                tag(is_beat),
                tag(first_beat),
            ),
            _ => format!(
                "`<beat id=\"{id}\">` is already declared in this document{at}; a bundle \
                 beat's id names one beat — rename one"
            ),
        };
        out.push(Diagnostic {
            code: E_BEAT_ID_DUP.to_string(),
            severity: Severity::Error,
            message,
            span,
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }
    out
}

/// One beat's attribute shape ([`E_BEAT_ATTR`]) and closure (`E-UNKNOWN-ATTR`).
fn check_shape(beat: &BundleBeat, diags: &mut Vec<Diagnostic>) {
    let mut residual: BTreeSet<&str> = BTreeSet::new();
    for attr in &beat.attrs {
        let key = attr.key.as_str();
        if !BUNDLE_BEAT_ATTRS.contains(&key) || key == "when" || key == "spentBy" {
            continue;
        }
        residual.insert(key);
        if key == "also" || matches!(attr.value, AttrValue::Str(_)) {
            // `also`: `check_flags` below; a repeated string key: the parser
            // took the first occurrence.
            continue;
        }
        let values = if key == "once" {
            format!(": `once=\"…\"` takes {}", crate::beats::ONCE_VALUES)
        } else {
            String::new()
        };
        diags.push(beat_attr(
            format!("`<beat>` attribute `{key}` must be a quoted string{values}"),
            attr.span,
        ));
    }
    crate::logic_attrs::check_flags(&beat.attrs, "beat", &["also"], diags);
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
    } else if let Some(fault) = lute_manifest::ident::name_fault("`<beat>` id", id) {
        diags.push(beat_attr(fault, beat.id_span));
    }
    if beat.on.is_none()
        && !residual.contains("on")
        && !beat.template.as_ref().is_some_and(|t| t.failed)
        && !crate::logic_attrs::names_occasion_misspelt(&beat.attrs, "on")
    {
        diags.push(beat_attr(
            format!(
                "`<beat id=\"{id}\">` names no occasion; a bundle beat answers one — add \
                 `on=\"<occasion>\"` (dsl 0.23.0 §4)"
            ),
            beat.id_span,
        ));
    }
    for (key, value) in [
        ("on", &beat.on),
        ("target", &beat.target),
        ("priority", &beat.priority),
        ("once", &beat.once),
        ("advances", &beat.advances),
    ] {
        if let Some((raw, span)) = value {
            diags.extend(value_faults(key, raw, *span));
        }
    }
    // A `spentBy` beat stays spent for its `once` period once the condition
    // has held; `once="false"` is no period.
    if let (Some(spent_by), Some((_, span))) = (
        &beat.spent_by,
        beat.once.as_ref().filter(|(o, _)| o == "false"),
    ) {
        diags.push(beat_attr(
            crate::beats::spent_by_once_false(spent_by.raw.trim()),
            *span,
        ));
    }
    // dsl 0.25.0 §2: a shared spend needs a spend to share.
    if let Some((key, span)) = &beat.share {
        let once = beat.once.as_ref().map(|(o, _)| o.as_str());
        if !is_name(key) {
            diags.extend(value_faults("share", key, *span));
        } else if beat.spent_by.is_some() {
            diags.push(beat_attr(crate::beats::share_with_spent_by(key), *span));
        } else if once == Some("false") || (once.is_none() && !residual.contains("once")) {
            diags.push(beat_attr(crate::beats::share_without_once(key), *span));
        }
    }
    if let Some((after, span)) = &beat.after {
        diags.extend(value_faults("after", after, *span));
    }
    if let Some((_, span)) = &beat.advances {
        if beat.on.is_none() && !residual.contains("on") {
            diags.push(beat_attr(
                "`<beat>` `advances` requires `on`; it moves the clock when a beat is presented \
                 (dsl 0.31.0 §1)"
                    .to_string(),
                *span,
            ));
        }
    }
}

/// The shape of one `<beat>` header value on its own ([`E_BEAT_ATTR`],
/// `E-CONN-PROFILE` for `after`), whatever the other keys say — the rules
/// [`check_shape`] and a beat template's header (`crate::templates`, dsl
/// 0.27.0 §6) share. Keys without a shape of their own yield nothing.
pub(crate) fn value_faults(key: &str, raw: &str, span: Span) -> Vec<Diagnostic> {
    if key == "advances" {
        let mut out = Vec::new();
        crate::beats::advances_from_attr(Some(&(raw.to_string(), span)), &mut out);
        return out;
    }
    let message = match key {
        "on" if !is_name(raw) => crate::beats::occasion_malformed("`<beat>`", raw),
        "target" if !is_beat_target(raw) => crate::beats::malformed_target("`<beat>`", raw, true),
        "priority" if parse_beat_priority(raw).is_none() => {
            format!("`<beat>` `priority=\"{raw}\"` must be an integer (dsl 0.23.0 §4)")
        }
        "once" if raw != "false" && BeatOnce::parse(raw).is_none() => format!(
            "`<beat>` `once=\"{raw}\"` must be {} (dsl 0.23.0 §4, 0.24.0 §1, 0.27.0 §5)",
            crate::beats::ONCE_VALUES
        ),
        "share" if !is_name(raw) => crate::beats::share_malformed("`<beat>`", raw),
        // dsl 0.25.0 §3: `after=` under the scene `after:` grammar
        // (`E-CONN-PROFILE`); an exact empty value declares no prerequisite.
        "after" if !raw.is_empty() => return crate::prereq::parse_prereq(raw, span).1,
        _ => return Vec::new(),
    };
    vec![beat_attr(message, span)]
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
