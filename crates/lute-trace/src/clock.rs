//! dsl 0.24.0 §1: the runtime values of the reserved `clock.*` paths —
//! derived from the clock's `day` / `slot` state, never stored. Shared by the
//! trace walk and the reference runner (`lute run` / `lute play`), so both
//! read one clock identically.

use std::collections::BTreeMap;

use lute_manifest::clock::{ClockAt, ClockDecl, ClockValue};

use crate::Value;

/// The clock's position in `state` (`None` when the day is not an integer
/// or the slot is not one of the clock's slots).
pub fn position(clock: &ClockDecl, state: &BTreeMap<String, Value>) -> Option<ClockAt> {
    match (state.get(&clock.day), state.get(&clock.slot)) {
        (Some(Value::Num(day)), Some(Value::Str(slot))) => clock.at(*day, slot),
        _ => None,
    }
}

/// Every reserved `clock.*` path and its value at `at`.
pub fn values(clock: &ClockDecl, at: ClockAt) -> Vec<(String, Value)> {
    clock
        .values(at)
        .into_iter()
        .map(|(path, v)| {
            let v = match v {
                ClockValue::Num(n) => Value::Num(n as f64),
                ClockValue::Str(s) => Value::Str(s),
            };
            (path.to_string(), v)
        })
        .collect()
}

/// Write the reserved `clock.*` values `state`'s `day` / `slot` imply into
/// `state` (removing them when the position is undecided).
pub fn refresh(clock: &ClockDecl, state: &mut BTreeMap<String, Value>) {
    state.retain(|k, _| !lute_manifest::clock::is_clock_path(k));
    if let Some(at) = position(clock, state) {
        state.extend(values(clock, at));
    }
}
