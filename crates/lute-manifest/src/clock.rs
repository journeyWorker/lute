//! dsl 0.24.0 §1: the declared clock.
//!
//! A schema MAY declare one clock over two existing `owner: engine` state
//! paths — an int `day` and an enum `slot` — plus the order of the slots,
//! an optional occasion raised after every advance, and an optional week.
//! The clock stores nothing of its own (D-B): it gives those paths meaning —
//! an order (`clock.index`), read-only aliases (`clock.day`, `clock.slot`),
//! a weekday (`clock.weekday`, `clock.weekdayLabel`), a finite clock's end
//! (`clock.ended`), `once: day|slot`, and `lute play`'s `advance:`.
//!
//! This module is pure data and arithmetic, shared by the checker, the
//! compiler (the IR carries the declaration verbatim), trace and the
//! reference runner, so every tool derives the reserved paths identically.

use serde::{Deserialize, Serialize};

/// `clock.index`: `(day - 1) * len(slots) + slotIndex` — monotone.
pub const CLOCK_INDEX: &str = "clock.index";
/// `clock.weekday`: `(week.first + day - 1) mod week.length`.
pub const CLOCK_WEEKDAY: &str = "clock.weekday";
/// `clock.weekdayLabel`: `week.labels[clock.weekday]`.
pub const CLOCK_WEEKDAY_LABEL: &str = "clock.weekdayLabel";
/// dsl 0.28.0 §5: `clock.day` — the day path's value, read-only.
pub const CLOCK_DAY: &str = "clock.day";
/// dsl 0.28.0 §5: `clock.slot` — the slot path's value, read-only (a clock
/// with a `slot:` only).
pub const CLOCK_SLOT: &str = "clock.slot";
/// dsl 0.28.0 §5: `clock.ended` — `true` once a finite clock ended (from the
/// settle of the `advance:` that ends it until a `newRun` starts it over). A
/// finite clock only. Not a function of the position: the runtime carries it.
pub const CLOCK_ENDED: &str = "clock.ended";
/// dsl 0.27.0 §4 (T2-5): an `advance:` step after a finite clock ended (or
/// from past its last position) — a `lute play` / `lute test` usage error.
pub const E_CLOCK_END: &str = "E-CLOCK-END";

/// `true` for any path rooted at the read-only `clock` root.
pub fn is_clock_path(path: &str) -> bool {
    path.split('.').next() == Some("clock")
}

/// A schema's `clock:` (dsl 0.24.0 §1), exactly as declared. Field order is
/// the IR's serialized order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockDecl {
    /// The day path — `int`, `owner: engine`.
    pub day: String,
    /// The slot path — an enum, `owner: engine`. Absent on a day-granular
    /// clock: every day is one slot, and `slots` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// The slot order; the same members as the slot enum. Empty exactly
    /// when `slot` is absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<String>,
    /// The occasions an advance raises.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raise: Option<ClockRaise>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub week: Option<WeekDecl>,
    /// dsl 0.27.0 §4 (T2-5): the clock's last position — a finite clock
    /// raises its last `dayEnd` there and stops. Absent: the clock never ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<ClockLast>,
    /// dsl 0.27.0 §4: `days: N` — shorthand for `last: { day: N, slot: <the
    /// last slot> }`. Never both with `last:`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub days: Option<u32>,
    /// dsl 0.28.0: the engine raises the clock's slot occasion (and
    /// `raise.dayStart`) itself at the run's first position, where no
    /// `advance:` stops. `false` (the default): nothing is raised there.
    #[serde(
        default,
        rename = "raiseAtStart",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub raise_at_start: bool,
}

/// A finite clock's `last:` — the day and (on a clock with slots) the slot
/// of its last position. `slot` omitted means the day's last slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockLast {
    pub day: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

/// A clock's `raise:` — one occasion (raised after every advance, the map
/// form's `slot`), or a map naming the occasion for each moment.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClockRaise {
    Slot(String),
    Moments(RaiseMoments),
}

