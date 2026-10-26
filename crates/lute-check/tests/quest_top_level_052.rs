//! dsl 0.52.0 §4 (#208): a quest's top level admits only `<on>` and
//! `<objective>`. A quest runs its handler and objective bodies only, so any
//! other node directly inside `<quest>` is `E-GRAMMAR-NOT-ADMITTED`; nested
//! bodies keep their admission. On v0.51.0 each top-level node below checks
//! clean (the `::assert`/`::retract` and `<match>` cases too, with the
//! relation and `<otherwise>` declared as here).

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_core_span::Severity;
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput::new(
        text.to_string(),
        "quest_top_level_052".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    check(&input)
}

/// Nine lines; the `<quest>` opens on line 10.
const HEAD: &str = "---\nkind: quest\nenums:\n  who: { members: [a] }\nrelations:\n  met: { \
                    args: [who], tier: run }\nstate:\n  run.d: { type: bool, default: false }\n\
                    ---\n";

/// `body` at the quest's top level, after an empty objective: line 12.
fn top(body: &str) -> String {
    format!(
        "{HEAD}<quest id=\"q\" title=\"Q\" tier=\"run\" start=\"true\">\n  <objective id=\"o\" \
         title=\"O\" done=\"run.d\"/>\n{body}</quest>\n"
    )
}

const MATCH: &str = "  <match subject=\"run.d\">\n    <when is=\"true\">\n      @a: yes\n    \
                     </when>\n    <otherwise>\n      @a: no\n    </otherwise>\n  </match>\n";

const BRANCH: &str =
    "  <branch id=\"b\">\n    <choice id=\"c\" text=\"C\">\n      @a: picked\n    \
                      </choice>\n  </branch>\n";

/// Each node kind of the rule, as `describe` names it, and its source.
const NODES: [(&str, &str); 9] = [
    ("a content line", "  @a: hi\n"),
    ("a `::directive`", "  ::clear\n"),
    ("a `::set`", "  ::set{run.d = true}\n"),
    ("a `::directive`", "  ::label{name=\"top\"}\n"),
    (
        "a `::directive`",
        "  ::jump{to=\"end\"}\n  <objective id=\"p\" title=\"P\" done=\"run.d\">\n    \
         ::label{name=\"end\"}\n    @a: there\n  </objective>\n",
    ),
    ("a `<branch>`", BRANCH),
    ("a `<match>`", MATCH),
    ("an `::assert`", "  ::assert{ met(a) }\n"),
    ("a `::retract`", "  ::retract{ met(a) }\n"),
];

fn refusal(what: &str) -> String {
    format!(
        "{what} at the top level of `<quest id=\"q\">` never runs — a quest runs only its `<on>` \
         handlers and `<objective>` bodies: move it into the `<on event=…>` or `<objective>` \
         whose moment it belongs to (dsl 0.2.0 §6.7)"
    )
}

fn errors(text: &str) -> Vec<(String, String, u32)> {
    run(text)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code, d.message, d.span.line))
        .collect()
}

#[test]
fn every_node_kind_at_a_quests_top_level_is_refused() {
    for (what, body) in NODES {
        assert_eq!(
            errors(&top(body)),
            [("E-GRAMMAR-NOT-ADMITTED".to_string(), refusal(what), 12)],
            "{body}"
        );
    }
}

/// The same nodes inside an `<objective>` body and an `<on>` handler, and
/// inside a `<branch>` choice / `<match>` arm there: admitted as before.
#[test]
fn nested_bodies_keep_their_admission() {
    // Every node kind of the rule, then a `<match>` inside a `<branch>`
    // choice and a `::set` inside a `<match>` arm.
    let inner = "    @a: hi\n    ::clear\n    ::set{run.d = true}\n    ::label{name=\"top\"}\n    \
                 ::jump{to=\"end\"}\n    @a: skipped\n    ::label{name=\"end\"}\n    \
                 ::assert{ met(a) }\n    ::retract{ met(a) }\n    <branch id=\"b\">\n      \
                 <choice id=\"c\" text=\"C\">\n        <match subject=\"run.d\">\n          \
                 <when is=\"true\">\n            @a: yes\n          </when>\n          \
                 <otherwise>\n            ::set{run.d = false}\n          </otherwise>\n        \
                 </match>\n      </choice>\n    </branch>\n";
    let objective =
        format!("  <objective id=\"o\" title=\"O\" done=\"run.d\">\n{inner}  </objective>\n");
    let handler = format!(
        "  <objective id=\"o\" title=\"O\" done=\"run.d\"/>\n  <on event=\"questComplete\">\n\
         {inner}  </on>\n"
    );
    for body in [objective, handler] {
        let quest = format!(
            "{HEAD}<quest id=\"q\" title=\"Q\" tier=\"run\" start=\"true\">\n{body}</quest>\n"
        );
        assert_eq!(errors(&quest), [], "{quest}");
    }
}

/// Content after the last body at a quest's top level used to run by
/// fallthrough on v0.51.0 — the last body's segment ran to the end of the
/// quest's commands (`machine/quest.rs` `quest_program`, `run_segment`) — so
/// a trailing line after an `<on>` handler played as part of it. dsl 0.52.0
/// §4 refuses it like any other top-level content: it moves into the
/// handler it belongs to.
#[test]
fn a_trailing_line_after_the_last_handler_is_refused() {
    let quest = format!(
        "{HEAD}<quest id=\"q\" title=\"Q\" tier=\"run\" start=\"true\">\n  <objective id=\"o\" \
         title=\"O\" done=\"run.d\"/>\n  <on event=\"questComplete\">\n    @a: done\n  </on>\n  \
         @a: after the handler\n</quest>\n"
    );
    assert_eq!(
        errors(&quest),
        [(
            "E-GRAMMAR-NOT-ADMITTED".to_string(),
            refusal("a content line"),
            15
        )]
    );
}

/// `<hub>`/`<timeline>` are refused anywhere in a quest and keep the message
/// they had (dsl 0.2.0 §6.7); a hub's option lines are not refused again.
#[test]
fn hub_and_timeline_keep_their_message() {
    for (what, body) in [
        (
            "a `<hub>`",
            "  <hub id=\"h\">\n    <choice id=\"out\" text=\"Out\" exit>\n      @a: bye\n    \
             </choice>\n  </hub>\n",
        ),
        (
            "a `<timeline>`",
            "  <timeline duration=\"1\">\n    <track subject=\"camera\">\n      \
             ::camera{focus=\"a\" framing=\"tight\" duration=\"0.4\"}\n    </track>\n  \
             </timeline>\n",
        ),
    ] {
        let refused: Vec<String> = errors(&top(body))
            .into_iter()
            .filter(|(code, ..)| code == "E-GRAMMAR-NOT-ADMITTED")
            .map(|(_, message, _)| message)
            .collect();
        assert_eq!(
            refused,
            [format!(
                "{what} is not admitted here (dsl 0.2.0 §3.3, §6.7): a quest body forbids \
                 `<hub>`/`<timeline>` (dsl 0.2.0 §6.7)"
            )],
            "{body}"
        );
    }
}
