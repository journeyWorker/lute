//! dsl 0.21.0 §7a.3: `::accept{quest="<id>"}` — the scene-side form of the
//! engine's "accept quest" action. A core directive of the language
//! ([`lute_syntax::ast::ACCEPT_DIRECTIVE`]), never a capability-snapshot
//! lookup.
//!
//! - Per file ([`check_accept_directive`]): `quest` present, a quoted
//!   string, and a quest identifier; `quest` is its only attribute.
//! - `check-project` ([`check_project_accepts`]): the target names a quest
//!   declared in the project, and that quest is accept-driven (no `start`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, Directive, Document, Node};

use crate::content_line::E_UNKNOWN_ATTR;

/// `::accept` names no quest, a malformed quest id, or (at `check-project`)
/// a quest that is not an accept-driven quest of the project (dsl 0.21.0
/// §7a.3).
pub const E_ACCEPT_TARGET: &str = "E-ACCEPT-TARGET";

/// Per-file shape of one `::accept` directive (dsl 0.21.0 §7a.3):
/// [`E_ACCEPT_TARGET`] for a missing, non-string, or non-identifier `quest`;
/// `E-UNKNOWN-ATTR` for any other attribute.
pub fn check_accept_directive(d: &Directive, diags: &mut Vec<Diagnostic>) {
    let mut quest_seen = false;
    for attr in &d.attrs {
        if attr.key != "quest" {
            diags.push(accept_diag(
                E_UNKNOWN_ATTR,
                format!(
                    "`::accept` has no attribute `{}`; its only attribute is `quest` \
                     (dsl 0.21.0 §7a.3)",
                    attr.key
                ),
                attr.span,
            ));
            continue;
        }
        quest_seen = true;
        match &attr.value {
            AttrValue::Str(id) if crate::check::is_cel_ident(id) => {}
            AttrValue::Str(id) => diags.push(accept_diag(
                E_ACCEPT_TARGET,
                format!(
                    "`::accept` `quest=\"{id}\"` is not a quest id — an identifier \
                     (`[A-Za-z_][A-Za-z0-9_]*`) (dsl 0.21.0 §7a.3)"
                ),
                attr.value_span,
            )),
            _ => diags.push(accept_diag(
                E_ACCEPT_TARGET,
                "`::accept` `quest` must be a quoted quest id (`quest=\"<id>\"`), not a \
                 reference or a bare flag (dsl 0.21.0 §7a.3)"
                    .to_string(),
                attr.span,
            )),
        }
    }
    if !quest_seen {
        diags.push(accept_diag(
            E_ACCEPT_TARGET,
            "`::accept` names no quest; write `::accept{quest=\"<quest id>\"}` \
             (dsl 0.21.0 §7a.3)"
                .to_string(),
            d.span,
        ));
    }
}

/// `check-project` resolution of every well-formed `::accept` target in one
/// resolved project root (dsl 0.21.0 §7a.3): the quest must be declared in
/// the project, and accept-driven (no `start`) — a quest with `start`
/// activates on its own, so accepting it is meaningless. Both faults are
/// [`E_ACCEPT_TARGET`] at the `quest` value. A malformed target is the
/// per-file check's, never re-reported here.
pub fn check_project_accepts(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
    // quest id → does ANY declaration carry `start` (a duplicated id is
    // `E-QUEST-ID-DUP`'s; either declaration's `start` disqualifies it).
    let mut quests: BTreeMap<&str, bool> = BTreeMap::new();
    for (_, doc) in docs {
        for q in &doc.quests {
            if !q.id.is_empty() {
                *quests.entry(q.id.as_str()).or_default() |= q.start.is_some();
            }
        }
    }
    let mut out = Vec::new();
    for (path, doc) in docs {
        let mut visit = |d: &Directive| check_target(d, &quests, path, &mut out);
        for shot in &doc.shots {
            walk(&shot.body, &mut visit);
        }
        for q in &doc.quests {
            walk(&q.body, &mut visit);
        }
        for e in &doc.entries {
            walk(&e.body, &mut visit);
        }
    }
    out
}

fn check_target(
    d: &Directive,
    quests: &BTreeMap<&str, bool>,
    path: &Path,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let Some((id, span)) = d.accept_quest() else {
        return;
    };
    if !crate::check::is_cel_ident(id) {
        return;
    }
    let message = match quests.get(id) {
        Some(false) => return,
        Some(true) => format!(
            "`::accept` targets quest `{id}`, which has a `start` condition and activates on \
             its own; only an accept-driven quest (no `start`) can be accepted (dsl 0.21.0 §7a.3)"
        ),
        None => {
            let hint = lute_manifest::suggest::nearest(id, quests.keys().copied(), 2)
                .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
            format!(
                "`::accept` targets quest `{id}`, but no quest in the project declares that \
                 id{hint} (dsl 0.21.0 §7a.3)"
            )
        }
    };
    out.push((path.to_path_buf(), accept_diag(E_ACCEPT_TARGET, message, span)));
}

/// Every `::accept` directive in `nodes`, recursing into every nested body.
/// A `<track>` clip is not walked: `::accept` there is not a staging leaf
/// and never reaches a walk.
fn walk(nodes: &[Node], f: &mut impl FnMut(&Directive)) {
    for node in nodes {
        match node {
            Node::Directive(d) if d.is_accept() => f(d),
            Node::Branch(b) => b.choices.iter().for_each(|c| walk(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| walk(&c.body, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => walk(body, f),
                    }
                }
            }
            Node::Objective(o) => walk(&o.body, f),
            Node::On(o) => walk(&o.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

fn accept_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
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