/// `raise: { slot, dayStart, dayEnd }` — each optional.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaiseMoments {
    /// Raised once after every advance, where the clock stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// Raised on every day an advance enters, at its first position, before
    /// the clock moves on.
    #[serde(default, rename = "dayStart", skip_serializing_if = "Option::is_none")]
    pub day_start: Option<String>,
    /// Raised on every day an advance leaves, at its last position (the
    /// day not yet advanced), before the clock crosses midnight.
    #[serde(default, rename = "dayEnd", skip_serializing_if = "Option::is_none")]
    pub day_end: Option<String>,
}

impl ClockRaise {
    /// The map form of either spelling.
    pub fn moments(&self) -> RaiseMoments {
        match self {
            ClockRaise::Slot(o) => RaiseMoments {
                slot: Some(o.clone()),
                ..RaiseMoments::default()
            },
            ClockRaise::Moments(m) => m.clone(),
        }
    }
}

/// A clock's `week:`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeekDecl {
    pub length: u32,
    /// The weekday index of day 1 (0-based).
    #[serde(default)]
    pub first: u32,
    /// One display label per weekday, in weekday order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
}

/// A position on the clock: a day and the index of a slot in `slots`.
/// Ordered by time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClockAt {
    pub day: i64,
    pub slot: usize,
}

/// How far a `lute play` `advance:` step moves the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advance {
    /// `advance: slot` (`1`) or `advance: <n>` — `n` slots, wrapping into
    /// the next day.
    Slots(u32),
    /// `advance: day` — the first slot of the next day.
    Day,
    /// dsl 0.26.0 §7 (T2-5): `advance: { to: <slot> }` / `{ to: { weekday,
    /// slot } }` — forward to the next position AFTER the current one on
    /// that weekday (`clock.weekday`) and/or in that slot (its index in
    /// `slots`). Never backward and never zero steps: like every advance it
    /// moves the clock, so already there it goes to the next such position
    /// (tomorrow's night, next week's Friday morning).
    To {
        weekday: Option<i64>,
        slot: Option<usize>,
    },
}

/// A value of one reserved `clock.*` path.
#[derive(Clone, Debug, PartialEq)]
pub enum ClockValue {
    Int(i64),
    Str(String),
}

/// The type of one reserved `clock.*` path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockPathType {
    /// An int (`clock.index`, `clock.day`, `clock.weekday`).
    Int,
    /// One of the week's labels (`clock.weekdayLabel`).
    WeekdayLabel,
    /// One of `slots` (`clock.slot`).
    Slot,
    /// `clock.ended`.
    Bool,
}

