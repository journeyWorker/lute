//! The `mono` speaker rule (dsl 0.37.0 §3.4, D8): an interior-monologue line
//! is spoken only by the document's effective point of view — frontmatter
//! `pov:` over the project's `defaults.pov` — or by a speaker its effective
//! `monoSpeakers:` lists. A component's lines are judged at every `::use`
//! site, in the calling document's context.

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, Directive, Document, Line, Node};

use crate::component_import::ComponentSet;
use crate::meta::TypedMeta;

/// `E-MONO-POV`: a `mono` line whose speaker is neither the effective POV
/// nor in the effective `monoSpeakers` list.
pub const E_MONO_POV: &str = "E-MONO-POV";

/// `E-MONO-NO-POV`: a `mono` line whose document resolves no POV at all and
/// whose speaker is not in the effective `monoSpeakers` list.
pub const E_MONO_NO_POV: &str = "E-MONO-NO-POV";

/// Who may speak `mono` in one document: its effective POV and allow-list.
struct MonoContext<'a> {
    pov: Option<&'a str>,
    allowed: &'a [String],
}

impl MonoContext<'_> {
    /// The diagnostic code and message for a `mono` line spoken by
    /// `speaker`, or `None` when the speaker may think aloud here.
    fn verdict(&self, speaker: &str) -> Option<(&'static str, String)> {
        if self.pov == Some(speaker) || self.allowed.iter().any(|s| s == speaker) {
            return None;
        }
        let listed = if self.allowed.is_empty() {
            String::new()
        } else {
            format!(
                " (it lists {})",
                self.allowed
                    .iter()
                    .map(|s| format!("`{s}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        Some(match self.pov {
            Some(pov) => (
                E_MONO_POV,
                format!(
                    "`@{speaker}{{mono}}` is an interior monologue, but `{speaker}` is not this \
                     document's point of view (`{pov}`) and not in `monoSpeakers:`{listed} — \
                     only the POV character and the speakers `monoSpeakers:` lists may speak \
                     `mono`; add `{speaker}` to `monoSpeakers:`, or write the line as dialogue, \
                     `{{os}}` or `{{vo}}` (dsl 0.37.0 §3.4)"
                ),
            ),
            None => (
                E_MONO_NO_POV,
                format!(
                    "`@{speaker}{{mono}}` is an interior monologue, but no point of view \
                     resolves for this document — it writes no `pov:` and the project's \
                     `defaults:` gives none — and `{speaker}` is not in `monoSpeakers:`{listed}; \
                     declare the POV (`pov: <speaker>`, or `defaults: {{ pov: <speaker> }}` in \
                     lute.project.yaml), or list `{speaker}` in `monoSpeakers:` (dsl 0.37.0 \
                     §3.4)"
                ),
            ),
        })
    }
}

/// The `mono` attribute's span when `line` carries the bare `mono` flag.
/// Narration never does (`E-DELIVERY-NARRATOR` owns that line).
fn mono_flag(line: &Line) -> Option<Span> {
    if line.speaker == "narrator" {
        return None;
    }
    line.attrs
        .iter()
        .find(|a| a.key == "mono" && matches!(a.value, AttrValue::BoolTrue))
        .map(|a| a.span)
}

/// Visit every content line and every `::use` of `nodes`, at any depth.
fn walk<'a>(nodes: &'a [Node], line: &mut impl FnMut(&'a Line), uses: &mut impl FnMut(&'a Directive)) {
    for node in nodes {
        match node {
            Node::Line(l) => line(l),
            Node::Directive(d) if d.tag == "use" => uses(d),
            Node::Branch(b) => b.choices.iter().for_each(|c| walk(&c.body, line, uses)),
            Node::Hub(h) => h.bodies().for_each(|b| walk(b, line, uses)),
            Node::On(o) => walk(&o.body, line, uses),
            Node::Objective(o) => walk(&o.body, line, uses),
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    walk(body, line, uses);
                }
            }
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|t| &t.clips) {
                    if let lute_syntax::ast::ClipNode::Directive(d) = &clip.node {
                        if d.tag == "use" {
                            uses(d);
                        }
                    }
                }
            }
            Node::Directive(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// Every body of `doc` that holds lines: sections, quests, lore entries and
/// bundle beats.
fn bodies(doc: &Document) -> impl Iterator<Item = &Vec<Node>> {
    doc.sections
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
}

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// One `mono` line a component body speaks through a `::use`: the bound
/// speaker, its span in the component file, and the component it is in.
struct ComponentMono {
    speaker: String,
    span: Span,
    component: String,
    src: std::path::PathBuf,
}

/// Every `mono` line the component `d` names speaks — its own and, through
/// nested `::use`s, its components' — with each `speaker` param bound to
/// the member this use passes. A speaker whose argument is no literal id
/// stays `@p` and is left out (the `::use` check reports it).
fn component_mono_lines(
    d: &Directive,
    components: &ComponentSet,
    stack: &mut Vec<String>,
    out: &mut Vec<ComponentMono>,
) {
    let Some((name, def)) = d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
        ("component", AttrValue::Str(s)) => components.table.get(s).map(|def| (s, def)),
        _ => None,
    }) else {
        return;
    };
    if stack.contains(name) {
        return;
    }
    let args = crate::component_effects::use_args_for(d, def);
    stack.push(name.clone());
    // Bound while walking, so a `@@p` line or a nested `::use` inside any
    // container `walk` visits (branch, hub, `<on>`, objective, timeline,
    // match) takes this use's arguments.
    let mut nested: Vec<Directive> = Vec::new();
    for section in &def.body.sections {
        walk(
            &section.body,
            &mut |l| {
                let Some(span) = mono_flag(l) else {
                    return;
                };
                let speaker = match l.speaker.strip_prefix('@') {
                    None => l.speaker.clone(),
                    Some(p) => match args.get(p) {
                        Some(AttrValue::Str(id)) => id.clone(),
                        // No literal id: the `::use` check reports it.
                        _ => return,
                    },
                };
                out.push(ComponentMono {
                    speaker,
                    span,
                    component: name.clone(),
                    src: def.src.clone(),
                });
            },
            &mut |inner| {
                let mut inner = inner.clone();
                crate::component_effects::bind_attrs(&mut inner.attrs, &args, &def.params);
                nested.push(inner);
            },
        );
    }
    for inner in &nested {
        component_mono_lines(inner, components, stack, out);
    }
    stack.pop();
}

