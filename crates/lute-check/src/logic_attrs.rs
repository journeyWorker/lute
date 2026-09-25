//! dsl 0.10.0 §4 (backlog #6, D-J, D-L): the six logic constructs close their
//! attribute sets.
//!
//! Three attribute surfaces existed in one language and two of them enforced:
//! content lines against a fixed const (`content_line.rs:25`, closed) and
//! directives against the capability schema (`directives.rs`, open by design
//! because plugins contribute attrs). Logic tags had nothing — five of six
//! accepted and dropped any invented attribute, and the sixth, `<otherwise>`,
//! enforced from the PARSER under `E-LOGIC-CONTENT`, a code whose other three
//! emission sites are body-shape rules about children.
//!
//! Per **D-J** this is a CHECKER rule with a fixed per-tag table, modelled on
//! the content-line surface: logic tags are core grammar and, unlike
//! directives, are NOT plugin-extensible, so the table is a constant and not a
//! capability lookup. It emits [`E_UNKNOWN_ATTR`] — the code the other two
//! surfaces already raise — at the offending attribute's own span, once per
//! attribute. A bespoke code would defeat the point: the surface becomes
//! uniform, or #6 was not worth doing.
//!
//! ## What the tables are enumerated FROM
//!
//! The AST and its readers, not `0.1.0 §7.3`'s prose, because the prose and
//! the parser do not agree about where an attribute lives. Two consequences:
//!
//! - `<hub>` has **no `id` field** (`ast.rs:120-125`; `blocks.rs:288`: *"[`Hub`]
//!   carries no `id` field, so `id=` stays in `attrs`"*), and the checker reads
//!   it from the residual list (`match_check.rs:432`). `id` MUST be permitted
//!   or the rule rejects every hub in existence. `<branch id>` by contrast IS
//!   extracted (`blocks.rs:160`), so `id` never reaches its residual list — the
//!   entry below is harmless and kept so the table reads as §4's table does.
//! - `once` and `exit` are bare flags (`AttrValue::BoolTrue`), not
//!   `key="value"` pairs. They are attributes for this rule's purposes and are
//!   matched by key alone; their `Attr::span` is the key bytes, so the
//!   column-exact anchor needs no special casing.
//!
//! ## Closure only
//!
//! This rule enforces that no key OUTSIDE its construct's permitted set
//! appears. It does not enforce §4's "required" column — a missing
//! `<branch id>`/`<hub id>`/`<match on>` already has its own diagnostic
//! (`E-DUP-BRANCH`, `E-NONEXHAUSTIVE`, …) and this rule does not mint a second
//! opinion. The required keys appear in the tables only because a required key
//! that survives into the residual list must be permitted.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Arm, Attr, AttrValue, Branch, BundleBeat, Choice, Entry, Hub, Match, Objective, On, Quest,
    Reward,
};

use crate::content_line::E_UNKNOWN_ATTR;

