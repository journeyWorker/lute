//! dsl 0.24.0 §1: the declared clock.
//!
//! A schema MAY declare one clock over two existing `owner: engine` state
//! paths — a number `day` and an enum `slot` — plus the order of the slots,
//! an optional occasion raised after every advance, and an optional week.
//! The clock stores nothing of its own (D-B): it gives those paths meaning —
//! an order (`clock.index`), a weekday (`clock.weekday`,
//! `clock.weekdayLabel`), `once: day|slot`, and `lute play`'s `advance:`.
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

/// `true` for any path rooted at the read-only `clock` root.
pub fn is_clock_path(path: &str) -> bool {
    path.split('.').next() == Some("clock")
}

/// A schema's `clock:` (dsl 0.24.0 §1), exactly as declared. Field order is
/// the IR's serialized order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockDecl {
    /// The day path — `number`, `owner: engine`.
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
}

/// A value of one reserved `clock.*` path.
#[derive(Clone, Debug, PartialEq)]
pub enum ClockValue {
    Num(i64),
    Str(String),
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
        out
    }

    /// The reserved paths this clock declares, with their types: `true` for
    /// a number, `false` for a string.
    pub fn reserved_paths(&self) -> Vec<(&'static str, bool)> {
        let mut out = vec![(CLOCK_INDEX, true)];
        if let Some(week) = &self.week {
            out.push((CLOCK_WEEKDAY, true));
            if !week.labels.is_empty() {
                out.push((CLOCK_WEEKDAY_LABEL, false));
            }
        }
        out
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

    /// `clock.weekdayLabel` of `day` (`None` without week labels).
    pub fn weekday_label(&self, day: i64) -> Option<&str> {
        let week = self.week.as_ref()?;
        let wd = self.weekday(day)?;
        week.labels.get(wd as usize).map(String::as_str)
    }

    /// Every reserved `clock.*` path's value at `at`, in
    /// [`Self::reserved_paths`] order.
    pub fn values(&self, at: ClockAt) -> Vec<(&'static str, ClockValue)> {
        let mut out = vec![(CLOCK_INDEX, ClockValue::Num(self.index(at)))];
        if let Some(wd) = self.weekday(at.day) {
            out.push((CLOCK_WEEKDAY, ClockValue::Num(wd)));
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
}
