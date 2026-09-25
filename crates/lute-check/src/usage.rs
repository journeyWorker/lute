//! dsl 0.24.0 (round-3 T3-16) `check-project` advisories over the whole
//! walked project: a declared relation written but never read
//! ([`W_RELATION_UNREAD`]) and a declared `@def` no one references
//! ([`W_DEF_UNUSED`]). Content only — play scripts and tests are not reads.
//! Each is reported once per project, at its declaration: the schema file
//! that declares it (`RelVocab::origins`, dsl 0.24 T3-6), or the document
//! whose own frontmatter declares it inline.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Document, Node};
use lute_syntax::datalog::BodyLiteral;

use crate::check::FoldedEnv;

/// A declared, non-reserved relation that some `::assert`, seed fact, or
/// rule writes but that no condition, rule body, or def reads.
pub const W_RELATION_UNREAD: &str = "W-RELATION-UNREAD";
/// A declared `@def` no `@name` reference names anywhere.
pub const W_DEF_UNUSED: &str = "W-DEF-UNUSED";

/// What a declaration site is looked up for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclKind {
    Relation,
    Def,
}

/// One walked document: its path, its source text, its AST, its fold.
pub struct UsageDoc<'a> {
    pub path: &'a Path,
    pub text: &'a str,
    pub doc: &'a Document,
    pub folded: &'a FoldedEnv,
}

