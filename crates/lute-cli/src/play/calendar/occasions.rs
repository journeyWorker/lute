use super::*;
use super::axes::split_top;

/// One `--occasion`: its name and, after `@`, the axes it varies over
/// (`path`) or is held at (`path=value`); every axis it does not name is
/// held at its first value.
pub(super) struct OccasionSpec<'a> {
    pub(super) raw: &'a str,
    pub(super) name: &'a str,
    pub(super) only: Option<Vec<(&'a str, Option<&'a str>)>>,
}

pub(super) fn parse_occasion(raw: &str) -> Result<OccasionSpec<'_>, String> {
    let Some((name, list)) = raw.split_once('@') else {
        return Ok(OccasionSpec {
            raw,
            name: raw.trim(),
            only: None,
        });
    };
    let mut only = Vec::new();
    for item in split_top(list, ',') {
        let (path, value) = match split_top(item, '=').as_slice() {
            [path] => (path.trim(), None),
            [path, value] => (path.trim(), Some(value.trim())),
            _ => {
                return Err(format!(
                    "`--occasion {raw}`: `{item}` has more than one `=`"
                ))
            }
        };
        if path.is_empty() || value == Some("") {
            return Err(format!(
                "`--occasion {raw}`: expected <occasion>@<axis>[=<value>],…, with no empty item"
            ));
        }
        only.push((path, value));
    }
    Ok(OccasionSpec {
        raw,
        name: name.trim(),
        only: Some(only),
    })
}

/// How a per-occasion column (`--occasion O@…`) treats one axis.
#[derive(Clone)]
pub(super) enum Pin {
    /// The occasion varies over the axis.
    Varies,
    /// Held at one value index.
    At(usize),
    /// dsl 0.24.0 §1: the clock axis, varied or held per part
    /// (`O@clock.day`, `O@run.day,run.slot=night`): the value indices whose
    /// position matches, and each part's held value (`None`: it varies).
    Clock {
        allowed: BTreeSet<usize>,
        day: Option<i64>,
        slot: Option<String>,
        /// The clock has a slot part (not a day-granular clock).
        slotted: bool,
        /// The `@` list holds the slot at a value it names
        /// (`clock.slot=afternoon`), not at the default.
        slot_named: bool,
    },
}

