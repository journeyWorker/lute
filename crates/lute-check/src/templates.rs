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

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Attr, AttrValue, BundleBeat, CelKind, CelSlot, Directive, Document, Meta, Node,
};

use crate::component_import::{ComponentDef, ComponentSet};
use lute_manifest::schema::{DefParam, OccasionDecl};

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
/// in declaration order, and the `beat:` key's span in the component file.
/// `faulty` when the header drew an `E-TEMPLATE` (a key that is not a
/// header key, a value that is not text, a `@name` no param declares): its
/// uses then derive what is sound and stay silent about the rest — the
/// fault is reported once, here.
#[derive(Clone, Debug)]
pub struct BeatTemplate {
    pub keys: Vec<TemplateKey>,
    pub span: Span,
    pub faulty: bool,
    /// The def names the component itself sees — its inline `defs:` and
    /// those of its own `uses:` (set by the component import). A condition
    /// key's `@name` that is none of these, no param and no def of the host
    /// is [`check_template_refs`]'s, reported once at the header.
    pub defs: BTreeSet<String>,
}

/// One `beat:` header key: its name, raw text, and the key's span in the
/// component file.
#[derive(Clone, Debug)]
pub struct TemplateKey {
    pub key: String,
    pub raw: String,
    pub span: Span,
}

impl TemplateKey {
    /// A value every use derives unchanged: no `@param` in it, and not a
    /// condition (a condition's `@name` may be a def of the host).
    fn fixed(&self) -> bool {
        !CEL_KEYS.contains(&self.key.as_str()) && at_refs(&self.raw).is_empty()
    }

    /// What is wrong with a [`Self::fixed`] value on its own — its shape
    /// (`crate::bundles::value_faults`) and, for `on`, the occasion
    /// vocabulary — anchored at `span`. Empty for a value that depends on
    /// the use.
    fn faults(&self, occasions: &BTreeMap<String, OccasionDecl>, span: Span) -> Vec<Diagnostic> {
        if !self.fixed() {
            return Vec::new();
        }
        let mut out = crate::bundles::value_faults(&self.key, &self.raw, span);
        if self.key == "on" && out.is_empty() {
            crate::beats::check_occasion(&self.raw, span, None, occasions, Layer::Logic, &mut out);
        }
        out
    }
}

/// dsl 0.27.0 §6: every header value the uses derive unchanged, judged once
/// in the template component — at the header key, not at each use (a use
/// drops a faulty value, see [`expand_beat_templates`]). A value with a
/// `@param` depends on the argument and is judged at each use.
pub fn check_template_header(
    template: &BeatTemplate,
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for k in &template.keys {
        for mut d in k.faults(occasions, k.span) {
            d.message = format!(
                "template header `beat.{}` (every `<beat use=…>` derives it): {}",
                k.key, d.message
            );
            out.push(d);
        }
    }
    out
}

/// dsl 0.27.0 §6: a `@name` in a condition key (`when:` / `spentBy:`) that
/// is neither a param nor a def the component sees (`defs`, params
/// included) is `E-UNDECLARED-REF` once, at the header key, with a
/// did-you-mean over both — not at every use, which derives nothing for
/// that key ([`expand_beat_templates`]).
pub fn check_template_refs(template: &BeatTemplate, defs: &BTreeSet<String>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for k in template
        .keys
        .iter()
        .filter(|k| CEL_KEYS.contains(&k.key.as_str()))
    {
        let mut seen = BTreeSet::new();
        for (_, _, name) in at_refs(&k.raw) {
            if defs.contains(name) || !seen.insert(name) {
                continue;
            }
            let hint = lute_manifest::suggest::nearest(name, defs.iter().map(String::as_str), 2)
                .map_or_else(String::new, |n| format!(" — did you mean `@{n}`?"));
            out.push(Diagnostic {
                code: "E-UNDECLARED-REF".to_string(),
                ..diag(
                    format!(
                        "template header `beat.{}` (every `<beat use=…>` derives it): `@{name}` \
                         is not a declared param or def{hint} (dsl §8.1)",
                        k.key
                    ),
                    k.span,
                )
            });
        }
    }
    out
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

/// A condition's text with every bound `@param` replaced, conjunct by
/// top-level `&&` conjunct: a conjunct that held a `@param` and comes out
/// empty or `true` (an optional extra condition left at its default) is
/// dropped, so `!holds(defeated(@who)) && (@only)` with `only` empty reads
/// `!holds(defeated(r3))`, never `… && (true)`. A condition whose every
/// conjunct drops is empty — the key is then omitted.
fn substitute_condition(raw: &str, args: &BTreeMap<String, String>) -> String {
    let parts = top_level_and(raw);
    let vacuous = |part: &str| {
        !at_refs(part).is_empty() && matches!(unparen(&substitute(part, args)), "" | "true")
    };
    if !parts.iter().any(|p| vacuous(p)) {
        return substitute(raw, args);
    }
    let kept: Vec<String> = parts
        .into_iter()
        .filter(|p| !vacuous(p))
        .map(|p| substitute(p, args).trim().to_string())
        .collect();
    kept.join(" && ")
}

/// `s` split at its top-level `op` (`&&` / `||`, outside parentheses,
/// brackets, braces and quotes).
fn top_level_split<'s>(s: &'s str, op: &[u8; 2]) -> Vec<&'s str> {
    let b = s.as_bytes();
    let (mut depth, mut quote, mut start, mut i) = (0i32, None::<u8>, 0, 0);
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(_) if c == b'\\' => i += 1,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                _ if depth == 0 && b[i..].starts_with(op) => {
                    out.push(&s[start..i]);
                    start = i + 2;
                    i += 1;
                }
                _ => {}
            },
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