/// dsl 0.11.0 (branch prompt/timeout): `<branch>`'s two engine-wire fields
/// for the countdown UI, joining `id` in the permitted set. `close` only
/// enforces that no OTHER key appears; their own VALUES are checked below
/// by [`check_branch_value_attrs`], because the parser accepts any `Str`.
const BRANCH_ATTRS: &[&str] = &["id", "prompt", "timeout"];
const MATCH_ATTRS: &[&str] = &["on"];
const WHEN_ATTRS: &[&str] = &["is", "test"];
const OTHERWISE_ATTRS: &[&str] = &[];
/// dsl 0.23.0 §4: `<hub prompt>` attaches the prompt line shown with the
/// hub's options — the same non-empty-string rule as `<branch prompt>`
/// ([`check_prompt_attr`], `E-BRANCH-PROMPT`).
pub const HUB_ATTRS: &[&str] = &["id", "prompt"];
/// dsl 0.16.0 §2: `<reward>` closes over the five wire-contract keys.
/// Every OTHER attribute is `E-UNKNOWN-ATTR` at its own span; a malformed
/// `amount=` survives here as a residual so [`check_reward_attrs`] can
/// anchor `E-REWARD-ATTR` at the value span. `id=` is deliberately absent
/// — a reward is a leaf, not an addressable construct.
pub(crate) const REWARD_ATTRS: &[&str] = &["kind", "target", "amount", "when", "on"];
/// dsl 0.19.0 §3: `<entry>` closes over its declared keys — the seven 0.19.0
/// keys plus the dsl 0.21.0 §3.2 beat keys `on` and `priority` and the dsl
/// 0.22.0 §7 beat key `once` (and dsl 0.25.0 §2's `share`). The parser extracts each into a typed field,
/// so a permitted key reaches the residual list only when its value was not
/// a quoted string — `crate::lore` owns that shape fault (`E-ENTRY-ATTR`,
/// or `E-BEAT-ATTR` for a beat key); every OTHER key is `E-UNKNOWN-ATTR`.
/// `also` (dsl 0.23.0 §3) is a scene beat key: [`check_entry_attrs`] leaves
/// it to `crate::beats`, which reports it on an entry as `E-BEAT-ATTR`.
pub const ENTRY_ATTRS: &[&str] = &[
    "id", "target", "category", "title", "series", "order", "when", "on", "priority", "once",
    "share",
];
/// dsl 0.2.0 §6.3 (+ `after`, connectivity T2; `tier`, dsl 0.22.0 §7;
/// `activate` / `complete`, dsl 0.24.0 §2; `accept`, dsl 0.25.0 §5):
/// `<quest>`'s keys. The parser extracts each into a typed field, so one
/// reaches the residual list only with a non-string value; every OTHER key —
/// a `fial=` typo — used to be accepted and dropped from the IR without a
/// word (0.21.1 T1-7).
pub const QUEST_ATTRS: &[&str] = &[
    "id", "title", "start", "fail", "after", "tier", "activate", "complete", "accept",
];
/// dsl 0.2.0 §6.4 (+ subquest `quest`, dsl 0.21.0 §7a.2 `on`, dsl 0.23.0 §2
/// `by` / `target`, dsl 0.24.0 §2.1 `until`): `<objective>`'s keys. A
/// non-string `on=` / `target=` stays residual for `crate::beats` to report
/// (`E-BEAT-ATTR`), so it is permitted here rather than double-reported.
pub const OBJECTIVE_ATTRS: &[&str] = &[
    "id", "done", "quest", "when", "title", "optional", "on", "by", "target", "until",
];
/// dsl 0.2.0 §4.1 (+ `target`, dsl 0.24.0 §2): `<on>`'s keys. A non-string
/// `target=` stays residual for [`crate::on::check_on_target`] to report
/// (`E-BEAT-ATTR`), so it is permitted here rather than double-reported.
pub const ON_ATTRS: &[&str] = &["event", "when", "target"];

/// D-L: the two `<choice>` positions have DIFFERENT permitted sets. `once` and
/// `exit` attach to `HubChoice` in `0.1.0 §7.3`'s grammar and to nothing else,
/// and the hub reducer is their only reader (`stage.rs:342-343,377`;
/// `match_check.rs:488-490`); `walk_branch` never reads them. Enforcing one
/// merged set would leave a branch choice carrying `exit` silent, which is the
/// defect §4 closes wearing a smaller hat.
const BRANCH_CHOICE_ATTRS: &[&str] = &["id", "label", "when", "into", "value"];
const HUB_CHOICE_ATTRS: &[&str] = &["id", "label", "when", "into", "value", "once", "exit"];

/// §4's fourth column: keys a DEDICATED removal code already reports. It is
/// NOT a permitted set — `as` and `persist` on a `<choice>` are errors, and
/// that is the whole point of §4 — but each is reported ONCE, by its own code.
/// The checker already holds this invariant for `persist` deliberately: the
/// attribute is recognised at its own site so it is *"never reported as
/// unknown/extra"* and `E-PERSIST-REMOVED` stays *"the sole report for it"*
/// (`check.rs:2564-2565`, `:2571-2572`). `as` joins it in `E-AS-REMOVED`.
const CHOICE_REMOVED_ATTRS: &[&str] = &["as", "persist"];

/// The remedy a hub-choice flag on a BRANCH choice carries: the key is not
/// unknown to the language, it is in the wrong position.
const HUB_ONLY_HINT: (&[&str], &str) = (
    &["once", "exit"],
    "`once`/`exit` are hub-choice flags, valid only on a `<choice>` inside a `<hub>`",
);

/// Which `<choice>` position is being checked (D-L).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChoicePos {
    Branch,
    Hub,
}

pub(crate) fn check_branch_attrs(b: &Branch, diags: &mut Vec<Diagnostic>) {
    close(&b.attrs, "branch", BRANCH_ATTRS, &[], None, diags);
    check_branch_value_attrs(b, diags);
}

/// `E-BRANCH-PROMPT`: `<branch prompt>` must be a non-empty string — it is
/// the choice-situation sentence the UI shows verbatim, and an empty/absent
/// one is a silent blank prompt, not a valid "no prompt" spelling (there is
/// none; the attribute is optional at the grammar level by being absent from
/// `b.attrs` entirely, which this loop never sees).
///
/// `E-BRANCH-TIMEOUT`: `<branch timeout>` must parse as a positive integer
/// number of seconds — the engine wire's countdown, which cannot count down
/// from zero, a negative number, or a fraction of a second.
const E_BRANCH_PROMPT: &str = "E-BRANCH-PROMPT";
const E_BRANCH_TIMEOUT: &str = "E-BRANCH-TIMEOUT";

