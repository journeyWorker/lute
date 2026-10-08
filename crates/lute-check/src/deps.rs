//! Span-bearing dependency feed for the semantic project graph.
//!
//! The feed is deliberately an adapter over the syntax tree.  It retains
//! authored provenance (and, through [`slot_dependencies`], an expanded slot's
//! provenance) without substituting for checker analyses.

use cel_parser::ast::{operators as op, EntryExpr, Expr};
use cel_parser::reference::Val;
use lute_core_span::Span;
use lute_syntax::ast::{Arm, Attr, AttrValue, CelSlot, ClipNode, Document, InterpKind, Node};
use lute_manifest::fact::{FactArg, FactPattern, FactTerm};

#[derive(Clone, Debug, PartialEq)]
pub struct StateUse {
    pub path: String,
    pub span: Span,
    pub reason: String,
    /// Whether the path is an ordinary value read or a presence guard.
    pub role: crate::cel_paths::PathRole,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StateWrite {
    pub path: String,
    /// The complete `::set` command span, not just the path token.
    pub span: Span,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactUse {
    pub pattern: FactPattern,
    pub span: Span,
    pub negated: bool,
    pub form: String,
    /// The complete raw CEL slot containing the matched query. CEL AST nodes do
    /// not retain source offsets, so this is intentionally the slot's source.
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FactWrite {
    pub pattern: FactPattern,
    /// The complete `::assert`/`::retract` command span.
    pub span: Span,
    pub retract: bool,
    /// Authored fact payload, retained for an edge reason/source display.
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GateUse {
    pub target: String,
    pub span: Span,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DefUse {
    pub name: String,
    pub span: Span,
    pub expanded_span: Option<Span>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentUse {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DependencyFeed {
    pub reads: Vec<StateUse>,
    pub writes: Vec<StateWrite>,
    pub queries: Vec<FactUse>,
    pub fact_writes: Vec<FactWrite>,
    pub gates: Vec<GateUse>,
    pub defs: Vec<DefUse>,
    pub components: Vec<ComponentUse>,
}

/// Collect dependencies from one CEL slot's raw text.
///
/// Callers that have expanded a definition/component slot should pass the
/// expanded text here.  `span` remains the authored enclosing slot span, as
/// required by the CEL parser's source-position limitation.
pub fn slot_dependencies(raw: &str, span: Span) -> DependencyFeed {
    let mut out = DependencyFeed::default();
    scan_raw_slot(raw, span, &mut out);
    out
}

/// Walk every authored body exactly once.  Expanded component/definition
/// bodies are supplied by the model through [`slot_dependencies`], since this
/// adapter intentionally has no expansion environment.
pub fn collect_document_dependencies(doc: &Document) -> DependencyFeed {
    let mut out = DependencyFeed::default();
    for shot in &doc.sections {
        walk_nodes(&shot.body, &mut out);
    }
    for quest in &doc.quests {
        scan_attrs(&quest.attrs, &mut out);
        scan_slot(quest.start.as_ref(), &mut out);
        scan_slot(quest.fail.as_ref(), &mut out);
        scan_slot(quest.rearm.as_ref(), &mut out);
        for reward in &quest.rewards {
            scan_attrs(&reward.attrs, &mut out);
            scan_slot(reward.when.as_ref(), &mut out);
        }
        walk_nodes(&quest.body, &mut out);
    }
    for entry in &doc.entries {
        scan_attrs(&entry.attrs, &mut out);
        scan_slot(entry.when.as_ref(), &mut out);
        scan_slot(entry.spent_by.as_ref(), &mut out);
        walk_nodes(&entry.body, &mut out);
    }
    for beat in &doc.beats {
        scan_attrs(&beat.attrs, &mut out);
        if let Some(template) = &beat.template {
            out.components.push(ComponentUse {
                name: template.name.clone(),
                span: template.span,
            });
        }
        scan_slot(beat.when.as_ref(), &mut out);
        scan_slot(beat.spent_by.as_ref(), &mut out);
        walk_nodes(&beat.body, &mut out);
    }
    out
}

fn walk_nodes(nodes: &[Node], out: &mut DependencyFeed) {
    for node in nodes {
        match node {
            Node::Line(line) => {
                scan_slot(line.when.as_ref(), out);
                for interp in &line.interps {
                    match interp.kind {
                        InterpKind::Path => out.reads.push(StateUse {
                            path: crate::cel_paths::text_state_path(&interp.raw)
                                .unwrap_or_else(|| interp.raw.clone()),
                            span: interp.span,
                            reason: interp.raw.clone(),
                            role: crate::cel_paths::PathRole::Read,
                        }),
                        InterpKind::Ref => push_def_refs(&interp.raw, interp.span, out),
                        InterpKind::Reserved => {}
                    }
                }
                scan_attrs(&line.attrs, out);
            }
            Node::Directive(d) => {
                scan_slot(d.when.as_ref(), out);
                if d.tag == "use" {
                    if let Some((name, span)) =
                        d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
                            ("component", AttrValue::Str(s)) | ("name", AttrValue::Str(s)) => {
                                Some((s.clone(), a.value_span))
                            }
                            _ => None,
                        })
                    {
                        out.components.push(ComponentUse { name, span });
                    }
                }
                scan_attrs(&d.attrs, out);
            }
            Node::Set(s) => {
                out.writes.push(StateWrite {
                    path: s.path.clone(),
                    span: s.span,
                    reason: format!("{} {} {}", s.path, s.op, s.expr.raw.trim()),
                });
                scan_slot(Some(&s.expr), out);
                scan_slot(s.when.as_ref(), out);
            }
            Node::Assert(a) => {
                out.fact_writes.push(FactWrite {
                    pattern: a.pattern.clone(),
                    span: a.span,
                    retract: false,
                    reason: a.raw.trim().to_string(),
                });
                scan_slot(a.when.as_ref(), out);
            }
            Node::Retract(r) => {
                out.fact_writes.push(FactWrite {
                    pattern: r.pattern.clone(),
                    span: r.span,
                    retract: true,
                    reason: r.raw.trim().to_string(),
                });
                scan_slot(r.when.as_ref(), out);
            }
            Node::Branch(b) => {
                scan_attrs(&b.attrs, out);
                for choice in &b.choices {
                    scan_attrs(&choice.attrs, out);
                    scan_slot(choice.when.as_ref(), out);
                    walk_nodes(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                scan_attrs(&h.attrs, out);
                for choice in &h.choices {
                    scan_attrs(&choice.attrs, out);
                    scan_slot(choice.when.as_ref(), out);
                    walk_nodes(&choice.body, out);
                }
                if let Some(ret) = &h.on_return {
                    scan_attrs(&ret.attrs, out);
                    walk_nodes(&ret.body, out);
                }
            }
            Node::Match(m) => {
                scan_attrs(&m.attrs, out);
                scan_slot(Some(&m.subject), out);
                for arm in &m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            scan_slot(Some(test), out);
                            walk_nodes(body, out);
                        }
                        Arm::Otherwise { body, .. } => walk_nodes(body, out),
                    }
                }
            }
            Node::Timeline(t) => {
                scan_slot(t.duration.as_ref(), out);
                for track in &t.tracks {
                    for clip in &track.clips {
                        match &clip.node {
                            ClipNode::Directive(d) => {
                                scan_slot(d.when.as_ref(), out);
                                scan_attrs(&d.attrs, out);
                            }
                            ClipNode::Set(s) => {
                                out.writes.push(StateWrite {
                                    path: s.path.clone(),
                                    span: s.span,
                                    reason: format!("{} {} {}", s.path, s.op, s.expr.raw.trim()),
                                });
                                scan_slot(Some(&s.expr), out);
                                scan_slot(s.when.as_ref(), out);
                            }
                        }
                    }
                }
            }
            Node::Objective(o) => {
                scan_attrs(&o.attrs, out);
                scan_slot(Some(&o.done), out);
                scan_slot(o.visible_when.as_ref(), out);
                scan_slot(o.by.as_ref(), out);
                scan_slot(o.until.as_ref(), out);
                for reward in &o.rewards {
                    scan_attrs(&reward.attrs, out);
                    scan_slot(reward.when.as_ref(), out);
                }
                walk_nodes(&o.body, out);
            }
            Node::On(on) => {
                scan_attrs(&on.attrs, out);
                scan_slot(on.when.as_ref(), out);
                walk_nodes(&on.body, out);
            }
        }
    }
}

fn scan_attrs(attrs: &[Attr], out: &mut DependencyFeed) {
    for attr in attrs {
        if let AttrValue::Ref(slot) = &attr.value {
            scan_slot(Some(slot), out);
        }
    }
}

fn scan_slot(slot: Option<&CelSlot>, out: &mut DependencyFeed) {
    if let Some(slot) = slot {
        scan_raw_slot(&slot.raw, slot.span, out);
    }
}

fn scan_raw_slot(raw: &str, span: Span, out: &mut DependencyFeed) {
    push_def_refs(raw, span, out);
    let mut arena = lute_cel::CelArena::default();
    let Some(handle) = lute_cel::parse_slot_marked_refs(&mut arena, raw) else {
        return;
    };
    let Some(root) = arena.get(handle) else {
        return;
    };
    walk_expr(&root.expr, raw, span, false, out);
    for use_ in crate::cel_paths::collect_path_uses(&root.expr) {
        out.reads.push(StateUse {
            path: use_.path,
            span,
            reason: raw.trim().to_string(),
            role: use_.role,
        });
    }
}

fn push_def_refs(raw: &str, span: Span, out: &mut DependencyFeed) {
    for reference in lute_cel::scan_refs(raw) {
        if reference.is_dollar || reference.name.is_empty() {
            continue;
        }
        out.defs.push(DefUse {
            name: reference.name,
            span: map_local_span(span, reference.span),
            expanded_span: None,
        });
    }
}

fn map_local_span(base: Span, local: Span) -> Span {
    Span {
        byte_start: base.byte_start + local.byte_start,
        byte_end: base.byte_start + local.byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

fn walk_expr(expr: &Expr, raw: &str, span: Span, negated: bool, out: &mut DependencyFeed) {
    match expr {
        Expr::Call(c) => {
            if c.target.is_none() && c.func_name == op::LOGICAL_NOT && c.args.len() == 1 {
                walk_expr(&c.args[0].expr, raw, span, !negated, out);
                return;
            }
            if c.target.is_none()
                && matches!(
                    c.func_name.as_str(),
                    "holds" | "count" | "countDistinct" | "validAt"
                )
            {
                if let Some(pattern) = query_pattern(c) {
                    out.queries.push(FactUse {
                        pattern,
                        span,
                        negated,
                        form: c.func_name.clone(),
                        reason: raw.trim().to_string(),
                    });
                    if c.func_name == "validAt" {
                        if let Some(time) = c.args.get(2) {
                            walk_expr(&time.expr, raw, span, negated, out);
                        }
                    }
                    return;
                }
            }
            if matches!(c.func_name.as_str(), "visited" | "completed" | "active")
                && c.target.is_none()
                && c.args.len() == 1
            {
                if let Expr::Literal(Val::String(target)) = &c.args[0].expr {
                    out.gates.push(GateUse {
                        target: target.clone(),
                        span,
                        reason: raw.trim().to_string(),
                    });
                }
            }
            if let Some(target) = &c.target {
                walk_expr(&target.expr, raw, span, negated, out);
            }
            for arg in &c.args {
                walk_expr(&arg.expr, raw, span, negated, out);
            }
        }
        Expr::Select(s) => walk_expr(&s.operand.expr, raw, span, negated, out),
        Expr::List(l) => {
            for element in &l.elements {
                walk_expr(&element.expr, raw, span, negated, out);
            }
        }
        Expr::Map(m) => {
            for entry in &m.entries {
                walk_entry(&entry.expr, raw, span, negated, out);
            }
        }
        Expr::Struct(s) => {
            for entry in &s.entries {
                walk_entry(&entry.expr, raw, span, negated, out);
            }
        }
        Expr::Comprehension(c) => {
            walk_expr(&c.iter_range.expr, raw, span, negated, out);
            walk_expr(&c.accu_init.expr, raw, span, negated, out);
            walk_expr(&c.loop_cond.expr, raw, span, negated, out);
            walk_expr(&c.loop_step.expr, raw, span, negated, out);
            walk_expr(&c.result.expr, raw, span, negated, out);
        }
        Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => {}
    }
}

fn walk_entry(entry: &EntryExpr, raw: &str, span: Span, negated: bool, out: &mut DependencyFeed) {
    match entry {
        EntryExpr::MapEntry(m) => {
            walk_expr(&m.key.expr, raw, span, negated, out);
            walk_expr(&m.value.expr, raw, span, negated, out);
        }
        EntryExpr::StructField(f) => walk_expr(&f.value.expr, raw, span, negated, out),
    }
}

fn query_pattern(c: &cel_parser::ast::CallExpr) -> Option<FactPattern> {
    let expected_args = match c.func_name.as_str() {
        "holds" | "count" => 2,
        "countDistinct" | "validAt" => 3,
        _ => return None,
    };
    if c.target.is_some() || c.args.len() != expected_args {
        return None;
    }
    let relation = match &c.args.first()?.expr {
        Expr::Literal(Val::String(s)) => s.clone(),
        _ => return None,
    };
    let Expr::List(list) = &c.args.get(1)?.expr else {
        return None;
    };
    let args = list
        .elements
        .iter()
        .map(|arg| {
            Some(FactArg {
                term: fact_term(&arg.expr)?,
                span: (0, 0),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(FactPattern {
        relation,
        relation_span: (0, 0),
        args,
        span: (0, 0),
    })
}

fn fact_term(expr: &Expr) -> Option<FactTerm> {
    match expr {
        Expr::Literal(Val::String(s)) if s == "_" => Some(FactTerm::Wildcard),
        Expr::Literal(Val::String(s)) => Some(FactTerm::Ident(s.clone())),
        Expr::Literal(Val::Boolean(b)) => Some(FactTerm::Bool(*b)),
        Expr::Ident(s) if s.starts_with(lute_cel::REF_MARKER) => {
            Some(FactTerm::Param(s[lute_cel::REF_MARKER.len()..].to_string()))
        }
        Expr::Ident(s) if s == "_" => Some(FactTerm::Wildcard),
        Expr::Select(_)
            if crate::cel_paths::select_path(expr).as_deref() == Some("occasion.target") =>
        {
            Some(FactTerm::Target)
        }
        _ => None,
    }
}
