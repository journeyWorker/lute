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
const MATCH_ATTRS: &[&str] = &["subject"];
const WHEN_ATTRS: &[&str] = &["is", "test"];
const OTHERWISE_ATTRS: &[&str] = &[];
/// dsl 0.23.0 §4: `<hub prompt>` attaches the prompt line shown with the
/// hub's options — the same non-empty-string rule as `<branch prompt>`
/// ([`check_prompt_attr`], `E-BRANCH-PROMPT`).
pub const HUB_ATTRS: &[&str] = &["id", "prompt"];
/// dsl 0.16.0 §2: `<reward>` closes over the five wire-contract keys.
/// Every OTHER attribute is `E-UNKNOWN-ATTR` at its own span; a malformed
/// `amount=` survives here as a residual so [`check_reward_attrs`] can
/// anchor `E-REWARD-ATTR` at the value span. `id=` (dsl 0.37.0 §3.5) is the
/// reward's identity within its quest; an unquoted one survives as a
/// residual for `E-REWARD-ATTR`.
pub const REWARD_ATTRS: &[&str] = &["id", "kind", "target", "amount", "when", "outcome"];
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
    "share", "spentBy", "for", "advances",
];
/// dsl 0.2.0 §6.3 (+ `follows`, connectivity T2; `tier`, dsl 0.22.0 §7;
/// `activate` / `complete`, dsl 0.24.0 §2; `accept`, dsl 0.25.0 §5):
/// `<quest>`'s keys. The parser extracts each into a typed field, so one
/// reaches the residual list only with a non-string value; every OTHER key —
/// a `fial=` typo — used to be accepted and dropped from the IR without a
/// word (0.21.1 T1-7).
pub const QUEST_ATTRS: &[&str] = &[
    "id", "title", "start", "fail", "follows", "tier", "activate", "complete", "accept", "rearm",
];
/// dsl 0.2.0 §6.4 (+ subquest `quest`, dsl 0.21.0 §7a.2 `on`, dsl 0.23.0 §2
/// `by` / `target`, dsl 0.24.0 §2.1 `until`): `<objective>`'s keys. A
/// non-string `on=` / `target=` stays residual for `crate::beats` to report
/// (`E-BEAT-ATTR`), so it is permitted here rather than double-reported.
pub const OBJECTIVE_ATTRS: &[&str] = &[
    "id",
    "done",
    "quest",
    "visibleWhen",
    "title",
    "optional",
    "on",
    "by",
    "target",
    "until",
];
/// Attributes that were renamed: the old spelling on that element is an
/// error whose message names the new one (clean cutover — the old key is
/// never read). `(tag, old key, code, message)`. The dsl 0.37.0 §4 renames
/// raise `E-RENAMED-TAG-ATTR`; the older ones stay `E-UNKNOWN-ATTR`.
const RENAMED_ATTRS: &[(&str, &str, &str, &str)] = &[
    (
        "quest",
        "after",
        E_UNKNOWN_ATTR,
        "`<quest after=>` is now `follows=` — it records the quest graph and does not gate the \
         quest; to wait, write `start=\"visited('<scene id>')\"`",
    ),
    (
        "reward",
        "on",
        E_UNKNOWN_ATTR,
        "`on=` on a `<reward>` is now `outcome=` (`outcome=\"failed\"` grants when the quest \
         fails; without it the reward grants on `complete`)",
    ),
    (
        "objective",
        "when",
        E_UNKNOWN_ATTR,
        "`when=` on an `<objective>` is now `visibleWhen=` — it only hides the objective; to \
         gate `done`, put the condition in `done=`",
    ),
    (
        "choice",
        "label",
        E_RENAMED_TAG_ATTR,
        "`<choice label=>` is now `text=` (dsl 0.37.0 §3.5) — `lute fix` rewrites it",
    ),
    (
        "match",
        "on",
        E_RENAMED_TAG_ATTR,
        "`<match on=>` is now `subject=` (dsl 0.37.0 §3.5) — `lute fix` rewrites it",
    ),
];

