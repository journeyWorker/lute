//! 0.21.1 T1-3 / T1-4: a def reference that must become artifact DATA.
//!
//! The artifact carries no defs table (compile-IR D4): every `@def` is inlined
//! at compile time. Two positions have no general CEL slot to inline into, so
//! a def there must be provably representable or the document is rejected —
//! never silently dropped or shipped as CEL source text:
//!
//! * **An attribute value** (`::camera{zoom=@closeUp}`): records carry literal
//!   fields only, so the def must fold to a §5.1-decided constant
//!   ([`fold_attr_ref`]); the compiler writes that literal. A state-dependent
//!   def is `E-ATTR-DEF-DYNAMIC`. The same holds for a def passed as a `::use`
//!   arg whose param lands in an attribute of the component body.
//! * **A `{{@def}}` interpolation**: the placeholder carries the expanded def
//!   body (`expr`), which the engine evaluates. A def that cannot be expanded
//!   into one standalone expression (an expansion cycle, a body reading `$`,
//!   a body that does not parse) is `E-INTERP-DEF`.
//!
//! [`fold_attr_ref`] / [`decided_literal`] / [`inline_interp_ref`] are the
//! single definitions both the checker (here) and the compiler's fold/inline
//! passes use, so `lute check` and `lute compile` agree by construction.

use std::collections::{BTreeMap, BTreeSet};

use lute_cel::scan_refs;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Arm, Attr, AttrValue, ClipNode, Directive, Document, Interp, InterpKind, Node,
};

use crate::cel_expand::{expand_cel, DefTable};
use crate::check::{CheckInput, FoldedEnv};
use crate::component_import::ComponentSet;
use crate::decide::{decide_slot, DecideCtx, Decided};
use crate::meta::StateSchema;

pub const E_ATTR_DEF_DYNAMIC: &str = "E-ATTR-DEF-DYNAMIC";
pub const E_INTERP_DEF: &str = "E-INTERP-DEF";

/// Fold an attribute's `@ref` value (`raw`, expanded or not) to the constant
/// it provably has on every run (§5.1). `None` = state-dependent.
pub fn fold_attr_ref(raw: &str, defs: &DefTable<'_>, schema: &StateSchema) -> Option<Decided> {
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    decide_slot(raw, defs, &ctx)
}

/// A decided constant as the literal attribute text the lowerer reads: a
/// number in shortest decimal form, a bool as `true`/`false`, a string as-is.
pub fn decided_literal(d: &Decided) -> String {
    match d {
        Decided::Bool(b) => b.to_string(),
        Decided::Num(n) => n.to_string(),
        Decided::Str(s) => s.clone(),
    }
}

/// Expand a `{{@def}}` / `{{@fn(args)}}` referent into the standalone CEL an
/// engine evaluates to render it. `Err` carries the reason it cannot be.
pub fn inline_interp_ref(raw: &str, defs: &DefTable<'_>) -> Result<String, String> {
    let body = expand_cel(raw, defs, None, &mut Vec::new())?;
    let mut arena = lute_cel::CelArena::default();
    lute_cel::parse_slot(&mut arena, &body, 0)
        .map(|_| body.clone())
        .map_err(|_| format!("its expansion `{body}` is not a well-formed CEL expression"))
}

/// Whether an attr key on a `<choice>` is owned by another gate: `into`/
/// `value` reject any `@ref` outright (`E-INTO-VALUE`).
pub fn choice_attr_owned_elsewhere(key: &str) -> bool {
    matches!(key, "into" | "value")
}

/// Every non-`$` ref in `raw` names a def with a body. A ref that does not
/// (an undeclared name, a component param) is someone else's diagnostic.
fn all_refs_have_bodies(raw: &str, defs: &DefTable<'_>) -> bool {
    scan_refs(raw)
        .iter()
        .filter(|r| !r.is_dollar)
        .all(|r| defs.bodies.contains_key(&r.name))
}

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The `E-ATTR-DEF-DYNAMIC` diagnostic, shared with the compiler backstop.
pub fn attr_def_dynamic_diag(key: &str, raw: &str, span: Span) -> Diagnostic {
    diag(
        E_ATTR_DEF_DYNAMIC,
        format!(
            "attribute `{key}={raw}` depends on state: an attribute value must be a \
             constant, and this def does not fold to one. Branch with `<match>` and \
             write a literal per arm, or make the def constant"
        ),
        span,
    )
}

