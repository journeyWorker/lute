//! The declared clock (dsl 0.24.0 §1): a schema's `clock:` parsed, checked
//! against the folded state schema, and turned into the reserved read-only
//! `clock.*` paths ([`lute_manifest::clock`] holds the arithmetic).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::clock::ClockDecl;
use lute_manifest::schema::OccasionDecl;
use lute_manifest::snapshot::Domain;
use lute_manifest::types::{Literal, Owner, Type};

use crate::meta::{Namespace, StateDecl, StateSchema};

/// A malformed `clock:` declaration (dsl 0.24.0 §1): a shape the clock
/// cannot use, a `day`/`slot` path that is undeclared, mistyped or not
/// `owner: engine`, `slots` that are not the slot enum's members, an unknown
/// `raise` occasion, or a second clock.
pub const E_CLOCK_DECL: &str = "E-CLOCK-DECL";

fn clock_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CLOCK_DECL.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// A clock problem as reported at the schema's own `clock:` line — one
/// template for the shape problems and the path/occasion problems alike.
fn at_schema(what: &str) -> String {
    format!("`clock:` {what} (dsl 0.24.0 §1)")
}

/// Lift a schema document's `clock:` value. Shape problems (an unknown key,
/// a value of the wrong kind, a missing field, repeated slots, a week that
/// does not add up) are [`E_CLOCK_DECL`] at the key they name inside the
/// clock (HW27-13), else at the `clock:` key; the paths are checked later,
/// against the folded schema ([`check_clock`]).
pub fn parse_clock(
    value: &serde_yaml::Value,
    meta: &lute_syntax::ast::Meta,
) -> (Option<ClockDecl>, Vec<Diagnostic>) {
    let clock_span = crate::meta::meta_key_span(meta, "clock");
    let at = |key: &str| clock_key_span(meta, clock_span, key);
    let keys = key_problems(value);
    if !keys.is_empty() {
        let diags = keys
            .into_iter()
            .map(|(key, p)| clock_diag(at_schema(&p), at(&key)))
            .collect();
        return (None, diags);
    }
    match serde_yaml::from_value::<ClockDecl>(value.clone()) {
        Ok(clock) => {
            let diags = clock
                .shape_problems()
                .into_iter()
                .map(|p| {
                    // Each problem opens with the key it is about: `` `days:` ``,
                    // `` `week.first` ``, `` `last.slot: h06` ``.
                    let key = p
                        .split('`')
                        .nth(1)
                        .map(|k| k.split([':', ' ']).next().unwrap_or(k).to_string());
                    let span = key.map_or(clock_span, |k| at(&k));
                    clock_diag(at_schema(&p), span)
                })
                .collect();
            (Some(clock), diags)
        }
        Err(e) => (
            None,
            vec![clock_diag(
                format!(
                    "`clock:` must be `{{ day: <number path>, slot: <enum path>, slots: [..], \
                     raise: <occasion> | {{ slot, dayStart, dayEnd }}, week: {{ length, first, \
                     labels }}, last: {{ day, slot }} | days: <n> }}` — `slot`/`slots` \
                     (together), `raise`, `week` and `last`/`days` optional (dsl 0.24.0 §1, \
                     0.27.0 §4): {e}"
                ),
                clock_span,
            )],
        ),
    }
}

/// The keys a clock and its sub-maps admit, by dotted parent (`""` the
/// clock itself).
const CLOCK_KEYS: [(&str, &[&str]); 4] = [
    (
        "",
        &["day", "slot", "slots", "raise", "week", "last", "days"],
    ),
    ("last", &["day", "slot"]),
    ("week", &["length", "first", "labels"]),
    ("raise", &["slot", "dayStart", "dayEnd"]),
];