impl ClockDecl {
    /// The problems the declaration has on its own, before any path is
    /// resolved: empty or repeated slots, a zero-length week, a `first`
    /// outside the week, a label count that is not the week's length.
    pub fn shape_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        match (&self.slot, self.slots.is_empty()) {
            (Some(_), true) => out.push("`slots:` must list at least one slot".to_string()),
            (None, false) => out.push(
                "`slots:` orders the members of `slot:`, and the clock declares no `slot:` — \
                 declare both, or neither for a clock that counts whole days"
                    .to_string(),
            ),
            _ => {}
        }
        let mut seen = std::collections::BTreeSet::new();
        for s in &self.slots {
            if !seen.insert(s.as_str()) {
                out.push(format!("`slots:` lists `{s}` twice"));
            }
        }
        if self.slot.as_deref() == Some(self.day.as_str()) {
            out.push(format!(
                "`day:` and `slot:` are the same path `{}`",
                self.day
            ));
        }
        if let Some(week) = &self.week {
            if week.length == 0 {
                out.push("`week.length` must be at least 1".to_string());
            } else if week.first >= week.length {
                out.push(format!(
                    "`week.first` is {} but a week of length {} has weekdays 0..{}",
                    week.first,
                    week.length,
                    week.length - 1
                ));
            }
            if !week.labels.is_empty() && week.labels.len() != week.length as usize {
                out.push(format!(
                    "`week.labels` has {} label(s) for a week of length {}",
                    week.labels.len(),
                    week.length
                ));
            }
        }
        match (&self.last, self.days) {
            (Some(_), Some(_)) => out.push(
                "declares both `last:` and `days:` — `days: N` is short for `last: { day: N }`; \
                 keep one"
                    .to_string(),
            ),
            (_, Some(0)) => out.push("`days:` must be at least 1".to_string()),
            (Some(last), None) => {
                if last.day < 1 {
                    out.push(format!(
                        "`last.day` is {} — day 1 is the first day",
                        last.day
                    ));
                }
                match (&last.slot, &self.slot) {
                    (Some(s), None) => out.push(format!(
                        "`last.slot: {s}` — the clock counts whole days (it declares no `slot:`)"
                    )),
                    (Some(s), Some(_))
                        if !self.slots.is_empty() && self.slot_index(s).is_none() =>
                    {
                        out.push(format!(
                            "`last.slot: {s}` is not one of `slots:` ({})",
                            self.slots.join(", ")
                        ))
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        let m = self.raises();
        if self.raise_at_start && m.slot.is_none() && m.day_start.is_none() {
            out.push(
                "`raiseAtStart: true` has nothing to raise: the clock's `raise:` names no slot \
                 occasion and no `dayStart` — declare one, or drop `raiseAtStart`"
                    .to_string(),
            );
        }
        out
    }

    /// dsl 0.27.0 §4: the clock's last position — `None` for a clock that
    /// never ends (or whose `last:` names no position, a shape problem).
    pub fn last_at(&self) -> Option<ClockAt> {
        if let Some(days) = self.days.filter(|d| *d > 0) {
            return Some(ClockAt {
                day: i64::from(days),
                slot: self.slot_count() - 1,
            });
        }
        let last = self.last.as_ref()?;
        let slot = match &last.slot {
            None => self.slot_count() - 1,
            Some(s) => self.slot_index(s)?,
        };
        (last.day >= 1).then_some(ClockAt {
            day: last.day,
            slot,
        })
    }

    /// dsl 0.27.0 §4: `true` when `at` lies past the clock's last position.
    pub fn is_past_end(&self, at: ClockAt) -> bool {
        self.last_at().is_some_and(|last| at > last)
    }

    /// The reserved paths this clock declares, with their types.
    pub fn reserved_paths(&self) -> Vec<(&'static str, ClockPathType)> {
        let mut out = vec![
            (CLOCK_INDEX, ClockPathType::Int),
            (CLOCK_DAY, ClockPathType::Int),
        ];
        if self.slot.is_some() {
            out.push((CLOCK_SLOT, ClockPathType::Slot));
        }
        if let Some(week) = &self.week {
            out.push((CLOCK_WEEKDAY, ClockPathType::Int));
            if !week.labels.is_empty() {
                out.push((CLOCK_WEEKDAY_LABEL, ClockPathType::WeekdayLabel));
            }
        }
        if self.is_finite() {
            out.push((CLOCK_ENDED, ClockPathType::Bool));
        }
        out
    }

    /// `true` for a clock that declares where it ends (`last:` / `days:`).
    pub fn is_finite(&self) -> bool {
        self.last.is_some() || self.days.is_some()
    }

    /// Slots per day: `slots`' length, or 1 for a day-granular clock.
    pub fn slot_count(&self) -> usize {
        self.slots.len().max(1)
    }

    /// The name of the slot at index `i` — `None` on a day-granular clock.
    pub fn slot_name(&self, i: usize) -> Option<&str> {
        self.slots.get(i).map(String::as_str)
    }

    /// The index of `slot` in the order.
    pub fn slot_index(&self, slot: &str) -> Option<usize> {
        self.slots.iter().position(|s| s == slot)
    }

    /// The position a `day` value and a `slot` member name. `None` when the
    /// day is not an integer or — on a clock with a `slot:` path — the slot
    /// is not one of `slots`. A day-granular clock ignores `slot`.
    pub fn at(&self, day: f64, slot: Option<&str>) -> Option<ClockAt> {
        if day.fract() != 0.0 || !day.is_finite() {
            return None;
        }
        let slot = match &self.slot {
            None => 0,
            Some(_) => self.slot_index(slot?)?,
        };
        Some(ClockAt {
            day: day as i64,
            slot,
        })
    }

    /// `clock.index` of a position.
    pub fn index(&self, at: ClockAt) -> i64 {
        (at.day - 1) * self.slot_count() as i64 + at.slot as i64
    }

    /// The position `n` slots after `at` (wrapping into later days), or
    /// the first slot of the next day.
    pub fn advance(&self, at: ClockAt, by: Advance) -> ClockAt {
        match by {
            Advance::Day => ClockAt {
                day: at.day + 1,
                slot: 0,
            },
            Advance::Slots(n) => {
                let len = self.slot_count() as i64;
                let flat = at.slot as i64 + i64::from(n);
                ClockAt {
                    day: at.day + flat / len,
                    slot: (flat % len) as usize,
                }
            }
            Advance::To { weekday, slot } => {
                // The next match strictly after `at`: at most one week of
                // slots to search (the last candidate is `at` one week on).
                // A target outside the clock (refused when the step is
                // planned) stays put.
                let week = self.week.as_ref().map_or(1, |w| i64::from(w.length.max(1)));
                let mut pos = at;
                for _ in 0..week * self.slot_count() as i64 {
                    pos = self.advance(pos, Advance::Slots(1));
                    if weekday.is_none_or(|wd| self.weekday(pos.day) == Some(wd))
                        && slot.is_none_or(|s| s == pos.slot)
                    {
                        return pos;
                    }
                }
                at
            }
        }
    }

    /// The last position of `at`'s day.
    pub fn day_end(&self, at: ClockAt) -> ClockAt {
        ClockAt {
            day: at.day,
            slot: self.slot_count() - 1,
        }
    }

    /// The occasions an advance raises (all `None` without a `raise:`).
    pub fn raises(&self) -> RaiseMoments {
        self.raise
            .as_ref()
            .map(ClockRaise::moments)
            .unwrap_or_default()
    }

    /// `clock.weekday` of `day` (`None` without a `week:`).
    pub fn weekday(&self, day: i64) -> Option<i64> {
        let week = self.week.as_ref().filter(|w| w.length > 0)?;
        Some((i64::from(week.first) + day - 1).rem_euclid(i64::from(week.length)))
    }

    /// dsl 0.27.0 §5: the clock week `day` falls in (`None` without a
    /// `week:`) — week 0 holds day 1, whose weekday is `week.first`, so a
    /// new week starts each time `clock.weekday` returns to `week.first`.
    /// What spends `once: week`.
    pub fn week_of(&self, day: i64) -> Option<i64> {
        let week = self.week.as_ref().filter(|w| w.length > 0)?;
        Some((day - 1).div_euclid(i64::from(week.length)))
    }

    /// `clock.weekdayLabel` of `day` (`None` without week labels).
    pub fn weekday_label(&self, day: i64) -> Option<&str> {
        let week = self.week.as_ref()?;
        let wd = self.weekday(day)?;
        week.labels.get(wd as usize).map(String::as_str)
    }

    /// Every reserved `clock.*` path's value at `at` that the position
    /// decides, in [`Self::reserved_paths`] order — all but `clock.ended`.
    pub fn values(&self, at: ClockAt) -> Vec<(&'static str, ClockValue)> {
        let mut out = vec![
            (CLOCK_INDEX, ClockValue::Int(self.index(at))),
            (CLOCK_DAY, ClockValue::Int(at.day)),
        ];
        if let (Some(_), Some(name)) = (&self.slot, self.slot_name(at.slot)) {
            out.push((CLOCK_SLOT, ClockValue::Str(name.to_string())));
        }
        if let Some(wd) = self.weekday(at.day) {
            out.push((CLOCK_WEEKDAY, ClockValue::Int(wd)));
        }
        if let Some(label) = self.weekday_label(at.day) {
            out.push((CLOCK_WEEKDAY_LABEL, ClockValue::Str(label.to_string())));
        }
        out
    }

    /// `day slot` — how a position reads in a transcript (`3 night`); a
    /// day-granular clock's position is its day alone (`day 3`).
    pub fn describe(&self, at: ClockAt) -> String {
        let slot = match &self.slot {
            None => String::new(),
            Some(_) => format!(
                " {}",
                self.slots.get(at.slot).map(String::as_str).unwrap_or("?")
            ),
        };
        match self.weekday_label(at.day) {
            Some(label) => format!("day {} ({label}){slot}", at.day),
            None => format!("day {}{slot}", at.day),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> ClockDecl {
        serde_yaml::from_str(
            "day: run.day\nslot: run.slot\nslots: [morning, afternoon, night]\n\
             raise: slotStart\nweek: { length: 7, first: 1, labels: [Sun, Mon, Tue, Wed, Thu, Fri, Sat] }\n",
        )
        .unwrap()
    }

    #[test]
    fn index_is_monotone_over_day_and_slot() {
        let c = clock();
        let a = c.at(1.0, Some("morning")).unwrap();
        let b = c.at(1.0, Some("night")).unwrap();
        let d = c.at(2.0, Some("morning")).unwrap();
        assert_eq!((c.index(a), c.index(b), c.index(d)), (0, 2, 3));
        assert!(c.at(1.5, Some("morning")).is_none() && c.at(1.0, Some("dusk")).is_none());
        assert!(
            c.at(1.0, None).is_none(),
            "a clock with a slot path needs a slot"
        );
    }

    #[test]
    fn advance_wraps_slots_into_the_next_day() {
        let c = clock();
        let night = c.at(1.0, Some("night")).unwrap();
        assert_eq!(
            c.advance(night, Advance::Slots(1)),
            ClockAt { day: 2, slot: 0 }
        );
        assert_eq!(
            c.advance(night, Advance::Slots(4)),
            ClockAt { day: 3, slot: 0 }
        );
        let noon = c.at(1.0, Some("afternoon")).unwrap();
        assert_eq!(c.advance(noon, Advance::Day), ClockAt { day: 2, slot: 0 });
    }

    /// dsl 0.26.0 §7 (T2-5): `to` moves forward to the next matching
    /// position — never backward, and never zero steps.
    #[test]
    fn advance_to_moves_forward_to_the_next_matching_position() {
        let c = clock();
        let to = |weekday, slot| Advance::To { weekday, slot };
        // Day 1 is Mon (weekday 1); night is slot 2.
        let mon_noon = c.at(1.0, Some("afternoon")).unwrap();
        assert_eq!(
            c.advance(mon_noon, to(None, Some(2))),
            ClockAt { day: 1, slot: 2 }
        );
        // A morning already past today is tomorrow's.
        assert_eq!(
            c.advance(mon_noon, to(None, Some(0))),
            ClockAt { day: 2, slot: 0 }
        );
        // Fri (weekday 5) morning from Mon: day 5.
        assert_eq!(
            c.advance(mon_noon, to(Some(5), Some(0))),
            ClockAt { day: 5, slot: 0 }
        );
        // Mon morning from Mon afternoon is next week's Mon.
        assert_eq!(
            c.advance(mon_noon, to(Some(1), Some(0))),
            ClockAt { day: 8, slot: 0 }
        );
        // Already there: the next such position, one week on.
        assert_eq!(
            c.advance(mon_noon, to(Some(1), Some(1))),
            ClockAt { day: 8, slot: 1 }
        );
        // A slot alone, already there: tomorrow's.
        assert_eq!(
            c.advance(mon_noon, to(None, Some(1))),
            ClockAt { day: 2, slot: 1 }
        );
    }

    #[test]
    fn the_week_starts_at_first() {
        let c = clock();
        assert_eq!(c.weekday(1), Some(1));
        assert_eq!(c.weekday_label(1), Some("Mon"));
        assert_eq!(c.weekday_label(7), Some("Sun"));
        assert_eq!(c.weekday_label(8), Some("Mon"));
    }

    #[test]
    fn shape_problems_name_each_defect() {
        let c: ClockDecl = serde_yaml::from_str(
            "day: run.d\nslot: run.s\nslots: [a, a]\nweek: { length: 3, first: 3, labels: [x] }\n",
        )
        .unwrap();
        let p = c.shape_problems();
        assert_eq!(p.len(), 3, "{p:?}");
        assert!(
            serde_yaml::from_str::<ClockDecl>("day: a\nslot: b\nslots: [x]\norder: [x]\n").is_err()
        );
        let stray: ClockDecl = serde_yaml::from_str("day: a\nslots: [x]\n").unwrap();
        assert_eq!(stray.shape_problems().len(), 1, "`slots:` without `slot:`");
    }

    #[test]
    fn a_day_granular_clock_counts_whole_days() {
        let c: ClockDecl = serde_yaml::from_str("day: run.day\nraise: arrive\n").unwrap();
        assert!(c.shape_problems().is_empty());
        let d3 = c.at(3.0, None).unwrap();
        assert_eq!(c.index(d3), 2);
        assert_eq!(
            c.advance(d3, Advance::Slots(2)),
            ClockAt { day: 5, slot: 0 }
        );
        assert_eq!(c.advance(d3, Advance::Day), ClockAt { day: 4, slot: 0 });
        assert_eq!(c.describe(d3), "day 3");
        assert_eq!(c.raises().slot.as_deref(), Some("arrive"));
    }

    #[test]
    fn raise_is_one_occasion_or_a_map_of_moments() {
        let c: ClockDecl = serde_yaml::from_str(
            "day: run.day\nslot: run.slot\nslots: [a]\nraise: { slot: slotStart, dayEnd: dayEnd }\n",
        )
        .unwrap();
        let m = c.raises();
        assert_eq!(
            (
                m.slot.as_deref(),
                m.day_start.as_deref(),
                m.day_end.as_deref()
            ),
            (Some("slotStart"), None, Some("dayEnd"))
        );
        assert!(
            serde_yaml::from_str::<ClockDecl>("day: d\nraise: { dusk: x }\n").is_err(),
            "an unknown moment is refused"
        );
    }

    /// dsl 0.27.0 §4 (T2-5): `last:` / `days:` bound the clock.
    #[test]
    fn a_finite_clock_knows_its_last_position() {
        let base = "day: run.night\nslot: run.hour\nslots: [h23, h00, h05]\n";
        let parse =
            |extra: &str| -> ClockDecl { serde_yaml::from_str(&format!("{base}{extra}")).unwrap() };
        let last = parse("last: { day: 1, slot: h05 }\n");
        assert!(last.shape_problems().is_empty());
        assert_eq!(last.last_at(), Some(ClockAt { day: 1, slot: 2 }));
        let days = parse("days: 1\n");
        assert!(days.shape_problems().is_empty());
        assert_eq!(
            days.last_at(),
            last.last_at(),
            "`days: 1` ≡ the last slot of day 1"
        );
        // `last.slot` omitted: the day's last slot.
        assert_eq!(
            parse("last: { day: 2 }\n").last_at(),
            Some(ClockAt { day: 2, slot: 2 })
        );
        let early = parse("last: { day: 1, slot: h00 }\n");
        assert!(!early.is_past_end(ClockAt { day: 1, slot: 1 }));
        assert!(early.is_past_end(ClockAt { day: 1, slot: 2 }));
        assert!(
            parse("").last_at().is_none(),
            "a clock without `last:` never ends"
        );
        assert!(!parse("").is_past_end(ClockAt { day: 99, slot: 0 }));
        let day_clock: ClockDecl = serde_yaml::from_str("day: run.day\ndays: 3\n").unwrap();
        assert_eq!(day_clock.last_at(), Some(ClockAt { day: 3, slot: 0 }));
        // The IR form is unchanged for a clock that declares neither.
        assert!(!serde_json::to_string(&parse("")).unwrap().contains("last"));
    }

    #[test]
    fn finite_clock_shape_problems_name_each_defect() {
        let problems = |yaml: &str| {
            serde_yaml::from_str::<ClockDecl>(yaml)
                .unwrap()
                .shape_problems()
        };
        let slotted = "day: d\nslot: s\nslots: [a, b]\n";
        let p = problems(&format!("{slotted}last: {{ day: 1, slot: c }}\n"));
        assert!(
            p.len() == 1 && p[0].contains("`last.slot: c` is not one of `slots:` (a, b)"),
            "{p:?}"
        );
        let p = problems(&format!("{slotted}last: {{ day: 1, slot: b }}\ndays: 1\n"));
        assert!(
            p.len() == 1 && p[0].contains("both `last:` and `days:`"),
            "{p:?}"
        );
        let p = problems(&format!("{slotted}days: 0\n"));
        assert!(
            p.len() == 1 && p[0].contains("`days:` must be at least 1"),
            "{p:?}"
        );
        let p = problems(&format!("{slotted}last: {{ day: 0 }}\n"));
        assert!(p.len() == 1 && p[0].contains("`last.day` is 0"), "{p:?}");
        let p = problems("day: d\nlast: { day: 2, slot: a }\n");
        assert!(p.len() == 1 && p[0].contains("counts whole days"), "{p:?}");
        assert!(
            serde_yaml::from_str::<ClockDecl>(&format!("{slotted}last: {{ day: 1, hour: a }}\n"))
                .is_err(),
            "an unknown `last:` key is refused"
        );
    }
}