/// The `E-INTERP-DEF` diagnostic, shared with the compiler backstop.
pub fn interp_def_diag(raw: &str, reason: &str, span: Span) -> Diagnostic {
    diag(
        E_INTERP_DEF,
        format!("`{{{{{raw}}}}}` cannot be rendered: {reason}"),
        span,
    )
}

/// The check-time pass: every attr / interp / `::use` arg in the root
/// document (shots, quests, entries). Component bodies are reached through
/// their `::use` sites (the param-flow check), never checked here directly —
/// a component file declares no defs.
pub(crate) fn check_def_inlining(
    doc: &Document,
    folded: &FoldedEnv,
    input: &CheckInput,
) -> Vec<Diagnostic> {
    let defs = DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let mut cx = Cx {
        defs,
        schema: &folded.env.state,
        input,
        domains: &folded.domains,
        diags: Vec::new(),
    };
    for shot in &doc.shots {
        cx.nodes(&shot.body);
    }
    for quest in &doc.quests {
        cx.attrs(&quest.attrs, None);
        cx.nodes(&quest.body);
    }
    for entry in &doc.entries {
        cx.attrs(&entry.attrs, None);
        cx.nodes(&entry.body);
    }
    cx.diags
}

struct Cx<'a> {
    defs: DefTable<'a>,
    schema: &'a StateSchema,
    input: &'a CheckInput,
    domains: &'a BTreeMap<String, lute_manifest::snapshot::Domain>,
    diags: Vec<Diagnostic>,
}

impl Cx<'_> {
    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    self.attrs(&l.attrs, None);
                    self.interps(&l.interps);
                }
                Node::Directive(d) if d.tag == "use" => self.use_args(d),
                Node::Directive(d) if d.is_accept() => {}
                Node::Directive(d) => self.attrs(&d.attrs, Some(d)),
                Node::Branch(b) => {
                    self.attrs(&b.attrs, None);
                    for c in &b.choices {
                        self.choice_attrs(&c.attrs);
                        self.interps(&lute_syntax::scan_label_interps(&c.label, c.span));
                        self.nodes(&c.body);
                    }
                }
                Node::Hub(h) => {
                    self.attrs(&h.attrs, None);
                    for c in &h.choices {
                        self.choice_attrs(&c.attrs);
                        self.interps(&lute_syntax::scan_label_interps(&c.label, c.span));
                        self.nodes(&c.body);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.nodes(body)
                            }
                        }
                    }
                }
                Node::Timeline(t) => {
                    for track in &t.tracks {
                        for clip in &track.clips {
                            if let ClipNode::Directive(d) = &clip.node {
                                self.attrs(&d.attrs, Some(d));
                            }
                        }
                    }
                }
                Node::On(o) => {
                    self.attrs(&o.attrs, None);
                    self.nodes(&o.body);
                }
                Node::Objective(o) => {
                    self.attrs(&o.attrs, None);
                    self.nodes(&o.body);
                }
                Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }

    fn choice_attrs(&mut self, attrs: &[Attr]) {
        for a in attrs {
            if !choice_attr_owned_elsewhere(&a.key) {
                self.attr(a, None);
            }
        }
    }

    fn attrs(&mut self, attrs: &[Attr], dir: Option<&Directive>) {
        for a in attrs {
            self.attr(a, dir);
        }
    }

    /// One `@ref` attr: it must fold; a folded literal on a directive is then
    /// validated like the author had written it (`E-BAD-ENUM`, `E-ATTR-TYPE`,
    /// …) — the literal is what the record ships.
    fn attr(&mut self, a: &Attr, dir: Option<&Directive>) {
        let AttrValue::Ref(slot) = &a.value else {
            return;
        };
        if !all_refs_have_bodies(&slot.raw, &self.defs)
            || expand_cel(&slot.raw, &self.defs, Some("$"), &mut Vec::new()).is_err()
        {
            return;
        }
        let Some(folded) = fold_attr_ref(&slot.raw, &self.defs, self.schema) else {
            self.diags
                .push(attr_def_dynamic_diag(&a.key, &slot.raw, a.value_span));
            return;
        };
        let Some(dir) = dir else {
            return;
        };
        let snapshot = &self.input.snapshot;
        let decl = snapshot
            .directive(&dir.tag)
            .and_then(|d| d.attrs.iter().find(|ad| ad.name == a.key))
            .or_else(|| snapshot.stamp_attrs.get(&a.key));
        if let Some(adecl) = decl {
            let literal = Attr {
                key: a.key.clone(),
                value: AttrValue::Str(decided_literal(&folded)),
                value_span: a.value_span,
                span: a.span,
            };
            crate::directives::check_attr_value(
                &format!("::{}", dir.tag),
                adecl,
                &literal,
                snapshot,
                &self.input.providers,
                self.domains,
                &mut self.diags,
            );
        }
    }

    fn interps(&mut self, interps: &[Interp]) {
        for i in interps {
            if i.kind != InterpKind::Ref || !all_refs_have_bodies(&i.raw, &self.defs) {
                continue;
            }
            if let Err(reason) = inline_interp_ref(&i.raw, &self.defs) {
                self.diags.push(interp_def_diag(&i.raw, &reason, i.span));
            }
        }
    }

    /// A `::use` arg is bound into the component body. A state-dependent def
    /// arg is legal there — unless the body puts that param into an attribute,
    /// where it would have to fold.
    fn use_args(&mut self, d: &Directive) {
        let Some(component) = d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
            ("component", AttrValue::Str(s)) => Some(s.as_str()),
            _ => None,
        }) else {
            return;
        };
        for a in &d.attrs {
            let AttrValue::Ref(slot) = &a.value else {
                continue;
            };
            if !all_refs_have_bodies(&slot.raw, &self.defs)
                || expand_cel(&slot.raw, &self.defs, Some("$"), &mut Vec::new()).is_err()
                || fold_attr_ref(&slot.raw, &self.defs, self.schema).is_some()
            {
                continue;
            }
            let mut seen = BTreeSet::new();
            if let Some(at) =
                param_attr_use(&self.input.components, component, &a.key, &mut seen)
            {
                let mut d = attr_def_dynamic_diag(&a.key, &slot.raw, a.value_span);
                d.message = format!(
                    "{} (component `{component}` puts `@{}` into attribute `{at}`)",
                    d.message, a.key
                );
                self.diags.push(d);
            }
        }
    }
}

