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
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use lute_syntax::ast::{AttrValue, CelKind, CelSlot, Document, Entry, Meta, Node, Quest};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{decide_slot, DecideCtx, Decided};
use crate::fact_env::{FactEnv, FactScope};
use crate::lore::{is_beat_target, is_entry_ident, is_entry_target, kind_target};

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
pub const BEAT_KEYS: &[&str] = &["on", "target", "when", "priority", "once", "also", "share"];

/// A scene beat's repetition policy (dsl 0.21.0 §3.1, D-F).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeatOnce {
    /// `once: run` (the default) — presented at most once per run.
    Run,
    /// `once: user` — presented at most once ever.
    User,
    /// `once: false` — repeatable; never spent.
    None,
    /// dsl 0.24.0 §1: `once: day` — spent until the clock's day changes.
    Day,
    /// dsl 0.24.0 §1: `once: slot` — spent until the clock's slot (or day)
    /// changes.
    Slot,
}

impl BeatOnce {
    /// The IR spelling (dsl 0.21.0 §8, 0.24.0 §1): `"run"`, `"user"`,
    /// `"none"`, `"day"`, or `"slot"`.
    pub fn as_str(self) -> &'static str {
        match self {
            BeatOnce::Run => "run",
            BeatOnce::User => "user",
            BeatOnce::None => "none",
            BeatOnce::Day => "day",
            BeatOnce::Slot => "slot",
        }
    }

    /// Spent per clock period (`day` / `slot`) — needs a declared clock.
    pub fn is_clock(self) -> bool {
        matches!(self, BeatOnce::Day | BeatOnce::Slot)
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
    /// `once:` is written (frontmatter or project `defaults:`), not defaulted
    /// — dsl 0.23.1: an authored `once: run` acknowledges a per-run beat.
    pub once_authored: bool,
    /// dsl 0.23.0 §3: `also: true` — on a `select: first` occasion, presented
    /// in addition to the winner, after it; never the winner itself.
    pub also: bool,
    /// dsl 0.25.0 §2: the shared-spend key — every beat of the key is spent
    /// when one is presented. Only with an authored, spending `once`.
    pub share: Option<String>,
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
        diags.push(beat_diag(
            E_BEAT_ATTR,
            Severity::Error,
            message,
            span,
            Layer::Content,
        ));
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

    let target = get("target").and_then(|v| match v.as_str().filter(|s| is_beat_target(s)) {
        Some(t) => Some(t.to_string()),
        None => {
            push(
                format!(
                    "`target:` must be a dotted id `Ident (\".\" Segment)*` with \
                     `Segment ::= [A-Za-z0-9_-]+`, e.g. `npc.achilles`, or `kind:<entity kind>`, \
                     got {} (dsl 0.21.0 §3.1, 0.26.0 §5)",
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
            Some("day") => BeatOnce::Day,
            Some("slot") => BeatOnce::Slot,
            _ => {
                push(
                    format!(
                        "`once:` must be `run` (once per run, the default), `user` (once ever), \
                         `day` / `slot` (once per clock day / slot), or `false` (repeatable), \
                         got {} (dsl 0.21.0 §3.1, 0.24.0 §1)",
                        describe(v)
                    ),
                    top_value_span(meta, "once"),
                );
                BeatOnce::Run
            }
        },
    };

    let also = match get("also") {
        None | Some(serde_yaml::Value::Bool(false)) => false,
        Some(serde_yaml::Value::Bool(true)) => true,
        Some(v) => {
            push(
                format!(
                    "`also:` must be `true` (presented after the occasion's winner, in addition \
                     to it) or `false`, got {} (dsl 0.23.0 §3)",
                    describe(v)
                ),
                top_value_span(meta, "also"),
            );
            false
        }
    };

    let share = get("share").and_then(|v| {
        let Some(key) = v.as_str().filter(|s| is_entry_ident(s)) else {
            push(
                format!(
                    "`share:` must be a key — an identifier (`[A-Za-z][A-Za-z0-9_-]*`) every \
                     beat standing for the same event writes, got {} (dsl 0.25.0 §2)",
                    describe(v)
                ),
                top_value_span(meta, "share"),
            );
            return None;
        };
        if get("once").is_none() || once == BeatOnce::None {
            push(share_without_once(key), top_key_span(meta, "share"));
        }
        Some(key.to_string())
    });

    let on = on?;
    check_occasion(
        &on,
        top_value_span(meta, "on"),
        target.as_ref().map(|_| top_value_span(meta, "target")),
        occasions,
        Layer::Content,
        diags,
    );
    // dsl 0.23.0 §3: a side remark rides along a single winner — on a
    // `select: all` / `sequence` occasion every eligible beat is already
    // offered or presented, so `also` means nothing there.
    if also {
        if let Some(decl) = occasions
            .get(&on)
            .filter(|d| d.select != OccasionSelect::First)
        {
            diags.push(beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                format!(
                    "`also: true` applies only to a `select: first` occasion; `{on}` is \
                     `select: {}`, which already presents or offers every eligible beat — \
                     remove `also:` (dsl 0.23.0 §3)",
                    decl.select.as_str()
                ),
                top_value_span(meta, "also"),
                Layer::Content,
            ));
        }
    }
    Some(BeatMeta {
        on,
        target,
        when,
        priority,
        once,
        once_authored: get("once").is_some(),
        also,
        share,
    })
}

/// An `<entry>`'s beat attributes' shape (dsl 0.21.0 §3.2, dsl 0.22.0 §7,
/// §5): `on` an identifier, `priority` an integer, `once` `run` / `user`,
/// `priority` / `once` only beside `on`, and all quoted strings. Called from
/// `crate::lore`'s per-entry shape check.
pub(crate) fn check_entry_beat_attrs(entry: &Entry, diags: &mut Vec<Diagnostic>) {
    let mut push = |message: String, span: Span| {
        diags.push(beat_diag(
            E_BEAT_ATTR,
            Severity::Error,
            message,
            span,
            Layer::Logic,
        ));
    };
    // A non-string value leaves the key residual (the parser extracts only
    // quoted strings); the shape fault is the beat's own.
    let mut residual_on = false;
    let mut residual_once = false;
    for attr in &entry.attrs {
        if matches!(attr.key.as_str(), "on" | "priority" | "once" | "share")
            && !matches!(attr.value, AttrValue::Str(_))
        {
            residual_on |= attr.key == "on";
            residual_once |= attr.key == "once";
            push(
                format!(
                    "`<entry>` attribute `{}` must be a quoted string (dsl 0.21.0 §3.2)",
                    attr.key
                ),
                attr.span,
            );
        }
    }
    // dsl 0.23.0 §3: `also` rides along a `select: first` winner as a scene
    // (or bundle) beat; an entry beat is read, not presented beside another.
    for attr in entry.attrs.iter().filter(|a| a.key == "also") {
        push(
            "`also` is a scene beat key (`also: true` in a scene's frontmatter, dsl 0.23.0 §3); \
             an `<entry>` beat cannot ride along another beat — remove `also`"
                .to_string(),
            attr.span,
        );
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
    if let Some((raw, span)) = &entry.once {
        if !matches!(raw.as_str(), "run" | "user" | "day" | "slot") {
            push(
                format!(
                    "`<entry>` `once=\"{raw}\"` must be `run` (not eligible again this run once \
                     read), `user` (never again once read), or `day` / `slot` (not again this \
                     clock day / slot once read); omit it for a repeatable entry beat \
                     (dsl 0.22.0 §7, 0.24.0 §1)"
                ),
                *span,
            );
        }
        if entry.on.is_none() && !residual_on {
            push(
                "`<entry>` `once` requires `on`; a repetition policy belongs to a beat \
                 (dsl 0.22.0 §7)"
                    .to_string(),
                *span,
            );
        }
    }
    if let Some((key, span)) = &entry.share {
        if !is_entry_ident(key) {
            push(share_malformed("`<entry>`", key), *span);
        } else if entry.once.is_none() && !residual_once {
            push(share_without_once(key), *span);
        }
    }
}

/// The occasion vocabulary of every entry beat of one document (dsl 0.21.0
/// §2): [`E_OCCASION_UNKNOWN`], exactly as a scene beat's. Shape-only while
/// `occasions` is empty. dsl 0.24.0 §6: on an occasion declared without a
/// target an entry's `target=` is metadata (what the entry is about, the
/// ordinary lore lookup key), not a candidate restriction — so, unlike a
/// scene or bundle beat's, it is no `E-BEAT-ATTR` there.
pub(crate) fn check_entry_occasions(
    entries: &[Entry],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for entry in entries {
        let Some((on, on_span)) = entry.on.as_ref().filter(|(on, _)| is_entry_ident(on)) else {
            continue;
        };
        check_occasion(on, *on_span, None, occasions, Layer::Logic, &mut diags);
    }
    diags
}

/// dsl 0.24.0 §6: whether a beat's `target` restricts its candidacy on
/// occasion `on` — every occasion but one DECLARED without a target, where an
/// entry's `target=` is metadata. (An undeclared occasion keeps the 0.21
/// shape-only meaning: a target restricts.) The one rule the checker's beat
/// passes and `lute play`'s candidate filter share.
pub fn beat_target_restricts(on: &str, occasions: &BTreeMap<String, OccasionDecl>) -> bool {
    occasions.get(on).is_none_or(|d| d.target.takes_target())
}

/// dsl 0.21.0 §7a.2 (D-I): every `<objective on="<occasion>">` of `quests`
/// — the occasion at which the objective's `done` is judged — checked
/// exactly as a beat's `on`: [`E_BEAT_ATTR`] when the value is not a quoted
/// identifier, [`E_OCCASION_UNKNOWN`] against the resolved vocabulary
/// (shape-only while `occasions` is empty). dsl 0.23.0 §2: its `target=`
/// follows the beat target rule — a dotted id, only beside `on`, only on an
/// occasion raised for a target (the domain is
/// [`check_beat_target_domains`]'s).
pub(crate) fn check_objective_occasions(
    quests: &[Quest],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for node in quests.iter().flat_map(|q| &q.body) {
        let Node::Objective(o) = node else { continue };
        for attr in o
            .attrs
            .iter()
            .filter(|a| matches!(a.key.as_str(), "on" | "target"))
        {
            // A non-string value stays residual (the parser extracts only
            // quoted strings).
            diags.push(beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                format!(
                    "`<objective>` attribute `{}` must be a quoted string (dsl 0.21.0 §7a.2, \
                     0.23.0 §2)",
                    attr.key
                ),
                attr.span,
                Layer::Logic,
            ));
        }
        let target_span = match &o.target {
            Some((t, span)) if !is_entry_target(t) => {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    format!(
                        "`<objective>` `target=\"{t}\"` must be a dotted id `Ident (\".\" \
                         Segment)*`, e.g. `npc.maud` (dsl 0.23.0 §2)"
                    ),
                    *span,
                    Layer::Logic,
                ));
                None
            }
            Some((_, span)) if o.on.is_none() => {
                if !o.attrs.iter().any(|a| a.key == "on") {
                    diags.push(beat_diag(
                        E_BEAT_ATTR,
                        Severity::Error,
                        "`<objective>` `target` requires `on`; the objective is judged when the \
                         occasion is raised for that target (dsl 0.23.0 §2)"
                            .to_string(),
                        *span,
                        Layer::Logic,
                    ));
                }
                None
            }
            Some((_, span)) => Some(*span),
            None => None,
        };
        // dsl 0.24.0 §2.1: `until` is the place-bound deadline — judged only
        // when the objective's occasion is raised, so it needs one.
        if let (Some(until), None) = (&o.until, &o.on) {
            if !o.attrs.iter().any(|a| a.key == "on") {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    "`<objective>` `until` requires `on`; it is judged only when that occasion \
                     is raised — a deadline that holds everywhere is `by=` (dsl 0.24.0 §2.1)"
                        .to_string(),
                    until.span,
                    Layer::Logic,
                ));
            }
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
        check_occasion(on, *span, target_span, occasions, Layer::Logic, &mut diags);
    }
    diags
}

