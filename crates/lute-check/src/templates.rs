//! dsl 0.27.0 §6: beat templates. A component MAY declare a `beat:` header
//! template in its frontmatter:
//!
//! ```yaml
//! component: bondStory
//! params: { hero: { entity: hero }, rank: { enum: [r1, r2, r3] }, prev: { type: string, default: "" } }
//! beat:
//!   on: bondStory
//!   target: "hero.@hero"
//!   once: user
//!   when: "holds(bondRank(@hero, @rank))"
//!   after: "@prev"        # omitted when empty
//! ```
//!
//! and a bundle document then writes `<beat use="bondStory" id="ariaR2"
//! hero="aria" rank="r2">…</beat>`. [`expand_beat_templates`] DESUGARS every
//! such use, before any check, into an ordinary [`BundleBeat`]:
//!
//! * each header key the `<beat>` does not write itself is the template's,
//!   with every `@param` replaced by the argument's text (or the param's
//!   `default:`); a key whose value comes out empty is omitted; an `after:`
//!   that comes out a bare id means `visited("<id>")`. Every derived key is
//!   spanned at the `use="…"` value — the use site;
//! * the body is `::use{component="bondStory" hero="aria" rank="r2"}` (the
//!   template's own body, run first, with the arguments checked exactly as
//!   any `::use`'s) followed by the use's body. A `::body` directive at the
//!   top level of the template body places the use's body there instead: the
//!   desugared beat then ends with a `::body{component="bondStory"}` marker
//!   and the compiler's `::use` expansion splits the template at it.
//!
//! So every checker pass, `lute beats`, the scenario graph, the compiler and
//! the editor see beats they already understand; the only template-specific
//! diagnostics are `E-TEMPLATE`'s ([`check_body_markers`] and the header
//! checks here).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Attr, AttrValue, BundleBeat, CelKind, CelSlot, Directive, Document, Meta, Node,
};

use crate::component_import::{ComponentDef, ComponentSet};
use lute_manifest::schema::DefParam;

/// Beat template misuse (dsl 0.27.0 §6).
pub const E_TEMPLATE: &str = "E-TEMPLATE";

/// `::body` — in a template body, where the use's body goes.
pub const BODY_DIRECTIVE: &str = "body";

/// The keys a `beat:` header template may declare: every `<beat>` header
/// attribute but `id` (each use names its own) and `also`.
pub const TEMPLATE_KEYS: &[&str] = &[
    "on", "target", "for", "title", "priority", "once", "share", "after", "when", "spentBy",
];

/// The header keys that are conditions: an argument may be an expression
/// (`rank=@best`) there; everywhere else a `@param` stands for literal text.
const CEL_KEYS: &[&str] = &["when", "spentBy"];

