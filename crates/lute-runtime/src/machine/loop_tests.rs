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

/// A refused stream: nothing runs, and the refusal is `Code`'s load fault
/// (`ExecProject` refuses the same document at load).
fn refused(commands: Vec<Json>) -> String {
    let art = json!({ "kind": "scene", "commands": commands.clone() });
    let fault = crate::machine::Code::of(&art).fault;
    let (rec, out) = run(commands);
    let err = out.unwrap_err();
    assert_eq!(Some(&err), fault.as_ref());
    assert!(rec.lines.is_empty(), "a refused stream runs nothing: {err}");
    err
}

/// A backward jump without `max` in a hand-built artifact is refused at
/// load: a cycle that never ends.
#[test]
fn a_cycle_without_max_is_refused_at_load() {
    let err = refused(vec![line(0, "a"), jump(1, 0, None)]);
    assert!(
        err.contains("`jump` record at 001-0200: its `target` goes back to 001-0100"),
        "{err}"
    );
    let err = refused(vec![line(0, "a"), jump(1, 1, None)]);
    assert!(
        err.contains("goes back to 001-0200"),
        "a jump to itself: {err}"
    );
}

/// A required target that is missing or not a position string is refused
/// at load (S3 re-review 2): the executor never falls back to another
/// record. The reviewer's stream — a bounded loop, then a `jump` without
/// `target` — would otherwise go back to the first record forever.
#[test]
fn a_missing_or_malformed_target_is_refused_at_load() {
    let mut no_target = jump(2, 0, None);
    no_target.as_object_mut().unwrap().remove("target");
    let err = refused(vec![line(0, "a"), jump(1, 0, Some(1)), no_target]);
    assert!(
        err.contains("`jump` record at 001-0300: `target` is missing or not a position"),
        "{err}"
    );
    let mut numeric = jump(1, 0, None);
    numeric["target"] = json!(100);
    refused(vec![line(0, "a"), numeric]);
    let no_converge = json!({ "kind": "match", "family": "control", "position": pos(1),
                              "arms": [] });
    let err = refused(vec![line(0, "a"), no_converge, line(2, "b")]);
    assert!(err.contains("`converge` is missing"), "{err}");
    let bad_option = json!({ "kind": "choice", "family": "control", "position": pos(1),
                             "branchId": "b", "options": [{ "id": "x" }], "converge": pos(2) });
    let err = refused(vec![line(0, "a"), bad_option, line(2, "b")]);
    assert!(err.contains("`options[].target` is missing"), "{err}");
}

/// Every control transfer is checked, not only `jump`: an empty-arm `match`
/// whose `otherwise` goes back around a bounded loop would re-enter it from
/// outside forever (S3 re-review), and is refused at load.
#[test]
fn a_backward_match_transfer_is_refused_at_load() {
    let back = json!({ "kind": "match", "family": "control", "position": pos(3),
                       "arms": [], "otherwise": pos(0), "converge": pos(4) });
    let err = refused(vec![line(0, "a"), line(1, "b"), jump(2, 1, Some(1)), back]);
    assert!(
        err.contains("`match` record at 001-0400: its `otherwise` goes back to 001-0100"),
        "{err}"
    );
}

/// [`super::transfers`] classifies every kind the runtime executes, so a new
/// kind cannot skip the load check.
#[test]
fn every_command_kind_lists_its_transfers() {
    for kind in super::COMMAND_KINDS {
        assert!(
            super::transfers(&json!({ "kind": kind })).is_some(),
            "`{kind}` is not classified"
        );
    }
    assert!(super::transfers(&json!({ "kind": "hologram" })).is_none());
}

/// An unbounded backward jump around a bounded loop would re-enter the loop
/// from outside forever, each take restarting the guard: refused at load.
#[test]
fn an_unbounded_cycle_around_a_loop_is_refused_at_load() {
    let err = refused(vec![
        line(0, "a"),
        line(1, "b"),
        jump(2, 1, Some(1)),
        jump(3, 0, None),
    ]);
    assert!(err.contains("at 001-0400"), "{err}");
    refused(vec![
        line(0, "a"),
        line(1, "b"),
        jump(2, 1, None),
        jump(3, 0, Some(3)),
    ]);
}

/// Two loops that overlap without nesting are refused at load.
#[test]
fn crossing_loops_are_refused_at_load() {
    // [0, 2] and [1, 3] cross.
    let err = refused(vec![
        line(0, "a"),
        line(1, "b"),
        jump(2, 0, Some(1)),
        jump(3, 1, Some(1)),
    ]);
    assert!(
        err.contains("001-0300 and 001-0400: their loops overlap without nesting"),
        "{err}"
    );
}

/// An empty loop (its label right before its jump, so the jump targets
/// itself) inside an outer loop starts over on each outer iteration.
#[test]
fn a_self_targeting_loop_resets_on_outside_arrival() {
    let (rec, out) = run(vec![
        line(0, "o"),
        jump(1, 1, Some(2)),
        jump(2, 0, Some(1)),
        line(3, "end"),
    ]);
    out.unwrap();
    assert_eq!(texts(&rec.lines), "o o end");
    let inner: Vec<String> = rec
        .jumps
        .iter()
        .filter(|j| j["position"] == pos(1))
        .map(Json::to_string)
        .collect();
    let taken = |n: u8| json!({ "kind": "jump", "position": pos(1), "taken": n, "max": 2 });
    let spent = json!({ "kind": "jump", "position": pos(1), "exhausted": true, "max": 2 });
    assert_eq!(
        inner,
        [taken(1), taken(2), spent.clone(), taken(1), taken(2), spent].map(|j| j.to_string())
    );
}

/// `max` outside 1..=99 is a malformed record, refused at load.
#[test]
fn a_malformed_max_is_refused() {
    for bad in [json!(0), json!(100), json!("2"), json!(-1)] {
        let mut j = jump(1, 0, None);
        j["max"] = bad.clone();
        let err = refused(vec![line(0, "a"), j]);
        assert!(
            err.contains("`max` must be an integer from 1 to 99"),
            "{bad}: {err}"
        );
    }
}
