//! dsl 0.28.0 §3: writing through `occasion.target`.
//!
//! In a beat or entry that targets a kind (`target="kind:K"`) or runs once
//! for each member of one (`for="kind:K"`), `occasion.target` is the member
//! the beat runs for, and it may be written like a literal member:
//! `::set{F[occasion.target] op v}`, `::assert{r(occasion.target)}` /
//! `::retract{…}`, a member-typed directive attribute and a component
//! argument. The checker judges each such write once per member of the
//! enclosing scope ([`crate::occasion_bind::OccasionScopes`]), exactly like a
//! read; the runtime binds the member when the write executes. Outside such a
//! scope the write names no one: one `E-UNDECLARED` at the write.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::AttrValue;
use lute_syntax::datalog::{FactPattern, FactTerm};

use crate::beats::OCCASION_TARGET;
use crate::Ctx;

/// A `::set` path's `[occasion.target]` index.
pub const TARGET_INDEX: &str = "[occasion.target]";

/// The family a `::set` path indexes by `occasion.target` — `run.count` for
/// `run.count[occasion.target]` — or `None` for any other path.
pub fn indexed_family(path: &str) -> Option<&str> {
    path.strip_suffix(TARGET_INDEX).filter(|f| !f.is_empty())
}

/// `path` with its `[occasion.target]` index read as `member`
/// (`run.count[occasion.target]` → `run.count.cod`); any other path as is.
pub fn member_path(path: &str, member: &str) -> String {
    match indexed_family(path) {
        Some(family) => format!("{family}.{member}"),
        None => path.to_string(),
    }
}

/// A path's segments joined with `.` — a last segment that is
/// `occasion.target` (a directive's `fromAttr` segment given the member)
/// indexes the family instead: `run.caught[occasion.target]`.
pub fn join_path(parts: &[String]) -> String {
    match parts.split_last() {
        Some((last, family)) if last == OCCASION_TARGET && !family.is_empty() => {
            format!("{}{TARGET_INDEX}", family.join("."))
        }
        _ => parts.join("."),
    }
}

/// An attribute or argument whose value is `occasion.target`.
pub fn is_target_value(value: &AttrValue) -> bool {
    matches!(value, AttrValue::Str(s) if s == OCCASION_TARGET)
}

/// A fact pattern with an `occasion.target` argument.
pub fn has_target(pattern: &FactPattern) -> bool {
    pattern.args.iter().any(|a| a.term == FactTerm::Target)
}

/// `pattern` with every `occasion.target` argument read as `member`.
pub fn instantiate_pattern(pattern: &FactPattern, member: &str) -> FactPattern {
    let mut p = pattern.clone();
    for a in &mut p.args {
        if a.term == FactTerm::Target {
            a.term = FactTerm::Ident(member.to_string());
        }
    }
    p
}

/// The members `occasion.target` names at `span`, or the one `E-UNDECLARED`
/// for a write outside every kind or `for=` beat.
pub(crate) fn scope_members<'c>(ctx: &'c Ctx<'_>, span: Span) -> Result<&'c [String], Diagnostic> {
    ctx.env
        .occasion_scopes
        .members_at(span.byte_start, span.byte_end)
        .ok_or_else(|| Diagnostic {
            code: "E-UNDECLARED".to_string(),
            severity: Severity::Error,
            message: crate::beats::occasion_target_scope_message(),
            span,
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        })
}

/// `judge` run once per member `occasion.target` names at `span`, the
/// findings grouped ([`crate::cel_resolve::group_per_member`]); outside a
/// kind or `for=` beat, the one scope error.
pub(crate) fn per_member(
    ctx: &Ctx<'_>,
    span: Span,
    judge: impl FnMut(&str) -> Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    match scope_members(ctx, span) {
        Ok(members) => crate::cel_resolve::group_per_member(members, judge),
        Err(d) => vec![d],
    }
}