/// `on` against the resolved occasion vocabulary (dsl 0.21.0 §2): unknown is
/// [`E_OCCASION_UNKNOWN`]; a `target` on an occasion declared without
/// `target: true` is [`E_BEAT_ATTR`]. Silent when no occasion is declared.
pub(crate) fn check_occasion(
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
    if let (false, Some(span)) = (decl.target.takes_target(), target_span) {
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

/// Whether `target` lies in `decl`'s target domain (dsl 0.22.0 §8), the one
/// rule `lute check` (beat targets, [`E_BEAT_ATTR`]) and `lute play` (step
/// targets, a usage error) share. `kinds` is the project's `entities:`
/// vocabulary (`Env::rel_vocab.kinds`).
///
/// `Ok` for an occasion without a domain (`target: true` keeps the 0.21
/// shape-only meaning), for a member of the named entity kind under the
/// domain's prefix, and for any `<prefix>.<id>` of an `open:` kind (its
/// members are engine-populated). A domain with a `members:` subset admits
/// exactly `<prefix>.<member>` for a listed member — and every listed member
/// must belong to a closed kind (an `open:` kind's cannot be known). The
/// `Err` is the whole reason, with a did-you-mean over the legal
/// `<prefix>.<member>` targets (or, for a stray listed member, over the
/// kind's members) when one is close.
pub fn occasion_target_ok(
    decl: &OccasionDecl,
    target: &str,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Result<(), String> {
    let OccasionTarget::Domain {
        prefix,
        entity,
        members: subset,
    } = &decl.target
    else {
        return Ok(());
    };
    let on = &decl.name;
    let Some(kind) = kinds.get(entity) else {
        return Err(format!(
            "occasion `{on}` draws its targets from entity kind `{entity}`, which the project \
             does not declare under `entities:` (dsl 0.22.0 §8)"
        ));
    };
    // `open:` members are engine-populated: only the prefix is checked. An
    // invalid kind is `E-ENTITY-KIND-SHAPE`'s, never this rule's.
    let kind_members: Option<&[String]> = match &kind.shape {
        KindShape::Members(ms) => Some(ms),
        KindShape::Open | KindShape::Invalid => None,
    };
    if let (Some(subset), Some(ms)) = (subset, kind_members) {
        if let Some(stray) = subset.iter().find(|m| !ms.contains(m)) {
            let hint = lute_manifest::suggest::nearest(stray, ms.iter().map(String::as_str), 2)
                .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
            return Err(format!(
                "occasion `{on}` lists `{stray}` in its target `members:`, but `{stray}` is not \
                 a member of entity kind `{entity}`{hint} (dsl 0.22.0 §8)"
            ));
        }
    }
    let member = target
        .strip_prefix(prefix.as_str())
        .and_then(|rest| rest.strip_prefix('.'))
        .filter(|m| !m.is_empty());
    let Some(legal) = decl.target.domain_members(kind_members) else {
        return match member {
            Some(_) => Ok(()),
            None => Err(format!(
                "target `{target}` is outside occasion `{on}`'s domain: its targets are \
                 `{prefix}.<{entity}>` (dsl 0.22.0 §8)"
            )),
        };
    };
    if member.is_some_and(|m| legal.iter().any(|x| x == m)) {
        return Ok(());
    }
    let domain: Vec<String> = legal.iter().map(|m| format!("{prefix}.{m}")).collect();
    let hint = lute_manifest::suggest::nearest(target, domain.iter().map(String::as_str), 2)
        .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
    let listed = domain
        .iter()
        .map(|t| format!("`{t}`"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(if subset.is_some() {
        format!(
            "target `{target}` is outside occasion `{on}`'s member list ({listed}), a subset of \
             entity kind `{entity}`{hint} (dsl 0.22.0 §8)"
        )
    } else {
        format!(
            "target `{target}` is outside occasion `{on}`'s domain `{prefix}.<{entity}>` \
             ({listed}){hint} (dsl 0.22.0 §8)"
        )
    })
}

/// dsl 0.26.0 §5: the targets a `target="kind:<kind>"` beat on `decl`
/// answers — the domain's prefix and the kind's members (a sub-kind's
/// members are already its parent's). The one rule `lute check` (beat
/// targets, [`E_BEAT_ATTR`]), the compiler (the beat's `targetKind`) and
/// `lute play` (candidates) share. `Err` when the occasion draws no targets
/// from an entity kind, the kind is unknown (with a did-you-mean) or `open:`,
/// or a member lies outside the occasion's domain.
pub fn kind_target_members(
    decl: &OccasionDecl,
    kind: &str,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Result<(String, Vec<String>), String> {
    let on = &decl.name;
    let OccasionTarget::Domain { prefix, entity, .. } = &decl.target else {
        return Err(format!(
            "`target=\"kind:{kind}\"` answers occasion `{on}` for every member of a kind, but \
             `{on}` does not draw its targets from an entity kind (declare its `target:` as \
             `{{ prefix, entity }}`) (dsl 0.26.0 §5)"
        ));
    };
    let Some(decl_kind) = kinds.get(kind) else {
        let hint = lute_manifest::suggest::nearest(kind, kinds.keys().map(String::as_str), 2)
            .map_or_else(String::new, |near| {
                format!(" — did you mean `kind:{near}`?")
            });
        return Err(format!(
            "`target=\"kind:{kind}\"`: `{kind}` is not a declared entity kind{hint} (dsl 0.26.0 §5)"
        ));
    };
    let KindShape::Members(members) = &decl_kind.shape else {
        return Err(format!(
            "`target=\"kind:{kind}\"`: entity kind `{kind}` is `open:`, so its members are not \
             known to check or play; target a kind that lists its `members:` (dsl 0.26.0 §5)"
        ));
    };
    let entity_members = match kinds.get(entity).map(|k| &k.shape) {
        Some(KindShape::Members(ms)) => Some(ms.as_slice()),
        _ => None,
    };
    let legal = decl.target.domain_members(entity_members);
    let outside: Vec<&str> = members
        .iter()
        .filter(|m| legal.is_none_or(|l| !l.contains(m)))
        .map(String::as_str)
        .collect();
    if !outside.is_empty() {
        return Err(format!(
            "`target=\"kind:{kind}\"`: {} outside occasion `{on}`'s domain `{prefix}.<{entity}>`; \
             a kind target names `{entity}` or one of its sub-kinds (dsl 0.26.0 §5)",
            outside
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ")
                + if outside.len() == 1 { " is" } else { " are" }
        ));
    }
    Ok((prefix.clone(), members.clone()))
}

/// dsl 0.22.0 §8: every beat target of one document — the scene's own
/// `target:` and each `<entry on= target=>` — against its occasion's target
/// domain ([`occasion_target_ok`], a `kind:` target [`kind_target_members`]);
/// outside it is [`E_BEAT_ATTR`] at the target value. Runs after the fold
/// (the entity vocabulary comes from the imported schema). An unknown
/// occasion, an untargeted one, or a malformed target is already someone
/// else's diagnostic.
pub(crate) fn check_beat_target_domains(
    doc: &Document,
    beat: Option<&BeatMeta>,
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // `restricts`: whether a target on an occasion raised for none is already
    // reported (a scene's / bundle beat's, [`check_occasion`]); an entry's is
    // metadata there, which a `kind:` target can never be.
    let mut judge = |on: &str, target: &str, span: Span, layer: Layer, restricts: bool| {
        let Some(decl) = occasions.get(on) else {
            // An unknown occasion is `E-OCCASION-UNKNOWN`'s; without any
            // declared occasion a kind has no target prefix to answer.
            if occasions.is_empty() && kind_target(target).is_some() {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    format!(
                        "`target=\"{target}\"` answers occasion `{on}` for every member of a \
                         kind, which needs `{on}` declared by a plugin with a `target:` domain \
                         (dsl 0.26.0 §5)"
                    ),
                    span,
                    layer,
                ));
            }
            return;
        };
        let verdict = match kind_target(target) {
            Some(_) if restricts && !decl.target.takes_target() => return,
            Some(kind) => kind_target_members(decl, kind, kinds).map(drop),
            None => occasion_target_ok(decl, target, kinds),
        };
        if let Err(why) = verdict {
            diags.push(beat_diag(E_BEAT_ATTR, Severity::Error, why, span, layer));
        }
    };
    if let Some(BeatMeta {
        on,
        target: Some(target),
        ..
    }) = beat
    {
        judge(
            on,
            target,
            top_value_span(&doc.meta, "target"),
            Layer::Content,
            true,
        );
    }
    for entry in &doc.entries {
        let (Some((on, _)), Some((target, span))) = (&entry.on, &entry.target) else {
            continue;
        };
        if is_entry_ident(on) && is_beat_target(target) {
            judge(on, target, *span, Layer::Logic, false);
        }
    }
    // dsl 0.23.0 §4: a bundle beat's target, like an entry beat's.
    for beat in &doc.beats {
        let (Some((on, _)), Some((target, span))) = (&beat.on, &beat.target) else {
            continue;
        };
        if is_entry_ident(on) && is_beat_target(target) {
            judge(on, target, *span, Layer::Logic, true);
        }
    }
    // dsl 0.23.0 §2: an objective's target is checked like a beat's.
    for node in doc.quests.iter().flat_map(|q| &q.body) {
        let Node::Objective(o) = node else { continue };
        let (Some((on, _)), Some((target, span))) = (&o.on, &o.target) else {
            continue;
        };
        if is_entry_ident(on) && is_entry_target(target) {
            judge(on, target, *span, Layer::Logic, true);
        }
    }
    diags
}

/// dsl 0.26.0 §5: the member a `target="kind:<kind>"` beat was raised for —
/// readable in its `when`, guards and text, typed by the kind.
pub const OCCASION_TARGET: &str = "occasion.target";

/// Why a read of [`OCCASION_TARGET`] outside a kind beat has no value.
pub(crate) fn occasion_target_scope_message() -> String {
    format!(
        "`{OCCASION_TARGET}` is readable only in a beat or entry that targets a kind \
         (`target=\"kind:<kind>\"`), where it is the member the occasion was raised for \
         (dsl 0.26.0 §5)"
    )
}

/// dsl 0.26.0 §5: the members the document's kind beats answer — the domain
/// of [`OCCASION_TARGET`], sorted, empty without a (well-formed) kind beat.
pub(crate) fn occasion_target_members(
    doc: &Document,
    beat: Option<&BeatMeta>,
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<String> {
    let scene = beat.and_then(|b| Some((b.on.as_str(), b.target.as_deref()?)));
    let entries = doc
        .entries
        .iter()
        .filter_map(|e| Some((e.on.as_ref()?.0.as_str(), e.target.as_ref()?.0.as_str())));
    let bundles = doc
        .beats
        .iter()
        .filter_map(|b| Some((b.on.as_ref()?.0.as_str(), b.target.as_ref()?.0.as_str())));
    let mut out: Vec<String> = scene
        .into_iter()
        .chain(entries)
        .chain(bundles)
        .filter_map(|(on, target)| {
            let kind = kind_target(target)?;
            kind_target_members(occasions.get(on)?, kind, kinds).ok()
        })
        .flat_map(|(_, members)| members)
        .collect();
    out.sort();
    out.dedup();
    out
}

/// dsl 0.26.0 §5: in a lore document, a read of [`OCCASION_TARGET`] in an
/// entry or bundle beat that does not target a kind is `E-UNDECLARED` (the
/// document declares it for its kind beats; this one is never raised for a
/// member). A scene has one beat, so its declaration is the whole scope.
pub(crate) fn check_occasion_target_scope(doc: &Document) -> Vec<Diagnostic> {
    let is_kind =
        |t: &Option<(String, Span)>| t.as_ref().is_some_and(|(t, _)| kind_target(t).is_some());
    let outside: Vec<Span> = doc
        .entries
        .iter()
        .filter(|e| !is_kind(&e.target))
        .map(|e| e.span)
        .chain(
            doc.beats
                .iter()
                .filter(|b| !is_kind(&b.target))
                .map(|b| b.span),
        )
        .collect();
    if outside.len() == doc.entries.len() + doc.beats.len() {
        return Vec::new();
    }
    let within = |s: Span| {
        outside
            .iter()
            .any(|o| o.byte_start <= s.byte_start && s.byte_end <= o.byte_end)
    };
    let mut spans: Vec<Span> = Vec::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        if slot.raw.contains(OCCASION_TARGET) && within(slot.span) {
            spans.push(slot.span);
        }
    });
    fn lines<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a lute_syntax::ast::Line)) {
        for n in nodes {
            match n {
                Node::Line(l) => f(l),
                Node::Branch(b) => b.choices.iter().for_each(|c| lines(&c.body, f)),
                Node::Hub(h) => h.choices.iter().for_each(|c| lines(&c.body, f)),
                Node::Match(m) => m.arms.iter().for_each(|arm| match arm {
                    lute_syntax::ast::Arm::When { body, .. }
                    | lute_syntax::ast::Arm::Otherwise { body, .. } => lines(body, f),
                }),
                Node::Objective(o) => lines(&o.body, f),
                Node::On(o) => lines(&o.body, f),
                _ => {}
            }
        }
    }
    let bodies = doc
        .entries
        .iter()
        .filter(|e| !is_kind(&e.target))
        .map(|e| &e.body)
        .chain(
            doc.beats
                .iter()
                .filter(|b| !is_kind(&b.target))
                .map(|b| &b.body),
        );
    for body in bodies {
        lines(body, &mut |l| {
            spans.extend(
                l.interps
                    .iter()
                    .filter(|i| i.raw == OCCASION_TARGET)
                    .map(|i| i.span),
            );
        });
    }
    spans
        .into_iter()
        .map(|span| {
            beat_diag(
                "E-UNDECLARED",
                Severity::Error,
                occasion_target_scope_message(),
                span,
                Layer::Cel,
            )
        })
        .collect()
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

/// What declares a [`ProjectBeat`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectBeatKind {
    /// A scene's frontmatter beat (dsl 0.21.0 §3.1).
    Scene,
    /// A lore `<entry on=…>` (dsl 0.21.0 §3.2).
    Entry,
    /// A lore `<beat>` of a beat bundle (dsl 0.23.0 §4).
    Bundle,
}

/// One well-formed beat of a project root (dsl 0.21.0 §4, 0.23.0 §1) — what
/// the project beat passes judge and `lute beats` lists. A beat whose `on`,
/// `target`, `priority` or `once` is malformed is `E-BEAT-ATTR`'s and is not
/// listed.
#[derive(Clone, Debug)]
pub struct ProjectBeat<'a> {
    pub path: &'a PathBuf,
    pub kind: ProjectBeatKind,
    /// A scene's canonical key (`this scene` without one), an entry's id, a
    /// bundle beat's canonical `<document id>.<beat id>`.
    pub id: String,
    pub on: &'a str,
    pub target: Option<&'a str>,
    /// dsl 0.26.0 §5: a `target="kind:<kind>"` beat's `<prefix>.<member>`
    /// targets (`target` keeps the authored `kind:<kind>`).
    pub kind_targets: Option<Vec<String>>,
    pub priority: i64,
    /// A scene's policy; an entry's authored `once` ([`BeatOnce::None`] when
    /// absent — an entry without `once` is repeatable).
    pub once: BeatOnce,
    /// `once` is written rather than defaulted (an entry's `once` always is).
    pub once_authored: bool,
    /// dsl 0.23.0 §3: a scene's `also: true`.
    pub also: bool,
    /// A scene's / bundle beat's non-blank `after:` / `after=`, raw.
    pub after: Option<&'a str>,
    /// dsl 0.25.0 §2: the well-formed `share` key and where it is written.
    pub share: Option<(&'a str, Span)>,
    /// The `when` slot as authored.
    pub when_slot: Option<&'a CelSlot>,
    /// The `when` after `@def` expansion in its own document.
    pub when: Option<String>,
    /// The scene's frontmatter `title:` / the entry's `title=`.
    pub title: Option<String>,
    /// The `on` key / attribute — where a beat diagnostic anchors.
    pub anchor: Span,
    pub folded: &'a FoldedEnv,
    /// The unit the beat presents, as [`crate::cast::FactProducers`] keys
    /// its assert sites: `0` for a scene, else the entry's / bundle beat's
    /// span start.
    pub unit: usize,
}

impl ProjectBeat<'_> {
    /// The beat as messages name it: ``scene `k` `` / ``entry `id` `` /
    /// ``beat `doc.id` ``.
    pub fn name(&self) -> String {
        match self.kind {
            ProjectBeatKind::Scene => format!("scene `{}`", self.id),
            ProjectBeatKind::Entry => format!("entry `{}`", self.id),
            ProjectBeatKind::Bundle => format!("beat `{}`", self.id),
        }
    }

    /// The targets the beat answers.
    pub fn cells(&self) -> BeatCells<'_> {
        BeatCells::of(self.target, self.kind_targets.as_deref())
    }
}

