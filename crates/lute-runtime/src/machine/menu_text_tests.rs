//! dsl 0.52.0 §6.3 (#218): every presentation renders each option label and
//! the prompt with the line renderer — markers against their placeholders,
//! translations by marker, `\{{` literal, a host token and an unknown marker
//! verbatim — and menu text never stops the walk. On hand-built command
//! streams.

use std::collections::BTreeMap;

use serde_json::{json, Value as Json};

use crate::driver::{
    BridgeCall, BridgeReply, Driver, Forced, Menu, MenuText, OnUnknown, Pick, UnknownSite, Verdict,
};
use crate::machine::{Machine, Seed};

/// One presentation as the driver saw it: the prompt and the option labels.
type Shown = (Option<MenuText>, Vec<MenuText>);

/// Answers each menu from its scripted list, one pick per presentation; a
/// menu with no pick left gets the first open option. Records every
/// presentation, every line and each hub record's `prompt`.
#[derive(Default)]
struct Script {
    picks: BTreeMap<String, Vec<&'static str>>,
    shown: Vec<Shown>,
    lines: Vec<String>,
    hub_prompts: Vec<Option<String>>,
}

impl Driver for Script {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        self.shown.push((
            menu.prompt.clone(),
            menu.options.iter().map(|o| o.text.clone()).collect(),
        ));
        match self.picks.get_mut(menu.id) {
            Some(list) if !list.is_empty() => Pick::Option(list.remove(0).to_string()),
            _ => Pick::AutoFirst,
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
            Some("hub") => self
                .hub_prompts
                .push(rec.get("prompt").and_then(Json::as_str).map(str::to_string)),
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

fn set(i: usize, op: &str, n: i64) -> Json {
    json!({ "kind": "set", "family": "state", "position": pos(i), "path": "run.gold",
            "op": op, "value": { "cel": n.to_string(), "expr": { "int": n } } })
}

fn gold() -> Json {
    json!({ "kind": "path", "path": "run.gold" })
}

fn text(text: &str, texts: &[(&str, &str)]) -> MenuText {
    MenuText {
        text: text.to_string(),
        texts: texts
            .iter()
            .map(|(l, t)| ((*l).to_string(), (*t).to_string()))
            .collect(),
    }
}

fn run(commands: Vec<Json>, picks: &[(&str, &[&'static str])]) -> (Script, Result<(), String>) {
    let art = json!({ "kind": "scene", "commands": commands });
    let script = Script {
        picks: picks
            .iter()
            .map(|(id, list)| ((*id).to_string(), list.to_vec()))
            .collect(),
        ..Script::default()
    };
    let mut m = Machine::new(&art, Seed::default(), script);
    let out = m.run().map_err(|e| e.to_string());
    let driver = std::mem::take(m.driver_mut());
    (driver, out)
}

/// The #218 branch: `run.gold` is 5; `yes` reads it, `odd` holds a host
/// token, a literal `\{{` and a never-set path.
fn issue_218() -> Vec<Json> {
    vec![
        set(0, "=", 5),
        json!({
            "kind": "choice", "family": "control", "position": pos(1), "branchId": "pay",
            "selectionKey": "scene.choices.pay", "converge": pos(6),
            "prompt": "You have {{run.gold}} gold.", "promptLineId": "gold.pay.prompt",
            "promptTexts": { "ko": "금화가 {{ run.gold }}개 있다." },
            "promptPlaceholders": [gold()],
            "options": [
                { "id": "yes", "text": "Pay {{run.gold}} gold", "lineId": "gold.pay.yes",
                  "target": pos(2), "placeholders": [gold()],
                  "texts": { "ko": "{{run.gold}}골드 낸다" } },
                { "id": "odd", "text": "Hi {{userName}}, \\{{x}} and {{run.never}}",
                  "lineId": "gold.pay.odd", "target": pos(4),
                  "placeholders": [{ "kind": "reserved", "token": "userName" },
                                   { "kind": "path", "path": "run.never" }] }
            ]
        }),
        line(2, "Paid."),
        json!({ "kind": "jump", "family": "control", "position": pos(3), "target": pos(6) }),
        line(4, "Odd."),
        json!({ "kind": "jump", "family": "control", "position": pos(5), "target": pos(6) }),
        line(6, "After."),
    ]
}

#[test]
fn a_branch_menu_carries_the_rendered_prompt_and_labels() {
    let (script, out) = run(issue_218(), &[("pay", &["yes"])]);
    out.unwrap();
    assert_eq!(
        script.shown,
        [(
            Some(text("You have 5 gold.", &[("ko", "금화가 5개 있다.")])),
            vec![
                text("Pay 5 gold", &[("ko", "5골드 낸다")]),
                // A host token and an unset path keep their markers; `\{{`
                // is a literal `{{` (0.52.0 §3.2).
                text("Hi {{userName}}, {{x}} and {{run.never}}", &[]),
            ]
        )]
    );
    assert_eq!(script.lines, ["Paid.", "After."]);
}

/// An unset path in a label renders verbatim and the menu is still
/// presented: menu text never consults the halt rule, though this driver
/// halts at every unknown.
#[test]
fn an_unset_marker_never_stops_the_menu() {
    let (script, out) = run(issue_218(), &[("pay", &["odd"])]);
    out.unwrap();
    assert_eq!(script.shown.len(), 1);
    assert_eq!(script.lines, ["Odd.", "After."]);
}

/// A hub renders at every presentation: after an option adds a coin, the
/// prompt, the label and the hub record's `prompt` read the new value.
#[test]
fn a_hub_renders_again_at_each_presentation() {
    let commands = vec![
        set(0, "=", 1),
        json!({
            "kind": "hub", "family": "control", "position": pos(1), "id": "purse",
            "selectionKey": "scene.choices.purse", "converge": pos(5),
            "prompt": "{{run.gold}} coins.", "promptLineId": "gold.purse.prompt",
            "promptPlaceholders": [gold()],
            "options": [
                { "id": "add", "text": "Add to {{run.gold}}", "lineId": "gold.purse.add",
                  "target": pos(2), "placeholders": [gold()] },
                { "id": "leave", "text": "Leave", "lineId": "gold.purse.leave",
                  "target": pos(3), "exit": true }
            ]
        }),
        set(2, "+=", 1),
        line(3, "Bye."),
        json!({ "kind": "jump", "family": "control", "position": pos(4), "target": pos(5) }),
        line(5, "After."),
    ];
    let (script, out) = run(commands, &[("purse", &["add", "leave"])]);
    out.unwrap();
    let prompts: Vec<&str> = script
        .shown
        .iter()
        .map(|(p, _)| p.as_ref().map_or("", |p| p.text.as_str()))
        .collect();
    assert_eq!(prompts, ["1 coins.", "2 coins."]);
    let labels: Vec<&str> = script
        .shown
        .iter()
        .map(|(_, o)| o[0].text.as_str())
        .collect();
    assert_eq!(labels, ["Add to 1", "Add to 2"]);
    assert_eq!(
        script.hub_prompts,
        [Some("1 coins.".to_string()), Some("2 coins.".to_string())]
    );
}

/// A menu without a prompt has none; a label without markers is its text.
#[test]
fn a_plain_menu_has_no_prompt() {
    let commands = vec![
        json!({
            "kind": "choice", "family": "control", "position": pos(0), "branchId": "b",
            "selectionKey": "scene.choices.b", "converge": pos(1),
            "options": [{ "id": "a", "text": "Plain", "lineId": "s.b.a", "target": pos(1) }]
        }),
        line(1, "Done."),
    ];
    let (script, out) = run(commands, &[]);
    out.unwrap();
    assert_eq!(script.shown, [(None, vec![text("Plain", &[])])]);
}
