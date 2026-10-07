use super::*;

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
    "on", "target", "when", "priority", "once", "also", "share", "spentBy", "for", "advances",
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
                    .filter(|n| lute_manifest::ident::is_name(n))?
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
/// How a presented beat moves the engine clock (dsl 0.31.0 §1). This is a
/// declaration only; the engine performs the move when the beat is presented.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvanceSpec {
    Slot,
    Day,
    Slots(u32),
}

impl AdvanceSpec {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slot => "slot",
            Self::Day => "day",
            Self::Slots(_) => "n",
        }
    }
}

impl serde::Serialize for AdvanceSpec {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Slot => serializer.serialize_str("slot"),
            Self::Day => serializer.serialize_str("day"),
            Self::Slots(n) => serializer.serialize_u32(*n),
        }
    }
}

pub(super) fn parse_advances(v: &serde_yaml::Value) -> Option<AdvanceSpec> {
    match v {
        serde_yaml::Value::String(s) if s == "slot" => Some(AdvanceSpec::Slot),
        serde_yaml::Value::String(s) if s == "day" => Some(AdvanceSpec::Day),
        serde_yaml::Value::Number(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n >= 1)
            .map(AdvanceSpec::Slots),
        _ => None,
    }
}

pub(crate) fn advances_from_yaml(
    map: &serde_yaml::Mapping,
    get_span: impl Fn(&str) -> Span,
    diags: &mut Vec<Diagnostic>,
) -> Option<AdvanceSpec> {
    let Some(v) = map.get(serde_yaml::Value::String("advances".to_string())) else {
        return None;
    };
    if let Some(spec) = parse_advances(v) {
        return Some(spec);
    }
    diags.push(beat_diag(
        E_BEAT_ATTR,
        Severity::Error,
        format!(
            "`advances:` must be `slot`, `day`, or a whole number ≥ 1, got {} (dsl 0.31.0 §1)",
            describe(v)
        ),
        get_span("advances"),
        Layer::Content,
    ));
    None
}

pub fn advances_from_attr(
    raw: Option<&(String, Span)>,
    diags: &mut Vec<Diagnostic>,
) -> Option<AdvanceSpec> {
    let Some((raw, span)) = raw else { return None };
    let v = match raw.as_str() {
        "slot" => AdvanceSpec::Slot,
        "day" => AdvanceSpec::Day,
        n => match n.parse::<u32>() {
            Ok(n) if n >= 1 => AdvanceSpec::Slots(n),
            _ => {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    format!(
                        "`advances` must be `slot`, `day`, or a whole number ≥ 1, got `{raw}` \
                         (dsl 0.31.0 §1)"
                    ),
                    *span,
                    Layer::Logic,
                ));
                return None;
            }
        },
    };
    Some(v)
}

pub(super) fn parse_advances_text(raw: &str) -> Option<AdvanceSpec> {
    match raw {
        "slot" => Some(AdvanceSpec::Slot),
        "day" => Some(AdvanceSpec::Day),
        n => n
            .parse::<u32>()
            .ok()
            .filter(|n| *n >= 1)
            .map(AdvanceSpec::Slots),
    }
}


/// A scene's validated beat declaration (dsl 0.21.0 §3.1), lifted onto
/// [`crate::meta::TypedMeta::beat`] only when `on:` is present and a name.
/// A malformed sibling key draws [`E_BEAT_ATTR`] and falls back
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
    /// dsl 0.31.0 §1: the clock movement performed when this beat presents.
    pub advances: Option<AdvanceSpec>,
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
            // A scene an unapplied chain lists
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
        Some(v) => match v.as_str() {
            Some(on) if is_name(on) => Some(on.to_string()),
            Some(on) => {
                push(occasion_malformed("scene", on), top_value_span(meta, "on"));
                None
            }
            None => {
                push(
                    format!(
                        "`on:` must name an occasion — a name (letters, digits, `_` or \
                         `-`, not starting with `-`), got {}",
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
                    "`target:` must be {TARGET_SHAPE}; or `kind:<entity kind>` for every member \
                     of a kind — got {}",
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
        let Some(key) = v.as_str().filter(|s| is_name(s)) else {
            let message = match v.as_str() {
                Some(key) => share_malformed("scene", key),
                None => format!(
                    "`share:` must be a key — a name every beat standing for the same \
                     event writes, got {}",
                    describe(v)
                ),
            };
            push(message, top_value_span(meta, "share"));
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
    let advances = advances_from_yaml(map, |key| top_value_span(meta, key), diags);

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
        advances,
    })
}

/// An `<entry>`'s beat attributes' shape (dsl 0.21.0 §3.2, dsl 0.22.0 §7,
/// §5): `on` a name, `priority` an integer, `once` `run` / `user`,
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
        if matches!(attr.key.as_str(), "on" | "priority" | "once" | "share" | "advances")
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
        if !is_name(on) {
            push(occasion_malformed("`<entry>`", on), *span);
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
        if !is_name(key) {
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
    if let Some((raw, span)) = &entry.advances {
        let valid = matches!(raw.as_str(), "slot" | "day")
            || raw.parse::<u32>().is_ok_and(|n| n >= 1);
        if !valid {
            push(
                format!(
                    "`advances` must be `slot`, `day`, or a whole number ≥ 1, got `{raw}` \
                     (dsl 0.31.0 §1)"
                ),
                *span,
            );
        }
        if entry.on.is_none() && !residual_on {
            push(
                "`<entry>` `advances` requires `on`; it moves the clock when a beat is presented \
                 (dsl 0.31.0 §1)"
                    .to_string(),
                *span,
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
        let Some((on, on_span)) = entry.on.as_ref().filter(|(on, _)| is_name(on)) else {
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