/// dsl 0.26.0 §5: the targets one beat answers — every target (untargeted),
/// one, or each member of a kind (`target="kind:<kind>"`).
#[derive(Clone, Copy, Debug)]
pub enum BeatCells<'a> {
    Any,
    One(&'a str),
    Kind(&'a [String]),
}

impl<'a> BeatCells<'a> {
    pub fn of(target: Option<&'a str>, kind_targets: Option<&'a [String]>) -> Self {
        match (kind_targets, target) {
            (Some(ts), _) => BeatCells::Kind(ts),
            (None, Some(t)) => BeatCells::One(t),
            (None, None) => BeatCells::Any,
        }
    }

    /// Whether the beat is a candidate when the occasion is raised for `t`.
    pub fn answers(self, t: &str) -> bool {
        match self {
            BeatCells::Any => true,
            BeatCells::One(x) => x == t,
            BeatCells::Kind(ts) => ts.iter().any(|x| x == t),
        }
    }

    /// Whether every target `other` answers, this beat answers too.
    pub fn covers(self, other: BeatCells<'_>) -> bool {
        match other {
            BeatCells::Any => matches!(self, BeatCells::Any),
            BeatCells::One(t) => self.answers(t),
            BeatCells::Kind(ts) => ts.iter().all(|t| self.answers(t)),
        }
    }

    /// Whether some raise has both beats as candidates.
    pub fn meets(self, other: BeatCells<'_>) -> bool {
        match (self, other) {
            (BeatCells::Any, _) | (_, BeatCells::Any) => true,
            (BeatCells::One(t), o) | (o, BeatCells::One(t)) => o.answers(t),
            (BeatCells::Kind(a), BeatCells::Kind(b)) => a.iter().any(|t| b.contains(t)),
        }
    }

    /// dsl 0.26.0 §5: at equal priority a kind beat ranks after every other
    /// candidate (the beat naming the member outranks it).
    pub fn is_kind(self) -> bool {
        matches!(self, BeatCells::Kind(_))
    }
}

/// Every well-formed beat of one project root, in `ProjectIndex.beats`
/// order: `docs` (parallel to `foldeds`) in `check-project` order — the
/// selection tiebreak after priority — then declaration order within a
/// document (a lore document's entry beats and bundle beats interleave by
/// source position, as their compiled records do).
pub fn project_beats<'a>(
    docs: &'a [(PathBuf, Document)],
    foldeds: &[&'a FoldedEnv],
) -> Vec<ProjectBeat<'a>> {
    let mut beats = Vec::new();
    for ((path, doc), &folded) in docs.iter().zip(foldeds) {
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let expand = |when: Option<&CelSlot>| {
            when.map(|w| {
                let mut stack = Vec::new();
                crate::cel_expand::expand_cel(&w.raw, &defs, None, &mut stack)
                    .unwrap_or_else(|_| w.raw.clone())
            })
        };
        // dsl 0.26.0 §5: `Some(None)` for a beat without a `kind:` target,
        // `Some(Some(targets))` for one whose kind resolves, `None` for one
        // `E-BEAT-ATTR` rejects (not listed).
        let kind_cells = |on: &str, target: Option<&str>| -> Option<Option<Vec<String>>> {
            let Some(kind) = target.and_then(kind_target) else {
                return Some(None);
            };
            let decl = folded.occasions.get(on)?;
            let (prefix, members) =
                kind_target_members(decl, kind, &folded.env.rel_vocab.kinds).ok()?;
            Some(Some(
                members.iter().map(|m| format!("{prefix}.{m}")).collect(),
            ))
        };
        if let Some((beat, kind_targets)) = folded
            .typed
            .beat
            .as_ref()
            .and_then(|b| Some((b, kind_cells(&b.on, b.target.as_deref())?)))
        {
            let title = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml)
                .ok()
                .and_then(|m| {
                    m.get(serde_yaml::Value::String("title".to_string()))?
                        .as_str()
                        .map(str::to_string)
                });
            beats.push(ProjectBeat {
                path,
                kind: ProjectBeatKind::Scene,
                id: scene_beat_name(folded),
                on: &beat.on,
                target: beat.target.as_deref(),
                kind_targets,
                priority: beat.priority,
                once: beat.once,
                once_authored: beat.once_authored,
                also: beat.also,
                after: folded
                    .typed
                    .after
                    .as_deref()
                    .filter(|a| !a.trim().is_empty()),
                // dsl 0.25.0 §2: a `share` without a written, spending `once`
                // is `E-BEAT-ATTR`'s alone; it joins no key.
                share: beat
                    .share
                    .as_deref()
                    .filter(|_| beat.once_authored && beat.once != BeatOnce::None)
                    .map(|k| (k, top_value_span(&doc.meta, "share"))),
                when_slot: beat.when.as_ref(),
                when: expand(beat.when.as_ref()),
                title,
                anchor: top_key_span(&doc.meta, "on"),
                folded,
                unit: 0,
            });
        }
        // A lore document's entry beats and bundle beats, by source position.
        let mut lore: Vec<(usize, ProjectBeat<'a>)> = Vec::new();
        for entry in &doc.entries {
            let Some((on, on_span)) = entry.on.as_ref().filter(|(on, _)| is_entry_ident(on)) else {
                continue;
            };
            let priority = match &entry.priority {
                None => 0,
                Some((raw, _)) => match parse_beat_priority(raw) {
                    Some(p) => p,
                    None => continue,
                },
            };
            // dsl 0.24.0 §6: on an occasion declared without a target the
            // entry's `target=` is metadata, not a candidate restriction.
            let target = match &entry.target {
                None => None,
                Some((t, _)) if kind_target(t).is_some() => Some(t.as_str()),
                Some((t, _)) if is_entry_target(t) => {
                    beat_target_restricts(on, &folded.occasions).then_some(t.as_str())
                }
                Some(_) => continue,
            };
            let Some(kind_targets) = kind_cells(on, target) else {
                continue;
            };
            let once = match entry.once.as_ref().map(|(o, _)| o.as_str()) {
                None => BeatOnce::None,
                Some("run") => BeatOnce::Run,
                Some("user") => BeatOnce::User,
                Some("day") => BeatOnce::Day,
                Some("slot") => BeatOnce::Slot,
                Some(_) => continue,
            };
            lore.push((
                entry.span.byte_start,
                ProjectBeat {
                    path,
                    kind: ProjectBeatKind::Entry,
                    id: entry.id.clone(),
                    on,
                    target,
                    kind_targets,
                    priority,
                    once,
                    once_authored: entry.once.is_some(),
                    also: false,
                    after: None,
                    share: well_formed_share(entry.share.as_ref())
                        .filter(|_| once != BeatOnce::None),
                    when_slot: entry.when.as_ref(),
                    when: expand(entry.when.as_ref()),
                    title: entry.title.as_ref().map(|(t, _)| t.clone()),
                    anchor: *on_span,
                    folded,
                    unit: entry.span.byte_start,
                },
            ));
        }
        // dsl 0.23.0 §4: bundle beats — skipped without a document `id:`
        // (their canonical id hangs off it; `E-BEAT-ATTR`) or with a
        // malformed `on` / `target` / `priority` / `once`.
        if let Some(doc_id) = folded.typed.id.as_deref() {
            for beat in &doc.beats {
                let Some((on, on_span)) = beat.on.as_ref().filter(|(on, _)| is_entry_ident(on))
                else {
                    continue;
                };
                if beat.id.is_empty()
                    || beat
                        .target
                        .as_ref()
                        .is_some_and(|(t, _)| !is_beat_target(t))
                    || beat
                        .priority
                        .as_ref()
                        .is_some_and(|(p, _)| parse_beat_priority(p).is_none())
                    || beat.once.as_ref().is_some_and(|(o, _)| {
                        !matches!(o.as_str(), "run" | "user" | "false" | "day" | "slot")
                    })
                {
                    continue;
                }
                let target = beat.target.as_ref().map(|(t, _)| t.as_str());
                let Some(kind_targets) = kind_cells(on, target) else {
                    continue;
                };
                lore.push((
                    beat.span.byte_start,
                    ProjectBeat {
                        path,
                        kind: ProjectBeatKind::Bundle,
                        id: crate::bundles::bundle_beat_key(doc_id, &beat.id),
                        on,
                        target,
                        kind_targets,
                        priority: crate::bundles::bundle_beat_priority(beat),
                        once: crate::bundles::bundle_beat_once(beat),
                        once_authored: beat.once.is_some(),
                        also: crate::bundles::bundle_beat_also(beat),
                        after: beat
                            .after
                            .as_ref()
                            .map(|(a, _)| a.as_str())
                            .filter(|a| !a.trim().is_empty()),
                        share: well_formed_share(beat.share.as_ref())
                            .filter(|_| beat.once.as_ref().is_some_and(|(o, _)| o != "false")),
                        when_slot: beat.when.as_ref(),
                        when: expand(beat.when.as_ref()),
                        title: beat.title.as_ref().map(|(t, _)| t.clone()),
                        anchor: *on_span,
                        folded,
                        unit: beat.span.byte_start,
                    },
                ));
            }
        }
        lore.sort_by_key(|(at, _)| *at);
        beats.extend(lore.into_iter().map(|(_, b)| b));
    }
    beats
}

