//! dsl 0.24.0 §1: the runtime values of the reserved `clock.*` paths —
//! derived from the clock's `day` / `slot` state, never stored. Shared by the
//! trace walk and the reference runner (`lute run` / `lute play`), so both
//! read one clock identically.

use std::collections::BTreeMap;

use lute_manifest::clock::{ClockAt, ClockDecl, ClockValue, CLOCK_ENDED};

use crate::Value;

/// The clock's position in `state` (`None` when the day is not an integer
/// or the slot is not one of the clock's slots; a day-granular clock reads
/// its day alone).
pub fn position(clock: &ClockDecl, state: &BTreeMap<String, Value>) -> Option<ClockAt> {
    let day = match state.get(&clock.day) {
        Some(Value::Int(day)) => *day,
        Some(Value::Double(day)) if day.fract() == 0.0 => *day as i64,
        _ => return None,
    };
    match clock.slot.as_ref().map(|s| state.get(s)) {
        None => clock.at(day as f64, None),
        Some(Some(Value::Str(slot))) => clock.at(day as f64, Some(slot)),
        Some(_) => None,
    }
}

/// Every reserved `clock.*` path and its value at `at`.
pub fn values(clock: &ClockDecl, at: ClockAt) -> Vec<(String, Value)> {
    clock
        .values(at)
        .into_iter()
        .map(|(path, v)| {
            let v = match v {
                ClockValue::Int(n) => Value::Int(n),
                ClockValue::Str(s) => Value::Str(s),
            };
            (path.to_string(), v)
        })
        .collect()
}

/// Write the reserved `clock.*` values `state`'s `day` / `slot` imply into
/// `state` (removing them when the position is undecided). A finite clock's
/// `clock.ended` is no function of the position: it keeps its value, `false`
/// until [`set_ended`] says otherwise.
pub fn refresh(clock: &ClockDecl, state: &mut BTreeMap<String, Value>) {
    let ended = state.remove(CLOCK_ENDED);
    state.retain(|k, _| !lute_manifest::clock::is_clock_path(k));
    if let Some(at) = position(clock, state) {
        state.extend(values(clock, at));
    }
    if clock.is_finite() {
        state.insert(CLOCK_ENDED.to_string(), ended.unwrap_or(Value::Bool(false)));
    }
}

/// dsl 0.28.0 §5: whether a finite clock ended (`clock.ended`).
pub fn ended(state: &BTreeMap<String, Value>) -> bool {
    state.get(CLOCK_ENDED) == Some(&Value::Bool(true))
}

/// dsl 0.28.0 §5: end a finite clock — or (`false`, a `newRun`) start it
/// over. A clock that never ends has no `clock.ended`.
pub fn set_ended(clock: &ClockDecl, state: &mut BTreeMap<String, Value>, ended: bool) {
    if clock.is_finite() {
        state.insert(CLOCK_ENDED.to_string(), Value::Bool(ended));
    }
}

/// Whether a `newRun` starts the clock over: its day path is run-tier, so
/// the reset puts it back at its default day. A clock whose day is `user.*`
/// or `app.*` keeps its position — and with it its `once: day|slot|week`
/// spends and its end — across runs.
pub fn restarts_each_run(clock: &ClockDecl) -> bool {
    clock.day.starts_with("run.")
}