/// The first attribute key in `component`'s body (following nested `::use`
/// forwarding) whose value reads `@param`, or `None` when the param only
/// reaches CEL slots / interpolations. `seen` guards (component, param)
/// revisits; cycles are `E-COMPONENT-CYCLE`'s.
fn param_attr_use(
    set: &ComponentSet,
    component: &str,
    param: &str,
    seen: &mut BTreeSet<(String, String)>,
) -> Option<String> {
    if !seen.insert((component.to_string(), param.to_string())) {
        return None;
    }
    let def = set.table.get(component)?;
    def.body
        .shots
        .iter()
        .find_map(|s| body_attr_use(set, &s.body, param, seen))
}

fn reads_param(raw: &str, param: &str) -> bool {
    scan_refs(raw).iter().any(|r| !r.is_dollar && r.name == param)
}

fn body_attr_use(
    set: &ComponentSet,
    nodes: &[Node],
    param: &str,
    seen: &mut BTreeSet<(String, String)>,
) -> Option<String> {
    let in_attrs = |attrs: &[Attr]| {
        attrs.iter().find_map(|a| match &a.value {
            AttrValue::Ref(slot) if reads_param(&slot.raw, param) => Some(a.key.clone()),
            _ => None,
        })
    };
    for node in nodes {
        let hit = match node {
            Node::Line(l) => in_attrs(&l.attrs),
            Node::Directive(d) if d.tag == "use" => {
                let inner = d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
                    ("component", AttrValue::Str(s)) => Some(s.clone()),
                    _ => None,
                });
                inner.and_then(|inner| {
                    d.attrs.iter().find_map(|a| match &a.value {
                        AttrValue::Ref(slot) if reads_param(&slot.raw, param) => {
                            param_attr_use(set, &inner, &a.key, seen)
                        }
                        _ => None,
                    })
                })
            }
            Node::Directive(d) => in_attrs(&d.attrs),
            Node::Match(m) => m.arms.iter().find_map(|arm| match arm {
                Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                    body_attr_use(set, body, param, seen)
                }
            }),
            Node::Timeline(t) => t.tracks.iter().find_map(|track| {
                track.clips.iter().find_map(|clip| match &clip.node {
                    ClipNode::Directive(d) => in_attrs(&d.attrs),
                    ClipNode::Set(_) => None,
                })
            }),
            _ => None,
        };
        if hit.is_some() {
            return hit;
        }
    }
    None
}