/// A lore beat's `share` attribute when it is a well-formed key.
fn well_formed_share(share: Option<&(String, Span)>) -> Option<(&str, Span)> {
    share
        .filter(|(k, _)| is_entry_ident(k))
        .map(|(k, s)| (k.as_str(), *s))
}

/// dsl 0.25.0 §2: what each beat's shared spend holds — `share key → the
/// beats of the key`, as indices into `beats`, in project order.
fn share_groups(beats: &[ProjectBeat<'_>]) -> BTreeMap<String, Vec<usize>> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, b) in beats.iter().enumerate() {
        if let Some((key, _)) = b.share {
            groups.entry(key.to_string()).or_default().push(i);
        }
    }
    groups
}

/// dsl 0.25.0 §2 ([`E_BEAT_ATTR`], `check-project`): every beat of one
/// `share` key declares the same `once` — the first beat of the key, in
/// project order, sets it; each beat that differs is reported at its
/// `share`.
fn check_share_once(beats: &[ProjectBeat<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (key, members) in share_groups(beats) {
        let first = &beats[members[0]];
        for &i in &members[1..] {
            let b = &beats[i];
            if b.once == first.once {
                continue;
            }
            out.push((
                b.path.clone(),
                beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    format!(
                        "{} shares `{key}` with {}, but declares `once: {}` where {} declares \
                         `once: {}`; the beats of one `share` key are spent together for one \
                         period, so every one of them declares the same `once` (dsl 0.25.0 §2)",
                        b.name(),
                        first.name(),
                        b.once.as_str(),
                        first.name(),
                        first.once.as_str(),
                    ),
                    b.share.map_or(b.anchor, |(_, s)| s),
                    Layer::Logic,
                ),
            ));
        }
    }
    out
}