/// The top-level `&&` conjuncts of `s` — `s` whole when a top-level `||`
/// or `?` binds looser than `&&` and splitting would change its meaning.
pub fn top_level_and(s: &str) -> Vec<&str> {
    if top_level_split(s, b"||").len() > 1 || s.contains('?') {
        return vec![s];
    }
    top_level_split(s, b"&&")
}

/// `s` trimmed, without the parentheses that wrap all of it.
pub fn unparen(s: &str) -> &str {
    let mut s = s.trim();
    while let Some(inner) = s.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        if !balanced(inner) {
            break;
        }
        s = inner.trim();
    }
    s
}

/// Every `(` in `s` closes before it ends, and no `)` closes early.
fn balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.bytes() {
        match c {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
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
        // Still a template: its uses are reported here, not each at its use.
        let keys = Vec::new();
        return (
            Some(BeatTemplate {
                keys,
                span,
                faulty: true,
                defs: BTreeSet::new(),
            }),
            diags,
        );
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
            let unknown: Vec<&str> = at_refs(&text)
                .into_iter()
                .map(|(_, _, name)| name)
                .filter(|name| !params.iter().any(|p| p.name == *name))
                .collect();
            for name in &unknown {
                let hint = lute_manifest::suggest::nearest(
                    name,
                    params.iter().map(|p| p.name.as_str()),
                    2,
                )
                .map(|n| format!("did you mean `@{n}`? Or declare"))
                .unwrap_or_else(|| "declare".to_string());
                diags.push(diag(
                    format!(
                        "`beat.{key}` names `@{name}`, but the component declares no param \
                         `{name}` — {hint} it under `params:` (dsl 0.27.0 §6)"
                    ),
                    at,
                ));
            }
            // A value its uses cannot substitute derives nothing.
            if !unknown.is_empty() {
                continue;
            }
        }
        keys.push(TemplateKey {
            key: key.to_string(),
            raw: text,
            span: at,
        });
    }
    let faulty = !diags.is_empty();
    let template = BeatTemplate {
        keys,
        span,
        faulty,
        defs: BTreeSet::new(),
    };
    (Some(template), diags)
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
/// expansion. `components` is the document's resolved `components:`;
/// `occasions` the resolved vocabulary a fixed `on:` is judged against;
/// `host_defs` the def names the document resolves `@name` against.
///
/// A header value no use can change ([`TemplateKey::fixed`]) that is faulty
/// is [`check_template_header`]'s, reported once in the component: the use
/// derives nothing for that key. So is a condition key naming a `@name`
/// that is no param, no def the component sees and no def of the host
/// ([`check_template_refs`]). Such a use — and one of a faulty header, an
/// unknown component or one with no `beat:` — is marked
/// `TemplateUse::failed`, so the checks say nothing about what the template
/// would have supplied.
pub fn expand_beat_templates(
    doc: &mut Document,
    components: &ComponentSet,
    occasions: &BTreeMap<String, OccasionDecl>,
    host_defs: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for beat in &mut doc.beats {
        let Some(tu) = beat.template.as_mut().filter(|t| !t.expanded) else {
            continue;
        };
        tu.expanded = true;
        tu.failed = true;
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
        let mut failed = template.faulty;
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
        for k in &template.keys {
            let (key, raw) = (&k.key, &k.raw);
            if written(beat, key) {
                continue;
            }
            if !k.faults(occasions, at).is_empty() {
                failed = true;
                continue;
            }
            if CEL_KEYS.contains(&key.as_str()) {
                let unknown = at_refs(raw).into_iter().any(|(_, _, n)| {
                    !def.params.iter().any(|(p, _)| p == n)
                        && !template.defs.contains(n)
                        && !host_defs.contains(n)
                });
                if unknown {
                    failed = true;
                    continue;
                }
            } else {
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
            let value = if CEL_KEYS.contains(&key.as_str()) {
                substitute_condition(raw, &text)
            } else {
                substitute(raw, &text)
            };
            if value.trim().is_empty() {
                continue;
            }
            set_key(beat, key, value, at);
        }
        if let Some(tu) = beat.template.as_mut() {
            tu.failed = failed;
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