/// HW27-13: the problems with a clock mapping's keys and whole-number
/// fields, as `(dotted key, problem)` — an unknown key (with a did-you-mean)
/// and a number field holding anything but a whole number, named in the
/// writer's terms rather than the YAML library's. Empty for a value that is
/// not a mapping (the shape message covers it).
fn key_problems(value: &serde_yaml::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(clock) = value.as_mapping() else {
        return out;
    };
    for (parent, known) in CLOCK_KEYS {
        let map = if parent.is_empty() {
            Some(clock)
        } else {
            clock.get(parent).and_then(|v| v.as_mapping())
        };
        for key in map.into_iter().flat_map(|m| m.keys()) {
            let name = match key.as_str() {
                Some(k) if known.contains(&k) => continue,
                Some(k) => k.to_string(),
                None => format!("{key:?}"),
            };
            let dotted = if parent.is_empty() {
                name.clone()
            } else {
                format!("{parent}.{name}")
            };
            let hint = match (parent, name.as_str()) {
                // dsl 0.28.0 (T3-44): `days:` is the clock's own shorthand.
                ("last", "days") => " — `days: N` is a key of the clock itself, short for \
                                     `last: { day: N }`; write `last: { day: N }` here, or \
                                     `days: N` beside `last:` instead of it"
                    .to_string(),
                _ => lute_manifest::suggest::nearest(&name, known.iter().copied(), 2)
                    .map_or_else(String::new, |near| format!(" — did you mean `{near}`?")),
            };
            out.push((
                dotted.clone(),
                format!(
                    "has no key `{dotted}`{hint} (a {} admits {})",
                    if parent.is_empty() {
                        "clock".to_string()
                    } else {
                        format!("`{parent}:`")
                    },
                    known
                        .iter()
                        .map(|k| format!("`{k}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
    // dsl 0.28.0 (T3-64): `week.labels` is a list, unlike an enum's or an
    // entity kind's `labels:` map — say so instead of the YAML library.
    let week_labels = clock
        .get("week")
        .and_then(|w| w.as_mapping())
        .and_then(|w| w.get("labels"));
    if let Some(labels) = week_labels.filter(|l| !l.is_sequence()) {
        let shape = if labels.is_mapping() {
            "a map"
        } else {
            "not a list"
        };
        out.push((
            "week.labels".to_string(),
            format!(
                "`week.labels` is {shape}, but it is a list: one label per weekday, in weekday \
                 order from weekday 0, as `labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun]` (an \
                 enum's or entity kind's `labels:` is the map, member → label)"
            ),
        ));
    }
    let whole = |v: &serde_yaml::Value, min: u64| {
        v.as_u64().is_some_and(|n| n >= min && n <= u32::MAX as u64)
    };
    for (dotted, min, example) in [
        ("days", 0, "`days: 1`"),
        ("last.day", 0, "`last: { day: 1 }`"),
        ("week.length", 0, "`week: { length: 7 }`"),
        ("week.first", 0, "`week: { length: 7, first: 0 }`"),
    ] {
        let v = match dotted.split_once('.') {
            None => clock.get(dotted),
            Some((parent, key)) => clock
                .get(parent)
                .and_then(|p| p.as_mapping())
                .and_then(|p| p.get(key)),
        };
        if let Some(v) = v.filter(|v| !whole(v, min)) {
            let shown = match v {
                serde_yaml::Value::String(s) => format!("\"{s}\""),
                other => serde_yaml::to_string(other)
                    .map_or_else(|_| "?".into(), |s| s.trim().to_string()),
            };
            out.push((
                dotted.to_string(),
                format!("`{dotted}: {shown}` must be a whole number, e.g. {example}"),
            ));
        }
    }
    out
}

/// Where `dotted` (`days`, `last.slot`) is written inside the clock whose
/// key is at `clock_span`: the key's own span, else `clock_span`. A text
/// scan of the clock's block (flow or block style), each segment searched
/// after the previous one.
fn clock_key_span(meta: &lute_syntax::ast::Meta, clock_span: Span, dotted: &str) -> Span {
    let authored = crate::chapters::authored_yaml(&meta.raw_yaml);
    if clock_span == meta.span {
        return clock_span;
    }
    // Offsets map raw → document the way `meta_key_span` does.
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != authored.len();
    let base = meta.span.byte_start + if enveloped { 4 } else { 0 };
    let Some(key_off) = clock_span
        .byte_start
        .checked_sub(base)
        .filter(|&o| o < authored.len())
    else {
        return clock_span;
    };
    // The clock's block: its line, then every more-indented or blank line.
    let end = authored[key_off..]
        .split_inclusive('\n')
        .enumerate()
        .take_while(|(i, line)| *i == 0 || line.trim().is_empty() || line.starts_with([' ', '\t']))
        .map(|(_, line)| line.len())
        .sum::<usize>();
    let block = &authored[key_off + "clock".len()..key_off + end];
    let mut from = 0;
    for seg in dotted.split('.') {
        let found = block[from..]
            .match_indices(seg)
            .map(|(i, _)| from + i)
            .find(|&i| {
                let before = block[..i].chars().next_back();
                let after = block[i + seg.len()..].trim_start_matches([' ', '"', '\'']);
                !before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                    && after.starts_with(':')
            });
        match found {
            Some(i) => from = i,
            None => return clock_span,
        }
    }
    let start = base + key_off + "clock".len() + from;
    Span {
        byte_start: start,
        byte_end: start + dotted.rsplit('.').next().unwrap_or(dotted).len(),
        ..clock_span
    }
}

/// The members of an enum-typed path (`{ enum: [..] }` inline, or a named
/// domain).
fn enum_members<'a>(ty: &'a Type, domains: &'a BTreeMap<String, Domain>) -> Option<&'a [String]> {
    match ty {
        Type::Enum(members) => Some(members),
        Type::Domain(name) => domains
            .get(name)
            .filter(|d| !d.open)
            .map(|d| d.members.as_slice()),
        _ => None,
    }
}

/// Where a clock is declared: its name in messages (`w.schema.yaml`, `this
/// schema`) and, for an imported schema, the file and the positioned span
/// of its `clock:` key — every problem with the clock is attributed there.
#[derive(Clone, Debug)]
pub struct ClockSite {
    pub name: String,
    pub at: Option<(String, Span)>,
}

/// Settle the project's clock for one document: `clocks` are every
/// declaration the document sees (its imports' and, for a schema document,
/// its own). More than one is [`E_CLOCK_DECL`]; the one kept is checked
/// against the folded `schema` ([`clock_problems`]). Every diagnostic is
/// anchored at `span` and names where the clock is declared; a clock from
/// an imported schema also carries that schema's `clock:` line as a
/// `related` entry, so `check-project` folds the identical report of every
/// importer into one attributed to the schema.
pub fn check_clock(
    clocks: &[(ClockSite, ClockDecl)],
    schema: &StateSchema,
    domains: &BTreeMap<String, Domain>,
    occasions: &BTreeMap<String, OccasionDecl>,
    span: Span,
) -> (Option<ClockDecl>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    let Some((site, clock)) = clocks.first() else {
        return (None, diags);
    };
    let origin = &site.name;
    if let Some((other, _)) = clocks.iter().skip(1).find(|(_, c)| c != clock) {
        diags.push(clock_diag(
            format!(
                "a project declares at most one clock, but `{origin}` and `{}` both \
                 declare `clock:` (dsl 0.24.0 §1)",
                other.name
            ),
            span,
        ));
    }
    for what in clock_problems(clock, schema, domains, occasions, false) {
        let mut d = clock_diag(
            format!("clock (declared in `{origin}`): {what} (dsl 0.24.0 §1)"),
            span,
        );
        if let Some((file, at)) = &site.at {
            d.related.push(lute_core_span::RelatedDiagnostic {
                file: file.clone(),
                diagnostic: clock_diag(at_schema(&what), *at),
            });
        }
        diags.push(d);
    }
    (Some(clock.clone()), diags)
}

/// What is wrong with `clock` against a state `schema`, the `domains` a
/// named slot enum resolves in and the `occasions` vocabulary: a `day` path
/// that is undeclared, not a number or not `owner: engine`; a `slot` path
/// that is undeclared, not an enum or not `owner: engine`, or `slots` that
/// are not its members; a `raise` occasion that is not declared (checked
/// only when `occasions` is non-empty). `partial`: `schema` is one schema
/// file's own state, whose imports may declare a path it lacks — an
/// undeclared path is then not a problem here.
pub fn clock_problems(
    clock: &ClockDecl,
    schema: &StateSchema,
    domains: &BTreeMap<String, Domain>,
    occasions: &BTreeMap<String, OccasionDecl>,
    partial: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    match schema.decls.get(&clock.day) {
        None if partial => {}
        None => out.push(format!("`day: {}` is not a declared state path", clock.day)),
        Some(decl) => {
            if decl.ty != Type::Number {
                out.push(format!("`day: {}` must be a `number` path", clock.day));
            }
            if decl.owner != Some(Owner::Engine) {
                out.push(format!(
                    "`day: {}` must be declared `owner: engine` — only the engine moves the clock",
                    clock.day
                ));
            }
            // dsl 0.28.0 (T3-44): the clock counts from day 1.
            if let Some(Literal::Num(d)) = &decl.default {
                if *d < 1.0 || d.fract() != 0.0 {
                    out.push(format!(
                        "`day: {}` starts at {d} (its default) — the clock counts whole days \
                         from day 1; give it `default: 1`",
                        clock.day
                    ));
                }
            }
        }
    }
    if let Some(slot) = &clock.slot {
        match schema.decls.get(slot) {
            None if partial => {}
            None => out.push(format!("`slot: {slot}` is not a declared state path")),
            Some(decl) => {
                match enum_members(&decl.ty, domains) {
                    // A named domain an import declares is not resolvable here.
                    None if partial && matches!(decl.ty, Type::Domain(_)) => {}
                    None => out.push(format!("`slot: {slot}` must be an enum path")),
                    Some(members) => {
                        let mut want: Vec<&str> = members.iter().map(String::as_str).collect();
                        let mut got: Vec<&str> = clock.slots.iter().map(String::as_str).collect();
                        want.sort_unstable();
                        got.sort_unstable();
                        if want != got {
                            out.push(format!(
                                "`slots: [{}]` must list exactly the members of `{slot}` ({}), in \
                                 clock order",
                                clock.slots.join(", "),
                                members.join(", ")
                            ));
                        }
                    }
                }
                if decl.owner != Some(Owner::Engine) {
                    out.push(format!(
                        "`slot: {slot}` must be declared `owner: engine` — only the engine moves the clock"
                    ));
                }
            }
        }
    }
    let moments = clock.raises();
    let named = [
        ("slot", &moments.slot),
        ("dayStart", &moments.day_start),
        ("dayEnd", &moments.day_end),
    ];
    for (moment, raise) in named {
        let Some(raise) = raise else { continue };
        let key = match &clock.raise {
            Some(lute_manifest::clock::ClockRaise::Slot(_)) => "raise".to_string(),
            _ => format!("raise.{moment}"),
        };
        match occasions.get(raise) {
            None if !occasions.is_empty() => {
                let hint =
                    lute_manifest::suggest::nearest(raise, occasions.keys().map(String::as_str), 2)
                        .map(|n| format!(" — did you mean `{n}`?"))
                        .unwrap_or_default();
                out.push(format!("`{key}: {raise}` is not a declared occasion{hint}"));
            }
            // A clock raise hands over nothing: its beats would read an
            // unset `occasion.payload.*`.
            Some(decl) if !decl.payload.is_empty() => {
                let fields: Vec<String> = decl.payload.keys().map(|f| format!("`{f}`")).collect();
                out.push(format!(
                    "`{key}: {raise}` names an occasion with a payload ({}), and a clock raise \
                     carries none — raise `{raise}` from the engine, or drop its `payload:`",
                    fields.join(", ")
                ));
            }
            _ => {}
        }
    }
    // dsl 0.27.0 §4: a finite clock starts where the day/slot defaults put
    // it — never past its last position.
    if let Some(last) = clock.last_at() {
        let default = |path: &str| schema.decls.get(path).and_then(|d| d.default.as_ref());
        let start = match (default(&clock.day), clock.slot.as_deref().map(default)) {
            (Some(Literal::Num(d)), None) => clock.at(*d, None),
            (Some(Literal::Num(d)), Some(Some(Literal::Str(s)))) => clock.at(*d, Some(s)),
            _ => None,
        };
        if let Some(start) = start.filter(|s| *s > last) {
            out.push(format!(
                "starts at {} (its paths' defaults), past its last position {}",
                clock.describe(start),
                clock.describe(last)
            ));
        }
    }
    out
}

/// The reserved read-only `clock.*` decls a clock implies: `clock.index`
/// and `clock.day` (numbers) always, `clock.slot` (the enum of `slots`) on a
/// clock with slots, `clock.weekday` (number, `0..length-1` — see
/// [`weekday_range`]) with a `week:`, `clock.weekdayLabel` (the enum of the
/// week's labels, so a `<match>` over it is exhaustive and typo-checked)
/// with week labels, and `clock.ended` (bool, default `false`) on a finite
/// clock. `owner: engine`, the day path's tier, and a default computed from
/// the `day`/`slot` defaults when both have one — so a read is exactly as
/// definitely-assigned as the clock paths themselves.
pub fn reserved_decls(clock: &ClockDecl, schema: &StateSchema) -> Vec<(String, StateDecl)> {
    let day = schema.decls.get(&clock.day);
    let namespace = day.map_or(Namespace::Run, |d| d.namespace);
    let slot_default = clock
        .slot
        .as_ref()
        .map(|s| schema.decls.get(s).and_then(|d| d.default.as_ref()));
    let at = match (day.and_then(|d| d.default.as_ref()), slot_default) {
        (Some(Literal::Num(d)), None) => clock.at(*d, None),
        (Some(Literal::Num(d)), Some(Some(Literal::Str(s)))) => clock.at(*d, Some(s)),
        _ => None,
    };
    let values: BTreeMap<&str, lute_manifest::clock::ClockValue> = at
        .map(|at| clock.values(at).into_iter().collect())
        .unwrap_or_default();
    clock
        .reserved_paths()
        .into_iter()
        .map(|(path, ty)| {
            use lute_manifest::clock::ClockPathType;
            let default = match ty {
                ClockPathType::Bool => Some(Literal::Bool(false)),
                _ => values.get(path).map(|v| match v {
                    lute_manifest::clock::ClockValue::Num(n) => Literal::Num(*n as f64),
                    lute_manifest::clock::ClockValue::Str(s) => Literal::Str(s.clone()),
                }),
            };
            let ty = match ty {
                ClockPathType::Number => Type::Number,
                ClockPathType::Bool => Type::Bool,
                ClockPathType::Slot => Type::Enum(clock.slots.clone()),
                ClockPathType::WeekdayLabel => Type::Enum(
                    clock
                        .week
                        .as_ref()
                        .map(|w| w.labels.clone())
                        .unwrap_or_default(),
                ),
            };
            (
                path.to_string(),
                StateDecl {
                    ty,
                    default,
                    namespace,
                    owner: Some(Owner::Engine),
                },
            )
        })
        .collect()
}

/// The clock's enum paths as domains a type can name — `{ domain:
/// clock.slot }` (the clock's slots) and `{ domain: clock.weekdayLabel }`
/// (the week's labels) — so a component param, a directive attr or a
/// `state:` path that carries one of them is member-checked against the
/// clock instead of a copied list.
pub fn clock_domains(clock: &ClockDecl) -> Vec<(String, Domain)> {
    use lute_manifest::clock::ClockPathType;
    clock
        .reserved_paths()
        .into_iter()
        .filter_map(|(path, ty)| {
            let members = match ty {
                ClockPathType::Slot => clock.slots.clone(),
                ClockPathType::WeekdayLabel => clock.week.as_ref()?.labels.clone(),
                ClockPathType::Number | ClockPathType::Bool => return None,
            };
            (!members.is_empty()).then(|| {
                (
                    path.to_string(),
                    Domain {
                        members,
                        ..Domain::default()
                    },
                )
            })
        })
        .collect()
}

/// `clock.weekday`'s whole-number range `(0, length - 1)` (dsl 0.24.0 §1),
/// `None` without a `week:`.
pub fn weekday_range(clock: &ClockDecl) -> Option<(i64, i64)> {
    let week = clock.week.as_ref().filter(|w| w.length > 0)?;
    Some((0, i64::from(week.length) - 1))
}

/// The whole-number ranges a clock gives its paths
/// ([`StateSchema::int_ranges`]): `clock.weekday` with a `week:` and — on a
/// finite clock (dsl 0.27.0 §4) — `clock.index` from where the clock starts
/// to its last position, and the day path (and `clock.day`) from its
/// declared default (day 1 when unknown) to the last day. The clock only
/// moves forward, and a `newRun` starts it at the defaults, so a `when`
/// needing a later position can never hold.
pub fn int_ranges(clock: &ClockDecl, schema: &StateSchema) -> Vec<(String, (i64, i64))> {
    let mut out = Vec::new();
    if let Some(range) = weekday_range(clock) {
        out.push((lute_manifest::clock::CLOCK_WEEKDAY.to_string(), range));
    }
    let Some((first, last)) = finite_span(clock, schema) else {
        return out;
    };
    out.push((
        lute_manifest::clock::CLOCK_INDEX.to_string(),
        (clock.index(first).max(0), clock.index(last)),
    ));
    out.push((clock.day.clone(), (first.day, last.day)));
    out.push((
        lute_manifest::clock::CLOCK_DAY.to_string(),
        (first.day, last.day),
    ));
    out
}

/// dsl 0.28.0 (T3-40): the slot members a finite clock that starts and
/// ends on the same day lets its slot path and `clock.slot` hold — its
/// starting slot through its last one ([`StateSchema::clock_members`]).
/// Empty for a clock that never ends, spans several days (every slot comes
/// round on an earlier day) or counts whole days.
pub fn slot_members(clock: &ClockDecl, schema: &StateSchema) -> Vec<(String, Vec<String>)> {
    let (Some(slot), Some((first, last))) = (&clock.slot, finite_span(clock, schema)) else {
        return Vec::new();
    };
    if first.day != last.day || (first.slot == 0 && last.slot + 1 == clock.slot_count()) {
        return Vec::new();
    }
    let members = clock.slots[first.slot..=last.slot].to_vec();
    vec![
        (slot.clone(), members.clone()),
        (lute_manifest::clock::CLOCK_SLOT.to_string(), members),
    ]
}

/// A finite clock's first position ([`first_at`]) and last one — `None`
/// for a clock that never ends or starts past its end.
fn finite_span(
    clock: &ClockDecl,
    schema: &StateSchema,
) -> Option<(lute_manifest::clock::ClockAt, lute_manifest::clock::ClockAt)> {
    let last = clock.last_at()?;
    let first = first_at(clock, schema);
    (first <= last).then_some((first, last))
}

/// Where the clock starts: its paths' defaults, day 1 and the first slot
/// when unknown.
pub(crate) fn first_at(clock: &ClockDecl, schema: &StateSchema) -> lute_manifest::clock::ClockAt {
    let default = |path: &str| schema.decls.get(path).and_then(|d| d.default.as_ref());
    let first_day = match default(&clock.day) {
        Some(Literal::Num(d)) if d.fract() == 0.0 => *d as i64,
        _ => 1,
    };
    let first_slot = match clock.slot.as_deref().map(default) {
        Some(Some(Literal::Str(s))) => clock.slot_index(s).unwrap_or(0),
        _ => 0,
    };
    lute_manifest::clock::ClockAt {
        day: first_day,
        slot: first_slot,
    }
}

/// Why guard `raw` is provably false because of the clock: no position the
/// clock can stand at satisfies it
/// ([`crate::clock_positions::position_reason`]), else a read past a finite
/// clock's end ([`end_reason`]).
pub fn false_reason(
    raw: &str,
    defs: &crate::cel_expand::DefTable<'_>,
    ctx: &crate::decide::DecideCtx<'_>,
) -> Option<String> {
    crate::clock_positions::position_reason(raw, defs, ctx).or_else(|| end_reason(ctx.schema, raw))
}

/// dsl 0.27.0 §4: why a guard reading a finite clock's paths can be
/// provably false — the reason an unreachable `when` gives. `None` when the
/// clock never ends or `raw` names none of its bounded paths (a best-effort
/// text match: a path read through a `@def` goes unnamed).
pub fn end_reason(schema: &StateSchema, raw: &str) -> Option<String> {
    schema.int_ranges.get(lute_manifest::clock::CLOCK_INDEX)?;
    let names = |path: &str| {
        raw.match_indices(path).any(|(i, _)| {
            let ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';
            !raw[..i].ends_with(ident) && !raw[i + path.len()..].starts_with(ident)
        })
    };
    let reads: Vec<String> = schema
        .int_ranges
        .iter()
        .filter(|(path, _)| path.as_str() != lute_manifest::clock::CLOCK_WEEKDAY && names(path))
        .map(|(path, (a, b))| format!("`{path}` only ranges over {a}..{b}"))
        .chain(
            schema
                .clock_members
                .iter()
                .filter(|(path, _)| names(path))
                .map(|(path, ms)| slot_holds(path, ms)),
        )
        .collect();
    (!reads.is_empty()).then(|| {
        format!(
            "the clock ends at its last position, so {}",
            reads.join(" and ")
        )
    })
}

/// dsl 0.28.0 (T3-40): why a `<match>` arm on a finite clock's slot `path`
/// can never match — the reason `E-ARM-DEAD` gives.
pub fn slot_end_reason(schema: &StateSchema, path: &str) -> Option<String> {
    let ms = schema.clock_members.get(path)?;
    Some(format!(
        "the clock ends at its last position, so {}",
        slot_holds(path, ms)
    ))
}

fn slot_holds(path: &str, members: &[String]) -> String {
    format!("`{path}` only holds {}", members.join(", "))
}

/// `once: day` / `once: slot` / `once: week` (a scene's frontmatter, an
/// entry's or a bundle beat's `once=`) spends a beat per clock period, so it
/// needs a declared clock — and `week` a clock with a `week:`: without one
/// each is [`crate::beats::E_BEAT_ATTR`] (dsl 0.24.0 §1, 0.27.0 §5).
pub fn check_once_needs_clock(
    doc: &lute_syntax::ast::Document,
    beat: Option<&crate::beats::BeatMeta>,
    clock: Option<&ClockDecl>,
) -> Vec<Diagnostic> {
    let has_week = clock.is_some_and(|c| c.week.is_some());
    if has_week {
        return Vec::new();
    }
    // `instead`: what the construct accepts without a clock — an entry has
    // no `once="false"` (omitting `once` is its never-spent form).
    let needs = |written: String, once: &str, instead: &str| {
        if clock.is_some() {
            format!(
                "`{written}` spends a beat once per clock week, but the project's `clock:` \
                 declares no `week:` — add `week: {{ length: 7 }}` to the clock, or {instead} \
                 (dsl 0.27.0 §5)"
            )
        } else {
            format!(
                "`{written}` spends a beat once per clock {once}, but the project declares no \
                 `clock:` — declare one in a schema, or {instead} (dsl 0.24.0 §1)"
            )
        }
    };
    // With a clock only `week` still needs something; without one every
    // clock period does.
    let lacks = |once: &str| match clock {
        Some(_) => once == "week",
        None => matches!(once, "day" | "slot" | "week"),
    };
    let scene_or_beat = "use `run` / `user` / `false`";
    let attr = |message: String, span: Span| Diagnostic {
        code: crate::beats::E_BEAT_ATTR.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    let mut out = Vec::new();
    if let Some(b) = beat.filter(|b| b.once.is_clock() && lacks(&b.once.as_str())) {
        out.push(attr(
            needs(
                format!("once: {}", b.once.as_str()),
                &b.once.as_str(),
                scene_or_beat,
            ),
            crate::meta::meta_key_span(&doc.meta, "once"),
        ));
    }
    let entry = "use `run` / `user`, or omit `once` (an entry without it is never spent)";
    let authored = doc
        .entries
        .iter()
        .filter_map(|e| e.once.as_ref().map(|o| (o, entry)))
        .chain(
            doc.beats
                .iter()
                .filter_map(|b| b.once.as_ref().map(|o| (o, scene_or_beat))),
        );
    for ((raw, span), instead) in authored {
        if lacks(raw) {
            out.push(attr(needs(format!("once=\"{raw}\""), raw, instead), *span));
        }
    }
    out
}