/// A component's `beat:` header template (dsl 0.27.0 §6): each declared key
/// with its raw text, in declaration order, and the `beat:` key's span in
/// the component file.
#[derive(Clone, Debug)]
pub struct BeatTemplate {
    pub keys: Vec<(String, String)>,
    pub span: Span,
}

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_TEMPLATE.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Every `@name` reference in `raw`: `(byte start, byte end, name)`. A
/// doubled `@@` (a speaker line's head) is no reference.
fn at_refs(raw: &str) -> Vec<(usize, usize, &str)> {
    let b = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' && (i == 0 || b[i - 1] != b'@') {
            let start = i + 1;
            let mut end = start;
            while end < b.len() && (b[end].is_ascii_alphanumeric() || b[end] == b'_') {
                end += 1;
            }
            if end > start && !b[start].is_ascii_digit() {
                out.push((i, end, &raw[start..end]));
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// `raw` with every `@param` that `args` binds replaced by its text.
fn substitute(raw: &str, args: &BTreeMap<String, String>) -> String {
    let mut out = raw.to_string();
    for (s, e, name) in at_refs(raw).into_iter().rev() {
        if let Some(text) = args.get(name) {
            out.replace_range(s..e, text);
        }
    }
    out
}

/// Lift a component's `beat:` value (dsl 0.27.0 §6). `params` are the
/// component's declared params: a `@name` in a non-condition key must name
/// one (in `when:`/`spentBy:` it may also be a `@def` of the host).
pub fn parse_beat_template(
    meta: &Meta,
    value: &serde_yaml::Value,
    params: &[DefParam],
) -> (Option<BeatTemplate>, Vec<Diagnostic>) {
    let span = crate::meta::meta_key_span(meta, "beat");
    let mut diags = Vec::new();
    let serde_yaml::Value::Mapping(map) = value else {
        diags.push(diag(
            "`beat:` must be a mapping of beat header keys, e.g. `beat: { on: bondStory, \
             once: user }` (dsl 0.27.0 §6)"
                .to_string(),
            span,
        ));
        return (None, diags);
    };
    let mut keys = Vec::new();
    for (k, v) in map {
        let Some(key) = k.as_str() else {
            diags.push(diag(
                "every `beat:` key must be a string (dsl 0.27.0 §6)".to_string(),
                span,
            ));
            continue;
        };
        let at = crate::meta::meta_key_span(meta, key);
        if !TEMPLATE_KEYS.contains(&key) {
            let why = match key {
                "id" => " — each `<beat use=…>` names its own `id=`".to_string(),
                _ => lute_manifest::suggest::nearest(key, TEMPLATE_KEYS.iter().copied(), 2)
                    .map(|n| format!(" — did you mean `{n}`?"))
                    .unwrap_or_else(|| format!("; the keys are {}", TEMPLATE_KEYS.join(", "))),
            };
            diags.push(diag(
                format!("`beat.{key}` is not a beat template key{why} (dsl 0.27.0 §6)"),
                at,
            ));
            continue;
        }
        let text = match v {
            serde_yaml::Value::String(s) => s.clone(),
            serde_yaml::Value::Number(n) => n.to_string(),
            serde_yaml::Value::Bool(b) => b.to_string(),
            _ => {
                diags.push(diag(
                    format!(
                        "`beat.{key}` must be text, like the `{key}=` attribute it becomes \
                         (dsl 0.27.0 §6)"
                    ),
                    at,
                ));
                continue;
            }
        };
        if !CEL_KEYS.contains(&key) {
            for (_, _, name) in at_refs(&text) {
                if !params.iter().any(|p| p.name == name) {
                    diags.push(diag(
                        format!(
                            "`beat.{key}` names `@{name}`, but the component declares no param \
                             `{name}` — declare it under `params:` (dsl 0.27.0 §6)"
                        ),
                        at,
                    ));
                }
            }
        }
        keys.push((key.to_string(), text));
    }
    (Some(BeatTemplate { keys, span }), diags)
}

/// An argument's text in a header: a literal as written, a `@def` as its
/// CEL text; `true` for a bare flag.
fn arg_text(value: &AttrValue) -> (String, bool) {
    match value {
        AttrValue::Str(s) => (s.clone(), true),
        AttrValue::BoolTrue => ("true".to_string(), true),
        AttrValue::Ref(slot) => (slot.raw.trim().to_string(), false),
    }
}

/// Does `def`'s body hold a top-level `::body` (the use's body's place)?
pub fn has_body_marker(def: &ComponentDef) -> bool {
    def.body
        .shots
        .iter()
        .flat_map(|s| &s.body)
        .any(|n| matches!(n, Node::Directive(d) if d.tag == BODY_DIRECTIVE))
}

/// Is `id` a bare canonical id (`bonds.aria.ariaR1`) — an `after:` that
/// names one beat or scene, sugar for `visited("<id>")`?
fn bare_id(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Whether the `<beat>` already writes header `key` itself (it wins).
fn written(beat: &BundleBeat, key: &str) -> bool {
    match key {
        "on" => beat.on.is_some(),
        "target" => beat.target.is_some(),
        "for" => beat.for_kind.is_some(),
        "title" => beat.title.is_some(),
        "priority" => beat.priority.is_some(),
        "once" => beat.once.is_some(),
        "share" => beat.share.is_some(),
        "after" => beat.after.is_some(),
        "when" => beat.when.is_some(),
        "spentBy" => beat.spent_by.is_some(),
        _ => true,
    }
}

fn set_key(beat: &mut BundleBeat, key: &str, value: String, at: Span) {
    let text = Some((value.clone(), at));
    match key {
        "on" => beat.on = text,
        "target" => beat.target = text,
        "for" => beat.for_kind = text,
        "title" => beat.title = text,
        "priority" => beat.priority = text,
        "once" => beat.once = text,
        "share" => beat.share = text,
        "after" => {
            let value = if bare_id(value.trim()) {
                format!("visited(\"{}\")", value.trim())
            } else {
                value
            };
            beat.after = Some((value, at));
        }
        "when" => beat.when = Some(CelSlot::raw(CelKind::Condition, value, at)),
        "spentBy" => beat.spent_by = Some(CelSlot::raw(CelKind::Condition, value, at)),
        _ => {}
    }
}

fn component_attr(name: &str, at: Span) -> Attr {
    Attr {
        key: "component".to_string(),
        value: AttrValue::Str(name.to_string()),
        value_span: at,
        span: at,
    }
}

/// Desugar every `<beat use="…">` of `doc` (dsl 0.27.0 §6; see the module
/// doc). Runs once per beat (`TemplateUse::expanded`), so every surface may
/// call it on the document it parsed; the diagnostics come with the first
/// expansion. `components` is the document's resolved `components:`.
pub fn expand_beat_templates(doc: &mut Document, components: &ComponentSet) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for beat in &mut doc.beats {
        let Some(tu) = beat.template.as_mut().filter(|t| !t.expanded) else {
            continue;
        };
        tu.expanded = true;
        let (name, at) = (tu.name.clone(), tu.span);
        let Some(def) = components.table.get(&name) else {
            let hint = lute_manifest::suggest::nearest(
                &name,
                components.table.keys().map(String::as_str),
                2,
            )
            .map(|n| format!(" — did you mean `{n}`?"))
            .unwrap_or_else(|| {
                " — import the component file under `components:` in this document's \
                 frontmatter (or the project's `defaults:`)"
                    .to_string()
            });
            diags.push(diag(
                format!("`<beat use=\"{name}\">` names no component{hint} (dsl 0.27.0 §6)"),
                at,
            ));
            beat.attrs.clear();
            continue;
        };
        let Some(template) = &def.beat else {
            diags.push(diag(
                format!(
                    "component `{name}` declares no `beat:` header, so `<beat use=\"{name}\">` has \
                     nothing to derive — add a `beat:` block to its frontmatter, or play it with \
                     `::use{{component=\"{name}\"}}` inside an ordinary beat (dsl 0.27.0 §6)"
                ),
                at,
            ));
            beat.attrs.clear();
            continue;
        };
        let args = std::mem::take(&mut beat.attrs);
        // Each param's text: the use's argument, else its `default:`.
        let mut text: BTreeMap<String, String> = BTreeMap::new();
        let mut expr: BTreeMap<String, Span> = BTreeMap::new();
        for (p, _) in &def.params {
            let (value, span) = match args.iter().find(|a| &a.key == p) {
                Some(a) => (&a.value, a.value_span),
                None => match def.defaults.get(p) {
                    Some(v) => (v, at),
                    None => continue,
                },
            };
            let (t, literal) = arg_text(value);
            if !literal {
                expr.insert(p.clone(), span);
            }
            text.insert(p.clone(), t);
        }
        for (key, raw) in &template.keys {
            if written(beat, key) {
                continue;
            }
            if !CEL_KEYS.contains(&key.as_str()) {
                let bad = at_refs(raw)
                    .into_iter()
                    .find_map(|(_, _, n)| expr.get(n).map(|s| (n, *s)));
                if let Some((param, span)) = bad {
                    diags.push(diag(
                        format!(
                            "template `{name}` writes `@{param}` into `{key}:`, which is plain text, \
                             but the argument `{param}=` is an expression — pass the literal id \
                             (dsl 0.27.0 §6)"
                        ),
                        span,
                    ));
                    continue;
                }
            }
            let value = substitute(raw, &text);
            if value.trim().is_empty() {
                continue;
            }
            set_key(beat, key, value, at);
        }
        let mut attrs = vec![component_attr(&name, at)];
        attrs.extend(args);
        beat.body.insert(
            0,
            Node::Directive(Directive {
                tag: "use".to_string(),
                attrs,
                when: None,
                span: at,
            }),
        );
        if has_body_marker(def) {
            beat.body.push(Node::Directive(Directive {
                tag: BODY_DIRECTIVE.to_string(),
                attrs: vec![component_attr(&name, at)],
                when: None,
                span: at,
            }));
        }
    }
    diags
}

/// `E-TEMPLATE` for every `::body` out of place (dsl 0.27.0 §6). It is
/// legal once, at the top level of a template component's body
/// (`template_component`), and as the closing marker
/// [`expand_beat_templates`] appends to a template use. Anywhere else — a
/// scene, a quest, an ordinary component, nested in a choice or match —
/// it places nothing.
pub fn check_body_markers(doc: &Document, template_component: bool) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let stray = |d: &Directive, diags: &mut Vec<Diagnostic>, why: &str| {
        diags.push(diag(
            format!(
                "`::body` {why} — it marks where a `<beat use=…>`'s own body goes, and belongs \
                 at the top level of a component that declares a `beat:` header (dsl 0.27.0 §6)"
            ),
            d.span,
        ));
    };
    let mut nested = Vec::new();
    let mut top_seen = false;
    for shot in &doc.shots {
        for node in &shot.body {
            match node {
                Node::Directive(d) if d.tag == BODY_DIRECTIVE => {
                    if !template_component {
                        stray(d, &mut diags, "is outside a beat template");
                    } else if top_seen {
                        stray(d, &mut diags, "appears twice in this template");
                    }
                    top_seen = true;
                }
                other => collect_nested(std::slice::from_ref(other), &mut nested),
            }
        }
    }
    for q in &doc.quests {
        collect_all(&q.body, &mut nested);
    }
    for e in &doc.entries {
        collect_all(&e.body, &mut nested);
    }
    for b in &doc.beats {
        let marker_at = b
            .template
            .as_ref()
            .filter(|t| t.expanded)
            .and_then(|_| b.body.len().checked_sub(1))
            .filter(|&i| matches!(&b.body[i], Node::Directive(d) if d.tag == BODY_DIRECTIVE));
        for (i, node) in b.body.iter().enumerate() {
            if Some(i) != marker_at {
                collect_all(std::slice::from_ref(node), &mut nested);
            }
        }
    }
    for d in nested {
        stray(
            d,
            &mut diags,
            if template_component {
                "is nested inside another block"
            } else {
                "is outside a beat template"
            },
        );
    }
    diags
}

/// Every `::body` directive in `nodes`' nested bodies (not `nodes` itself).
fn collect_nested<'a>(nodes: &'a [Node], out: &mut Vec<&'a Directive>) {
    for node in nodes {
        match node {
            Node::Branch(b) => b.choices.iter().for_each(|c| collect_all(&c.body, out)),
            Node::Hub(h) => h.choices.iter().for_each(|c| collect_all(&c.body, out)),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        lute_syntax::ast::Arm::When { body, .. }
                        | lute_syntax::ast::Arm::Otherwise { body, .. } => collect_all(body, out),
                    }
                }
            }
            Node::On(o) => collect_all(&o.body, out),
            Node::Objective(o) => collect_all(&o.body, out),
            _ => {}
        }
    }
}

/// Every `::body` directive in `nodes`, at any depth.
fn collect_all<'a>(nodes: &'a [Node], out: &mut Vec<&'a Directive>) {
    for node in nodes {
        if let Node::Directive(d) = node {
            if d.tag == BODY_DIRECTIVE {
                out.push(d);
            }
        }
    }
    collect_nested(nodes, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution_replaces_params_only() {
        let args: BTreeMap<String, String> = [
            ("hero".to_string(), "aria".to_string()),
            ("rank".to_string(), "r2".to_string()),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            substitute("holds(bondRank(@hero, @rank)) && @other", &args),
            "holds(bondRank(aria, r2)) && @other"
        );
        assert_eq!(substitute("hero.@hero", &args), "hero.aria");
        assert_eq!(substitute("@@hero", &args), "@@hero");
    }

    #[test]
    fn a_bare_after_id_is_visited() {
        assert!(bare_id("bonds.aria.ariaR1"));
        assert!(!bare_id("visited('x')"));
    }
}