/// One beat of the project, in selection-tiebreak order, with what the
/// selection passes judge about it.
struct Beat<'a> {
    path: &'a PathBuf,
    /// `scene `k`` / `entry `id`` — the beat as messages name it.
    name: String,
    on: &'a str,
    target: Option<&'a str>,
    /// dsl 0.26.0 §5: [`ProjectBeat::kind_targets`].
    kind_targets: Option<Vec<String>>,
    priority: i64,
    once: BeatOnce,
    /// dsl 0.23.0 §3: presented beside the winner, never instead of it.
    also: bool,
    /// Always eligible: no `after:`, and a `when` absent or deciding true.
    always: bool,
    /// Never spent: a scene's `once: false`, or an entry without `once`.
    unspent: bool,
    /// The `when` after `@def` expansion in its own document (`None` when
    /// absent) — what messages quote.
    when: Option<String>,
    /// The `when` slot's span — where the fact envelope's must set is read.
    when_span: Option<Span>,
    /// dsl 0.24.0 (T3-3): the eligibility the tie check conjoins across
    /// documents — the expanded `when` AND what the beat's `once` requires
    /// ([`once_guard`]) AND (dsl 0.26.0 §8) `!holds(F)` for every fact `F`
    /// only its own unplayed presentation can assert
    /// ([`crate::cast::unit_facts`]); `None` when none constrains.
    eligible: Option<String>,
    /// [`Self::eligible`] in disjunctive normal form, typed in its own
    /// document (a pure-schedule `holds(A)` contributing its rules' `cel()`
    /// guards).
    dnf: crate::reachability::Dnf,
    /// The facts the beat asserts that may hold before it plays, as
    /// `holds(rel(a,b))` pseudo-paths, each with why — what a tie message
    /// says when another beat reads one.
    persists: Vec<(String, String)>,
    /// The flag that stays set across runs once the beat played:
    /// `visited('<id>')` for a scene or bundle beat, `entry.<id>.everRead`
    /// for an entry.
    ever_flag: String,
    /// A defaulted `once: run` (a scene's or bundle beat's; never an
    /// entry's, whose `once` is always written) with a `when` that reads
    /// only user-tier state (dsl 0.23.1).
    run_once_user_when: bool,
    /// Where a warning anchors: the `on` key / attribute.
    anchor: Span,
    folded: &'a FoldedEnv,
}

impl Beat<'_> {
    fn cells(&self) -> BeatCells<'_> {
        BeatCells::of(self.target, self.kind_targets.as_deref())
    }
}

/// `W-BEAT-PRIORITY-TIE` (dsl 0.22.0 §13): two beats on one `select: first`
/// occasion whose selection falls to file order.
pub const W_BEAT_PRIORITY_TIE: &str = "W-BEAT-PRIORITY-TIE";
/// `W-BEAT-ONCE-RUN-USER` (dsl 0.22.0 §13 advisory, dsl 0.23.1): a beat
/// spent once per run by DEFAULT whose `when` reads only user-tier state, so
/// it replays every run. An authored `once: run` silences it.
pub const W_BEAT_ONCE_RUN_USER: &str = "W-BEAT-ONCE-RUN-USER";

