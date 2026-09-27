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
pub const BEAT_KEYS: &[&str] = &[
    "on", "target", "when", "priority", "once", "also", "share", "spentBy", "for",
];

/// A scene beat's repetition policy (dsl 0.21.0 §3.1, D-F).
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// dsl 0.27.0 §5: `once: week` — spent until `clock.weekday` returns to
    /// `week.first` (the next clock week). Needs a clock with a `week:`.
    Week,
    /// dsl 0.27.0 §5: `once: season:<name>` — spent until the season opens
    /// again. Needs a declared season (`E-SEASON-DECL`).
    Season(String),
}

impl BeatOnce {
    /// The IR spelling (dsl 0.21.0 §8, 0.24.0 §1, 0.27.0 §5): `"run"`,
    /// `"user"`, `"none"`, `"day"`, `"slot"`, `"week"` or `"season:<name>"`.
    pub fn as_str(&self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed(match self {
            BeatOnce::Run => "run",
            BeatOnce::User => "user",
            BeatOnce::None => "none",
            BeatOnce::Day => "day",
            BeatOnce::Slot => "slot",
            BeatOnce::Week => "week",
            BeatOnce::Season(name) => {
                return std::borrow::Cow::Owned(format!(
                    "{}{name}",
                    lute_manifest::season::SEASON_PREFIX
                ))
            }
        })
    }

    /// An authored spending `once` value — `run`, `user`, `day`, `slot`,
    /// `week` or `season:<name>` (a well-formed name; whether the season
    /// is declared is `E-SEASON-DECL`'s). `false` / `none` is not one.
    pub fn parse(raw: &str) -> Option<BeatOnce> {
        Some(match raw {
            "run" => BeatOnce::Run,
            "user" => BeatOnce::User,
            "day" => BeatOnce::Day,
            "slot" => BeatOnce::Slot,
            "week" => BeatOnce::Week,
            _ => BeatOnce::Season(
                lute_manifest::season::season_ref(raw)
                    .filter(|n| lute_manifest::season::is_season_name(n))?
                    .to_string(),
            ),
        })
    }

    /// Spent per clock period (`day` / `slot` / `week`) — needs a declared
    /// clock.
    pub fn is_clock(&self) -> bool {
        matches!(self, BeatOnce::Day | BeatOnce::Slot | BeatOnce::Week)
    }

    /// The season a `once: season:<name>` names.
    pub fn season(&self) -> Option<&str> {
        match self {
            BeatOnce::Season(name) => Some(name),
            _ => None,
        }
    }
}

