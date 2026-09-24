//! dsl 0.21.0 beats and occasions (§2, §3, §5): the static semantics of a
//! scene that answers an occasion through its frontmatter (`on` / `target` /
//! `when` / `priority` / `once`) and of a lore entry that answers one through
//! `<entry on= priority=>`.
//!
//! - Shape — [`E_BEAT_ATTR`]: a non-identifier `on`, a malformed `target`, a
//!   non-integer `priority`, a `once` outside `run` / `user` / `false`, a beat
//!   key without `on`, a `target` on an occasion declared without
//!   `target: true`, and a scene `when` reading the scene's own `scene.*`
//!   state (it does not exist yet when the beat is chosen).
//! - Vocabulary — [`E_OCCASION_UNKNOWN`]: `on` names an occasion the resolved
//!   capability snapshot does not declare. Shape-only (any identifier) while
//!   no plugin declares occasions, the `rewardKinds` precedent (0.16.0 §4).
//! - Reachability — [`E_BEAT_UNREACHABLE`]: a scene beat whose `when`
//!   provably never holds (per file in `crate::reachability`, under the fact
//!   envelope in `crate::fact_check`). An entry beat keeps
//!   `E-ENTRY-UNREACHABLE`: its `when` is the entry's own eligibility guard.
//! - Selection — [`W_BEAT_SHADOWED`] (`check-project`,
//!   [`check_project_beats`]): a `select: first` beat an earlier-ordered,
//!   always-eligible, never-spent beat wins against every time.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_syntax::ast::{AttrValue, CelKind, CelSlot, Document, Entry, Meta, Node, Quest};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{decide_slot, DecideCtx, Decided};
use crate::lore::{is_entry_ident, is_entry_target};

/// A beat attribute's shape (dsl 0.21.0 §5), anchored at the offending key
/// or value.
pub const E_BEAT_ATTR: &str = "E-BEAT-ATTR";
/// `on` names an occasion no resolved plugin declares (dsl 0.21.0 §2, §5) —
/// only when some plugin declares occasions.
pub const E_OCCASION_UNKNOWN: &str = "E-OCCASION-UNKNOWN";
/// A scene beat whose `when` provably never holds (dsl 0.21.0 §5).
pub const E_BEAT_UNREACHABLE: &str = "E-BEAT-UNREACHABLE";
/// A `select: first` beat that can never win (dsl 0.21.0 §5,
/// `check-project` only).
pub const W_BEAT_SHADOWED: &str = "W-BEAT-SHADOWED";

/// The scene-frontmatter beat keys (dsl 0.21.0 §3.1). Scene-only, never
/// defaultable: a beat is one scene's own declaration.
pub const BEAT_KEYS: &[&str] = &["on", "target", "when", "priority", "once"];

/// A scene beat's repetition policy (dsl 0.21.0 §3.1, D-F).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatOnce {
    /// `once: run` (the default) — presented at most once per run.
    Run,
    /// `once: user` — presented at most once ever.
    User,
    /// `once: false` — repeatable; never spent.
    None,
}

impl BeatOnce {
    /// The IR spelling (dsl 0.21.0 §8): `"run"`, `"user"`, or `"none"`.
    pub fn as_str(self) -> &'static str {
        match self {
            BeatOnce::Run => "run",
            BeatOnce::User => "user",
            BeatOnce::None => "none",
        }
    }
}

/// A scene's validated beat declaration (dsl 0.21.0 §3.1), lifted onto
/// [`crate::meta::TypedMeta::beat`] only when `on:` is present and an
/// identifier. A malformed sibling key draws [`E_BEAT_ATTR`] and falls back
/// to its default here (`target`/`when` absent, `priority` 0, `once: run`).
#[derive(Clone, Debug)]
pub struct BeatMeta {
    /// The occasion the scene answers.
    pub on: String,
    /// The target the beat is restricted to (`<entry target>` shape).
    pub target: Option<String>,
    /// The eligibility condition — a `Condition` CEL slot whose span is the
    /// frontmatter value. `ast` is unfilled: frontmatter is not part of the
    /// parsed node tree, so `check()` parses it itself.
    pub when: Option<CelSlot>,
    /// Higher wins; default `0`.
    pub priority: i64,
    /// Default [`BeatOnce::Run`].
    pub once: BeatOnce,
}