/// The project beat passes over one resolved project root (dsl 0.21.0 §4,
/// §5; dsl 0.22.0 §13). `docs` and `foldeds` are parallel and in
/// `check-project` order, which is the `ProjectIndex.beats` tiebreak order
/// (documents in order, declaration order within). Beats are ordered by
/// priority descending, then that order.
///
/// - [`W_BEAT_SHADOWED`]: a beat `B` on a `select: first` occasion (an
///   undeclared occasion counts as `first`) is shadowed by the first earlier
///   beat `A` on the same occasion whose target is absent or equal to `B`'s —
///   so `A` is a candidate whenever `B` is — that is always eligible (no
///   `after:`, `when` absent or deciding true without facts) and never spent
///   (`once: false`, or an entry without `once`). `A` then wins every time
///   `B` could. Conservative: an undecided `when` never shadows. dsl 0.24.0
///   (T1-8): an untargeted `B` on an occasion whose target domain is closed
///   is also shadowed when, for EVERY `<prefix>.<member>` of the domain, some
///   earlier such `A` targets that member (or none) — it can win no ladder.
/// - [`W_BEAT_PRIORITY_TIE`]: an unshadowed `B` with EQUAL priority to
///   earlier beats on the same `select: first` occasion that can be
///   candidates at once (either target absent, or equal) and whose
///   eligibilities are not provably exclusive — neither the decider folds
///   their conjunction to `false` nor two of their conjuncts pin one path to
///   disjoint values. dsl 0.24.0 (T3-3): the eligibility is the `when` AND
///   what the `once` requires ([`once_guard`]), and a `holds(A)` conjunct of
///   a pure-schedule derived atom contributes its rules' `cel()` guards. File
///   order then picks the winner. One warning per `B`, naming every partner.
/// - [`W_BEAT_ONCE_RUN_USER`]: a beat whose `once: run` is DEFAULTED (not
///   written — dsl 0.23.1) and whose `when` reads state, all of it user-tier
///   ([`reads_only_user`]: `user.*`, `entry.<id>.everRead`, a user-tier
///   quest's `quest.<id>.*`, a `tier: user` relation's `holds`/`count`;
///   `prev.run.*` is run history, not user-tier): once true it stays true
///   across runs, so the beat plays again at the start of every run.
pub fn check_project_beats(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    producers: &crate::cast::FactProducers,
    env: Option<&FactEnv>,
) -> Vec<(PathBuf, Diagnostic)> {
    let params = BTreeMap::new();
    // dsl 0.23.0 §6: a quest's tier (`tier="run"`, else user) — project-wide,
    // since a beat may read a quest declared anywhere.
    let quest_tiers: BTreeMap<&str, bool> = docs
        .iter()
        .flat_map(|(_, doc)| &doc.quests)
        .filter(|q| !q.id.is_empty())
        .map(|q| {
            (
                q.id.as_str(),
                !q.tier.as_ref().is_some_and(|(t, _)| t == "run"),
            )
        })
        .collect();
    let pbs = project_beats(docs, foldeds);
    let groups = share_groups(&pbs);
    let share_diags = check_share_once(&pbs);
    let guards: Vec<Option<String>> = pbs.iter().map(|pb| once_guard(pb, &pbs, &groups)).collect();
    let mut beats: Vec<Beat<'_>> = pbs
        .into_iter()
        .zip(guards)
        .map(|(pb, guard)| {
            let folded = pb.folded;
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
            let holds = pb.when_slot.is_none_or(|w| {
                matches!(decide_slot(&w.raw, &defs, &ctx), Some(Decided::Bool(true)))
            });
            // dsl 0.26.0 §8: a fact only this beat's unplayed presentation
            // asserts does not hold while it is eligible.
            let mut absent = Vec::new();
            let mut persists = Vec::new();
            for f in
                crate::cast::unit_facts(producers, &folded.env.rel_vocab, pb.path, pb.unit, pb.once)
            {
                match f.persists {
                    None => absent.push(format!("!{}", f.query)),
                    Some(why) => persists.push((f.query.replace(", ", ","), why)),
                }
            }
            let eligible = pb
                .when
                .as_deref()
                .map(|w| format!("({w})"))
                .into_iter()
                .chain(guard)
                .chain(absent)
                .reduce(|acc, c| format!("{acc} && {c}"));
            let user_tier = UserTier {
                relations: &folded.env.rel_vocab.relations,
                quests: &quest_tiers,
            };
            Beat {
                path: pb.path,
                name: pb.name(),
                on: pb.on,
                target: pb.target,
                kind_targets: pb.kind_targets,
                priority: pb.priority,
                once: pb.once,
                also: pb.also,
                always: pb.after.is_none() && holds,
                unspent: pb.once == BeatOnce::None,
                run_once_user_when: pb.once == BeatOnce::Run
                    && !pb.once_authored
                    && pb
                        .when
                        .as_deref()
                        .is_some_and(|w| reads_only_user(w, &user_tier)),
                dnf: eligible.as_deref().map_or_else(Default::default, |e| {
                    crate::reachability::when_dnf(
                        e,
                        &defs,
                        &folded.env.state,
                        Some(&folded.env.rel_vocab),
                    )
                }),
                eligible,
                persists,
                ever_flag: match pb.kind {
                    ProjectBeatKind::Entry => format!("entry.{}.everRead", pb.id),
                    ProjectBeatKind::Scene | ProjectBeatKind::Bundle => {
                        format!("visited('{}')", pb.id)
                    }
                },
                when_span: pb.when_slot.map(|w| w.span),
                when: pb.when,
                anchor: pb.anchor,
                folded,
            }
        })
        .collect();
    // Stable: equal priorities keep the tiebreak order, a kind beat after
    // the others of its priority (dsl 0.26.0 §5).
    beats.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.cells().is_kind().cmp(&b.cells().is_kind()))
    });

    let mut out = share_diags;
    for b in beats.iter().filter(|b| b.run_once_user_when) {
        out.push((
            b.path.clone(),
            beat_diag(
                W_BEAT_ONCE_RUN_USER,
                Severity::Warning,
                format!(
                    "{} is spent once per run by default (`once: run`), but its `when` `{}` \
                     reads only user-tier state, which a new run does not reset — once it holds \
                     it holds every run, so the beat plays again each run; write `once: run` if \
                     it should replay every run, use `once: user` for a beat heard once ever, or \
                     gate it on run-tier state (dsl 0.22.0 §13)",
                    b.name,
                    b.when.as_deref().unwrap_or_default().trim()
                ),
                b.anchor,
                Layer::Logic,
            ),
        ));
    }
    for (j, b) in beats.iter().enumerate() {
        let select = b
            .folded
            .occasions
            .get(b.on)
            .map_or(OccasionSelect::First, |o| o.select);
        // dsl 0.23.0 §3: an `also` beat never competes for the win — it is
        // presented beside the winner — so it is neither shadowed nor
        // shadows, and its order among other `also` beats is no tie.
        if select != OccasionSelect::First || b.also {
            continue;
        }
        let target = b.target.map_or_else(String::new, |t| format!(" for `{t}`"));
        if let Some(a) = beats[..j].iter().find(|a| {
            !a.also && a.on == b.on && a.cells().covers(b.cells()) && a.always && a.unspent
        }) {
            let spent = if a.name.starts_with("entry") {
                "an entry without `once`"
            } else {
                "`once: false`"
            };
            out.push((
                b.path.clone(),
                beat_diag(
                    W_BEAT_SHADOWED,
                    Severity::Warning,
                    format!(
                        "{} can never win occasion `{}`{target}: {} (priority {}) is ordered \
                         before it, is always eligible (no `after:`, and its `when` is absent or \
                         always true), and is never spent ({spent}), so it wins every time \
                         (dsl 0.21.0 §5)",
                        b.name, b.on, a.name, a.priority
                    ),
                    b.anchor,
                    Layer::Logic,
                ),
            ));
            continue;
        }
        // dsl 0.24.0 (T1-8): an untargeted `B` on an occasion with a closed
        // target domain is a candidate at every member — and shadowed when,
        // at EVERY member, an earlier always-eligible never-spent beat for
        // that member wins (the per-target ladders of `lute beats`). dsl
        // 0.26.0 §5: so is a kind beat at every member of its kind.
        if b.target.is_none() || b.cells().is_kind() {
            if let Some(shadowers) = shadowed_on_every_target(b, &beats[..j]) {
                let listed: Vec<String> = shadowers
                    .iter()
                    .map(|(t, a)| format!("`{t}`: {}", a.name))
                    .collect();
                out.push((
                    b.path.clone(),
                    beat_diag(
                        W_BEAT_SHADOWED,
                        Severity::Warning,
                        format!(
                            "{} can never win occasion `{}`: it answers every {}, but on \
                             every {} an earlier beat for that \
                             target is always eligible (no `after:`, and its `when` is absent \
                             or always true) and never spent, so it wins every time — {} \
                             (dsl 0.21.0 §5)",
                            b.name,
                            b.on,
                            b.target.filter(|_| b.cells().is_kind()).map_or_else(
                                || "target".to_string(),
                                |t| format!("member of `{t}`")
                            ),
                            if b.cells().is_kind() {
                                "one of them"
                            } else {
                                "target of the occasion's domain"
                            },
                            listed.join(", ")
                        ),
                        b.anchor,
                        Layer::Logic,
                    ),
                ));
                continue;
            }
        }
        let partners: Vec<&Beat<'_>> = beats[..j]
            .iter()
            .filter(|a| {
                !a.also
                    && a.on == b.on
                    && a.priority == b.priority
                    && a.cells().meets(b.cells())
                    // dsl 0.26.0 §5: at equal priority the beat naming the
                    // member (or none) outranks a kind beat — no file order.
                    && a.cells().is_kind() == b.cells().is_kind()
                    && !provably_exclusive(a, b, env)
            })
            .collect();
        if partners.is_empty() {
            continue;
        }
        let names: Vec<&str> = partners.iter().map(|a| a.name.as_str()).collect();
        // dsl 0.26.0 §8 (T3-2): say why each pair is not exclusive.
        let why = match partners.as_slice() {
            [a] => why_not_exclusive(a, b),
            many => many
                .iter()
                .map(|a| format!("with {}: {}", a.name, why_not_exclusive(a, b)))
                .collect::<Vec<_>>()
                .join("; "),
        };
        out.push((
            b.path.clone(),
            beat_diag(
                W_BEAT_PRIORITY_TIE,
                Severity::Warning,
                format!(
                    "{} ties {} on occasion `{}`{target} at priority {}, and their `when`s are \
                     not provably exclusive ({why}) — when both are eligible the winner is \
                     whichever comes first in file order (today {}), so renaming or moving a \
                     file changes it; give one a different `priority`, or make the conditions \
                     exclusive (dsl 0.22.0 §13)",
                    b.name,
                    names.join(", "),
                    b.on,
                    b.priority,
                    partners[0].name,
                ),
                b.anchor,
                Layer::Logic,
            ),
        ));
    }
    out
}