/// The accepted `once` values, for messages.
pub const ONCE_VALUES: &str = "`run` (once per run, the default), `user` (once ever), \
     `day` / `slot` / `week` (once per clock day / slot / week), `season:<name>` (once per \
     window of a declared season), or `false` (repeatable)";

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
    /// dsl 0.27.0 §5: `spentBy:` — instead of `once`, eligible until this
    /// condition holds (the beat is otherwise repeatable, `once: false`).
    pub spent_by: Option<CelSlot>,
    /// dsl 0.27.0 §3 (T2-10): `for: "kind:<kind>"` — as a `<beat for=…>`,
    /// presented once per member; raw text + value span, validated with the
    /// kinds in `check()` ([`crate::occasion_bind::for_kind_members`]).
    pub for_kind: Option<(String, Span)>,
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
        // T3-9: a scene that wrote `occasion:` / `event:` meant `on:` — the
        // unknown key says so, once; "without `on:`" per beat key would
        // repeat it.
        None if get("occasion").or_else(|| get("event")).is_some() => return None,
        None => {
            let mut keys: Vec<&str> = BEAT_KEYS[1..]
                .iter()
                .copied()
                .filter(|key| get(key).is_some())
                .collect();
            keys.sort_by_key(|key| top_key_span(meta, key).byte_start);
            // A scene an unapplied chain (or the retired `sequence:`) lists
            // has one cause for all its keys: said once, at the first.
            if let (Some(u), [first, ..]) = (crate::chapters::unapplied(meta), keys.as_slice()) {
                let (written, belong) = match keys.as_slice() {
                    [one] => (format!("`{one}:`"), format!("`{one}` belongs")),
                    many => {
                        let named: Vec<String> = many.iter().map(|k| format!("`{k}:`")).collect();
                        let (last, init) = named.split_last().expect("two or more keys");
                        (
                            format!("{} and {last}", init.join(", ")),
                            "they belong".to_string(),
                        )
                    }
                };
                push(
                    format!(
                        "{written} without `on:`; {belong} to a beat, and a scene becomes a beat \
                         by naming the occasion it answers — {}",
                        crate::chapters::unapplied_note(u)
                    ),
                    top_key_span(meta, first),
                );
                return None;
            }
            for key in keys {
                push(
                    format!(
                        "`{key}:` without `on:`; `{key}` belongs to a beat, and a scene becomes a \
                         beat by naming the occasion it answers — add `on: <occasion>` or list \
                         the scene in a chain of the project's `chapters:`, or remove `{key}:` \
                         for a scene your engine starts itself"
                    ),
                    top_key_span(meta, key),
                );
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
                    "`target:` must be {TARGET_SHAPE}, or `kind:<entity kind>` for every member \
                     of a kind; got {}",
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
        Some(v) => match v.as_str().and_then(BeatOnce::parse) {
            Some(once) => once,
            None => {
                push(
                    format!(
                        "`once:` must be {ONCE_VALUES}, got {} (dsl 0.21.0 §3.1, 0.24.0 §1, \
                         0.27.0 §5)",
                        describe(v)
                    ),
                    top_value_span(meta, "once"),
                );
                BeatOnce::Run
            }
        },
    };

    // `spentBy:` spends the beat by its condition instead of by being
    // presented: once the condition has held, the beat stays spent for its
    // `once` period (`run` unless written). `once: false` would never keep
    // it spent, which is a `when: "!(…)"`.
    let spent_by = get("spentBy").and_then(|v| match v.as_str() {
        Some(raw) if !raw.trim().is_empty() => {
            if once == BeatOnce::None {
                push(spent_by_once_false(raw), top_value_span(meta, "once"));
            }
            Some(CelSlot::raw(
                CelKind::Condition,
                raw.to_string(),
                top_value_span(meta, "spentBy"),
            ))
        }
        _ => {
            push(
                format!(
                    "`spentBy:` must be a CEL condition string — the beat is spent once it has \
                     held, e.g. `spentBy: \"holds(solved(valves))\"`, got {}",
                    describe(v)
                ),
                top_value_span(meta, "spentBy"),
            );
            None
        }
    });

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
        if spent_by.is_some() {
            push(share_with_spent_by(key), top_key_span(meta, "share"));
        } else if get("once").is_none() || once == BeatOnce::None {
            push(share_without_once(key), top_key_span(meta, "share"));
        }
        Some(key.to_string())
    });

    // dsl 0.27.0 §3 (T2-10): `for: "kind:<kind>"` — its meaning is checked
    // with the kinds (`crate::occasion_bind::check_for_kinds`); only its
    // shape here.
    let for_kind = get("for").and_then(|v| match v.as_str() {
        Some(raw) => Some((raw.to_string(), top_value_span(meta, "for"))),
        None => {
            push(
                format!(
                    "`for:` must be a string naming a kind, `for: \"kind:<kind>\"`, got {} \
                     (dsl 0.27.0 §3)",
                    describe(v)
                ),
                top_value_span(meta, "for"),
            );
            None
        }
    });

    let on = on?;
    let mut occasion_diags = Vec::new();
    check_occasion(
        &on,
        top_value_span(meta, "on"),
        target
            .as_deref()
            .map(|t| (t, top_value_span(meta, "target"))),
        true,
        occasions,
        Layer::Content,
        &mut occasion_diags,
    );
    // dsl 0.28.0 §4: an `on:` the manifest's `chapters:` derived is judged
    // once, at the chain's `on:` (`crate::chapters`), not in every scene;
    // anything else said about it names where it came from.
    if crate::chapters::derived(meta, "on") {
        occasion_diags.retain(|d| d.code != E_OCCASION_UNKNOWN);
        for d in &mut occasion_diags {
            d.message.push_str(crate::chapters::PROVENANCE);
        }
    }
    diags.extend(occasion_diags);
    if also {
        diags.extend(also_fault(
            &on,
            top_value_span(meta, "also"),
            true,
            occasions,
            Layer::Content,
        ));
    }
    Some(BeatMeta {
        on,
        target,
        when,
        priority,
        once,
        // An authored `spentBy` is an authored repetition policy.
        once_authored: get("once").is_some() || spent_by.is_some(),
        also,
        share,
        spent_by,
        for_kind,
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
            let values = if attr.key == "once" {
                format!(": `once=\"…\"` takes {ENTRY_ONCE_VALUES}")
            } else {
                String::new()
            };
            push(
                format!(
                    "`<entry>` attribute `{}` must be a quoted string{values}",
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
        // `once="false"` is the omission's meaning (a repeatable entry beat),
        // as on a scene or bundle beat.
        if raw != "false" && BeatOnce::parse(raw).is_none() {
            push(
                format!("`<entry>` `once=\"{raw}\"` must be {ENTRY_ONCE_VALUES}"),
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
        } else if entry.spent_by.is_some() {
            push(share_with_spent_by(key), *span);
        } else if entry.once.as_ref().is_none_or(|(o, _)| o == "false") && !residual_once {
            push(share_without_once(key), *span);
        }
    }
    if let Some(spent_by) = &entry.spent_by {
        if let Some((_, span)) = entry.once.as_ref().filter(|(o, _)| o == "false") {
            push(spent_by_once_false(spent_by.raw.trim()), *span);
        }
        if entry.on.is_none() && !residual_on {
            push(
                "`<entry>` `spentBy` requires `on`: an entry is spent only as a beat answering an \
                 occasion"
                    .to_string(),
                spent_by.span,
            );
        }
    }
}

/// The accepted `<entry once="…">` values, for messages.
pub const ENTRY_ONCE_VALUES: &str = "`run` (not eligible again this run once read), `user` \
     (never again once read), `day` / `slot` / `week` (not again this clock day / slot / week \
     once read), `season:<name>` (not again this season window), or `false` (repeatable, the \
     same as leaving `once` out)";

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
        check_occasion(
            on,
            *on_span,
            None,
            false,
            occasions,
            Layer::Logic,
            &mut diags,
        );
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
                    malformed_target("`<objective>`", t, false),
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
            Some((t, span)) => Some((t.as_str(), *span)),
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
        check_occasion(
            on,
            *span,
            target_span,
            false,
            occasions,
            Layer::Logic,
            &mut diags,
        );
    }
    diags
}

/// dsl 0.23.0 §3: a side remark rides along a single winner — on a
/// `select: all` / `sequence` occasion every eligible beat is already offered
/// or presented, so `also` means nothing there. The one rule a scene's
/// `also: true`, a bundle `<beat also>`, and a template header's `also: true`
/// share; `yaml` spells the key as frontmatter writes it. `None` for a
/// `select: first` or undeclared occasion.
pub(crate) fn also_fault(
    on: &str,
    span: Span,
    yaml: bool,
    occasions: &BTreeMap<String, OccasionDecl>,
    layer: Layer,
) -> Option<Diagnostic> {
    let decl = occasions
        .get(on)
        .filter(|d| d.select != OccasionSelect::First)?;
    let (written, remove) = if yaml {
        ("also: true", "also:")
    } else {
        ("also", "also")
    };
    Some(beat_diag(
        E_BEAT_ATTR,
        Severity::Error,
        format!(
            "`{written}` applies only to a `select: first` occasion; `{on}` is `select: {}`, \
             which already presents or offers every eligible beat — remove `{remove}` \
             (dsl 0.23.0 §3)",
            decl.select.as_str()
        ),
        span,
        layer,
    ))
}

/// `on` against the resolved occasion vocabulary (dsl 0.21.0 §2): unknown is
/// [`E_OCCASION_UNKNOWN`]; a `target` on an occasion declared without
/// `target: true` is [`E_BEAT_ATTR`]. Silent when no occasion is declared.
/// `target` is the value with its span; `beat`: the target is a beat's
/// (not an objective's), so a `kind:<K>` value there points at `for`,
/// spelled for the layer (frontmatter in [`Layer::Content`]).
pub(crate) fn check_occasion(
    on: &str,
    on_span: Span,
    target: Option<(&str, Span)>,
    beat: bool,
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
    if let (false, Some((value, span))) = (decl.target.takes_target(), target) {
        let sequence = decl.select == lute_manifest::schema::OccasionSelect::Sequence;
        let (key, spelled): (&str, fn(&str) -> String) = if layer == Layer::Content {
            ("target:", |k: &str| format!("for: \"kind:{k}\""))
        } else {
            ("target", |k: &str| format!("for=\"kind:{k}\""))
        };
        let fix = match kind_target(value).filter(|_| beat) {
            Some(kind) if sequence => format!(
                "; to present this beat once for each member of `{kind}`, write `{}` instead \
                 of `{key}`",
                spelled(kind)
            ),
            Some(kind) => format!(
                "; remove `{key}` (`{}` presents a beat once for each member, but only on a \
                 `select: sequence` occasion, and `{on}` is `select: {}`)",
                spelled(kind),
                decl.select.as_str()
            ),
            None => format!("; remove `{key}`"),
        };
        diags.push(beat_diag(
            E_BEAT_ATTR,
            Severity::Error,
            format!(
                "occasion `{on}` is not raised for a target (declared without `target: true`), \
                 so a beat on it cannot restrict itself to one{fix}"
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
             does not declare under `entities:`"
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
                 a member of entity kind `{entity}`{hint}"
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
                 `{prefix}.<{entity}>`"
            )),
        };
    };
    if member.is_some_and(|m| legal.iter().any(|x| x == m)) {
        return Ok(());
    }
    let domain: Vec<String> = legal.iter().map(|m| format!("{prefix}.{m}")).collect();
    let hint = lute_manifest::suggest::nearest(target, domain.iter().map(String::as_str), 2)
        .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
    // A long domain is named by its first few members; the did-you-mean
    // carries the one that matters.
    const SHOWN: usize = 8;
    let mut listed = domain
        .iter()
        .take(SHOWN)
        .map(|t| format!("`{t}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if domain.len() > SHOWN {
        listed.push_str(&format!(", … {} more", domain.len() - SHOWN));
    }
    Err(if subset.is_some() {
        format!(
            "target `{target}` is outside occasion `{on}`'s member list ({listed}), a subset of \
             entity kind `{entity}`{hint}"
        )
    } else {
        format!(
            "target `{target}` is outside occasion `{on}`'s domain `{prefix}.<{entity}>` \
             ({listed}){hint}"
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
            // An entry's `kind:` target on a `select: sequence` occasion
            // raised for no target meant `for=`, as a scene's does
            // ([`check_occasion`]).
            Some(kind)
                if !decl.target.takes_target()
                    && decl.select == lute_manifest::schema::OccasionSelect::Sequence =>
            {
                Err(format!(
                    "occasion `{on}` is not raised for a target (declared without `target: \
                     true`), so `target=\"{target}\"` cannot answer it for each member; to \
                     present this entry once for each member of `{kind}`, write \
                     `for=\"kind:{kind}\"` instead of `target`"
                ))
            }
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
         (`target=\"kind:<kind>\"`) or runs once for each member of one \
         (`for=\"kind:<kind>\"`), where it is the member the beat answers (dsl 0.27.0 §3)"
    )
}

/// G-7: [`occasion_target_scope_message`] for a use of `@def`, whose body
/// (`body`, as declared) reads [`OCCASION_TARGET`].
fn occasion_target_def_scope_message(def: &str, body: &str) -> String {
    format!(
        "`@{def}` reads `{OCCASION_TARGET}` (`{def}: {}`), which has a value only in a beat or \
         entry that targets a kind (`target=\"kind:<kind>\"`) or runs once for each member of \
         one (`for=\"kind:<kind>\"`) — use `@{def}` there, or read something this beat has \
         (dsl 0.27.0 §3)",
        body.trim()
    )
}

/// G-7: the defs of `bodies` whose body reads [`OCCASION_TARGET`], directly
/// or through another def.
pub(crate) fn defs_reading_target(
    bodies: &BTreeMap<String, String>,
) -> std::collections::BTreeSet<String> {
    let mut out: std::collections::BTreeSet<String> = bodies
        .iter()
        .filter(|(_, body)| crate::occasion_bind::mentions_target(body))
        .map(|(name, _)| name.clone())
        .collect();
    loop {
        let more: Vec<String> = bodies
            .iter()
            .filter(|(name, body)| {
                !out.contains(*name)
                    && lute_cel::scan_refs(body)
                        .iter()
                        .any(|r| !r.is_dollar && out.contains(&r.name))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if more.is_empty() {
            return out;
        }
        out.extend(more);
    }
}

/// dsl 0.26.0 §5: the members the document's kind beats answer — the domain
/// of [`OCCASION_TARGET`], sorted, empty without a (well-formed) kind beat
/// ([`crate::occasion_bind::occasion_scopes`], dsl 0.27.0 §3).
pub fn occasion_target_members(
    doc: &Document,
    beat: Option<&BeatMeta>,
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<String> {
    crate::occasion_bind::occasion_scopes(doc, beat, occasions, kinds).members()
}

/// dsl 0.26.0 §5: in a lore document, a read of [`OCCASION_TARGET`] in an
/// entry or bundle beat that does not target a kind is `E-UNDECLARED` (the
/// document declares it for its kind beats; this one is never raised for a
/// member). A scene has one beat, so its declaration is the whole scope.
/// `scoped`: the document has a kind beat — without one, every direct read
/// is already [`crate::cel_resolve::check_cel_slot`]'s (nothing declares
/// the path), so only a def's read is judged here. G-7: a use of a def of
/// `def_bodies` reading it ([`defs_reading_target`]) is judged the same.
pub(crate) fn check_occasion_target_scope(
    doc: &Document,
    def_bodies: &BTreeMap<String, String>,
    scoped: bool,
) -> Vec<Diagnostic> {
    let target_defs = defs_reading_target(def_bodies);
    if !scoped && target_defs.is_empty() {
        return Vec::new();
    }
    let is_kind =
        |t: &Option<(String, Span)>| t.as_ref().is_some_and(|(t, _)| kind_target(t).is_some());
    let outside: Vec<Span> = doc
        .entries
        .iter()
        .filter(|e| !is_kind(&e.target) && !is_kind(&e.for_kind))
        .map(|e| e.span)
        .chain(
            doc.beats
                .iter()
                .filter(|b| !is_kind(&b.target) && !is_kind(&b.for_kind))
                .map(|b| b.span),
        )
        .collect();
    if scoped && outside.len() == doc.entries.len() + doc.beats.len() {
        return Vec::new();
    }
    let within = |s: Span| {
        !scoped
            || outside
                .iter()
                .any(|o| o.byte_start <= s.byte_start && s.byte_end <= o.byte_end)
    };
    // What `raw` reads that has no value here: `occasion.target` itself, or
    // the first def reading it.
    let fault = |raw: &str| -> Option<String> {
        if raw.contains(OCCASION_TARGET) {
            return scoped.then(occasion_target_scope_message);
        }
        lute_cel::scan_refs(raw)
            .into_iter()
            .find(|r| !r.is_dollar && target_defs.contains(&r.name))
            .map(|r| occasion_target_def_scope_message(&r.name, &def_bodies[&r.name]))
    };
    let mut faults: Vec<(Span, String)> = Vec::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        if within(slot.span) {
            faults.extend(fault(&slot.raw).map(|m| (slot.span, m)));
        }
    });
    fn lines<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a lute_syntax::ast::Line)) {
        for n in nodes {
            match n {
                Node::Line(l) => f(l),
                Node::Branch(b) => b.choices.iter().for_each(|c| lines(&c.body, f)),
                Node::Hub(h) => h.bodies().for_each(|b| lines(b, f)),
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
        .filter(|e| !scoped || (!is_kind(&e.target) && !is_kind(&e.for_kind)))
        .map(|e| &e.body)
        .chain(
            doc.beats
                .iter()
                .filter(|b| !scoped || (!is_kind(&b.target) && !is_kind(&b.for_kind)))
                .map(|b| &b.body),
        )
        .chain(doc.shots.iter().filter(|_| !scoped).map(|s| &s.body));
    for body in bodies {
        lines(body, &mut |l| {
            faults.extend(
                l.interps
                    .iter()
                    .filter_map(|i| fault(&i.raw).map(|m| (i.span, m))),
            );
        });
    }
    faults
        .into_iter()
        .map(|(span, message)| {
            beat_diag("E-UNDECLARED", Severity::Error, message, span, Layer::Cel)
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
    /// dsl 0.27.0 §3: the authored `for` (`kind:<kind>`) and, when the
    /// checker accepts it ([`crate::occasion_bind::for_kind_members`]), its
    /// kind and members — the beat is presented once per member.
    pub for_kind: Option<(&'a str, Option<(String, Vec<String>)>)>,
    pub priority: i64,
    /// dsl 0.28.0 §4: a scene's `priority` the manifest's `chapters:`
    /// derived from its place in the chain, not one it wrote.
    pub priority_derived: bool,
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
    /// A `<beat use>` whose own `when=` replaces its template's `when:`
    /// (`W-TEMPLATE-OVERRIDE`): the template's name and that `when:` as its
    /// header writes it.
    pub replaces_when: Option<(&'a str, &'a str)>,
    /// dsl 0.27.0 §5: the `spentBy` condition after `@def` expansion — the
    /// beat is eligible only while it does not hold.
    pub spent_by: Option<String>,
    /// The scene's frontmatter `title:` / the entry's `title=`.
    pub title: Option<String>,
    /// The `on` key / attribute — where a beat diagnostic anchors.
    pub anchor: Span,
    pub folded: &'a FoldedEnv,
    /// The unit the beat presents, as [`crate::cast::FactProducers`] keys
    /// its assert sites: `0` for a scene, else the entry's / bundle beat's
    /// span start.
    pub unit: usize,
    /// The bytes of the beat's declaration in its document — the whole
    /// document for a scene, the element for an entry or bundle beat. An
    /// error reported inside it (or in the document's frontmatter) leaves
    /// the beat out of the selection passes ([`check_project_beats`]).
    pub extent: std::ops::Range<usize>,
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

    /// dsl 0.27.0 (T3-10): both are kind beats and one's members are a
    /// strict subset of the other's — a sub-kind and its parent, which
    /// [`selection_order`] ranks by specificity, never by file order.
    pub fn nested(self, other: BeatCells<'_>) -> bool {
        match (self, other) {
            (BeatCells::Kind(a), BeatCells::Kind(b)) => strict_subset(a, b) || strict_subset(b, a),
            _ => false,
        }
    }
}

fn strict_subset(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() && a.iter().all(|m| b.contains(m))
}

/// dsl 0.26.0 §5, dsl 0.27.0 (T3-10): the selection order of beats given in
/// project order as `(on, priority, kind members)` — the indices, priority
/// descending; at equal priority a beat naming its target (or none) before
/// a kind beat, and a kind beat before every kind beat on its occasion whose
/// members strictly include its own (a sub-kind before its parent: member >
/// sub-kind > kind); otherwise project order. Each beat in turn is placed
/// just before the first placed beat of its occasion, priority and rank that
/// strictly includes it, else last — so the order restricted to one raise's
/// candidates (which hold every beat including a candidate kind beat) is the
/// order of those candidates alone. The one rule `check-project`,
/// `lute beats` and `lute play` rank by.
pub fn selection_order(keys: &[(&str, i64, Option<&[String]>)]) -> Vec<usize> {
    let mut sorted: Vec<usize> = (0..keys.len()).collect();
    sorted.sort_by_key(|&i| (std::cmp::Reverse(keys[i].1), keys[i].2.is_some()));
    let mut out: Vec<usize> = Vec::with_capacity(sorted.len());
    for i in sorted {
        let (on, priority, members) = keys[i];
        let at = members.and_then(|ms| {
            out.iter().position(|&o| {
                let (on2, p2, ms2) = keys[o];
                on2 == on && p2 == priority && ms2.is_some_and(|ms2| strict_subset(ms, ms2))
            })
        });
        match at {
            Some(k) => out.insert(k, i),
            None => out.push(i),
        }
    }
    out
}

/// `items` permuted into `order` (a permutation of its indices).
pub fn reorder<T>(items: Vec<T>, order: &[usize]) -> Vec<T> {
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order.iter().filter_map(|&i| slots[i].take()).collect()
}

/// [`project_beats`] (project order) in [`selection_order`].
pub fn in_selection_order(beats: Vec<ProjectBeat<'_>>) -> Vec<ProjectBeat<'_>> {
    let order = selection_order(
        &beats
            .iter()
            .map(|b| (b.on, b.priority, b.kind_targets.as_deref()))
            .collect::<Vec<_>>(),
    );
    reorder(beats, &order)
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
        let for_cell = |on: &str, raw: Option<&'a str>, has_target: bool| {
            raw.map(|raw| {
                let kind = crate::occasion_bind::for_kind_members(
                    on,
                    raw,
                    has_target,
                    &folded.occasions,
                    &folded.env.rel_vocab.kinds,
                );
                (raw, kind.ok())
            })
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
                for_kind: for_cell(
                    &beat.on,
                    beat.for_kind.as_ref().map(|(f, _)| f.as_str()),
                    beat.target.is_some(),
                ),
                priority: beat.priority,
                priority_derived: crate::chapters::derived(&doc.meta, "priority"),
                once: beat.once.clone(),
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
                replaces_when: None,
                spent_by: expand(beat.spent_by.as_ref()),
                title,
                anchor: top_key_span(&doc.meta, "on"),
                folded,
                unit: 0,
                extent: 0..usize::MAX,
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
                // A `spentBy` entry stays spent for its `once` period, `run`
                // unless written.
                None if entry.spent_by.is_some() => BeatOnce::Run,
                None | Some("false") => BeatOnce::None,
                Some(raw) => match BeatOnce::parse(raw) {
                    Some(once) => once,
                    None => continue,
                },
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
                    for_kind: for_cell(
                        on,
                        entry.for_kind.as_ref().map(|(f, _)| f.as_str()),
                        entry.target.is_some(),
                    ),
                    priority,
                    priority_derived: false,
                    once_authored: entry.once.is_some() || entry.spent_by.is_some(),
                    also: false,
                    after: None,
                    share: well_formed_share(entry.share.as_ref())
                        .filter(|_| once != BeatOnce::None),
                    once,
                    when_slot: entry.when.as_ref(),
                    when: expand(entry.when.as_ref()),
                    replaces_when: None,
                    spent_by: expand(entry.spent_by.as_ref()),
                    title: entry.title.as_ref().map(|(t, _)| t.clone()),
                    anchor: *on_span,
                    folded,
                    unit: entry.span.byte_start,
                    extent: entry.span.byte_start..entry.span.byte_end,
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
                    // A use of a faulty template derives only what is sound;
                    // the header's one report stands for the beat.
                    || beat.template.as_ref().is_some_and(|t| t.failed)
                    || beat
                        .target
                        .as_ref()
                        .is_some_and(|(t, _)| !is_beat_target(t))
                    || beat
                        .priority
                        .as_ref()
                        .is_some_and(|(p, _)| parse_beat_priority(p).is_none())
                    || beat
                        .once
                        .as_ref()
                        .is_some_and(|(o, _)| o != "false" && BeatOnce::parse(o).is_none())
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
                        for_kind: for_cell(
                            on,
                            beat.for_kind.as_ref().map(|(f, _)| f.as_str()),
                            beat.target.is_some(),
                        ),
                        priority: crate::bundles::bundle_beat_priority(beat),
                        priority_derived: false,
                        once: crate::bundles::bundle_beat_once(beat),
                        once_authored: beat.once.is_some() || beat.spent_by.is_some(),
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
                        replaces_when: beat
                            .template
                            .as_ref()
                            .and_then(|t| t.replaced_when.as_deref().map(|w| (t.name.as_str(), w))),
                        spent_by: expand(beat.spent_by.as_ref()),
                        title: beat.title.as_ref().map(|(t, _)| t.clone()),
                        anchor: *on_span,
                        folded,
                        unit: beat.span.byte_start,
                        extent: beat.span.byte_start..beat.span.byte_end,
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
    /// [`ProjectBeat::priority_derived`].
    priority_derived: bool,
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
    /// The eligibility the author wrote — `when`, `once`, `spentBy` and
    /// `after:`, without the facts the beat's own presentation asserts — and
    /// its DNF when that differs from [`Self::dnf`]: what a tie message
    /// explains, since only these are the author's to change.
    stated: Option<String>,
    stated_dnf: Option<crate::reachability::Dnf>,
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
    /// dsl 0.27.0 §4: provably never presented — its `when` alone, or under
    /// its occasion's gate and `!terminal`, is false (a finite clock's range
    /// included). Such a beat ties with nothing.
    never: bool,
    folded: &'a FoldedEnv,
}

impl Beat<'_> {
    fn cells(&self) -> BeatCells<'_> {
        BeatCells::of(self.target, self.kind_targets.as_deref())
    }

    /// [`Beat::stated`] in DNF.
    fn stated_dnf(&self) -> &crate::reachability::Dnf {
        self.stated_dnf.as_ref().unwrap_or(&self.dnf)
    }
}

/// `W-BEAT-PRIORITY-TIE` (dsl 0.22.0 §13): two beats on one `select: first`
/// occasion whose selection falls to file order.
pub const W_BEAT_PRIORITY_TIE: &str = "W-BEAT-PRIORITY-TIE";
/// `W-BEAT-ONCE-RUN-USER` (dsl 0.22.0 §13 advisory, dsl 0.23.1): a beat
/// spent once per run by DEFAULT whose `when` reads only user-tier state, so
/// it replays every run. An authored `once: run` silences it.
pub const W_BEAT_ONCE_RUN_USER: &str = "W-BEAT-ONCE-RUN-USER";

/// The error-severity diagnostics the per-file `check()` reported, as byte
/// ranges per document — what [`check_project_beats`] leaves out.
pub type ReportedErrors = BTreeMap<PathBuf, Vec<std::ops::Range<usize>>>;

/// Whether an error sits inside `pb`'s declaration ([`ProjectBeat::extent`])
/// or its document's frontmatter.
fn reported_in(
    pb: &ProjectBeat<'_>,
    docs: &[(PathBuf, Document)],
    errors: &ReportedErrors,
) -> bool {
    let Some(spans) = errors.get(pb.path) else {
        return false;
    };
    let meta = docs
        .iter()
        .find(|(p, _)| p == pb.path)
        .map(|(_, d)| d.meta.span.byte_start..d.meta.span.byte_end);
    spans.iter().any(|s| {
        pb.extent.contains(&s.start) || meta.as_ref().is_some_and(|m| m.contains(&s.start))
    })
}

/// The project beat passes over one resolved project root (dsl 0.21.0 §4,
/// §5; dsl 0.22.0 §13). `docs` and `foldeds` are parallel and in
/// `check-project` order, which is the `ProjectIndex.beats` tiebreak order
/// (documents in order, declaration order within). Beats are in
/// [`selection_order`]: priority descending, member > sub-kind > kind, then
/// that order.
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
///   what the `once` requires ([`once_guard`]) AND (dsl 0.27.0, T3-11) the
///   `visited(…)` its `after:` requires ([`after_premise`]), and a
///   `holds(A)` conjunct of a pure-schedule derived atom contributes its
///   rules' `cel()` guards. File order then picks the winner. dsl 0.27.0
///   (T3-11): one warning per group of beats tying one another
///   ([`tie_warnings`]), naming each and each distinct reason once.
/// - [`W_BEAT_ONCE_RUN_USER`]: a beat whose `once: run` is DEFAULTED (not
///   written — dsl 0.23.1) and whose `when` reads state, all of it user-tier
///   ([`reads_only_user`]: `user.*`, `entry.<id>.everRead`, a user-tier
///   quest's `quest.<id>.*`, a `tier: user` relation's `holds`/`count`;
///   `prev.run.*` is run history, not user-tier): once true it stays true
///   across runs, so the beat plays again at the start of every run.
///
/// A beat with an error inside its declaration or its document's
/// frontmatter (`errors`, what the per-file `check()` reported) is left out
/// of every pass above: that error is the one report about it, and a
/// ranking of a beat the checker rejected would be judged on text the author
/// is about to change.
pub fn check_project_beats(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    producers: &crate::cast::FactProducers,
    env: Option<&FactEnv>,
    errors: &ReportedErrors,
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
                // dsl 0.27.0 §5: a `season:<name>` quest resets like a run one.
                !q.tier
                    .as_ref()
                    .is_some_and(|(t, _)| t == "run" || t.starts_with("season:")),
            )
        })
        .collect();
    let mut pbs = project_beats(docs, foldeds);
    let share_diags = check_share_once(&pbs);
    pbs.retain(|pb| !reported_in(pb, docs, errors));
    let groups = share_groups(&pbs);
    let guards: Vec<Option<String>> = pbs.iter().map(|pb| once_guard(pb, &pbs, &groups)).collect();
    // Visited key → `after:` text, for the tie check's after-closure (G-1);
    // a key two beats share is ambiguous and left out.
    let mut afters: BTreeMap<String, Option<&str>> = BTreeMap::new();
    for pb in pbs.iter().filter(|pb| pb.kind != ProjectBeatKind::Entry) {
        afters
            .entry(pb.id.clone())
            .and_modify(|a| *a = None)
            .or_insert(pb.after);
    }
    let afters: BTreeMap<String, &str> = afters
        .into_iter()
        .filter_map(|(k, a)| Some((k, a?)))
        .collect();
    let order = selection_order(
        &pbs.iter()
            .map(|b| (b.on, b.priority, b.kind_targets.as_deref()))
            .collect::<Vec<_>>(),
    );
    let beats: Vec<Beat<'_>> = pbs
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
            for f in crate::cast::unit_facts(
                producers,
                &folded.env.rel_vocab,
                pb.path,
                pb.unit,
                // A `spentBy` beat is not spent by being presented.
                &if pb.spent_by.is_some() {
                    BeatOnce::None
                } else {
                    pb.once.clone()
                },
            ) {
                match f.persists {
                    None => absent.push(format!("!{}", f.query)),
                    Some(why) => persists.push((f.query.replace(", ", ","), why)),
                }
            }
            // dsl 0.27.0 (T3-11): what the beat's `after:` requires — a
            // `once: user` beat `X` and one waiting on `visited('X')` are
            // never eligible together.
            let after = pb.after.and_then(|a| after_premise(a, &afters));
            let stated = pb
                .when
                .as_deref()
                .map(|w| format!("({w})"))
                .into_iter()
                .chain(guard)
                // dsl 0.27.0 §5: eligible only while `spentBy` does not hold.
                .chain(pb.spent_by.as_deref().map(|s| format!("!({s})")))
                .chain(after)
                .reduce(|acc, c| format!("{acc} && {c}"));
            let eligible = stated
                .iter()
                .cloned()
                .chain(absent)
                .reduce(|acc, c| format!("{acc} && {c}"));
            let user_tier = UserTier {
                relations: &folded.env.rel_vocab.relations,
                quests: &quest_tiers,
            };
            let never = crate::gates::beat_never_eligible(
                folded,
                pb.on,
                pb.target,
                pb.when_slot.map(|w| w.raw.as_str()),
                |c| {
                    let span = pb.when_slot.map_or(pb.anchor, |w| w.span);
                    crate::gates::provably_false(
                        c,
                        folded,
                        env.map(|e| (e, pb.path.as_path(), span)),
                    )
                },
            );
            let dnf_of = |e: Option<&str>| {
                e.map_or_else(Default::default, |e| {
                    crate::reachability::when_dnf(
                        e,
                        &defs,
                        &folded.env.state,
                        Some(&folded.env.rel_vocab),
                    )
                })
            };
            let dnf = dnf_of(eligible.as_deref());
            Beat {
                path: pb.path,
                name: pb.name(),
                on: pb.on,
                target: pb.target,
                kind_targets: pb.kind_targets,
                priority: pb.priority,
                priority_derived: pb.priority_derived,
                once: pb.once.clone(),
                also: pb.also,
                always: pb.after.is_none() && holds && pb.spent_by.is_none(),
                unspent: pb.once == BeatOnce::None && pb.spent_by.is_none(),
                run_once_user_when: pb.once == BeatOnce::Run
                    && !pb.once_authored
                    && pb
                        .when
                        .as_deref()
                        .is_some_and(|w| reads_only_user(w, &user_tier)),
                stated_dnf: (stated != eligible).then(|| dnf_of(stated.as_deref())),
                dnf,
                stated,
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
                never,
            }
        })
        .collect();
    // dsl 0.26.0 §5, dsl 0.27.0 (T3-10): member > sub-kind > kind at equal
    // priority, then the tiebreak order.
    let beats = reorder(beats, &order);

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
    let mut ties: Vec<(usize, usize)> = Vec::new();
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
        for (i, a) in beats[..j].iter().enumerate() {
            if !a.also
                && a.on == b.on
                && a.priority == b.priority
                && a.cells().meets(b.cells())
                // dsl 0.26.0 §5: at equal priority the beat naming the
                // member (or none) outranks a kind beat, and (dsl 0.27.0,
                // T3-10) a sub-kind beat its parent's — no file order.
                && a.cells().is_kind() == b.cells().is_kind()
                && !a.cells().nested(b.cells())
                // dsl 0.27.0 §4: a beat that is never presented ties with
                // nothing (its unreachable verdict says why).
                && !a.never
                && !b.never
                && !provably_exclusive(a, b, env)
            {
                ties.push((i, j));
            }
        }
    }
    out.extend(tie_warnings(&beats, &ties));
    out
}

/// dsl 0.27.0 (T3-11): one [`W_BEAT_PRIORITY_TIE`] per group of beats that
/// tie with one another (`ties`, index pairs into `beats`, joined
/// transitively), anchored at the group's first beat in selection order:
/// every beat of the group named once, and each distinct reason two of them
/// are not exclusive said once.
fn tie_warnings(beats: &[Beat<'_>], ties: &[(usize, usize)]) -> Vec<(PathBuf, Diagnostic)> {
    let mut root: Vec<usize> = (0..beats.len()).collect();
    fn find(root: &mut [usize], mut x: usize) -> usize {
        while root[x] != x {
            root[x] = root[root[x]];
            x = root[x];
        }
        x
    }
    for &(a, b) in ties {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        root[ra.max(rb)] = ra.min(rb);
    }
    let mut groups: BTreeMap<usize, (Vec<usize>, Vec<(usize, usize)>)> = BTreeMap::new();
    for &(a, b) in ties {
        let g = groups.entry(find(&mut root, a)).or_default();
        for x in [a, b] {
            if !g.0.contains(&x) {
                g.0.push(x);
            }
        }
        g.1.push((a, b));
    }
    let mut out = Vec::new();
    for (_, (mut members, pairs)) in groups {
        members.sort_unstable();
        let first = &beats[members[0]];
        let names: Vec<&str> = members.iter().map(|&m| beats[m].name.as_str()).collect();
        let target = match first.target {
            Some(t) if members.iter().all(|&m| beats[m].target == Some(t)) => {
                format!(" for `{t}`")
            }
            _ => String::new(),
        };
        // Each distinct reason once; the beats with no condition at all
        // said together.
        let mut reasons: Vec<String> = Vec::new();
        let mut no_when = false;
        for &(a, b) in &pairs {
            match why_not_exclusive(&beats[a], &beats[b]) {
                Why::NoWhen => no_when = true,
                Why::Other(r) => {
                    if !reasons.contains(&r) {
                        reasons.push(r);
                    }
                }
            }
        }
        if no_when {
            let unconstrained: Vec<&Beat<'_>> = members
                .iter()
                .map(|&m| &beats[m])
                .filter(|x| x.stated.is_none())
                .collect();
            // A scene's or bundle beat's `once: run` sets no flag a `when`
            // can read.
            let run = unconstrained
                .iter()
                .any(|x| x.once == BeatOnce::Run && !x.name.starts_with("entry"));
            let reason = match unconstrained.as_slice() {
                [x] => format!(
                    "{} has no `when`{}",
                    x.name,
                    if run {
                        ", and its `once: run` sets no flag a `when` can read"
                    } else {
                        ""
                    }
                ),
                xs => format!(
                    "{}{}",
                    if xs.len() == members.len() {
                        "none of them has a `when`".to_string()
                    } else {
                        let ns: Vec<&str> = xs.iter().map(|x| x.name.as_str()).collect();
                        format!("{} have no `when`", and_list(&ns))
                    },
                    if run {
                        ", and `once: run` sets no flag a `when` can read"
                    } else {
                        ""
                    }
                ),
            };
            reasons.insert(0, reason);
        }
        // dsl 0.28.0 §4 (T3-18): a priority `chapters:` derived is not in
        // the scene's file — say where it comes from, and fix the others.
        let (mut derived, mut written): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
        for b in members.iter().map(|&m| &beats[m]) {
            if b.priority_derived {
                derived.push(&b.name);
            } else {
                written.push(&b.name);
            }
        }
        let provenance = match derived.as_slice() {
            [] => String::new(),
            [one] => format!(
                " ({one}'s is written by `chapters:` in lute.project.yaml, from its place in its \
                 chain)"
            ),
            many => format!(
                " (the priority of {} is written by `chapters:` in lute.project.yaml, from their \
                 places in their chains)",
                and_list(many)
            ),
        };
        let fix = match (derived.as_slice(), written.as_slice()) {
            ([], _) if members.len() == 2 => "give one a different `priority`".to_string(),
            ([], _) => "give them different priorities".to_string(),
            (_, []) => "write a `priority:` of its own in one of the scenes".to_string(),
            (_, [one]) => format!("give {one} a different `priority`"),
            (_, many) => format!("give {} different priorities", and_list(many)),
        };
        out.push((
            first.path.clone(),
            beat_diag(
                W_BEAT_PRIORITY_TIE,
                Severity::Warning,
                format!(
                    "{} share priority {}{provenance} on occasion `{}`{target} and can be \
                     eligible at once, so file order picks the winner (today {}, and renaming or \
                     moving a file changes it) — {fix}, or make their `when`s exclusive; they \
                     are not provably exclusive because {} (dsl 0.22.0 §13)",
                    and_list(&names),
                    first.priority,
                    first.on,
                    first.name,
                    reasons.join("; "),
                ),
                first.anchor,
                Layer::Logic,
            ),
        ));
    }
    out
}

/// `a`, `b` and `c`.
fn and_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => one.to_string(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
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
    match (pb.kind, &pb.once) {
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
    let pbs = in_selection_order(project_beats(docs, foldeds));
    let groups = share_groups(&pbs);
    let spent_conds: Vec<Option<String>> = pbs
        .iter()
        .map(|pb| spent_condition(pb, &pbs, &groups))
        .collect();
    let beats: Vec<(ProjectBeat<'_>, bool, Option<String>)> = pbs
        .into_iter()
        .zip(spent_conds)
        .map(|(pb, spent)| {
            let always = always_eligible(&pb);
            (pb, always, spent)
        })
        .collect();
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

/// A beat that is always eligible: no `after:`, no `spentBy` (dsl 0.27.0 §5:
/// it drops out once its condition holds), and a `when` absent or deciding
/// true without facts.
pub fn always_eligible(pb: &ProjectBeat<'_>) -> bool {
    let params = BTreeMap::new();
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
    pb.after.is_none()
        && pb.spent_by.is_none()
        && pb
            .when_slot
            .is_none_or(|w| matches!(decide_slot(&w.raw, &defs, &ctx), Some(Decided::Bool(true))))
}

/// dsl 0.27.0 (T3-9, `lute beats`): the verdict of one ladder cell — the
/// earlier beats (indices into `beats`, one root's beats in
/// [`selection_order`]) that win every time `beats[j]` could on the
/// occasion raised for each of `targets` (empty: raised without a target):
/// for every target, the first earlier non-`also` beat answering it that is
/// [`always_eligible`] (`always`, parallel to `beats`) and never spent.
/// `None` unless every target has one, or on a non-`select: first`
/// occasion, or for an `also` beat. Where [`W_BEAT_SHADOWED`] is
/// project-wide (every target), this is per ladder.
pub fn shadowers_at(
    beats: &[ProjectBeat<'_>],
    always: &[bool],
    j: usize,
    targets: &[&str],
) -> Option<Vec<usize>> {
    let b = &beats[j];
    let select = b
        .folded
        .occasions
        .get(b.on)
        .map_or(OccasionSelect::First, |o| o.select);
    if select != OccasionSelect::First || b.also {
        return None;
    }
    let wins = |i: usize, answers: &dyn Fn(BeatCells<'_>) -> bool| {
        let a = &beats[i];
        !a.also && a.on == b.on && always[i] && a.once == BeatOnce::None && answers(a.cells())
    };
    let mut out: Vec<usize> = Vec::new();
    let firsts: Vec<Option<usize>> = if targets.is_empty() {
        vec![(0..j).find(|&i| wins(i, &|c| matches!(c, BeatCells::Any)))]
    } else {
        targets
            .iter()
            .map(|t| (0..j).find(|&i| wins(i, &|c| c.answers(t))))
            .collect()
    };
    for i in firsts {
        let i = i?;
        if !out.contains(&i) {
            out.push(i);
        }
    }
    Some(out)
}

/// dsl 0.27.0 (T3-11): what an `after:` requires, as a condition the tie
/// check conjoins — each `visited('<id>')` it needs (a `once: user` beat's
/// own guard is `!visited('<id>')`), and, since a beat is visited only once
/// its own `after:` held, what that beat's `after:` required in turn (G-1:
/// `r3` after `visited(r2)`, `r2` after `visited(r1)` ⇒ `visited(r1)`),
/// through `afters` (visited key → `after:` text). `completed` / `active`
/// read no flag the eligibility has, so they drop out (weakening, never
/// strengthening: an `||` with a dropped side drops whole). `None` when
/// nothing remains or the value is out of profile.
fn after_premise(raw: &str, afters: &BTreeMap<String, &str>) -> Option<String> {
    use crate::prereq::PrereqFormula as F;
    fn cel(f: &F, afters: &BTreeMap<String, &str>, seen: &mut Vec<String>) -> Option<String> {
        match f {
            F::Visited(k) => {
                let own = format!("visited('{k}')");
                if seen.contains(k) {
                    return Some(own);
                }
                seen.push(k.clone());
                let before = afters
                    .get(k.as_str())
                    .and_then(|raw| crate::prereq::parse_prereq(raw, bare_span(0, 0)).0)
                    .and_then(|f| cel(&f, afters, seen));
                seen.pop();
                Some(match before {
                    Some(b) => format!("{own} && {b}"),
                    None => own,
                })
            }
            F::Completed(_) | F::Active(_) => None,
            F::And(a, b) => match (cel(a, afters, seen), cel(b, afters, seen)) {
                (Some(a), Some(b)) => Some(format!("{a} && {b}")),
                (a, b) => a.or(b),
            },
            F::Or(a, b) => Some(format!(
                "(({}) || ({}))",
                cel(a, afters, seen)?,
                cel(b, afters, seen)?
            )),
        }
    }
    if raw.trim().is_empty() {
        return None;
    }
    let span = bare_span(0, 0);
    cel(
        &crate::prereq::parse_prereq(raw, span).0?,
        afters,
        &mut Vec::new(),
    )
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

/// Why two beats are not [`provably_exclusive`] ([`why_not_exclusive`]).
enum Why {
    /// One of the two constrains nothing (its eligibility is unconditional).
    NoWhen,
    /// Any other reason, as a clause.
    Other(String),
}

/// dsl 0.26.0 §8 (T3-2): why `a` and `b` are not [`provably_exclusive`]:
/// a beat that constrains nothing, a flag that outlives a `once: run`
/// spend, a fact one asserts that may hold before it plays, or the paths
/// the two alternatives that overlap constrain.
fn why_not_exclusive(a: &Beat<'_>, b: &Beat<'_>) -> Why {
    let Some((da, db)) = crate::reachability::non_exclusive_witness(a.stated_dnf(), b.stated_dnf())
    else {
        return Why::Other("their conjunction does not decide `false`".to_string());
    };
    for (x, y, dy) in [(a, b, db), (b, a, da)] {
        if x.once == BeatOnce::Run && dy.requires_true(&x.ever_flag) {
            return Why::Other(format!(
                "`{}` persists across runs; `once: run` does not, so in a later run both are \
                 eligible",
                x.ever_flag
            ));
        }
        if let Some((fact, why)) = x.persists.iter().find(|(f, _)| dy.requires_true(f)) {
            return Why::Other(format!(
                "{} reads `{fact}`, which {} asserts, but {why}",
                y.name, x.name
            ));
        }
    }
    if a.stated.is_none() || b.stated.is_none() {
        return Why::NoWhen;
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
        return Why::Other(format!(
            "both allow a common value of {}",
            shared.join(", ")
        ));
    }
    Why::Other(match (list(da), list(db)) {
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
    })
}

/// What [`read_tier`] needs to know about tiers: the relations the reading
/// document sees, and the quests whose tier is known (`true` = user, `false`
/// = run; an absent quest's tier is unknown).
pub(crate) struct UserTier<'a> {
    pub(crate) relations: &'a BTreeMap<String, lute_manifest::relations::RelationDecl>,
    pub(crate) quests: &'a BTreeMap<&'a str, bool>,
}

/// The lifetime tier of one state read (dsl 0.23.0 §6, dsl 0.24.0 T3-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadTier {
    /// Reset by every new run: `run.*`, a `tier="run"` quest's
    /// `quest.<id>.*`, a query of a relation without `tier:` or `tier: run`.
    Run,
    /// Kept across runs: `user.*`, `entry.<id>.everRead`, a user-tier
    /// quest's `quest.<id>.*`, a query of a `tier: user` relation.
    User,
    /// Kept across users: `app.*`, a query of a `tier: app` relation.
    App,
    /// Anything else — `scene.*`, `prev.run.*` (run history, replaced every
    /// run: dsl 0.23.1), a quest of unknown tier, a local variable.
    Other,
}

/// The tier `expr` reads when it is a read leaf — a state path, or a
/// `holds` / `count` / `countDistinct` query — else `None`.
pub(crate) fn read_tier(expr: &cel_parser::ast::Expr, tiers: &UserTier<'_>) -> Option<ReadTier> {
    use cel_parser::ast::Expr;
    match expr {
        Expr::Ident(_) | Expr::Select(_) => {
            let Some(p) = crate::cel_paths::select_path(expr) else {
                return Some(ReadTier::Other);
            };
            Some(
                if p.starts_with("user.") || crate::cel_paths::is_entry_ever_read(&p) {
                    ReadTier::User
                } else if p.starts_with("app.") {
                    ReadTier::App
                } else if p.starts_with("run.") {
                    ReadTier::Run
                } else {
                    match p
                        .strip_prefix("quest.")
                        .and_then(|rest| rest.split('.').next())
                        .and_then(|id| tiers.quests.get(id))
                    {
                        Some(true) => ReadTier::User,
                        Some(false) => ReadTier::Run,
                        None => ReadTier::Other,
                    }
                },
            )
        }
        Expr::Call(c) if matches!(c.func_name.as_str(), "holds" | "count" | "countDistinct") => {
            let relation = match c.args.first().map(|a| &a.expr) {
                Some(Expr::Call(atom)) if c.target.is_none() => {
                    tiers.relations.get(&atom.func_name)
                }
                _ => None,
            };
            Some(match relation.map(|r| r.tier.as_deref()) {
                Some(None | Some("run")) => ReadTier::Run,
                Some(Some("user")) => ReadTier::User,
                Some(Some("app")) => ReadTier::App,
                _ => ReadTier::Other,
            })
        }
        _ => None,
    }
}

/// Every read tier `expr` touches, in walk order ([`read_tier`] at each
/// leaf, recursing through every other sub-expression).
pub(crate) fn read_tiers(
    expr: &cel_parser::ast::Expr,
    tiers: &UserTier<'_>,
    out: &mut Vec<ReadTier>,
) {
    use cel_parser::ast::{EntryExpr, Expr};
    if let Some(t) = read_tier(expr, tiers) {
        out.push(t);
        return;
    }
    match expr {
        Expr::Call(c) => {
            for e in c
                .target
                .iter()
                .map(|t| &t.expr)
                .chain(c.args.iter().map(|a| &a.expr))
            {
                read_tiers(e, tiers, out);
            }
        }
        Expr::List(l) => l
            .elements
            .iter()
            .for_each(|e| read_tiers(&e.expr, tiers, out)),
        Expr::Map(m) => m.entries.iter().for_each(|e| match &e.expr {
            EntryExpr::MapEntry(m) => {
                read_tiers(&m.key.expr, tiers, out);
                read_tiers(&m.value.expr, tiers, out);
            }
            EntryExpr::StructField(f) => read_tiers(&f.value.expr, tiers, out),
        }),
        Expr::Struct(s) => s.entries.iter().for_each(|e| match &e.expr {
            EntryExpr::MapEntry(m) => {
                read_tiers(&m.key.expr, tiers, out);
                read_tiers(&m.value.expr, tiers, out);
            }
            EntryExpr::StructField(f) => read_tiers(&f.value.expr, tiers, out),
        }),
        Expr::Comprehension(c) => {
            for e in [
                &c.iter_range,
                &c.accu_init,
                &c.loop_cond,
                &c.loop_step,
                &c.result,
            ] {
                read_tiers(&e.expr, tiers, out);
            }
        }
        _ => {}
    }
}

/// `when` (already `@def`-expanded) reads at least one piece of state, every
/// one of them user-tier ([`ReadTier::User`]), and calls no function but the
/// CEL operators, `isSet`/`has`, and a query of a user-tier relation. A
/// run-tier relation or quest, `visited()`, or `now()` may change within a
/// run. `prev.run.*` is the previous run's snapshot, which every run
/// replaces: run history, not user-tier (dsl 0.23.1).
fn reads_only_user(when: &str, tiers: &UserTier<'_>) -> bool {
    use cel_parser::ast::Expr;
    fn walk(expr: &Expr, tiers: &UserTier<'_>, reads: &mut usize) -> bool {
        if let Some(tier) = read_tier(expr, tiers) {
            let user = tier == ReadTier::User;
            *reads += usize::from(user);
            return user;
        }
        match expr {
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

/// The YAML value an author wrote, for the "got …" half of a message. A
/// quoted `"false"` / `"10"` is a string where a bool or number is meant:
/// the message says so instead of printing `false` against `false`.
fn describe(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s)
            if s == "true" || s == "false" || s.trim().parse::<f64>().is_ok() =>
        {
            format!("the quoted string `\"{s}\"` — write it unquoted, `{s}`")
        }
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

/// The TOP-LEVEL (unindented) `key:` line of the AUTHORED frontmatter:
/// `(line start offset in raw_yaml, the text after the colon)`. A nested key
/// of the same name (`extra: { on: … }`) never matches, and neither does a
/// key a `chapters:` chain derived — it has no text in the file (T3-18).
fn top_key_line<'m>(meta: &'m Meta, key: &str) -> Option<(usize, &'m str)> {
    let mut line_start = 0usize;
    for line in crate::chapters::authored_yaml(&meta.raw_yaml).split_inclusive('\n') {
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

/// A top-level frontmatter key's inline value text (unquoted) and its
/// [`top_value_span`]; `None` for an absent key or a block value.
pub(crate) fn top_value_text<'m>(meta: &'m Meta, key: &str) -> Option<(&'m str, Span)> {
    let (at, _) = top_key_line(meta, key)?;
    let span = top_value_span(meta, key);
    let base = interior_base(meta);
    if span.byte_start == base + at {
        return None;
    }
    let text = meta
        .raw_yaml
        .get(span.byte_start.checked_sub(base)?..span.byte_end.checked_sub(base)?)?;
    Some((text, span))
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

/// What a target looks like, for messages: the shape `is_entry_target`
/// accepts, in words.
pub(crate) const TARGET_SHAPE: &str = "a dotted id such as `npc.maud` — an identifier, then one \
     or more `.`-separated parts of letters, digits, `_` or `-`";

/// Why `t` is no target of `what` (`` `<objective>` ``, `` `<beat>` `` …).
/// `kind_ok`: `what` also takes `kind:<entity kind>` (beats and entries);
/// one that takes a single member (an objective, an `<on>` handler) says so
/// when handed a kind.
pub(crate) fn malformed_target(what: &str, t: &str, kind_ok: bool) -> String {
    if !kind_ok && t.starts_with("kind:") {
        return format!(
            "{what} `target=\"{t}\"` names a kind, but {what} takes one target — a single member \
             such as `npc.maud`; kind targets are for beats and entries"
        );
    }
    let kind = if kind_ok {
        ", or `kind:<entity kind>` for every member of a kind"
    } else {
        ""
    };
    format!("{what} `target=\"{t}\"` must be {TARGET_SHAPE}{kind}")
}

/// dsl 0.25.0 §2: a `share` key that is no identifier (`what` names the
/// construct: `` `<entry>` `` / `` `<beat>` ``).
pub(crate) fn share_malformed(what: &str, key: &str) -> String {
    format!(
        "{what} `share=\"{key}\"` must be a key — an identifier (`[A-Za-z][A-Za-z0-9_-]*`) every \
         beat standing for the same event writes (dsl 0.25.0 §2)"
    )
}

/// The spending `once` periods — every value [`BeatOnce::parse`] accepts —
/// in the order messages list them.
pub const SPENDING_ONCE: [&str; 6] = ["run", "user", "day", "slot", "week", "season:<name>"];

/// dsl 0.25.0 §2: `share` names a spend, and only a written, spending
/// `once` is one.
pub(crate) fn share_without_once(key: &str) -> String {
    let periods: Vec<String> = SPENDING_ONCE.iter().map(|p| format!("`{p}`")).collect();
    let (last, init) = periods.split_last().expect("SPENDING_ONCE is not empty");
    format!(
        "`share` `{key}` without `once`: a `share` key spends its beats together when one is \
         presented, for their common `once` period, so each beat of the key writes the same \
         `once` ({} or {last}) — add it, or remove `share`",
        init.join(", ")
    )
}

/// A `share` key beside `spentBy`: `share` spends its beats together when
/// one is presented, and a `spentBy` beat is never spent by presenting it.
pub(crate) fn share_with_spent_by(key: &str) -> String {
    format!(
        "`share` `{key}` beside `spentBy`: a `share` key spends its beats together when one is \
         presented, but a `spentBy` beat is spent by its condition, not by being presented — \
         remove `share`, or give each beat of the key the same `spentBy`"
    )
}

/// `once: false` beside `spentBy`: a `spentBy` beat stays spent for its
/// `once` period once the condition has held, and `false` is no period.
pub(crate) fn spent_by_once_false(raw: &str) -> String {
    format!(
        "`once: false` beside `spentBy`: a `spentBy` beat stays spent once its condition has held, \
         for its `once` period (`run` unless written: `user`, `day`, `slot`, `week` or \
         `season:<name>`); to keep the beat eligible only while the condition is false, write \
         `when: \"!({raw})\"` instead"
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