/// A beat `priority` (dsl 0.21.0 §3): an integer `-?[0-9]+` that fits `i64`.
/// `None` for anything else (`E-BEAT-ATTR`). Reads an entry's raw
/// `priority=` text; a scene's `priority:` is a YAML integer.
pub fn parse_beat_priority(raw: &str) -> Option<i64> {
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

/// Lift and validate a SCENE document's beat keys (dsl 0.21.0 §3.1, §5) from
/// its (defaults-merged) frontmatter mapping. Called by
/// [`crate::meta::parse_meta_kind_with_defaults`] for `MetaKind::Scene` only;
/// on every other kind the beat keys are `E-META-UNKNOWN-KEY`.
pub(crate) fn lift_scene_beat(
    meta: &Meta,
    map: &serde_yaml::Mapping,
    occasions: &BTreeMap<String, OccasionDecl>,
    diags: &mut Vec<Diagnostic>,
) -> Option<BeatMeta> {
    let get = |key: &str| map.get(serde_yaml::Value::String(key.to_string()));
    let mut push = |message: String, span: Span| {
        diags.push(beat_diag(E_BEAT_ATTR, Severity::Error, message, span, Layer::Content));
    };

    let on = match get("on") {
        None => {
            for key in &BEAT_KEYS[1..] {
                if get(key).is_some() {
                    push(
                        format!(
                            "`{key}:` without `on:`; `{key}` belongs to a beat, and a scene \
                             becomes a beat by naming the occasion it answers — add \
                             `on: <occasion>`, or remove `{key}:` for a scene reached by \
                             explicit flow (dsl 0.21.0 §3.1)"
                        ),
                        top_key_span(meta, key),
                    );
                }
            }
            return None;
        }
        Some(v) => match v.as_str().filter(|s| is_entry_ident(s)) {
            Some(on) => Some(on.to_string()),
            None => {
                push(
                    format!(
                        "`on:` must name an occasion — an identifier \
                         (`[A-Za-z][A-Za-z0-9_-]*`), got {} (dsl 0.21.0 §3.1)",
                        describe(v)
                    ),
                    top_value_span(meta, "on"),
                );
                None
            }
        },
    };

    let target = get("target").and_then(|v| match v.as_str().filter(|s| is_entry_target(s)) {
        Some(t) => Some(t.to_string()),
        None => {
            push(
                format!(
                    "`target:` must be a dotted id `Ident (\".\" Segment)*` with \
                     `Segment ::= [A-Za-z0-9_-]+`, e.g. `npc.achilles`, got {} (dsl 0.21.0 §3.1)",
                    describe(v)
                ),
                top_value_span(meta, "target"),
            );
            None
        }
    });

    let when = get("when").and_then(|v| match v.as_str() {
        Some(raw) if !raw.trim().is_empty() => Some(CelSlot::raw(
            CelKind::Condition,
            raw.to_string(),
            top_value_span(meta, "when"),
        )),
        Some(_) => {
            push(
                "`when:` is empty; omit it for a beat that is always eligible \
                 (dsl 0.21.0 §3.1)"
                    .to_string(),
                top_key_span(meta, "when"),
            );
            None
        }
        None => {
            push(
                format!(
                    "`when:` must be a CEL condition string, e.g. `when: 'user.runs >= 10'`, \
                     got {} (dsl 0.21.0 §3.1)",
                    describe(v)
                ),
                top_value_span(meta, "when"),
            );
            None
        }
    });

    let priority = match get("priority") {
        None => 0,
        Some(v) => v.as_i64().unwrap_or_else(|| {
            push(
                format!(
                    "`priority:` must be an integer (higher wins, default `0`), got {} \
                     (dsl 0.21.0 §3.1)",
                    describe(v)
                ),
                top_value_span(meta, "priority"),
            );
            0
        }),
    };

    let once = match get("once") {
        None => BeatOnce::Run,
        Some(serde_yaml::Value::Bool(false)) => BeatOnce::None,
        Some(v) => match v.as_str() {
            Some("run") => BeatOnce::Run,
            Some("user") => BeatOnce::User,
            _ => {
                push(
                    format!(
                        "`once:` must be `run` (once per run, the default), `user` (once ever), \
                         or `false` (repeatable), got {} (dsl 0.21.0 §3.1)",
                        describe(v)
                    ),
                    top_value_span(meta, "once"),
                );
                BeatOnce::Run
            }
        },
    };

    let on = on?;
    check_occasion(
        &on,
        top_value_span(meta, "on"),
        target.as_ref().map(|_| top_value_span(meta, "target")),
        occasions,
        Layer::Content,
        diags,
    );
    Some(BeatMeta {
        on,
        target,
        when,
        priority,
        once,
    })
}

/// An `<entry>`'s beat attributes' shape (dsl 0.21.0 §3.2, §5): `on` an
/// identifier, `priority` an integer, `priority` only beside `on`, and both
/// quoted strings. Called from `crate::lore`'s per-entry shape check.
pub(crate) fn check_entry_beat_attrs(entry: &Entry, diags: &mut Vec<Diagnostic>) {
    let mut push = |message: String, span: Span| {
        diags.push(beat_diag(E_BEAT_ATTR, Severity::Error, message, span, Layer::Logic));
    };
    // A non-string value leaves the key residual (the parser extracts only
    // quoted strings); the shape fault is the beat's own.
    let mut residual_on = false;
    for attr in &entry.attrs {
        if (attr.key == "on" || attr.key == "priority") && !matches!(attr.value, AttrValue::Str(_)) {
            residual_on |= attr.key == "on";
            push(
                format!(
                    "`<entry>` attribute `{}` must be a quoted string (dsl 0.21.0 §3.2)",
                    attr.key
                ),
                attr.span,
            );
        }
    }
    if let Some((on, span)) = &entry.on {
        if !is_entry_ident(on) {
            push(
                format!(
                    "`<entry>` `on=\"{on}\"` must name an occasion — an identifier \
                     (`[A-Za-z][A-Za-z0-9_-]*`) (dsl 0.21.0 §3.2)"
                ),
                *span,
            );
        }
    }
    if let Some((raw, span)) = &entry.priority {
        if parse_beat_priority(raw).is_none() {
            push(
                format!("`<entry>` `priority=\"{raw}\"` must be an integer (dsl 0.21.0 §3.2)"),
                *span,
            );
        }
        if entry.on.is_none() && !residual_on {
            push(
                "`<entry>` `priority` requires `on`; a priority orders the beats answering one \
                 occasion (dsl 0.21.0 §3.2)"
                    .to_string(),
                *span,
            );
        }
    }
}

/// The occasion vocabulary of every entry beat of one document (dsl 0.21.0
/// §2): [`E_OCCASION_UNKNOWN`] and the untargeted-occasion `target`
/// [`E_BEAT_ATTR`], exactly as a scene beat's. Shape-only while
/// `occasions` is empty.
pub(crate) fn check_entry_occasions(
    entries: &[Entry],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for entry in entries {
        let Some((on, on_span)) = entry.on.as_ref().filter(|(on, _)| is_entry_ident(on)) else {
            continue;
        };
        let target_span = entry
            .target
            .as_ref()
            .filter(|(t, _)| is_entry_target(t))
            .map(|(_, span)| *span);
        check_occasion(on, *on_span, target_span, occasions, Layer::Logic, &mut diags);
    }
    diags
}

/// dsl 0.21.0 §7a.2 (D-I): every `<objective on="<occasion>">` of `quests`
/// — the occasion at which the objective's `done` is judged — checked
/// exactly as a beat's `on`: [`E_BEAT_ATTR`] when the value is not a quoted
/// identifier, [`E_OCCASION_UNKNOWN`] against the resolved vocabulary
/// (shape-only while `occasions` is empty). Objectives have no `target`.
pub(crate) fn check_objective_occasions(
    quests: &[Quest],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for node in quests.iter().flat_map(|q| &q.body) {
        let Node::Objective(o) = node else { continue };
        for attr in o.attrs.iter().filter(|a| a.key == "on") {
            // A non-string value stays residual (the parser extracts only
            // quoted strings).
            diags.push(beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                "`<objective>` attribute `on` must be a quoted occasion name (dsl 0.21.0 §7a.2)"
                    .to_string(),
                attr.span,
                Layer::Logic,
            ));
        }
        let Some((on, span)) = &o.on else { continue };
        if !is_entry_ident(on) {
            diags.push(beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                format!(
                    "`<objective>` `on=\"{on}\"` must name an occasion — an identifier \
                     (`[A-Za-z][A-Za-z0-9_-]*`) (dsl 0.21.0 §7a.2)"
                ),
                *span,
                Layer::Logic,
            ));
            continue;
        }
        check_occasion(on, *span, None, occasions, Layer::Logic, &mut diags);
    }
    diags
}

