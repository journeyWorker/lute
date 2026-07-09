//! `<on>` ECA trigger validation (dsl 0.2.0 §4): the `event` name.
//!
//! `<on>`'s `when` guard reuses the SAME [`crate::check_cel_slot`] profile gate
//! every other boolean guard flows through (a `<choice when>`, a `<when test>`)
//! — it is validated at the `Walker::walk` `Node::On` arm, not here. This
//! module owns only the event-name half, which is a plain `String` lookup
//! against the built-in lifecycle events (dsl 0.2.0 §4.5, Plan B) and the
//! capability-declared world events, never CEL.

use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::snapshot::{CapabilitySnapshot, BUILTIN_LIFECYCLE_EVENTS};
use lute_syntax::ast::On;

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
    if BUILTIN_LIFECYCLE_EVENTS.contains(&on.event.as_str()) || snapshot.event(&on.event).is_some() {
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

fn diag(code: &str, message: String, on: &On) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span: on.event_span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
}
