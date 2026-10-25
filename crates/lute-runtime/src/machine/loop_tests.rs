//! dsl 0.51.0 §3.7: bounded backward jumps on hand-built command streams —
//! counters, reset by position, the shared head and the runaway guard.

use serde_json::{json, Value as Json};

use crate::driver::{
    BridgeCall, BridgeReply, Driver, Forced, Menu, OnUnknown, Pick, UnknownSite, Verdict,
};
use crate::machine::{Machine, Seed};

/// Records every line's text and every jump observation.
#[derive(Default)]
struct Recorder {
    lines: Vec<String>,
    jumps: Vec<Json>,
}

impl Driver for Recorder {
    fn choose(&mut self, _menu: &Menu<'_>) -> Pick {
        Pick::Unscripted { scripted: 0 }
    }
    fn forced(&mut self, _menu: &Menu<'_>, _option: &str, _verdict: &Verdict) -> Forced {
        Forced::Refuse
    }
    fn bridge(&mut self, _call: &BridgeCall<'_>) -> BridgeReply {
        BridgeReply::Unanswered
    }
    fn unknown(&mut self, _site: &UnknownSite<'_>) -> OnUnknown {
        OnUnknown::Halt
    }
    fn emit(&mut self, rec: Json) {
        if rec["kind"] == "line" {
            self.lines
                .push(rec["text"].as_str().unwrap_or("").to_string());
        }
    }
    fn observe(&mut self, rec: Json) {
        if rec["kind"] == "jump" {
            self.jumps.push(rec);
        }
    }
}

fn pos(i: usize) -> String {
    format!("001-{:04}", (i + 1) * 100)
}

fn line(i: usize, text: &str) -> Json {
    json!({ "kind": "line", "family": "content", "position": pos(i), "role": "narration",
            "speaker": "narrator", "text": text })
}

fn jump(i: usize, to: usize, max: Option<u8>) -> Json {
    let mut j =
        json!({ "kind": "jump", "family": "control", "position": pos(i), "target": pos(to) });
    if let Some(max) = max {
        j["max"] = json!(max);
    }
    j
}

/// Run a scene over `commands`: its lines, jump observations and outcome.
fn run(commands: Vec<Json>) -> (Recorder, Result<(), String>) {
    let art = json!({ "kind": "scene", "commands": commands });
    let mut m = Machine::new(&art, Seed::default(), Recorder::default());
    let out = m.run().map_err(|e| e.to_string());
    let rec = m.driver();
    let rec = Recorder {
        lines: rec.lines.clone(),
        jumps: rec.jumps.clone(),
    };
    (rec, out)
}

fn texts(lines: &[String]) -> String {
    lines.join(" ")
}

/// An inner loop's counter resets each time the outer loop re-enters its
/// head from outside the inner region, and an inner take does not reset it.
#[test]
fn inner_counter_resets_on_outside_arrival_only() {
    let (rec, out) = run(vec![
        line(0, "o"),
        line(1, "i"),
        jump(2, 1, Some(2)),
        jump(3, 0, Some(1)),
        line(4, "end"),
    ]);
    out.unwrap();
    assert_eq!(texts(&rec.lines), "o i i i o i i i end");
}

/// Two bounded jumps sharing a head keep their own counters: the inner
/// one's take keeps the outer's count, the outer's take resets the inner's.
#[test]
fn a_shared_head_keeps_one_counter_per_jump() {
    let (rec, out) = run(vec![
        line(0, "h"),
        jump(1, 0, Some(1)),
        jump(2, 0, Some(2)),
        line(3, "end"),
    ]);
    out.unwrap();
    assert_eq!(texts(&rec.lines), "h h h h h h end");
    let outer: Vec<String> = rec
        .jumps
        .iter()
        .filter(|j| j["position"] == pos(2))
        .map(Json::to_string)
        .collect();
    assert_eq!(
        outer,
        [
            json!({ "kind": "jump", "position": pos(2), "taken": 1, "max": 2 }).to_string(),
            json!({ "kind": "jump", "position": pos(2), "taken": 2, "max": 2 }).to_string(),
            json!({ "kind": "jump", "position": pos(2), "exhausted": true, "max": 2 }).to_string(),
        ]
    );
}

/// The runaway guard counts from the last bounded take: a 99 × 99 nested
/// loop runs to completion although its steps exceed the per-range limit.
#[test]
fn a_99_by_99_loop_completes() {
    let (rec, out) = run(vec![
        line(0, "a"),
        line(1, "b"),
        jump(2, 1, Some(99)),
        jump(3, 0, Some(99)),
        line(4, "end"),
    ]);
    out.unwrap();
    assert_eq!(rec.lines.iter().filter(|l| *l == "b").count(), 100 * 100);
    assert_eq!(rec.lines.iter().filter(|l| *l == "a").count(), 100);
    assert_eq!(rec.lines.last().map(String::as_str), Some("end"));
}

/// A backward jump without `max` in a hand-built artifact still trips the
/// guard.
#[test]
fn a_cycle_without_max_trips_the_guard() {
    let (_, out) = run(vec![line(0, "a"), jump(1, 0, None)]);
    let err = out.unwrap_err();
    assert!(err.contains("did not terminate"), "{err}");
}

/// A cycle that a bounded take does not close still trips the guard: the
/// guard restarts at a take, and between takes the walk must stay acyclic.
#[test]
fn an_unbounded_cycle_inside_a_loop_trips_the_guard() {
    let (_, out) = run(vec![
        line(0, "a"),
        line(1, "b"),
        jump(2, 1, None),
        jump(3, 0, Some(3)),
    ]);
    assert!(out.unwrap_err().contains("did not terminate"));
}

/// `max` outside 1..=99 is a malformed record.
#[test]
fn a_malformed_max_is_refused() {
    for bad in [json!(0), json!(100), json!("2"), json!(-1)] {
        let mut j = jump(1, 0, None);
        j["max"] = bad.clone();
        let (_, out) = run(vec![line(0, "a"), j]);
        let err = out.unwrap_err();
        assert!(
            err.contains("`max` must be an integer from 1 to 99"),
            "{bad}: {err}"
        );
    }
}