/// `on` against the resolved occasion vocabulary (dsl 0.21.0 §2): unknown is
/// [`E_OCCASION_UNKNOWN`]; a `target` on an occasion declared without
/// `target: true` is [`E_BEAT_ATTR`]. Silent when no occasion is declared.
fn check_occasion(
    on: &str,
    on_span: Span,
    target_span: Option<Span>,
    occasions: &BTreeMap<String, OccasionDecl>,
    layer: Layer,
    diags: &mut Vec<Diagnostic>,
) {
    if occasions.is_empty() {
        return;
    }
    let Some(decl) = occasions.get(on) else {
        let hint = lute_manifest::suggest::nearest(on, occasions.keys().map(String::as_str), 2)
            .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
        diags.push(beat_diag(
            E_OCCASION_UNKNOWN,
            Severity::Error,
            format!(
                "occasion `{on}` is not declared by any resolved plugin (declared: {}){hint} \
                 (dsl 0.21.0 §2)",
                occasions
                    .keys()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            on_span,
            layer,
        ));
        return;
    };
    if let (false, Some(span)) = (decl.target, target_span) {
        diags.push(beat_diag(
            E_BEAT_ATTR,
            Severity::Error,
            format!(
                "occasion `{on}` is not raised for a target (declared without `target: true`), \
                 so a beat on it cannot restrict itself to one; remove `target` (dsl 0.21.0 §2)"
            ),
            span,
            layer,
        ));
    }
}

/// A scene `when` reads the scene's own `scene.*` state (dsl 0.21.0 §3.1):
/// the beat is chosen before the scene runs, so that state does not exist
/// yet. One [`E_BEAT_ATTR`] per distinct path, at the slot.
pub(crate) fn scene_when_scene_reads(paths: &[String], slot: &CelSlot) -> Vec<Diagnostic> {
    paths
        .iter()
        .map(|path| {
            beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                format!(
                    "`when:` reads `{path}`, but a beat's `when` is evaluated before the scene \
                     runs, when its own `scene.*` state does not exist yet — gate on `run.*` / \
                     `user.*` / `app.*` state, `quest.*`, `entry.<id>.read`, or a fact query \
                     (dsl 0.21.0 §3.1)"
                ),
                slot.span,
                Layer::Cel,
            )
        })
        .collect()
}

