use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Node;
use crate::ProjectDoc;

/// Two documents of one project declare the same `<branch id>` / `<hub id>`.
/// Each document is its own episode, so the ids are legal, but a play's or
/// test's `choose:` names a menu by its id alone: one key answers both, and
/// a list of decisions is consumed across both.
pub const W_BRANCH_ID_SHARED: &str = "W-BRANCH-ID-SHARED";

/// [`W_BRANCH_ID_SHARED`] over `docs` (pre-sorted by path): every document
/// after the first to declare an id is warned at its first menu with that
/// id, naming the first document's menu relative to `root`. A repeat inside
/// one document is that document's `E-DUP-BRANCH`, not this.
pub fn check_project_branch_ids(
    root: &Path,
    docs: &[ProjectDoc<'_>],
) -> Vec<(PathBuf, Diagnostic)> {
    fn menus<'a>(nodes: &'a [Node], out: &mut Vec<(&'a str, &'static str, Span)>) {
        for node in nodes {
            match node {
                Node::Branch(b) => {
                    out.push((b.id.as_str(), "branch", b.span));
                    for c in &b.choices {
                        menus(&c.body, out);
                    }
                }
                Node::Hub(h) => {
                    let id = h
                        .attrs
                        .iter()
                        .find(|a| a.key == "id")
                        .and_then(|a| match &a.value {
                            lute_syntax::ast::AttrValue::Str(s) => Some(s.as_str()),
                            _ => None,
                        });
                    if let Some(id) = id {
                        out.push((id, "hub", h.span));
                    }
                    for body in h.bodies() {
                        menus(body, out);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            lute_syntax::ast::Arm::When { body, .. }
                            | lute_syntax::ast::Arm::Otherwise { body, .. } => menus(body, out),
                        }
                    }
                }
                Node::On(o) => menus(&o.body, out),
                Node::Objective(o) => menus(&o.body, out),
                Node::Line(_)
                | Node::Directive(_)
                | Node::Set(_)
                | Node::Timeline(_)
                | Node::Assert(_)
                | Node::Retract(_) => {}
            }
        }
    }
    let shown = |p: &Path| {
        p.strip_prefix(root)
            .unwrap_or(p)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    // id -> the first document's menu (file, tag, span).
    let mut first: BTreeMap<&str, (&Path, &'static str, Span)> = BTreeMap::new();
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        let mut found = Vec::new();
        let bodies = doc.sections
            .iter()
            .map(|s| &s.body)
            .chain(doc.quests.iter().map(|q| &q.body))
            .chain(doc.entries.iter().map(|e| &e.body))
            .chain(doc.beats.iter().map(|b| &b.body));
        for body in bodies {
            menus(body, &mut found);
        }
        let mut here: BTreeSet<&str> = BTreeSet::new();
        for (id, tag, span) in found {
            if id.is_empty() || !here.insert(id) {
                continue;
            }
            let Some(&(other, other_tag, other_span)) = first.get(id) else {
                first.insert(id, (path, tag, span));
                continue;
            };
            out.push((
                path.to_path_buf(),
                Diagnostic {
                    code: W_BRANCH_ID_SHARED.to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "`<{tag} id=\"{id}\">` shares its id with the `<{other_tag}>` at `{}:{}` \
                         — a `choose:` in a play or test names a menu by its id alone, so \
                         `choose: {{ {id}: … }}` answers both; rename one",
                        shown(other),
                        other_span.line
                    ),
                    evidence: None,
                    span,
                    layer: Layer::Logic,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                },
            ));
        }
    }
    out
}