fn check_branch_value_attrs(b: &Branch, diags: &mut Vec<Diagnostic>) {
    for attr in &b.attrs {
        let bad = match attr.key.as_str() {
            "prompt" => check_prompt_attr(attr, "branch", "dsl 0.11.1 §4"),
            "timeout" => match &attr.value {
                AttrValue::Str(s) if s.parse::<u32>().is_ok_and(|n| n > 0) => None,
                _ => Some((
                    E_BRANCH_TIMEOUT,
                    "`<branch timeout>` must be a positive integer number of seconds (dsl 0.11.1 §4)"
                        .to_string(),
                )),
            },
            _ => None,
        };
        if let Some((code, message)) = bad {
            push_logic_error(diags, code, message, attr);
        }
    }
}

/// `E-BRANCH-PROMPT` for a `<tag prompt>` value that is not a non-empty
/// string. Shared by `<branch>` and `<hub>` (dsl 0.23.0 §4) so the rule and
/// its code cannot drift apart; the message names the construct.
fn check_prompt_attr(attr: &Attr, tag: &str, cite: &str) -> Option<(&'static str, String)> {
    match &attr.value {
        AttrValue::Str(s) if !s.trim().is_empty() => None,
        _ => Some((
            E_BRANCH_PROMPT,
            format!("`<{tag} prompt>` must be a non-empty string ({cite})"),
        )),
    }
}

fn push_logic_error(diags: &mut Vec<Diagnostic>, code: &str, message: String, attr: &Attr) {
    diags.push(Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: attr.span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    });
}

pub(crate) fn check_hub_attrs(h: &Hub, diags: &mut Vec<Diagnostic>) {
    close(&h.attrs, "hub", HUB_ATTRS, &[], None, diags);
    for attr in h.attrs.iter().filter(|a| a.key == "prompt") {
        if let Some((code, message)) = check_prompt_attr(attr, "hub", "dsl 0.23.0 §4") {
            push_logic_error(diags, code, message, attr);
        }
    }
}

pub(crate) fn check_match_attrs(m: &Match, diags: &mut Vec<Diagnostic>) {
    close(&m.attrs, "match", MATCH_ATTRS, &[], None, diags);
}

pub(crate) fn check_reward_attrs(r: &Reward, diags: &mut Vec<Diagnostic>) {
    close(&r.attrs, "reward", REWARD_ATTRS, &[], None, diags);
}

pub(crate) fn check_entry_attrs(e: &Entry, diags: &mut Vec<Diagnostic>) {
    // dsl 0.23.0 §3: `also` on an entry is `crate::beats`' `E-BEAT-ATTR`.
    close(&e.attrs, "entry", ENTRY_ATTRS, &["also"], None, diags);
}

/// dsl 0.23.0 §4: `<beat>` closes over [`crate::bundles::BUNDLE_BEAT_ATTRS`];
/// a permitted key left residual is `crate::bundles`' `E-BEAT-ATTR`.
pub(crate) fn check_bundle_beat_attrs(b: &BundleBeat, diags: &mut Vec<Diagnostic>) {
    close(
        &b.attrs,
        "beat",
        crate::bundles::BUNDLE_BEAT_ATTRS,
        &[],
        None,
        diags,
    );
}