/// `E-RENAMED-TAG-ATTR` (dsl 0.37.0 §4): an old `<choice>`/`<match>`
/// attribute spelling; the message names `text`/`subject`.
pub const E_RENAMED_TAG_ATTR: &str = "E-RENAMED-TAG-ATTR";
/// Keys an author reaches for that the construct spells another way, or that
/// belong to another construct or layer: `(tag, keys, remedy)` — the
/// `E-UNKNOWN-ATTR` names the remedy instead of a spelling neighbour. The
/// frontmatter counterpart is `crate::meta`'s `owning_layer`.
const MEANT_ATTRS: &[(&str, &[&str], &str)] = &[
    (
        "quest",
        &[
            "repeatable",
            "repeat",
            "repeats",
            "recurring",
            "reset",
            "once",
        ],
        "a quest repeats with `rearm=\"<condition>\"`: each time the condition turns from false \
         to true, the quest returns to `unset` and can start again",
    ),
    (
        "quest",
        &["spentBy"],
        "`spentBy=` is a beat attribute (`<beat spentBy=…>`, `<entry spentBy=…>`, a scene's \
         `spentBy:`); a quest repeats with `rearm=\"<condition>\"`",
    ),
    (
        "quest",
        &["on", "occasion", "event"],
        "a quest answers no occasion itself: an objective does, `<objective on=\"<occasion>\">` \
         (judged when that occasion is raised), and a handler runs on an event, \
         `<on event=\"<name>\">`; the quest activates with `start=`",
    ),
    (
        "objective",
        &["occasion", "event"],
        "an objective names the occasion that judges it with `on=`, e.g. \
         `<objective on=\"<occasion>\">` (`occasion:` is a play step's key)",
    ),
    (
        "on",
        &["on", "occasion"],
        "a quest handler names the world event it answers with `event=`, e.g. `<on \
         event=\"<name>\">`; a raised occasion runs it only when a world event of the same name \
         is declared (to judge an objective on an occasion, write `<objective on=\"<occasion>\">`)",
    ),
    (
        "entry",
        &["occasion", "event"],
        "an entry names the occasion it answers with `on=`, e.g. `<entry on=\"<occasion>\">` \
         (`occasion:` is a play step's key)",
    ),
    (
        "beat",
        &["occasion", "event"],
        "a beat names the occasion it answers with `on=`, e.g. `<beat on=\"<occasion>\">` \
         (`occasion:` is a play step's key)",
    ),
    (
        "beat",
        &["rearm"],
        "`rearm=` is an attribute of a `<quest>`; a beat comes back with `once=` (how long it \
         stays spent) or `spentBy=`",
    ),
    (
        "entry",
        &["rearm"],
        "`rearm=` is an attribute of a `<quest>`; an entry comes back with `once=` (how long it \
         stays spent) or `spentBy=`",
    ),
    (
        "beat",
        &["tier"],
        "`tier=` is an attribute of a `<quest>`; a beat's repetition is its `once=` period",
    ),
    (
        "entry",
        &["tier"],
        "`tier=` is an attribute of a `<quest>`; an entry's repetition is its `once=` period",
    ),
];
/// `true` when `attrs` names the occasion (or event) a beat or a handler
/// answers in another construct's spelling — [`MEANT_ATTRS`] reports it, so
/// the construct's own "names no occasion/event" error would only repeat it.
/// `own` is the construct's key (`on` for a beat, `event` for `<on>`).
pub(crate) fn names_occasion_misspelt(attrs: &[Attr], own: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.key != own && ["on", "occasion", "event"].contains(&a.key.as_str()))
}
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
const BRANCH_CHOICE_ATTRS: &[&str] = &["id", "text", "when", "into", "value"];
const HUB_CHOICE_ATTRS: &[&str] = &["id", "text", "when", "into", "value", "once", "exit"];

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
        evidence: None,
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
    // dsl 0.28.0 §5: `<return>` takes no attributes.
    if let Some(r) = &h.on_return {
        let hint: (&[&str], &str) = (
            &["when"],
            "`<return>` runs every time an option hands control back; guard its lines \
             instead (`@narrator{when=\"…\"}: …`) or wrap them in a `<match>`",
        );
        close(&r.attrs, "return", &[], &[], Some(hint), diags);
    }
}

pub(crate) fn check_match_attrs(m: &Match, diags: &mut Vec<Diagnostic>) {
    close(&m.attrs, "match", MATCH_ATTRS, &[], None, diags);
}

pub(crate) fn check_reward_attrs(r: &Reward, diags: &mut Vec<Diagnostic>) {
    close(&r.attrs, "reward", REWARD_ATTRS, &[], None, diags);
}

