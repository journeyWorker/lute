//! Beat selection semantics shared by the checker and the runtime.

use std::collections::BTreeMap;

use crate::schema::OccasionDecl;

/// dsl 0.24.0 §6: whether a beat's `target` restricts its candidacy on
/// occasion `on` — every occasion but one DECLARED without a target, where an
/// entry's `target=` is metadata. (An undeclared occasion keeps the 0.21
/// shape-only meaning: a target restricts.) The one rule the checker's beat
/// passes and `lute play`'s candidate filter share.
pub fn beat_target_restricts(on: &str, occasions: &BTreeMap<String, OccasionDecl>) -> bool {
    occasions.get(on).is_none_or(|d| d.target.takes_target())
}

/// dsl 0.26.0 §5: the member a `target="kind:<kind>"` beat was raised for —
/// readable in its `when`, guards and text, typed by the kind.
pub const OCCASION_TARGET: &str = "occasion.target";

pub fn strict_subset(a: &[String], b: &[String]) -> bool {
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