/// Resolve an [`OccasionSpec`]'s `@` list against the axes into
/// [`Column::pins`]. With a `clock` axis, its parts are named `clock.day` /
/// `clock.slot` or by the clock's own day / slot paths (`run.day`); an
/// unnamed part is held at its first value — an unnamed slot of the clock's
/// `raise.dayEnd` occasion at the day's last slot, where the clock raises it.
pub(super) fn occasion_pins(
    spec: &OccasionSpec<'_>,
    axes: &[Axis],
    clock: Option<&lute_manifest::clock::ClockDecl>,
    is_target: &dyn Fn(&str) -> bool,
) -> Result<Option<Vec<Pin>>, String> {
    let Some(only) = &spec.only else {
        return Ok(None);
    };
    let raw = spec.raw;
    let mut pins = vec![Pin::At(0); axes.len()];
    let mut named = BTreeSet::new();
    let clock_axis = axes.iter().position(|a| matches!(a.apply, Apply::Clock));
    // Per clock part: `None` unnamed, `Some(None)` varies, `Some(Some(v))` held.
    let (mut day_part, mut slot_part): (Option<Option<&str>>, Option<Option<&str>>) = (None, None);
    for &(path, value) in only {
        let Some(i) = axes.iter().position(|a| a.path == path) else {
            if let (Some(ci), Some(clock)) = (clock_axis, clock) {
                let day = path == "clock.day" || path == clock.day;
                let slot = path == "clock.slot" || clock.slot.as_deref() == Some(path);
                if day || slot {
                    if named.contains(&ci) {
                        return Err(format!(
                            "`--occasion {raw}`: `{path}` is part of `clock`, named already"
                        ));
                    }
                    if slot && clock.slot.is_none() {
                        return Err(format!(
                            "`--occasion {raw}`: the clock counts whole days — it has no slot to vary or hold"
                        ));
                    }
                    let part = if day { &mut day_part } else { &mut slot_part };
                    if part.replace(value).is_some() {
                        return Err(format!("`--occasion {raw}`: `{path}` is named twice"));
                    }
                    continue;
                }
            }
            // `@` here names axes; a target is `--target`'s, as in `lute beats`.
            if value.is_none() && is_target(path) {
                return Err(format!(
                    "`--occasion {raw}`: `@` names the axes `{0}` varies over, and `{path}` is a \
                     target of `{0}` — raise it for that target with `--occasion {0} --target \
                     {path}`",
                    spec.name
                ));
            }
            let mut paths: Vec<&str> = axes.iter().map(|a| a.path.as_str()).collect();
            if clock_axis.is_some() {
                paths.extend(["clock.day", "clock.slot"]);
            }
            let hint = lute_manifest::suggest::nearest(path, paths.iter().copied(), 3)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            return Err(format!(
                "`--occasion {raw}`: `{path}` is no `--axis` of this calendar (axes: {}){hint}",
                if paths.is_empty() {
                    "none".to_string()
                } else {
                    paths.join(", ")
                }
            ));
        };
        if !named.insert(i)
            || (Some(i) == clock_axis && (day_part.is_some() || slot_part.is_some()))
        {
            return Err(format!("`--occasion {raw}`: `{path}` is named twice"));
        }
        pins[i] = match value {
            None => Pin::Varies,
            Some(v) => Pin::At(axes[i].values.iter().position(|(t, _)| t == v).ok_or_else(
                || {
                    let vals: Vec<&str> = axes[i].values.iter().map(|(t, _)| t.as_str()).collect();
                    format!(
                        "`--occasion {raw}`: `{v}` is not a value of `--axis {path}` ({})",
                        vals.join(", ")
                    )
                },
            )?),
        };
    }
    if let (Some(ci), Some(clock)) = (clock_axis, clock) {
        if day_part.is_some() || slot_part.is_some() {
            let positions: Vec<lute_manifest::clock::ClockAt> = axes[ci]
                .values
                .iter()
                .map(|(_, v)| clock_axis_at(clock, v))
                .collect();
            let first = positions[0];
            let day = match day_part {
                None => Some(first.day),
                Some(None) => None,
                Some(Some(v)) => Some(
                    v.parse::<i64>()
                        .ok()
                        .filter(|d| positions.iter().any(|at| at.day == *d))
                        .ok_or_else(|| {
                            format!("`--occasion {raw}`: `{v}` is not a day of `--axis clock`")
                        })?,
                ),
            };
            let slot = match slot_part {
                None if clock.raises().day_end.as_deref() == Some(spec.name) => {
                    Some(clock.slot_count() - 1)
                }
                None => Some(first.slot),
                Some(None) => None,
                Some(Some(v)) => Some(clock.slot_index(v).ok_or_else(|| {
                    format!(
                        "`--occasion {raw}`: `{v}` is not a slot of the clock ({})",
                        clock.slots.join(", ")
                    )
                })?),
            };
            let allowed = positions
                .iter()
                .enumerate()
                .filter(|(_, at)| {
                    day.is_none_or(|d| at.day == d) && slot.is_none_or(|s| at.slot == s)
                })
                .map(|(k, _)| k)
                .collect();
            pins[ci] = Pin::Clock {
                allowed,
                day,
                slot: slot.and_then(|s| clock.slot_name(s)).map(str::to_string),
                slotted: clock.slot.is_some(),
                slot_named: matches!(slot_part, Some(Some(_))),
            };
        }
    }
    Ok(Some(pins))
}