/// [`W_RELATION_UNREAD`] and [`W_DEF_UNUSED`] over every walked document of
/// the project (every root). `extra_sources` are other project texts a use
/// may live in — the declaring schema files
/// ([`schema_sources`] names them). A declaration neither imported from a
/// project schema file nor written in a document's own frontmatter (a plugin
/// def, say) is not the project's and is never reported. Output is sorted by
/// anchor.
pub fn check_project_usage(
    docs: &[UsageDoc<'_>],
    extra_sources: &[&str],
) -> Vec<(PathBuf, Diagnostic)> {
    let texts: Vec<&str> = docs.iter().map(|d| d.text).chain(extra_sources.iter().copied()).collect();
    let mut out = Vec::new();

    // --- relations -----------------------------------------------------------
    let mut declared: BTreeMap<&str, &lute_manifest::relations::RelationDecl> = BTreeMap::new();
    let mut written: BTreeSet<String> = BTreeSet::new();
    let mut read: BTreeSet<String> = BTreeSet::new();
    let mut def_bodies: BTreeMap<&str, &str> = BTreeMap::new();
    for d in docs {
        let vocab = &d.folded.env.rel_vocab;
        for (name, decl) in &vocab.relations {
            declared.entry(name.as_str()).or_insert(decl);
        }
        written.extend(vocab.facts.iter().map(|f| f.fact.relation.clone()));
        for r in &vocab.rules {
            written.insert(r.rule.head.relation.clone());
            for lit in &r.rule.body {
                match lit {
                    BodyLiteral::Pos(a) | BodyLiteral::Neg(a) => {
                        read.insert(a.relation.clone());
                    }
                    BodyLiteral::Guard { cel, .. } => queried_relations(cel, &mut read),
                    BodyLiteral::Cmp { .. } => {}
                }
            }
        }
        for (name, body) in &d.folded.def_bodies {
            def_bodies.entry(name.as_str()).or_insert(body.as_str());
        }
        let bodies = d
            .doc
            .shots
            .iter()
            .map(|s| &s.body)
            .chain(d.doc.quests.iter().map(|q| &q.body))
            .chain(d.doc.entries.iter().map(|e| &e.body))
            .chain(d.doc.beats.iter().map(|b| &b.body));
        for body in bodies {
            scan(body, &mut |n| {
                if let Node::Assert(a) = n {
                    written.insert(a.pattern.relation.clone());
                }
            });
        }
    }
    for text in &texts {
        queried_relations(text, &mut read);
    }
    for body in def_bodies.values() {
        queried_relations(body, &mut read);
    }
    for (name, decl) in declared {
        if decl.reserved || !written.contains(name) || read.contains(name) {
            continue;
        }
        let Some((path, span)) = declaration(docs, DeclKind::Relation, "relations", name)
        else {
            continue;
        };
        out.push((
            path,
            diag(
                W_RELATION_UNREAD,
                format!(
                    "relation `{name}` is written (asserted, seeded, or derived) but never read: \
                     no condition queries it (`holds` / `count`), no rule body uses it, and no \
                     def reads it — the facts it records change nothing; read it where it \
                     matters, or drop it (dsl 0.24.0)"
                ),
                span,
            ),
        ));
    }

    // --- defs ----------------------------------------------------------------
    let mut used: BTreeSet<String> = BTreeSet::new();
    for text in &texts {
        ref_names(text, &mut used);
    }
    for d in docs {
        for r in &d.folded.env.rel_vocab.rules {
            ref_names(&r.raw, &mut used);
        }
    }
    for (name, body) in &def_bodies {
        let mut inner = BTreeSet::new();
        ref_names(body, &mut inner);
        inner.remove(*name);
        used.extend(inner);
    }
    for name in def_bodies.keys() {
        if used.contains(*name) {
            continue;
        }
        let Some((path, span)) = declaration(docs, DeclKind::Def, "defs", name) else {
            continue;
        };
        out.push((
            path,
            diag(
                W_DEF_UNUSED,
                format!(
                    "def `{name}` is declared but no `@{name}` reference uses it anywhere in the \
                     project (content, other defs, or rule guards); use it or drop it \
                     (dsl 0.24.0)"
                ),
                span,
            ),
        ));
    }
    out.sort_by(|(pa, a), (pb, b)| (pa, a.span.byte_start).cmp(&(pb, b.span.byte_start)));
    out
}

/// Where `name` is declared: the schema file an import resolved it from
/// (`RelVocab::origins`), else the first walked document whose own
/// frontmatter `block:` (`relations` / `defs`) declares it, at the key.
fn declaration(
    docs: &[UsageDoc<'_>],
    kind: DeclKind,
    block: &str,
    name: &str,
) -> Option<(PathBuf, Span)> {
    let imported = docs.iter().find_map(|d| {
        let origins = &d.folded.env.rel_vocab.origins;
        match kind {
            DeclKind::Relation => origins.relations.get(name),
            DeclKind::Def => origins.defs.get(name),
        }
    });
    if let Some(o) = imported {
        return Some((o.file.clone(), o.span));
    }
    docs.iter().find_map(|d| {
        let map = serde_yaml::from_str::<serde_yaml::Mapping>(&d.doc.meta.raw_yaml).ok()?;
        map.get(serde_yaml::Value::String(block.to_string()))?
            .as_mapping()?
            .get(serde_yaml::Value::String(name.to_string()))?;
        Some((d.path.to_path_buf(), crate::meta::meta_key_span(&d.doc.meta, name)))
    })
}

/// Every schema file the walked documents' imports declared a relation or
/// def in — the texts [`check_project_usage`] also scans for uses.
pub fn schema_sources(foldeds: &[&FoldedEnv]) -> BTreeSet<PathBuf> {
    foldeds
        .iter()
        .flat_map(|f| {
            let o = &f.env.rel_vocab.origins;
            o.relations.values().chain(o.defs.values()).chain(o.rules.values())
        })
        .map(|o| o.file.clone())
        .collect()
}

/// Every relation `text` queries: `holds(R(`, `count(R(`, `countDistinct(R(`,
/// whitespace allowed. Textual on purpose — a use in a comment only ever
/// silences the advisory, never fakes one.
fn queried_relations(text: &str, out: &mut BTreeSet<String>) {
    for f in ["holds", "count", "countDistinct"] {
        let mut rest = text;
        while let Some(at) = rest.find(f) {
            let before = &rest[..at];
            let after = &rest[at + f.len()..];
            rest = after;
            if before.chars().next_back().is_some_and(is_ident_char) {
                continue;
            }
            let Some(inner) = after.trim_start().strip_prefix('(') else { continue };
            let inner = inner.trim_start();
            let name: String = inner.chars().take_while(|c| is_ident_char(*c)).collect();
            if !name.is_empty() && inner[name.len()..].trim_start().starts_with('(') {
                out.insert(name);
            }
        }
    }
}

/// Every `@name` in `text` (an identifier after `@`).
fn ref_names(text: &str, out: &mut BTreeSet<String>) {
    for (i, _) in text.match_indices('@') {
        let name: String = text[i + 1..].chars().take_while(|c| is_ident_char(*c)).collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Visit every node of `nodes`, nested bodies included.
fn scan<'n>(nodes: &'n [Node], f: &mut impl FnMut(&'n Node)) {
    for node in nodes {
        f(node);
        match node {
            Node::Branch(b) => b.choices.iter().for_each(|c| scan(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| scan(&c.body, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    scan(body, f);
                }
            }
            Node::On(o) => scan(&o.body, f),
            Node::Objective(o) => scan(&o.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
