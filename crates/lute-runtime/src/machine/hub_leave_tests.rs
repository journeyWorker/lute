//! dsl 0.52.0 §5.3: jumps that leave hubs, on hand-built command streams —
//! the visits a `leaves` count ends, and the load check that confines the
//! count to the hub ranges around the jump (Review052 F1).

use std::collections::BTreeMap;

use serde_json::{json, Value as Json};

use crate::driver::{
    BridgeCall, BridgeReply, Driver, Forced, Menu, OnUnknown, Pick, UnknownSite, Verdict,
};
use crate::machine::{Machine, Seed};

/// Answers each hub from its scripted list, one pick per presentation, and
/// records every line and hub presentation.
#[derive(Default)]
struct Script {
    picks: BTreeMap<String, Vec<&'static str>>,
    lines: Vec<String>,
}

impl Driver for Script {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        match self.picks.get_mut(menu.id) {
            Some(list) if !list.is_empty() => Pick::Option(list.remove(0).to_string()),
            _ => Pick::Unscripted { scripted: 0 },
        }
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
        match rec["kind"].as_str() {
            Some("line") => self
                .lines
                .push(rec["text"].as_str().unwrap_or("").to_string()),
            Some("hub") => self.lines.push(format!(
                "[{} {}]",
                rec["hub"].as_str().unwrap_or(""),
                rec["chose"]
            )),
            Some("hubReturn") => self.lines.push("[return]".to_string()),
            _ => {}
        }
    }
    fn observe(&mut self, _rec: Json) {}
}

fn pos(i: usize) -> String {
    format!("001-{:04}", (i + 1) * 100)
}

fn line(i: usize, text: &str) -> Json {
    json!({ "kind": "line", "family": "content", "position": pos(i), "role": "narration",
            "speaker": "narrator", "text": text })
}

fn jump(i: usize, to: usize) -> Json {
    json!({ "kind": "jump", "family": "control", "position": pos(i), "target": pos(to) })
}

fn leave(i: usize, to: usize, leaves: u32) -> Json {
    let mut j = jump(i, to);
    j["leaves"] = json!(leaves);
    j
}

/// A hub record at `i`: `(option id, its start, exit)` in order, an
/// optional `<return>` start, and its converge.
fn hub(
    i: usize,
    id: &str,
    options: &[(&str, usize, bool)],
    back: Option<usize>,
    converge: usize,
) -> Json {
    let options: Vec<Json> = options
        .iter()
        .map(|&(oid, at, exit)| {
            let mut o = json!({ "id": oid, "target": pos(at) });
            if exit {
                o["exit"] = json!(true);
            }
            o
        })
        .collect();
    let mut h = json!({ "kind": "hub", "family": "control", "position": pos(i), "id": id,
                        "selectionKey": format!("scene.choices.{id}"), "options": options,
                        "converge": pos(converge) });
    if let Some(back) = back {
        h["return"] = json!(pos(back));
    }
    h
}

