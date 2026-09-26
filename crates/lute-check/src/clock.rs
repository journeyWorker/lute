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
/// a missing field, repeated slots, a week that does not add up) are
/// [`E_CLOCK_DECL`] at `span`; the paths are checked later, against the
/// folded schema ([`check_clock`]).
pub fn parse_clock(value: &serde_yaml::Value, span: Span) -> (Option<ClockDecl>, Vec<Diagnostic>) {
    match serde_yaml::from_value::<ClockDecl>(value.clone()) {
        Ok(clock) => {
            let diags = clock
                .shape_problems()
                .into_iter()
                .map(|p| clock_diag(at_schema(&p), span))
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
                span,
            )],
        ),
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
        if !occasions.is_empty() && !occasions.contains_key(raise) {
            let hint =
                lute_manifest::suggest::nearest(raise, occasions.keys().map(String::as_str), 2)
                    .map(|n| format!(" — did you mean `{n}`?"))
                    .unwrap_or_default();
            let key = match &clock.raise {
                Some(lute_manifest::clock::ClockRaise::Slot(_)) => "raise".to_string(),
                _ => format!("raise.{moment}"),
            };
            out.push(format!("`{key}: {raise}` is not a declared occasion{hint}"));
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
/// (number) always, `clock.weekday` (number, `0..length-1` — see
/// [`weekday_range`]) with a `week:`, and `clock.weekdayLabel` (the enum of
/// the week's labels, so a `<match>` over it is exhaustive and typo-checked)
/// with week labels. `owner: engine`, the
/// day path's tier, and a default computed from the `day`/`slot` defaults
/// when both have one — so a read is exactly as definitely-assigned as the
/// clock paths themselves.
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
        .map(|(path, number)| {
            let default = values.get(path).map(|v| match v {
                lute_manifest::clock::ClockValue::Num(n) => Literal::Num(*n as f64),
                lute_manifest::clock::ClockValue::Str(s) => Literal::Str(s.clone()),
            });
            (
                path.to_string(),
                StateDecl {
                    ty: if number {
                        Type::Number
                    } else {
                        Type::Enum(
                            clock
                                .week
                                .as_ref()
                                .map(|w| w.labels.clone())
                                .unwrap_or_default(),
                        )
                    },
                    default,
                    namespace,
                    owner: Some(Owner::Engine),
                },
            )
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
/// to its last position, and the day path from its declared default (day 1
/// when unknown) to the last day. The clock only moves forward, and a
/// `newRun` starts it at the defaults, so a `when` needing a later position
/// can never hold.
pub fn int_ranges(clock: &ClockDecl, schema: &StateSchema) -> Vec<(String, (i64, i64))> {
    let mut out = Vec::new();
    if let Some(range) = weekday_range(clock) {
        out.push((lute_manifest::clock::CLOCK_WEEKDAY.to_string(), range));
    }
    let Some(last) = clock.last_at() else {
        return out;
    };
    let default = |path: &str| schema.decls.get(path).and_then(|d| d.default.as_ref());
    let first_day = match default(&clock.day) {
        Some(Literal::Num(d)) if d.fract() == 0.0 => *d as i64,
        _ => 1,
    };
    let first_slot = match clock.slot.as_deref().map(default) {
        Some(Some(Literal::Str(s))) => clock.slot_index(s).unwrap_or(0),
        _ => 0,
    };
    let first = lute_manifest::clock::ClockAt {
        day: first_day,
        slot: first_slot,
    };
    if first > last {
        return out;
    }
    out.push((
        lute_manifest::clock::CLOCK_INDEX.to_string(),
        (clock.index(first).max(0), clock.index(last)),
    ));
    out.push((clock.day.clone(), (first_day, last.day)));
    out
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
        .collect();
    (!reads.is_empty()).then(|| {
        format!(
            "the clock ends at its last position, so {}",
            reads.join(" and ")
        )
    })
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
