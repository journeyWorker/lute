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

use lute_core_span::{Diagnostic, Layer, Severity};
use lute_syntax::ast::{Arm, Attr, AttrValue, Branch, Choice, Hub, Match};

use crate::content_line::E_UNKNOWN_ATTR;

/// dsl 0.11.0 (branch prompt/timeout): `<branch>`'s two engine-wire fields
/// for the countdown UI, joining `id` in the permitted set. `close` only
/// enforces that no OTHER key appears; their own VALUES are checked below
/// by [`check_branch_value_attrs`], because the parser accepts any `Str`.
const BRANCH_ATTRS: &[&str] = &["id", "prompt", "timeout"];
const MATCH_ATTRS: &[&str] = &["on"];
const WHEN_ATTRS: &[&str] = &["is", "test"];
const OTHERWISE_ATTRS: &[&str] = &[];
const HUB_ATTRS: &[&str] = &["id"];

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
            "prompt" => match &attr.value {
                AttrValue::Str(s) if !s.trim().is_empty() => None,
                _ => Some((
                    E_BRANCH_PROMPT,
                    "`<branch prompt>` must be a non-empty string (dsl 0.11.0 §4)".to_string(),
                )),
            },
            "timeout" => match &attr.value {
                AttrValue::Str(s) if s.parse::<u32>().is_ok_and(|n| n > 0) => None,
                _ => Some((
                    E_BRANCH_TIMEOUT,
                    "`<branch timeout>` must be a positive integer number of seconds (dsl 0.11.0 §4)"
                        .to_string(),
                )),
            },
            _ => None,
        };
        if let Some((code, message)) = bad {
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
    }
}

pub(crate) fn check_hub_attrs(h: &Hub, diags: &mut Vec<Diagnostic>) {
    close(&h.attrs, "hub", HUB_ATTRS, &[], None, diags);
}

pub(crate) fn check_match_attrs(m: &Match, diags: &mut Vec<Diagnostic>) {
    close(&m.attrs, "match", MATCH_ATTRS, &[], None, diags);
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
    close(&c.attrs, "choice", permitted, CHOICE_REMOVED_ATTRS, hint, diags);
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
            _ => format!("`<{tag}>` has no attribute `{key}` (dsl 0.10.0 §4)"),
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