pub(crate) fn check_entry_attrs(e: &Entry, diags: &mut Vec<Diagnostic>) {
    // dsl 0.27.0 §6: `use=` applies a beat template, which only a `<beat>`
    // takes; its other attributes are that template's arguments, so the one
    // report stands for all of them.
    if let Some(u) = e.attrs.iter().find(|a| a.key == "use") {
        let name = match &u.value {
            AttrValue::Str(s) => s.as_str(),
            _ => "…",
        };
        diags.push(Diagnostic {
            code: crate::templates::E_TEMPLATE.to_string(),
            severity: Severity::Error,
            message: format!(
                "beat templates apply to `<beat>` only — write `<beat use=\"{name}\" id=\"{}\" …>`; \
                 an `<entry>` takes no template",
                e.id
            ),
            evidence: None,
            span: u.span,
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
        return;
    }
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
            "\"run\" (status and objectives reset at a new run), \"user\" (persists across \
             runs, the default) or \"season:<name>\" (reset when that declared season opens) \
             (dsl 0.22.0 §7, 0.27.0 §5)",
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
            .filter(|(v, _)| {
                !legal.contains(&v.as_str())
                    // dsl 0.27.0 §5: `season:<name>` (declared: `E-SEASON-DECL`).
                    && !(key == "tier"
                        && lute_manifest::season::season_ref(v)
                            .is_some_and(lute_manifest::ident::is_name))
            })
            .map(|(v, span)| {
                let hint = lute_manifest::suggest::did_you_mean(v, legal.iter().copied());
                (hint, *span)
            })
            .into_iter()
            .chain(
                q.attrs
                    .iter()
                    .filter(|a| a.key == key)
                    .map(|a| (String::new(), a.span)),
            );
        for (hint, span) in bad {
            diags.push(attr_type(
                format!("attribute `{key}` of `<quest>` expects {expects}{hint}"),
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
        evidence: None,
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
    // A flag value the parser could not read stays residual.
    check_flags(&o.attrs, "objective", &["optional"], diags);
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
    if pos == ChoicePos::Hub {
        check_flags(&c.attrs, "choice", &["once", "exit"], diags);
    }
}

/// dsl 0.28.0 §1: a flag attribute takes no value — bare, `="true"` or
/// `="false"` ([`AttrValue::flag`], the reader the parser, the compiler and
/// the hub-exit rule share). Any other value used to read as `false`
/// without a word (`optional="yes"` made the objective required).
pub const E_FLAG_VALUE: &str = "E-FLAG-VALUE";

/// [`E_FLAG_VALUE`] at every attr among `flags` whose value is not a flag
/// value. Shared by `<objective optional>`, `<choice once/exit>` and
/// `<beat also>` (`crate::bundles`) so one rule has one message.
pub(crate) fn check_flags(attrs: &[Attr], tag: &str, flags: &[&str], diags: &mut Vec<Diagnostic>) {
    for attr in attrs {
        let key = attr.key.as_str();
        if !flags.contains(&key) || attr.value.flag().is_some() {
            continue;
        }
        let written = match &attr.value {
            AttrValue::Str(s) => format!("{key}=\"{s}\""),
            AttrValue::Ref(slot) => format!("{key}={}", slot.raw),
            AttrValue::BoolTrue => unreachable!("a bare flag reads as true"),
        };
        // A beat/entry repetition period on a choice (R-6): a choice's `once`
        // has one meaning, so the period is refused, not silently `false`.
        let period = matches!(&attr.value, AttrValue::Str(s)
            if s.starts_with("season:") || crate::beats::BeatOnce::parse(s).is_some());
        let message = if tag == "choice" && key == "once" && period {
            format!(
                "`<choice {written}>`: a period (`run`, `user`, `day`, `week`, `slot`, \
                 `season:<name>`) is a beat/entry `once` key; a `<choice>`'s `once` means once \
                 per hub visit — write it bare (`<choice … once>`) (dsl 0.28.0 §1)"
            )
        } else {
            format!(
                "`{key}` is a flag: write it bare (`<{tag} … {key}>`), or `{key}=\"false\"` to \
                 turn it off — `{written}` is not a flag value (dsl 0.28.0 §1)"
            )
        };
        push_logic_error(diags, E_FLAG_VALUE, message, attr);
    }
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
        let renamed = RENAMED_ATTRS
            .iter()
            .find(|(t, old, _, _)| *t == tag && *old == key);
        let meant = MEANT_ATTRS
            .iter()
            .find(|(t, keys, _)| *t == tag && keys.contains(&key));
        let code = renamed.map_or(E_UNKNOWN_ATTR, |(_, _, code, _)| *code);
        let message = match (renamed, meant, hint) {
            (Some((_, _, _, message)), _, _) => (*message).to_string(),
            (None, Some((_, _, remedy)), _) => {
                format!("`<{tag}>` has no attribute `{key}` — {remedy}")
            }
            (None, None, Some((keys, remedy))) if keys.contains(&key) => {
                format!("`<{tag}>` has no attribute `{key}` here: {remedy} (dsl 0.10.0 §4)")
            }
            // dsl 0.5.0 §2.2 "did you mean", over the construct's own table:
            // a misspelt key (`fial`, `optinal`) is the common case, and
            // naming the intended key makes the error a one-keystroke fix.
            _ if permitted.is_empty() => {
                format!("`<{tag}>` has no attribute `{key}`: it takes no attributes")
            }
            _ => {
                let near = lute_manifest::suggest::did_you_mean(key, permitted.iter().copied());
                let listed: Vec<String> = permitted.iter().map(|k| format!("`{k}`")).collect();
                format!(
                    "`<{tag}>` has no attribute `{key}`{near}; its attributes are {}",
                    listed.join(", ")
                )
            }
        };
        diags.push(Diagnostic {
            code: code.to_string(),
            severity: Severity::Error,
            message,
            evidence: None,
            span: attr.span,
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }
}