/// dsl 0.24.0 (T1-8): for an untargeted `b` on an occasion whose target
/// domain is closed (enumerable members) — or (dsl 0.26.0 §5) a kind beat,
/// over its kind's members — the shadowing beat per target: `Some` only when
/// EVERY target has an earlier (in `earlier`, selection order) non-`also`
/// beat on the occasion answering it that is always eligible and never spent.
fn shadowed_on_every_target<'b, 'a>(
    b: &Beat<'a>,
    earlier: &'b [Beat<'a>],
) -> Option<Vec<(String, &'b Beat<'a>)>> {
    let targets: Vec<String> = match &b.kind_targets {
        Some(ts) => ts.clone(),
        None => {
            let decl = b.folded.occasions.get(b.on)?;
            let OccasionTarget::Domain { prefix, entity, .. } = &decl.target else {
                return None;
            };
            let kind_members = match &b.folded.env.rel_vocab.kinds.get(entity)?.shape {
                KindShape::Members(ms) => Some(ms.as_slice()),
                KindShape::Open | KindShape::Invalid => None,
            };
            decl.target
                .domain_members(kind_members)?
                .iter()
                .map(|m| format!("{prefix}.{m}"))
                .collect()
        }
    };
    if targets.is_empty() {
        return None;
    }
    targets
        .into_iter()
        .map(|t| {
            let a = earlier.iter().find(|a| {
                !a.also && a.on == b.on && a.cells().answers(&t) && a.always && a.unspent
            })?;
            Some((t, a))
        })
        .collect()
}

/// dsl 0.24.0 (T3-3): the readable flag a beat's own presentation sets and
/// its `once` reads — an entry's `once="user"` `entry.<id>.everRead`,
/// `once="run"` `entry.<id>.read`; a scene's or bundle beat's `once: user`
/// `visited('<id>')` (the save-scoped visited set). A scene's `once: run`
/// (or a clock period) has no readable flag, and a repeatable beat none.
fn spend_flag(pb: &ProjectBeat<'_>) -> Option<String> {
    match (pb.kind, pb.once) {
        (ProjectBeatKind::Entry, BeatOnce::User) => Some(format!("entry.{}.everRead", pb.id)),
        (ProjectBeatKind::Entry, BeatOnce::Run) => Some(format!("entry.{}.read", pb.id)),
        (ProjectBeatKind::Scene, BeatOnce::User)
            if crate::meta::canonical_scene_key(&pb.folded.typed).is_some() =>
        {
            Some(format!("visited('{}')", pb.id))
        }
        (ProjectBeatKind::Bundle, BeatOnce::User) => Some(format!("visited('{}')", pb.id)),
        _ => None,
    }
}

/// The beats whose presentation spends `pb` (dsl 0.25.0 §2): every beat of
/// its `share` key, else `pb` alone.
fn spenders<'b, 'a>(
    pb: &'b ProjectBeat<'a>,
    beats: &'b [ProjectBeat<'a>],
    groups: &BTreeMap<String, Vec<usize>>,
) -> Vec<&'b ProjectBeat<'a>> {
    match pb.share.and_then(|(k, _)| groups.get(k)) {
        Some(members) => members.iter().map(|&i| &beats[i]).collect(),
        None => vec![pb],
    }
}