/// The per-file [`E_BEAT_UNREACHABLE`] message (the fact-envelope pass
/// appends its reasons).
pub(crate) fn beat_unreachable_message(scene: &str, when: &str, reasons: Option<&str>) -> String {
    match reasons {
        Some(reasons) => format!(
            "beat `{scene}` is never eligible: its `when` `{when}` is provably false — {reasons} \
             (dsl 0.21.0 §5)"
        ),
        None => format!(
            "beat `{scene}` is never eligible: its `when` `{when}` is provably false \
             (dsl 0.21.0 §5)"
        ),
    }
}

/// The name a beat scene goes by in messages: its canonical scene key, or
/// `this scene` when it has none (an `E-META-MISSING` document).
pub(crate) fn scene_beat_name(folded: &FoldedEnv) -> String {
    crate::meta::canonical_scene_key(&folded.typed).unwrap_or_else(|| "this scene".to_string())
}

/// One beat of the project, in selection-tiebreak order.
struct Beat<'a> {
    path: &'a PathBuf,
    /// `scene `k`` / `entry `id`` — the beat as messages name it.
    name: String,
    on: &'a str,
    target: Option<&'a str>,
    priority: i64,
    /// Always eligible: no `after:`, and a `when` absent or deciding true.
    always: bool,
    /// Never spent: `once: false`, or an entry (entries have no `once`).
    unspent: bool,
    /// Where a shadowing warning anchors: the `on` key / attribute.
    anchor: Span,
    folded: &'a FoldedEnv,
}