/// Run a scene over `commands` with `picks`: its lines and outcome.
fn run(
    commands: Vec<Json>,
    picks: &[(&str, &[&'static str])],
) -> (Vec<String>, Result<(), String>) {
    let art = json!({ "kind": "scene", "commands": commands });
    let script = Script {
        picks: picks
            .iter()
            .map(|(hub, list)| ((*hub).to_string(), list.to_vec()))
            .collect(),
        lines: Vec::new(),
    };
    let mut m = Machine::new(&art, Seed::default(), script);
    let out = m.run().map_err(|e| e.to_string());
    (m.driver().lines.clone(), out)
}

/// The #216 scene: a non-`exit` option jumps past the hub.
fn issue_216(take: Json) -> Vec<Json> {
    vec![
        line(0, "Before the hub."),
        hub(
            1,
            "desk",
            &[("take", 2, false), ("leave", 4, true)],
            None,
            6,
        ),
        line(2, "Taking it, then jumping past the hub."),
        take,
        line(4, "Leaving."),
        jump(5, 6),
        line(6, "After the hub."),
    ]
}

#[test]
fn a_leaving_jump_ends_the_hub_visit() {
    let (lines, out) = run(issue_216(leave(3, 6, 1)), &[("desk", &["take", "leave"])]);
    out.unwrap();
    assert_eq!(
        lines,
        [
            "Before the hub.",
            "[desk \"take\"]",
            "Taking it, then jumping past the hub.",
            "After the hub."
        ]
    );
    // Without `leaves` (the 0.51 record), the hub presents again.
    let (lines, out) = run(issue_216(jump(3, 6)), &[("desk", &["take", "leave"])]);
    out.unwrap();
    assert!(lines.contains(&"Leaving.".to_string()), "{lines:?}");
}

/// An outer hub whose option `a` holds an inner hub; the jump at 3 in the
/// inner option `x` goes to `to` with `leaves`. The outer hub has an exit
/// option after `a`, so `a`'s range ends before the outer converge.
fn nested(to: usize, leaves: u32) -> Vec<Json> {
    vec![
        hub(0, "outer", &[("a", 1, false), ("out", 8, true)], None, 10),
        hub(1, "inner", &[("x", 2, false), ("y", 4, true)], None, 6),
        line(2, "x"),
        leave(3, to, leaves),
        line(4, "y"),
        jump(5, 6),
        line(6, "inner done"),
        line(7, "a done"),
        line(8, "out"),
        jump(9, 10),
        line(10, "after"),
    ]
}

#[test]
fn leaving_two_hubs_continues_after_both() {
    let (lines, out) = run(nested(10, 2), &[("outer", &["a"]), ("inner", &["x"])]);
    out.unwrap();
    assert_eq!(lines, ["[outer \"a\"]", "[inner \"x\"]", "x", "after"]);
}

#[test]
fn leaving_the_inner_hub_only_presents_the_outer_hub_again() {
    let (lines, out) = run(nested(6, 1), &[("outer", &["a", "out"]), ("inner", &["x"])]);
    out.unwrap();
    assert_eq!(
        lines,
        [
            "[outer \"a\"]",
            "[inner \"x\"]",
            "x",
            "inner done",
            "a done",
            "[outer \"out\"]",
            "out",
            "after"
        ]
    );
}

/// A leave from the `<return>` body ends the visit: no presentation again.
#[test]
fn a_leave_from_the_return_body_ends_the_visit() {
    let commands = vec![
        hub(0, "h", &[("a", 1, false), ("out", 3, true)], Some(5), 7),
        line(1, "a"),
        line(2, "a again"),
        line(3, "out"),
        jump(4, 7),
        line(5, "back"),
        leave(6, 7, 1),
        line(7, "after"),
    ];
    let (lines, out) = run(commands, &[("h", &["a", "out"])]);
    out.unwrap();
    assert_eq!(
        lines,
        ["[h \"a\"]", "a", "a again", "[return]", "back", "after"]
    );
}

fn refused(commands: Vec<Json>) -> String {
    let (lines, out) = run(commands, &[]);
    assert!(lines.is_empty(), "a refused stream runs nothing: {lines:?}");
    out.expect_err("refused at load")
}

/// Review052 F1: two hubs around the jump, `leaves: 1`, a target after
/// both — the outer hub would present again (#216 recreated).
#[test]
fn an_undercount_is_refused_at_load() {
    assert_eq!(
        refused(nested(10, 1)),
        "malformed `jump` record at 001-0400: `leaves` is 1, but reaching its target 001-1100 \
         leaves 2 hub option or `<return>` bodies"
    );
}

#[test]
fn an_overcount_is_refused_at_load() {
    assert_eq!(
        refused(nested(6, 2)),
        "malformed `jump` record at 001-0400: `leaves` is 2, but reaching its target 001-0700 \
         leaves 1 hub option or `<return>` bodies"
    );
}

/// A target inside the sibling option `y`: no count reaches it.
#[test]
fn a_sibling_target_is_refused_at_load() {
    assert_eq!(
        refused(nested(5, 1)),
        "malformed `jump` record at 001-0400: `leaves` is 1, but its target 001-0600 lies in a \
         body the jump is not in"
    );
}

#[test]
fn max_with_leaves_is_refused_at_load() {
    let mut commands = issue_216(leave(3, 6, 1));
    commands[3]["max"] = json!(2);
    assert_eq!(
        refused(commands),
        "malformed `jump` record at 001-0400: it carries both `max` and `leaves`, but a bounded \
         jump never leaves a hub"
    );
}

#[test]
fn a_count_that_is_no_positive_integer_is_refused_at_load() {
    for bad in [json!(0), json!(-1), json!("1"), json!(1.5)] {
        let mut commands = issue_216(leave(3, 6, 1));
        commands[3]["leaves"] = bad.clone();
        assert_eq!(
            refused(commands),
            format!(
                "malformed `jump` record at 001-0400: `leaves` must be a positive integer, got \
                 {bad}"
            )
        );
    }
}

/// The top-level range never receives a leave: no hub encloses the jump,
/// so any count is refused before the walk starts.
#[test]
fn a_leave_outside_every_hub_is_refused_at_load() {
    assert_eq!(
        refused(vec![
            line(0, "a"),
            leave(1, 3, 1),
            line(2, "b"),
            line(3, "c")
        ]),
        "malformed `jump` record at 001-0200: `leaves` is 1, but reaching its target 001-0400 \
         leaves 0 hub option or `<return>` bodies"
    );
}

/// S3 review P1: a jump from outside the hub into its option body runs the
/// body without a hub visit, so the leave inside it would reach no visit to
/// end. Refused at load (before 0.52's check, the walk reached a panic).
#[test]
fn a_transfer_into_a_hub_body_from_outside_is_refused_at_load() {
    assert_eq!(
        refused(vec![
            jump(0, 2),
            hub(1, "h", &[("a", 2, false)], None, 3),
            leave(2, 3, 1),
            line(3, "after"),
        ]),
        "malformed `jump` record at 001-0100: its `target` goes to 001-0300, inside a hub \
         option or `<return>` body it is not in; a body is entered only through its `hub` \
         record"
    );
}

/// S4 review P2: a declaration's body start ends no range, so landing at
/// the end of the declaration's segment is no exemption — the reviewer's
/// lore artifact (entry `e`'s body is the hub's option body) is refused at
/// load instead of reaching `run_body`'s assertion.
#[test]
fn a_body_start_inside_a_hub_body_is_refused_at_load() {
    let art = json!({ "kind": "lore", "commands": [
        { "kind": "entry", "family": "declaration", "position": pos(0), "id": "e",
          "body": pos(2) },
        hub(1, "h", &[("a", 2, false)], None, 3),
        leave(2, 3, 1),
        line(3, "after"),
    ] });
    let mut m = Machine::new(&art, Seed::default(), Script::default());
    assert_eq!(
        m.run()
            .map_err(|e| e.to_string())
            .expect_err("refused at load"),
        "malformed `entry` record at 001-0100: its `body` goes to 001-0300, inside a hub \
         option or `<return>` body it is not in; a body is entered only through its `hub` \
         record"
    );
    assert!(m.driver().lines.is_empty());
}

/// The end of an option body is the next option's start (bodies are laid
/// out consecutively): a transfer there from inside the option ends it and
/// is no entry into the next body.
#[test]
fn a_transfer_to_the_end_of_its_own_option_loads_and_ends_it() {
    let commands = vec![
        hub(
            0,
            "h",
            &[("a", 1, false), ("b", 3, false), ("out", 5, true)],
            None,
            7,
        ),
        line(1, "a"),
        jump(2, 3),
        line(3, "b"),
        line(4, "b again"),
        line(5, "out"),
        jump(6, 7),
        line(7, "after"),
    ];
    let (lines, out) = run(commands, &[("h", &["a", "out"])]);
    out.unwrap();
    assert_eq!(lines, ["[h \"a\"]", "a", "[h \"out\"]", "out", "after"]);
}

/// S3 review P2: an inner hub in the outer hub's last non-exit option, no
/// outer `<return>`: the outer option's end and the outer converge are one
/// address. Both counts are legal and mean different things — `leaves: 1`
/// ends the inner visit only, completes the outer option and presents the
/// outer hub again; `leaves: 2` ends both visits and continues after the
/// outer hub. `leaves` is authoritative.
fn coincident(leaves: u32) -> Vec<Json> {
    vec![
        hub(0, "outer", &[("out", 1, true), ("a", 3, false)], None, 8),
        line(1, "out"),
        jump(2, 8),
        hub(3, "inner", &[("x", 4, false), ("y", 6, true)], None, 8),
        line(4, "x"),
        leave(5, 8, leaves),
        line(6, "y"),
        jump(7, 8),
        line(8, "after"),
    ]
}

#[test]
fn at_a_coincident_end_one_leave_presents_the_outer_hub_again() {
    let (lines, out) = run(
        coincident(1),
        &[("outer", &["a", "out"]), ("inner", &["x"])],
    );
    out.unwrap();
    assert_eq!(
        lines,
        [
            "[outer \"a\"]",
            "[inner \"x\"]",
            "x",
            "[outer \"out\"]",
            "out",
            "after"
        ]
    );
}

#[test]
fn at_a_coincident_end_two_leaves_continue_after_the_outer_hub() {
    let (lines, out) = run(
        coincident(2),
        &[("outer", &["a", "out"]), ("inner", &["x"])],
    );
    out.unwrap();
    assert_eq!(lines, ["[outer \"a\"]", "[inner \"x\"]", "x", "after"]);
}