pub(crate) fn check_quest_attrs(q: &Quest, diags: &mut Vec<Diagnostic>) {
    close(&q.attrs, "quest", QUEST_ATTRS, &[], None, diags);
    // Each enumerated `<quest>` attribute is a quoted string from a fixed
    // set; a bare/non-string value stays residual and is reported too.
    let enumerated: [(&str, &Option<(String, Span)>, &[&str], &str); 4] = [
        (
            "tier",
            &q.tier,
            &["run", "user"],
            "\"run\" (status and objectives reset at a new run) or \"user\" (persists across \
             runs, the default) (dsl 0.22.0 §7)",
        ),
        (
            "activate",
            &q.activate,
            &["accept"],
            "\"accept\" (a subquest child that waits for `::accept` instead of activating with \
             its parent) (dsl 0.24.0 §2)",
        ),
        (
            "complete",
            &q.complete,
            &["all", "any"],
            "\"all\" (every required objective done, the default) or \"any\" (one required \
             objective done; the other still-active children fail as superseded) (dsl 0.24.0 §2)",
        ),
        (
            "accept",
            &q.accept,
            &["external"],
            "\"external\" (the quest is accepted outside the script — a quest board, a menu, a \
             UI) (dsl 0.25.0 §5)",
        ),
    ];
    for (key, value, legal, expects) in enumerated {
        let bad = value
            .as_ref()
            .filter(|(v, _)| !legal.contains(&v.as_str()))
            .map(|(_, span)| *span)
            .into_iter()
            .chain(q.attrs.iter().filter(|a| a.key == key).map(|a| a.span));
        for span in bad {
            diags.push(attr_type(
                format!("attribute `{key}` of `<quest>` expects {expects}"),
                span,
            ));
        }
    }
    // dsl 0.24.0 §2: an accept-driven child activates when `::accept` names
    // it, so a `start` condition beside `activate="accept"` contradicts it.
    if let (Some((_, span)), true, Some(_)) = (&q.activate, q.activates_on_accept(), &q.start) {
        diags.push(attr_type(
            format!(
                "`<quest id=\"{}\">` carries both `activate=\"accept\"` and `start`: an \
                 accept-driven quest activates when `::accept` names it (while its parent is \
                 active), never by a `start` condition; remove one (dsl 0.24.0 §2)",
                q.id
            ),
            *span,
        ));
    }
    // dsl 0.25.0 §5: an externally accepted quest is accept-driven — the
    // engine activates it when the player takes it, never a `start`.
    if let (Some((_, span)), true, Some(_)) = (&q.accept, q.accepted_externally(), &q.start) {
        diags.push(attr_type(
            format!(
                "`<quest id=\"{}\">` carries both `accept=\"external\"` and `start`: an \
                 externally accepted quest activates when the engine accepts it, never by a \
                 `start` condition; remove one (dsl 0.25.0 §5)",
                q.id
            ),
            *span,
        ));
    }
}

fn attr_type(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: "E-ATTR-TYPE".to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

pub(crate) fn check_objective_attrs(o: &Objective, diags: &mut Vec<Diagnostic>) {
    close(&o.attrs, "objective", OBJECTIVE_ATTRS, &[], None, diags);
}

pub(crate) fn check_on_attrs(o: &On, diags: &mut Vec<Diagnostic>) {
    close(&o.attrs, "on", ON_ATTRS, &[], None, diags);
}

pub(crate) fn check_arm_attrs(a: &Arm, diags: &mut Vec<Diagnostic>) {
    match a {
        Arm::When { attrs, .. } => close(attrs, "when", WHEN_ATTRS, &[], None, diags),
        Arm::Otherwise { attrs, .. } => {
            close(attrs, "otherwise", OTHERWISE_ATTRS, &[], None, diags)
        }
    }
}

pub(crate) fn check_choice_attrs(c: &Choice, pos: ChoicePos, diags: &mut Vec<Diagnostic>) {
    let permitted = match pos {
        ChoicePos::Branch => BRANCH_CHOICE_ATTRS,
        ChoicePos::Hub => HUB_CHOICE_ATTRS,
    };
    let hint = matches!(pos, ChoicePos::Branch).then_some(HUB_ONLY_HINT);
    close(
        &c.attrs,
        "choice",
        permitted,
        CHOICE_REMOVED_ATTRS,
        hint,
        diags,
    );
}

/// One construct's closure: every attr whose key is outside `permitted` and not
/// in `told_elsewhere` draws one [`E_UNKNOWN_ATTR`] at its own span.
fn close(
    attrs: &[Attr],
    tag: &str,
    permitted: &[&str],
    told_elsewhere: &[&str],
    hint: Option<(&[&str], &str)>,
    diags: &mut Vec<Diagnostic>,
) {
    for attr in attrs {
        let key = attr.key.as_str();
        if permitted.contains(&key) || told_elsewhere.contains(&key) {
            continue;
        }
        let message = match hint {
            Some((keys, remedy)) if keys.contains(&key) => {
                format!("`<{tag}>` has no attribute `{key}` here: {remedy} (dsl 0.10.0 §4)")
            }
            // dsl 0.5.0 §2.2 "did you mean", over the construct's own table:
            // a misspelt key (`fial`, `optinal`) is the common case, and
            // naming the intended key makes the error a one-keystroke fix.
            _ => {
                let near = lute_manifest::suggest::nearest(key, permitted.iter().copied(), 2)
                    .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
                format!("`<{tag}>` has no attribute `{key}`{near} (dsl 0.10.0 §4)")
            }
        };
        diags.push(Diagnostic {
            code: E_UNKNOWN_ATTR.to_string(),
            severity: Severity::Error,
            message,
            span: attr.span,
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }
}