/// What one column decides at one cell.
pub(super) struct Outcome {
    /// `select: first`: the beat presented; `None` when nothing is eligible
    /// or an unknown `when` decides the outcome.
    pub(super) winner: Option<String>,
    /// Every beat presented, in order: the winner, or the whole offered /
    /// sequenced list.
    pub(super) presented: Vec<String>,
    /// Eligible but not presented (shadowed by the winner).
    pub(super) shadowed: Vec<String>,
    /// `(id, why)` of every candidate whose `when` evaluated unknown.
    pub(super) unknown: Vec<(String, String)>,
    /// An unknown `when` decides this outcome — play would halt here.
    pub(super) undecided: bool,
    /// The occasion's `raisedWhen` gate is false here: the engine would not
    /// raise it, so nothing is presented.
    pub(super) gated: bool,
    /// With `--axis clock`: the clock's `raise:` map never raises the
    /// occasion at this position, so nothing is judged here.
    pub(super) not_raised: bool,
}

impl Outcome {
    /// The outcome where the clock does not raise the occasion.
    pub(super) fn not_raised() -> Self {
        Outcome {
            winner: None,
            presented: Vec::new(),
            shadowed: Vec::new(),
            unknown: Vec::new(),
            undecided: false,
            gated: false,
            not_raised: true,
        }
    }
}

/// Where the clock's `raise:` map raises one occasion
/// ([`lute_check::clock_positions::RaiseRule`], the rule `lute play`'s
/// `advance:` follows), on a run that starts at `start`: nothing is raised
/// there and `dayStart` is never raised on its day — unless the clock
/// declares `raiseAtStart: true`, when the slot occasion and `dayStart` are
/// raised where the run starts.
pub(super) struct RaiseRule {
    pub(super) rule: lute_check::clock_positions::RaiseRule,
    pub(super) start: lute_manifest::clock::ClockAt,
}

impl RaiseRule {
    /// The rule of `occasion`; `None` when the clock does not raise it.
    pub(super) fn of(
        clock: &lute_manifest::clock::ClockDecl,
        occasion: &str,
        start: lute_manifest::clock::ClockAt,
    ) -> Option<Self> {
        lute_check::clock_positions::RaiseRule::of(clock, occasion)
            .map(|rule| RaiseRule { rule, start })
    }

    /// Whether the clock raises the occasion at `at`. `slot_named`: the
    /// column holds the slot the user named — an `advance: day` raises
    /// `dayEnd` at whatever slot the clock stands.
    pub(super) fn raises(
        &self,
        clock: &lute_manifest::clock::ClockDecl,
        at: lute_manifest::clock::ClockAt,
        slot_named: bool,
    ) -> bool {
        self.rule.raises(clock, self.start, at, slot_named)
    }

    /// Where the clock raises `occasion`, in words.
    pub(super) fn describe(&self, clock: &lute_manifest::clock::ClockDecl, occasion: &str) -> String {
        self.rule.describe(clock, occasion, self.start)
    }
}

/// `positions` in words, in clock order: `day 1 (Mon) morning, afternoon and
/// night; day 2 (Tue) morning`.
pub(super) fn describe_positions(
    clock: &lute_manifest::clock::ClockDecl,
    positions: &BTreeSet<lute_manifest::clock::ClockAt>,
) -> String {
    let and = |items: Vec<String>| match items.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    let mut days: Vec<String> = Vec::new();
    let mut day: Vec<lute_manifest::clock::ClockAt> = Vec::new();
    let mut flush = |day: &mut Vec<lute_manifest::clock::ClockAt>| {
        let Some((first, rest)) = day.split_first() else {
            return;
        };
        let mut items = vec![clock.describe(*first)];
        items.extend(
            rest.iter()
                .filter_map(|at| clock.slot_name(at.slot).map(str::to_string)),
        );
        days.push(and(items));
        day.clear();
    };
    for at in positions {
        if day.first().is_some_and(|d| d.day != at.day) {
            flush(&mut day);
        }
        day.push(*at);
    }
    flush(&mut day);
    days.join("; ")
}