/// `W-BEAT-SHADOWED` over one resolved project root (dsl 0.21.0 §4, §5).
/// `docs` and `foldeds` are parallel and in `check-project` order, which is
/// the `ProjectIndex.beats` tiebreak order (documents in order, declaration
/// order within).
///
/// Beats are ordered by priority descending, then that order. A beat `B` on
/// a `select: first` occasion (an undeclared occasion counts as `first`) is
/// shadowed by the first earlier beat `A` on the same occasion whose target
/// is absent or equal to `B`'s — so `A` is a candidate whenever `B` is —
/// that is always eligible (no `after:`, `when` absent or deciding true
/// without facts) and never spent (`once: false`, or an entry). `A` then wins
/// every time `B` could. Conservative: an undecided `when` never shadows.
pub fn check_project_beats(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let params = BTreeMap::new();
    let mut beats: Vec<Beat<'_>> = Vec::new();
    for ((path, doc), folded) in docs.iter().zip(foldeds) {
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let ctx = DecideCtx {
            schema: &folded.env.state,
            dollar: None,
            params: &params,
            facts: None,
        };
        let holds = |when: Option<&CelSlot>| {
            when.is_none_or(|w| {
                matches!(decide_slot(&w.raw, &defs, &ctx), Some(Decided::Bool(true)))
            })
        };
        if let Some(beat) = &folded.typed.beat {
            let has_after = folded.typed.after.as_deref().is_some_and(|a| !a.trim().is_empty());
            beats.push(Beat {
                path,
                name: format!("scene `{}`", scene_beat_name(folded)),
                on: &beat.on,
                target: beat.target.as_deref(),
                priority: beat.priority,
                always: !has_after && holds(beat.when.as_ref()),
                unspent: beat.once == BeatOnce::None,
                anchor: top_key_span(&doc.meta, "on"),
                folded,
            });
        }
        for entry in &doc.entries {
            let Some((on, on_span)) = entry.on.as_ref().filter(|(on, _)| is_entry_ident(on))
            else {
                continue;
            };
            let priority = match &entry.priority {
                None => 0,
                Some((raw, _)) => match parse_beat_priority(raw) {
                    Some(p) => p,
                    None => continue,
                },
            };
            let target = match &entry.target {
                None => None,
                Some((t, _)) if is_entry_target(t) => Some(t.as_str()),
                Some(_) => continue,
            };
            beats.push(Beat {
                path,
                name: format!("entry `{}`", entry.id),
                on,
                target,
                priority,
                always: holds(entry.when.as_ref()),
                unspent: true,
                anchor: *on_span,
                folded,
            });
        }
    }
    // Stable: equal priorities keep the tiebreak order.
    beats.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut out = Vec::new();
    for (j, b) in beats.iter().enumerate() {
        let select = b
            .folded
            .occasions
            .get(b.on)
            .map_or(OccasionSelect::First, |o| o.select);
        if select != OccasionSelect::First {
            continue;
        }
        let Some(a) = beats[..j].iter().find(|a| {
            a.on == b.on
                && (a.target.is_none() || a.target == b.target)
                && a.always
                && a.unspent
        }) else {
            continue;
        };
        let target = b.target.map_or_else(String::new, |t| format!(" for `{t}`"));
        let spent = if a.name.starts_with("entry") {
            "an entry has no `once`"
        } else {
            "`once: false`"
        };
        out.push((
            b.path.clone(),
            beat_diag(
                W_BEAT_SHADOWED,
                Severity::Warning,
                format!(
                    "{} can never win occasion `{}`{target}: {} (priority {}) is ordered before \
                     it, is always eligible (no `after:`, and its `when` is absent or always \
                     true), and is never spent ({spent}), so it wins every time (dsl 0.21.0 §5)",
                    b.name, b.on, a.name, a.priority
                ),
                b.anchor,
                Layer::Logic,
            ),
        ));
    }
    out
}