/// dsl 0.24.0 (T3-3): what a beat's `once` adds to its eligibility — no
/// [`spend_flag`] of a beat that spends it is set. dsl 0.25.0 §2: a shared
/// beat is spent by every beat of its key, so each member's flag counts
/// (`!visited('a') && !entry.b.everRead`). `None` when no spender has one.
fn once_guard(
    pb: &ProjectBeat<'_>,
    beats: &[ProjectBeat<'_>],
    groups: &BTreeMap<String, Vec<usize>>,
) -> Option<String> {
    let flags: Vec<String> = spenders(pb, beats, groups)
        .into_iter()
        .filter_map(spend_flag)
        .map(|f| format!("!{f}"))
        .collect();
    (!flags.is_empty()).then(|| flags.join(" && "))
}

/// What "`pb` is spent" reads as, for the presence ladder: some beat that
/// spends it was presented — its own [`spend_flag`], or (dsl 0.25.0 §2)
/// the disjunction over its `share` key. `None` unless EVERY spender has a
/// readable flag: one without makes the spend unreadable.
fn spent_condition(
    pb: &ProjectBeat<'_>,
    beats: &[ProjectBeat<'_>],
    groups: &BTreeMap<String, Vec<usize>>,
) -> Option<String> {
    let flags: Option<Vec<String>> = spenders(pb, beats, groups)
        .into_iter()
        .map(spend_flag)
        .collect();
    match flags?.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        many => Some(format!("({})", many.join(" || "))),
    }
}

/// dsl 0.24.0 §4 (`W-CAST-ABSENT`): what each beat may assume about the
/// ladder above it. A `select: first`, non-`also` beat `B` wins only once
/// every beat ordered before it on the same occasion that is a candidate
/// whenever `B` is (untargeted, or `B`'s target), always eligible (no
/// `after:`, `when` absent or always true) and spent by a readable flag
/// ([`spent_condition`]) has been spent: `entry.<id>.everRead`,
/// `entry.<id>.read` or `visited('<id>')` — dsl 0.25.0 §2: for a shared
/// beat, any of its key's flags. Keyed by document, then by the
/// beat's `on` key/attribute offset ([`ProjectBeat::anchor`]).
pub fn presence_ladder(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> BTreeMap<PathBuf, BTreeMap<usize, Vec<String>>> {
    let params = BTreeMap::new();
    let pbs = project_beats(docs, foldeds);
    let groups = share_groups(&pbs);
    let spent_conds: Vec<Option<String>> = pbs
        .iter()
        .map(|pb| spent_condition(pb, &pbs, &groups))
        .collect();
    let mut beats: Vec<(ProjectBeat<'_>, bool, Option<String>)> = pbs
        .into_iter()
        .zip(spent_conds)
        .map(|(pb, spent)| {
            let defs = DefTable {
                bodies: &pb.folded.def_bodies,
                params: &pb.folded.env.def_params,
            };
            let ctx = DecideCtx {
                schema: &pb.folded.env.state,
                dollar: None,
                params: &params,
                facts: None,
            };
            let always = pb.after.is_none()
                && pb.when_slot.is_none_or(|w| {
                    matches!(decide_slot(&w.raw, &defs, &ctx), Some(Decided::Bool(true)))
                });
            (pb, always, spent)
        })
        .collect();
    // Stable: equal priorities keep the tiebreak order, a kind beat after
    // the others of its priority (dsl 0.26.0 §5).
    beats.sort_by(|a, b| {
        b.0.priority
            .cmp(&a.0.priority)
            .then(a.0.cells().is_kind().cmp(&b.0.cells().is_kind()))
    });
    let mut out: BTreeMap<PathBuf, BTreeMap<usize, Vec<String>>> = BTreeMap::new();
    for (j, (b, _, _)) in beats.iter().enumerate() {
        let select = b
            .folded
            .occasions
            .get(b.on)
            .map_or(OccasionSelect::First, |o| o.select);
        if select != OccasionSelect::First || b.also {
            continue;
        }
        let spent: Vec<String> = beats[..j]
            .iter()
            .filter(|(a, always, _)| {
                *always && !a.also && a.on == b.on && a.cells().covers(b.cells())
            })
            .filter_map(|(_, _, spent)| spent.clone())
            .collect();
        if !spent.is_empty() {
            out.entry(b.path.clone())
                .or_default()
                .insert(b.anchor.byte_start, spent);
        }
    }
    out
}

/// `a` and `b` can never be eligible together: two of the alternatives of
/// their eligibilities (`when`, `once` and the facts only their own
/// presentation asserts — [`Beat::eligible`]), negations normalized (dsl
/// 0.26.0 §8), pin one path to disjoint values in every pairing; or the
/// decider folds their conjunction to `false` — in either beat's document,
/// under `env`'s must set at that beat's `when` slot when `check-project`
/// has one (both are judged in the same state, so a fact guaranteed at
/// either slot holds). An unconstrained eligibility never excludes
/// anything.
fn provably_exclusive(a: &Beat<'_>, b: &Beat<'_>, env: Option<&FactEnv>) -> bool {
    let (Some(wa), Some(wb)) = (&a.eligible, &b.eligible) else {
        return false;
    };
    if crate::reachability::provably_exclusive(&a.dnf, &b.dnf) {
        return true;
    }
    let params = BTreeMap::new();
    let both = format!("({wa}) && ({wb})");
    [b, a].into_iter().any(|x| {
        let defs = DefTable {
            bodies: &x.folded.def_bodies,
            params: &x.folded.env.def_params,
        };
        let ctx = DecideCtx {
            schema: &x.folded.env.state,
            dollar: None,
            params: &params,
            facts: env.zip(x.when_span).map(|(env, span)| FactScope {
                env,
                vocab: &x.folded.env.rel_vocab,
                path: x.path.as_path(),
                span,
                wip: false,
            }),
        };
        matches!(decide_slot(&both, &defs, &ctx), Some(Decided::Bool(false)))
    })
}

/// dsl 0.26.0 §8 (T3-2): why `a` and `b` are not [`provably_exclusive`],
/// as a clause: a beat that constrains nothing, a flag that outlives a
/// `once: run` spend, a fact one asserts that may hold before it plays, or
/// the paths the two alternatives that overlap constrain.
fn why_not_exclusive(a: &Beat<'_>, b: &Beat<'_>) -> String {
    let Some((da, db)) = crate::reachability::non_exclusive_witness(&a.dnf, &b.dnf) else {
        return "their conjunction does not decide `false`".to_string();
    };
    for (x, y, dy) in [(a, b, db), (b, a, da)] {
        if x.once == BeatOnce::Run && dy.requires_true(&x.ever_flag) {
            return format!(
                "`{}` persists across runs; `once: run` does not, so in a later run both are \
                 eligible",
                x.ever_flag
            );
        }
        if let Some((fact, why)) = x.persists.iter().find(|(f, _)| dy.requires_true(f)) {
            return format!(
                "{} reads `{fact}`, which {} asserts, but {why}",
                y.name, x.name
            );
        }
    }
    for x in [a, b] {
        if x.eligible.is_none() {
            let spend = match (x.once, x.name.starts_with("entry")) {
                (BeatOnce::Run, false) => ", and its `once: run` sets no flag a `when` can read",
                _ => "",
            };
            return format!("{} has no `when`{spend}", x.name);
        }
    }
    let list = |d: &crate::reachability::Disjunct| {
        let paths: std::collections::BTreeSet<&str> = d.paths().collect();
        paths
            .into_iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let shared: std::collections::BTreeSet<&str> =
        da.paths().filter(|p| db.paths().any(|q| q == *p)).collect();
    if !shared.is_empty() {
        let shared: Vec<String> = shared.into_iter().map(|p| format!("`{p}`")).collect();
        return format!("both allow a common value of {}", shared.join(", "));
    }
    match (list(da), list(db)) {
        (pa, pb) if pa.is_empty() && pb.is_empty() => {
            "neither `when` compares a path the checker can read".to_string()
        }
        (pa, pb) if pa.is_empty() => format!(
            "{}'s condition compares no path the checker can read; {} reads {pb}",
            a.name, b.name
        ),
        (pa, pb) if pb.is_empty() => format!(
            "{}'s condition compares no path the checker can read; {} reads {pa}",
            b.name, a.name
        ),
        (pa, pb) => format!(
            "{} reads {pa} and {} reads {pb}: no path both constrain",
            a.name, b.name
        ),
    }
}

/// What [`reads_only_user`] needs to know about tiers: the relations the
/// beat's document sees, and every project quest's tier (`true` = user).
struct UserTier<'a> {
    relations: &'a BTreeMap<String, lute_manifest::relations::RelationDecl>,
    quests: &'a BTreeMap<&'a str, bool>,
}

/// `when` (already `@def`-expanded) reads at least one piece of state, every
/// one of them user-tier, and calls no function but the CEL operators,
/// `isSet`/`has`, and a query of a user-tier relation. User-tier reads
/// (dsl 0.24.0, T3-4): `user.*`, `entry.<id>.everRead`, `quest.<id>.*` of a
/// quest without `tier="run"`, and `holds` / `count` / `countDistinct` of a
/// relation declared `tier: user`. A run-tier relation or quest, `visited()`,
/// or `now()` may change within a run. `prev.run.*` is the previous run's
/// snapshot, which every run replaces: run history, not user-tier (dsl
/// 0.23.1).
fn reads_only_user(when: &str, tiers: &UserTier<'_>) -> bool {
    use cel_parser::ast::Expr;
    fn walk(expr: &Expr, tiers: &UserTier<'_>, reads: &mut usize) -> bool {
        match expr {
            Expr::Ident(_) | Expr::Select(_) => {
                let user = crate::cel_paths::select_path(expr).is_some_and(|p| {
                    p.starts_with("user.")
                        || crate::cel_paths::is_entry_ever_read(&p)
                        || p.strip_prefix("quest.")
                            .and_then(|rest| rest.split('.').next())
                            .is_some_and(|id| tiers.quests.get(id) == Some(&true))
                });
                *reads += usize::from(user);
                user
            }
            Expr::Call(c)
                if matches!(c.func_name.as_str(), "holds" | "count" | "countDistinct") =>
            {
                let user = c.target.is_none()
                    && matches!(c.args.first().map(|a| &a.expr), Some(Expr::Call(atom))
                        if tiers.relations.get(&atom.func_name)
                            .is_some_and(|r| r.tier.as_deref() == Some("user")));
                *reads += usize::from(user);
                user
            }
            Expr::Call(c) => {
                let operator = !c.func_name.starts_with(|ch: char| ch.is_ascii_alphabetic())
                    || matches!(c.func_name.as_str(), "isSet" | "has");
                c.target.is_none() && operator && c.args.iter().all(|a| walk(&a.expr, tiers, reads))
            }
            Expr::List(l) => l.elements.iter().all(|e| walk(&e.expr, tiers, reads)),
            Expr::Literal(_) => true,
            _ => false,
        }
    }
    let mut arena = lute_cel::CelArena::default();
    let Some(ided) = lute_cel::parse_slot_marked_refs(&mut arena, when).and_then(|h| arena.get(h))
    else {
        return false;
    };
    let mut reads = 0;
    walk(&ided.expr, tiers, &mut reads) && reads > 0
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
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != meta.raw_yaml.len();
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

fn beat_diag(
    code: &str,
    severity: Severity,
    message: String,
    span: Span,
    layer: Layer,
) -> Diagnostic {
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

/// dsl 0.25.0 §2: a `share` key that is no identifier (`what` names the
/// construct: `` `<entry>` `` / `` `<beat>` ``).
pub(crate) fn share_malformed(what: &str, key: &str) -> String {
    format!(
        "{what} `share=\"{key}\"` must be a key — an identifier (`[A-Za-z][A-Za-z0-9_-]*`) every \
         beat standing for the same event writes (dsl 0.25.0 §2)"
    )
}

/// dsl 0.25.0 §2: `share` names a spend, and only a written, spending
/// `once` is one.
pub(crate) fn share_without_once(key: &str) -> String {
    format!(
        "`share` `{key}` without `once`; beats with one `share` key are spent together for their \
         `once` period, so each one writes the same `once` (`run`, `user`, `day` or `slot`) — add \
         it, or remove `share` (dsl 0.25.0 §2)"
    )
}

/// dsl 0.26.0 §8 (T3-3, `lute beats`): per beat of `beats` — one root's
/// [`project_beats`] in selection order (priority descending, then project
/// order) — the index of the earlier beat that always wins where it is
/// eligible: a non-`also` beat on the same `select: first` occasion, a
/// candidate whenever it is (untargeted, or its target), never spent, with
/// no `after:`, whose `when` the later beat's `when` implies (the same
/// condition, or a stronger one on the paths it reads). A fallback so
/// covered never plays while the beat above it stands; unlike
/// [`W_BEAT_SHADOWED`] that is often the design (a lead's fallback for areas
/// that do not answer), so it is a verdict, not a warning. A beat whose
/// coverer has no `when` at all is shadowed instead, and not listed here.
pub fn coverers(beats: &[ProjectBeat<'_>]) -> Vec<Option<usize>> {
    let norm = |w: &str| w.split_whitespace().collect::<Vec<_>>().join(" ");
    let dnfs: Vec<Option<crate::reachability::Dnf>> = beats
        .iter()
        .map(|pb| {
            pb.when.as_deref().map(|w| {
                let defs = DefTable {
                    bodies: &pb.folded.def_bodies,
                    params: &pb.folded.env.def_params,
                };
                crate::reachability::when_dnf(
                    w,
                    &defs,
                    &pb.folded.env.state,
                    Some(&pb.folded.env.rel_vocab),
                )
            })
        })
        .collect();
    beats
        .iter()
        .enumerate()
        .map(|(j, b)| {
            let select = b
                .folded
                .occasions
                .get(b.on)
                .map_or(OccasionSelect::First, |o| o.select);
            if select != OccasionSelect::First || b.also {
                return None;
            }
            let bw = dnfs[j].as_ref()?;
            (0..j).find(|&i| {
                let a = &beats[i];
                let Some(aw) = dnfs[i].as_ref() else {
                    return false;
                };
                !a.also
                    && a.on == b.on
                    && a.cells().covers(b.cells())
                    && a.once == BeatOnce::None
                    && a.after.is_none()
                    && (a.when.as_deref().map(norm) == b.when.as_deref().map(norm)
                        || crate::reachability::implies(bw, aw))
            })
        })
        .collect()
}
