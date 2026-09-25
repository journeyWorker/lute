//! `<on>` ECA trigger validation (dsl 0.2.0 §4): the `event` name, and its
//! `target` (dsl 0.24.0 §2).
//!
//! `<on>`'s `when` guard reuses the SAME [`crate::check_cel_slot`] profile gate
//! every other boolean guard flows through (a `<choice when>`, a `<when test>`)
//! — it is validated at the `Walker::walk` `Node::On` arm, not here. This
//! module owns the event-name half, which is a plain `String` lookup
//! against the built-in lifecycle events (dsl 0.2.0 §4.5, Plan B) and the
//! capability-declared world events, never CEL, and the `target=` half,
//! checked like an `<objective on target>`'s against the occasion vocabulary.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::EntityKindDecl;
use lute_manifest::schema::OccasionDecl;
use lute_manifest::snapshot::{CapabilitySnapshot, BUILTIN_LIFECYCLE_EVENTS};
use lute_syntax::ast::{Arm, Node, On, Quest};

use crate::beats::{occasion_target_ok, E_BEAT_ATTR};
use crate::lore::is_entry_target;

/// `<on>` without an `event` attribute (dsl 0.2.0 §4.1).
pub const E_ON_NO_EVENT: &str = "E-ON-NO-EVENT";
/// `<on event>` naming no built-in lifecycle event or capability-declared
/// world event (dsl 0.2.0 §4.5).
pub const E_UNKNOWN_EVENT: &str = "E-UNKNOWN-EVENT";

/// Validate an `<on>` trigger's `event` (dsl 0.2.0 §4.1, §4.5): an empty
/// `event` is `E-ON-NO-EVENT`; a non-empty `event` that resolves to neither a
/// [`BUILTIN_LIFECYCLE_EVENTS`] name nor a `snapshot.events` entry (a
/// capability-declared world event, Plan B) is `E-UNKNOWN-EVENT`.
pub fn check_on_event(on: &On, snapshot: &CapabilitySnapshot) -> Vec<Diagnostic> {
    if on.event.is_empty() {
        return vec![diag(
            E_ON_NO_EVENT,
            "`<on>` has no `event`; every `<on>` must be anchored to a discrete event \
             (dsl 0.2.0 §4.1)"
                .to_string(),
            on,
        )];
    }
    if BUILTIN_LIFECYCLE_EVENTS.contains(&on.event.as_str()) || snapshot.event(&on.event).is_some()
    {
        return Vec::new();
    }
    vec![diag(
        E_UNKNOWN_EVENT,
        format!(
            "`<on event=\"{}\">` names no built-in lifecycle event or capability-declared \
             world event (dsl 0.2.0 §4.5)",
            on.event
        ),
        on,
    )]
}

/// dsl 0.24.0 §2: every `<on event="E" target="T">` of `quests`, wherever it
/// sits in a quest body. The handler fires only when the same-named occasion
/// `E` is raised for `T`, so `target=` follows the `<objective on target>`
/// rule ([`crate::beats::check_objective_occasions`]) with [`E_BEAT_ATTR`]:
/// a quoted dotted id; never on a quest lifecycle event (those fire for
/// their own quest, never for a target); on an occasion declared with a
/// target; inside that occasion's target domain ([`occasion_target_ok`];
/// `kinds` is the project's `entities:` vocabulary). The occasion half is
/// shape-only while no occasion is declared, as an objective's is.
pub(crate) fn check_on_targets(
    quests: &[Quest],
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for q in quests {
        for_each_on(&q.body, &mut |on| check_on_target(on, occasions, kinds, &mut diags));
    }
    diags
}

fn check_on_target(
    on: &On,
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
    diags: &mut Vec<Diagnostic>,
) {
    // A non-string value stays residual (the parser extracts only quoted
    // strings).
    for attr in on.attrs.iter().filter(|a| a.key == "target") {
        diags.push(target_diag(
            "`<on>` attribute `target` must be a quoted string (dsl 0.24.0 §2)".to_string(),
            attr.span,
        ));
    }
    let Some((target, span)) = &on.target else {
        return;
    };
    let event = on.event.as_str();
    let message = if !is_entry_target(target) {
        format!(
            "`<on>` `target=\"{target}\"` must be a dotted id `Ident (\".\" Segment)*`, e.g. \
             `npc.maud` (dsl 0.24.0 §2)"
        )
    } else if event.is_empty() {
        // `E-ON-NO-EVENT` is the whole story.
        return;
    } else if BUILTIN_LIFECYCLE_EVENTS.contains(&event) {
        format!(
            "`<on event=\"{event}\">` is a quest lifecycle event: it fires for its own quest and \
             is never raised for a target; remove `target` (dsl 0.24.0 §2)"
        )
    } else if occasions.is_empty() {
        return;
    } else {
        match occasions.get(event) {
            Some(decl) if decl.target.takes_target() => match occasion_target_ok(decl, target, kinds)
            {
                Ok(()) => return,
                Err(why) => why,
            },
            Some(_) => format!(
                "`<on event=\"{event}\">` `target=` needs a targeted occasion named `{event}`, but \
                 occasion `{event}` is not raised for a target (declared without `target: true`); \
                 remove `target` (dsl 0.24.0 §2)"
            ),
            None => {
                let targeted = occasions
                    .iter()
                    .filter(|(_, d)| d.target.takes_target())
                    .map(|(name, _)| name.as_str());
                let hint = lute_manifest::suggest::nearest(event, targeted, 2)
                    .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
                format!(
                    "`<on event=\"{event}\">` `target=` needs a targeted occasion named `{event}` \
                     — the handler fires when that occasion is raised for the target — but no \
                     resolved plugin declares occasion `{event}`{hint} (dsl 0.24.0 §2)"
                )
            }
        }
    };
    diags.push(target_diag(message, *span));
}

/// Every `<on>` in `nodes`, recursing into every nested body.
fn for_each_on<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a On)) {
    for node in nodes {
        match node {
            Node::On(o) => {
                f(o);
                for_each_on(&o.body, f);
            }
            Node::Branch(b) => b.choices.iter().for_each(|c| for_each_on(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| for_each_on(&c.body, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            for_each_on(body, f)
                        }
                    }
                }
            }
            Node::Objective(o) => for_each_on(&o.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

fn target_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_BEAT_ATTR.to_string(),
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

fn diag(code: &str, message: String, on: &On) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: on.event_span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