/// The YAML value an author wrote, for the "got …" half of a message.
fn describe(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => format!("`{s}`"),
        serde_yaml::Value::Bool(b) => format!("`{b}`"),
        serde_yaml::Value::Number(n) => format!("`{n}`"),
        serde_yaml::Value::Null => "an empty value".to_string(),
        serde_yaml::Value::Sequence(_) => "a list".to_string(),
        serde_yaml::Value::Mapping(_) => "a mapping".to_string(),
        serde_yaml::Value::Tagged(_) => "a tagged value".to_string(),
    }
}

/// Where the frontmatter interior starts in the document (see
/// [`crate::meta::meta_key_span`] for the envelope rule).
fn interior_base(meta: &Meta) -> usize {
    const OPENER_LEN: usize = 4; // "---\n"
    let enveloped =
        meta.span.byte_end.saturating_sub(meta.span.byte_start) != meta.raw_yaml.len();
    meta.span.byte_start + if enveloped { OPENER_LEN } else { 0 }
}

fn bare_span(byte_start: usize, byte_end: usize) -> Span {
    Span {
        byte_start,
        byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// The TOP-LEVEL (unindented) `key:` line of the frontmatter: `(line start
/// offset in raw_yaml, the text after the colon)`. A nested key of the same
/// name (`extra: { on: … }`) never matches.
fn top_key_line<'m>(meta: &'m Meta, key: &str) -> Option<(usize, &'m str)> {
    let mut line_start = 0usize;
    for line in meta.raw_yaml.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(after) = rest.trim_start_matches([' ', '\t']).strip_prefix(':') {
                return Some((line_start, after));
            }
        }
        line_start += line.len();
    }
    None
}

/// The span of a top-level frontmatter `key` (the key text itself), falling
/// back to [`crate::meta::meta_key_span`]. Line/column stay zeroed — the
/// diagnostic normalizers recompute them from bytes.
pub(crate) fn top_key_span(meta: &Meta, key: &str) -> Span {
    match top_key_line(meta, key) {
        Some((at, _)) => {
            let start = interior_base(meta) + at;
            bare_span(start, start + key.len())
        }
        None => crate::meta::meta_key_span(meta, key),
    }
}

/// The span of a top-level frontmatter key's inline VALUE — its text on the
/// key's own line, minus surrounding quotes and a trailing comment — so a CEL
/// slot's raw offsets map onto the source. Falls back to the key span for a
/// block value (nothing inline).
pub(crate) fn top_value_span(meta: &Meta, key: &str) -> Span {
    let Some((at, after)) = top_key_line(meta, key) else {
        return crate::meta::meta_key_span(meta, key);
    };
    let text = after.trim_end_matches(['\n', '\r']);
    let lead = text.len() - text.trim_start().len();
    let value = text.trim_start();
    let (start, len) = match value.chars().next() {
        Some(q @ ('\'' | '"')) => match value[1..].rfind(q) {
            Some(close) => (lead + 1, close),
            None => (lead, value.trim_end().len()),
        },
        _ => {
            let plain = value.find(" #").map_or(value, |c| &value[..c]).trim_end();
            (lead, plain.len())
        }
    };
    if len == 0 {
        return top_key_span(meta, key);
    }
    // `at` + the key + the colon (and any space before it) + the offset.
    let colon = meta.raw_yaml[at..].find(':').unwrap_or(key.len());
    let begin = interior_base(meta) + at + colon + 1 + start;
    bare_span(begin, begin + len)
}

fn beat_diag(code: &str, severity: Severity, message: String, span: Span, layer: Layer) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