/// The `mono` rule over `doc` (dsl 0.37.0 §3.4): each of its own `mono`
/// lines, then each component line every `::use` site brings in, judged
/// with `meta`'s effective `pov:` / `monoSpeakers:` (frontmatter over the
/// project defaults, already merged). A component document's own lines are
/// not judged — it has no caller context; each caller judges them.
///
/// A component line's fault is reported per use site: anchored at the
/// `::use` (the one position this document's diagnostic surface can hold),
/// naming the component and its line, with the component line itself as the
/// `related` location.
pub(super) fn check_mono(doc: &Document, meta: &TypedMeta, components: &ComponentSet) -> Vec<Diagnostic> {
    if meta.component.is_some() {
        return Vec::new();
    }
    let ctx = MonoContext {
        pov: meta.pov.as_deref(),
        allowed: &meta.mono_speakers,
    };
    let mut out = Vec::new();
    let mut sites: Vec<&Directive> = Vec::new();
    for body in bodies(doc) {
        walk(
            body,
            &mut |l| {
                if let Some(span) = mono_flag(l) {
                    if let Some((code, message)) = ctx.verdict(&l.speaker) {
                        out.push(diag(code, message, span));
                    }
                }
            },
            &mut |d| sites.push(d),
        );
    }
    for site in sites {
        let mut lines = Vec::new();
        component_mono_lines(site, components, &mut Vec::new(), &mut lines);
        for hit in &lines {
            let Some((code, message)) = ctx.verdict(&hit.speaker) else {
                continue;
            };
            let path = super::component_body::project_relative_display(&hit.src);
            let mut d = diag(
                code,
                format!(
                    "component `{}` ({path}:{}:{}), used here: {message}",
                    hit.component, hit.span.line, hit.span.column
                ),
                site.span,
            );
            d.related.push(RelatedDiagnostic {
                file: hit.src.display().to_string(),
                diagnostic: diag(code, message, hit.span),
            });
            out.push(d);
        }
    }
    out
}
