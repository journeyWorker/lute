//! `lute scenario <dir> reach --endings[=<occasion>]` (T3-20): one row per
//! ending with every static verdict the toolchain already reaches about it.
//!
//! An ending is, with `=<occasion>`, every beat answering that occasion;
//! bare, every beat — scene beat, bundle beat, entry — whose content can run
//! `::end` (its own body, or a component it `::use`s). Each row carries:
//!
//! - the `after:` verdict — the SAME reach verdict `lute scenario reach
//!   <node>` prints ([`crate::reach_verdict_text`]); an entry, never a graph
//!   node, is an entry point;
//! - the `when` verdict — the `check-project` verdicts about the beat, the
//!   ones `lute beats` shows (`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`:
//!   it never holds; `W-BEAT-SHADOWED`: it never wins);
//! - for a `when` nothing refutes, the state paths and fact atoms it reads
//!   and who produces them — the documents writing the path (their own
//!   `::set` / `<choice into>`, a `::use`d component's, a plugin directive's
//!   declared `effects.writes`, a reward kind's `credits:`), and for a fact
//!   `lute scenario knowledge`'s producer line. `nothing writes it` is said
//!   only for a content-writable `run.*` / `user.*` path no write of any of
//!   those shapes can reach. No value-range analysis: whether a written path
//!   can reach the value the `when` needs is the checker's
//!   (`E-BEAT-UNREACHABLE`) or a play's to prove.
//!
//! Nothing here proves an ending reachable in play; a play presenting it is
//! the proof, and `lute test --coverage` lists the beats no play presents.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cel_parser::ast::operators as op;
use lute_check::connectivity::{NodeId, Reachability};
use lute_check::{ProjectBeat, ProjectBeatKind};
use lute_core_span::Diagnostic;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::PathSegment;
use lute_syntax::ast::{Arm, AttrValue, ClipNode, Directive, Document, Node, Reward};
use serde_json::{json, Value as Json};

use crate::{ByRoot, DocGroup};

/// The verdict words of one row, and the summary bucket they fall in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bucket {
    Reachable,
    Unreachable,
    Unknown,
}

impl Bucket {
    fn as_str(self) -> &'static str {
        match self {
            Bucket::Reachable => "reachable",
            Bucket::Unreachable => "unreachable",
            Bucket::Unknown => "unknown",
        }
    }
}

/// One requirement of a `when`: a state path or a fact atom, and who
/// produces it.
struct Need {
    /// `run.aff.ren`, or `holds(cleared(ren, good))` / `!holds(…)`.
    what: String,
    producers: Vec<String>,
    /// The sound "nothing produces it" case.
    none: bool,
    /// The words for `producers` when the list alone does not say it.
    note: Option<String>,
}

/// One ending row.
struct Row {
    id: String,
    kind: &'static str,
    file: String,
    after_token: &'static str,
    after_text: String,
    when: Option<String>,
    /// `never-holds` / `never-wins` / `unrefuted` / `absent`.
    when_token: &'static str,
    when_text: String,
    needs: Vec<Need>,
    bucket: Bucket,
}

/// One root's rows.
struct RootRows {
    root: PathBuf,
    rows: Vec<Row>,
}

/// `Err(exit)` on an unknown `--endings=<occasion>` (reported).
fn rows(
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    occasion: Option<&str>,
) -> Result<Vec<RootRows>, ExitCode> {
    let (reconciled, project_diags, _, _) =
        crate::reconcile_collected(file_results.to_vec(), by_root, false);
    let verdict_codes = [
        lute_check::E_BEAT_UNREACHABLE,
        lute_check::E_ENTRY_UNREACHABLE,
        lute_check::W_BEAT_SHADOWED,
    ];
    let mut verdicts: Vec<(&PathBuf, &Diagnostic)> = reconciled
        .iter()
        .flat_map(|(p, r)| r.diagnostics.iter().map(move |d| (p, d)))
        .chain(project_diags.iter().map(|(p, d)| (p, d)))
        .collect();
    verdicts.retain(|(_, d)| verdict_codes.contains(&d.code.as_str()));

    let mut out = Vec::new();
    let mut known: BTreeSet<String> = BTreeSet::new();
    for (root, group) in by_root {
        let scenario = crate::assemble_root_scenario(group, file_results);
        let foldeds: Vec<&lute_check::FoldedEnv> = group.iter().map(|(_, _, f)| f).collect();
        let beats = lute_check::beats::in_selection_order(lute_check::project_beats(
            &scenario.docs,
            &foldeds,
        ));
        known.extend(beats.iter().map(|b| b.on.to_string()));
        let producers = Producers::of(root, group);
        let mut rows = Vec::new();
        for b in &beats {
            let doc = scenario
                .docs
                .iter()
                .find(|(p, _)| p == b.path)
                .map(|(_, d)| d);
            let is_ending = match occasion {
                Some(o) => b.on == o,
                None => doc.is_some_and(|d| {
                    beat_body(d, b).is_some_and(|body| producers.can_end(body, 0))
                }),
            };
            if !is_ending {
                continue;
            }
            let about: Vec<&Diagnostic> = verdicts
                .iter()
                .filter(|(p, d)| crate::beats_cmd::names_beat(p, d, b))
                .map(|(_, d)| *d)
                .collect();
            rows.push(row(root, group, &scenario, &producers, b, &about));
        }
        out.push(RootRows {
            root: root.clone(),
            rows,
        });
    }
    if let Some(o) = occasion.filter(|o| !known.contains(*o)) {
        let known: Vec<&str> = known.iter().map(String::as_str).collect();
        eprintln!(
            "lute scenario reach: `--endings={o}` names no occasion a beat of this project \
             answers (answered: {})",
            known.join(", ")
        );
        return Err(ExitCode::from(2));
    }
    Ok(out)
}

/// The body a beat presents.
fn beat_body<'d>(doc: &'d Document, b: &ProjectBeat<'_>) -> Option<Vec<&'d [Node]>> {
    match b.kind {
        ProjectBeatKind::Scene => Some(doc.shots.iter().map(|s| s.body.as_slice()).collect()),
        ProjectBeatKind::Entry => doc
            .entries
            .iter()
            .find(|e| e.id == b.id)
            .map(|e| vec![e.body.as_slice()]),
        ProjectBeatKind::Bundle => {
            let bundle = lute_check::connectivity::bundle_id(doc);
            doc.beats
                .iter()
                .find(|x| match &bundle {
                    Some(d) => lute_check::bundle_beat_key(d, &x.id) == b.id,
                    None => x.id == b.id,
                })
                .map(|x| vec![x.body.as_slice()])
        }
    }
}

fn row(
    root: &Path,
    group: &DocGroup,
    scenario: &crate::RootScenario,
    producers: &Producers<'_>,
    b: &ProjectBeat<'_>,
    about: &[&Diagnostic],
) -> Row {
    let node = match b.kind {
        ProjectBeatKind::Scene => NodeId::Scene(b.id.clone()),
        ProjectBeatKind::Bundle => NodeId::Beat(b.id.clone()),
        ProjectBeatKind::Entry => NodeId::Entry(b.id.clone()),
    };
    let (after_token, after_text) =
        if matches!(b.kind, ProjectBeatKind::Entry) && !scenario.graph.nodes.contains_key(&node) {
            (
                "reachable",
                "an entry, not a prerequisite-graph node — no `after:` gates it".to_string(),
            )
        } else if crate::node_cycle_degraded(scenario, &node) {
            ("unknown", crate::reach_verdict_text(scenario, &node))
        } else {
            let token = match scenario.reach.get(&node) {
                Some(Reachability::Reachable) => "reachable",
                Some(Reachability::Unreachable) => "unreachable",
                _ => "unknown",
            };
            (token, crate::reach_verdict_text(scenario, &node))
        };
    let when_authored = b.when_slot.map(|s| one_line(&s.raw));
    let unreachable = about.iter().find(|d| {
        d.code == lute_check::E_BEAT_UNREACHABLE || d.code == lute_check::E_ENTRY_UNREACHABLE
    });
    let shadowed = about.iter().find(|d| d.code == lute_check::W_BEAT_SHADOWED);
    let (when_token, when_text) = if let Some(d) = unreachable {
        (
            "never-holds",
            format!("never holds ({}: {})", d.code, d.text()),
        )
    } else if let Some(d) = shadowed {
        (
            "never-wins",
            format!("never wins ({}: {})", d.code, d.text()),
        )
    } else if b.when.is_none() {
        ("absent", "no `when` — always holds".to_string())
    } else {
        (
            "unrefuted",
            "check-project does not refute it; it needs:".to_string(),
        )
    };
    let needs = match (&b.when, when_token) {
        (Some(expanded), "unrefuted") => needs(root, group, producers, expanded),
        _ => Vec::new(),
    };
    let bucket = if after_token == "unreachable" || unreachable.is_some() || shadowed.is_some() {
        Bucket::Unreachable
    } else if after_token == "unknown" {
        Bucket::Unknown
    } else {
        Bucket::Reachable
    };
    Row {
        id: b.id.clone(),
        kind: match b.kind {
            ProjectBeatKind::Scene => "scene",
            ProjectBeatKind::Entry => "entry",
            ProjectBeatKind::Bundle => "beat",
        },
        file: b
            .path
            .strip_prefix(root)
            .unwrap_or(b.path)
            .display()
            .to_string(),
        after_token,
        after_text,
        when: when_authored,
        when_token,
        when_text,
        needs,
        bucket,
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The state paths and fact atoms `expanded` reads, with their producers.
fn needs(root: &Path, group: &DocGroup, producers: &Producers<'_>, expanded: &str) -> Vec<Need> {
    let mut out: Vec<Need> = read_paths(expanded)
        .into_iter()
        .map(|path| producers.of_path(&path))
        .collect();
    for (atom, negated, line) in
        crate::knowledge::condition_atoms(root, group, expanded, &producers.asserts)
    {
        let none = line.starts_with("NO PRODUCER");
        out.push(Need {
            what: if negated {
                format!("!holds({atom})")
            } else {
                format!("holds({atom})")
            },
            producers: Vec::new(),
            none: none && !negated,
            note: Some(if negated {
                format!("holds until something asserts it — {line}")
            } else {
                line
            }),
        });
    }
    out
}

/// Every dotted state path a condition reads (`run.aff.ren`, `quest.q.state`;
/// a computed index reads `run.aff.*`).
fn read_paths(expanded: &str) -> BTreeSet<String> {
    use cel_parser::ast::Expr;
    use cel_parser::reference::Val;
    fn path_of(e: &Expr) -> Option<String> {
        match e {
            Expr::Ident(s) => Some(s.clone()),
            Expr::Select(s) => Some(format!("{}.{}", path_of(&s.operand.expr)?, s.field)),
            Expr::Call(c)
                if c.target.is_none() && c.func_name == op::INDEX && c.args.len() == 2 =>
            {
                let base = path_of(&c.args[0].expr)?;
                Some(match &c.args[1].expr {
                    Expr::Literal(Val::String(k)) => format!("{base}.{k}"),
                    _ => format!("{base}.*"),
                })
            }
            _ => None,
        }
    }
    fn walk(e: &Expr, out: &mut BTreeSet<String>) {
        if let Some(p) = path_of(e) {
            if p.contains('.') {
                out.insert(p);
                if let Expr::Call(c) = e {
                    // A computed index reads its key too.
                    walk(&c.args[1].expr, out);
                }
                return;
            }
        }
        match e {
            Expr::Call(c) => {
                if let Some(t) = &c.target {
                    walk(&t.expr, out);
                }
                for a in &c.args {
                    walk(&a.expr, out);
                }
            }
            Expr::List(l) => l.elements.iter().for_each(|x| walk(&x.expr, out)),
            Expr::Select(s) => walk(&s.operand.expr, out),
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    let mut arena = lute_cel::CelArena::default();
    if let Some(root) =
        lute_cel::parse_slot_marked_refs(&mut arena, expanded).and_then(|h| arena.get(h))
    {
        walk(&root.expr, &mut out);
    }
    // Only state reads; `holds(…)` argument idents never contain a dot.
    out.retain(|p| {
        matches!(
            p.split('.').next(),
            Some("run" | "user" | "prev" | "quest" | "entry" | "occasion" | "app" | "clock")
        )
    });
    out
}

/// A write path with `[…]` indexes as segments, `@param`s bound from the
/// `::use`, and anything computed as `*`.
fn normalize(raw: &str, params: &BTreeMap<String, String>) -> String {
    let mut segs: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = raw.chars().peekable();
    let resolve = |seg: &str| -> String {
        let seg = seg.trim();
        let unquoted = seg
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .or_else(|| seg.strip_prefix('"').and_then(|s| s.strip_suffix('"')));
        if let Some(lit) = unquoted {
            return lit.to_string();
        }
        if let Some(p) = seg.strip_prefix('@') {
            return params.get(p).cloned().unwrap_or_else(|| "*".to_string());
        }
        if !seg.is_empty()
            && seg
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return seg.to_string();
        }
        "*".to_string()
    };
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !cur.is_empty() {
                    segs.push(resolve(&cur));
                    cur.clear();
                }
            }
            '[' => {
                if !cur.is_empty() {
                    segs.push(resolve(&cur));
                    cur.clear();
                }
                let mut inner = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    inner.push(c);
                }
                // A dotted index (`occasion.target`) is computed.
                segs.push(if inner.contains('.') {
                    "*".to_string()
                } else {
                    resolve(&inner)
                });
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        segs.push(resolve(&cur));
    }
    segs.join(".")
}

/// Whether write path `w` can write read path `r`: equal (or `*`) on every
/// segment both have — writing a whole record writes its fields, and a
/// field write changes the record.
fn overlaps(w: &str, r: &str) -> bool {
    w.split('.')
        .zip(r.split('.'))
        .all(|(a, b)| a == b || a == "*" || b == "*")
}

/// One root's writers of state and asserters of facts, as `(label, path)`.
struct Producers<'g> {
    writes: Vec<(String, String)>,
    /// `(label, relation)` asserting sites the knowledge walk does not see.
    asserts: Vec<(String, String)>,
    /// component name -> its document.
    components: BTreeMap<String, &'g Document>,
    snapshot: CapabilitySnapshot,
    group: &'g DocGroup,
}

impl<'g> Producers<'g> {
    fn of(root: &Path, group: &'g DocGroup) -> Self {
        let project = lute_manifest::project::load_project(root).ok().flatten();
        let snapshot = lute_manifest::project::resolve_document_snapshot(
            project.as_ref(),
            None,
            &BTreeMap::new(),
        )
        .0;
        let components = group
            .iter()
            .filter_map(|(_, d, f)| f.typed.component.clone().map(|n| (n, d)))
            .collect();
        let mut p = Producers {
            writes: Vec::new(),
            asserts: Vec::new(),
            components,
            snapshot,
            group,
        };
        for (_, doc, folded) in group {
            if folded.typed.component.is_some() {
                continue;
            }
            if let Some(key) = lute_check::connectivity::scene_key(doc) {
                let label = format!("scene `{key}`");
                for shot in &doc.shots {
                    p.walk(&shot.body, &label, &BTreeMap::new(), 0);
                }
            }
            for e in &doc.entries {
                p.walk(&e.body, &format!("entry `{}`", e.id), &BTreeMap::new(), 0);
            }
            let bundle = lute_check::connectivity::bundle_id(doc);
            for b in &doc.beats {
                let id = match &bundle {
                    Some(d) => lute_check::bundle_beat_key(d, &b.id),
                    None => b.id.clone(),
                };
                p.walk(&b.body, &format!("beat `{id}`"), &BTreeMap::new(), 0);
            }
            for q in &doc.quests {
                let label = format!("quest `{}`", q.id);
                p.rewards(&q.rewards, &label);
                p.walk(&q.body, &label, &BTreeMap::new(), 0);
            }
        }
        p
    }

    fn rewards(&mut self, rewards: &[Reward], label: &str) {
        for r in rewards {
            if let Some(path) = self
                .snapshot
                .reward_kinds
                .get(&r.kind)
                .and_then(|k| k.credits.clone())
            {
                self.writes
                    .push((format!("{label} (reward `{}`)", r.kind), path));
            }
        }
    }

    /// Record every write / assert `nodes` makes, labelled `label`; a
    /// `::use` walks its component with the call's arguments bound.
    fn walk(&mut self, nodes: &[Node], label: &str, params: &BTreeMap<String, String>, depth: u8) {
        for node in nodes {
            match node {
                Node::Set(s) => self
                    .writes
                    .push((label.to_string(), normalize(&s.path, params))),
                Node::Directive(d) => self.directive(d, label, params, depth),
                Node::Branch(b) => {
                    for c in &b.choices {
                        self.choice(&c.attrs, &c.body, label, params, depth);
                    }
                }
                Node::Hub(h) => {
                    for c in &h.choices {
                        self.choice(&c.attrs, &c.body, label, params, depth);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                        self.walk(body, label, params, depth);
                    }
                }
                Node::On(o) => self.walk(&o.body, label, params, depth),
                Node::Objective(o) => {
                    self.rewards(&o.rewards, label);
                    self.walk(&o.body, label, params, depth);
                }
                Node::Timeline(t) => {
                    for clip in t.tracks.iter().flat_map(|t| &t.clips) {
                        match &clip.node {
                            ClipNode::Set(s) => self
                                .writes
                                .push((label.to_string(), normalize(&s.path, params))),
                            ClipNode::Directive(d) => self.directive(d, label, params, depth),
                        }
                    }
                }
                // Asserts in a document's own body are the knowledge walk's.
                Node::Assert(a) if depth > 0 => {
                    self.asserts
                        .push((label.to_string(), a.pattern.relation.clone()));
                }
                Node::Line(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }

    fn choice(
        &mut self,
        attrs: &[lute_syntax::ast::Attr],
        body: &[Node],
        label: &str,
        params: &BTreeMap<String, String>,
        depth: u8,
    ) {
        if let Some(AttrValue::Str(into)) = attrs.iter().find(|a| a.key == "into").map(|a| &a.value)
        {
            self.writes
                .push((format!("{label} (choice into)"), normalize(into, params)));
        }
        self.walk(body, label, params, depth);
    }

    fn directive(
        &mut self,
        d: &Directive,
        label: &str,
        params: &BTreeMap<String, String>,
        depth: u8,
    ) {
        let args: BTreeMap<String, String> = d
            .attrs
            .iter()
            .map(|a| {
                let v = match &a.value {
                    AttrValue::Str(s) => match s.strip_prefix('@') {
                        Some(p) => params.get(p).cloned().unwrap_or_else(|| "*".to_string()),
                        None => s.clone(),
                    },
                    AttrValue::BoolTrue => "true".to_string(),
                    AttrValue::Ref(r) => match r.raw.trim().strip_prefix('@') {
                        Some(p) => params.get(p).cloned().unwrap_or_else(|| "*".to_string()),
                        None => "*".to_string(),
                    },
                };
                (a.key.clone(), v)
            })
            .collect();
        if d.tag == "use" {
            let Some(name) = args.get("component") else {
                return;
            };
            let Some(doc) = self.components.get(name).copied() else {
                return;
            };
            if depth > 8 {
                return;
            }
            let label = if depth == 0 {
                format!("{label} via component `{name}`")
            } else {
                label.to_string()
            };
            for shot in &doc.shots {
                self.walk(&shot.body, &label, &args, depth + 1);
            }
            return;
        }
        let Some(effects) = self
            .snapshot
            .directive(&d.tag)
            .and_then(|decl| decl.effects.as_ref())
        else {
            return;
        };
        let label = format!("{label} via `::{}`", d.tag);
        for w in &effects.writes {
            let mut segs = vec![w.scope.clone()];
            segs.extend(w.path.iter().map(|s| {
                match s {
                    PathSegment::Literal(l) => l.clone(),
                    PathSegment::FromAttr { from_attr } => args
                        .get(&from_attr.name)
                        .cloned()
                        .unwrap_or_else(|| "*".to_string()),
                }
            }));
            self.writes.push((label.clone(), segs.join(".")));
        }
        for a in &effects.asserts {
            self.asserts.push((label.clone(), a.relation.clone()));
        }
    }

    /// Whether `bodies` (or a component they `::use`) hold an `::end`.
    fn can_end(&self, bodies: Vec<&[Node]>, depth: u8) -> bool {
        bodies.into_iter().any(|b| self.nodes_end(b, depth))
    }

    fn nodes_end(&self, nodes: &[Node], depth: u8) -> bool {
        let directive = |d: &Directive| {
            if d.tag == lute_manifest::core::END_DIRECTIVE {
                return true;
            }
            d.tag == "use"
                && depth < 8
                && d.attrs.iter().any(|a| {
                    a.key == "component"
                        && matches!(&a.value, AttrValue::Str(n) if self
                            .components
                            .get(n)
                            .is_some_and(|doc| doc.shots.iter().any(|s| self.nodes_end(&s.body, depth + 1))))
                })
        };
        nodes.iter().any(|node| match node {
            Node::Directive(d) => directive(d),
            Node::Branch(b) => b.choices.iter().any(|c| self.nodes_end(&c.body, depth)),
            Node::Hub(h) => h.choices.iter().any(|c| self.nodes_end(&c.body, depth)),
            Node::On(o) => self.nodes_end(&o.body, depth),
            Node::Objective(o) => self.nodes_end(&o.body, depth),
            Node::Match(m) => m.arms.iter().any(|arm| {
                let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                self.nodes_end(body, depth)
            }),
            Node::Timeline(t) => t.tracks.iter().any(|track| {
                track.clips.iter().any(|clip| match &clip.node {
                    ClipNode::Directive(d) => directive(d),
                    ClipNode::Set(_) => false,
                })
            }),
            Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => false,
        })
    }

    /// The declaration a path falls under (its longest declared prefix).
    fn decl(&self, path: &str) -> Option<&lute_check::meta::StateDecl> {
        let mut p = path;
        loop {
            if let Some(d) = self
                .group
                .iter()
                .find_map(|(_, _, f)| f.env.state.decls.get(p))
            {
                return Some(d);
            }
            p = &p[..p.rfind('.')?];
        }
    }

    /// Who writes `path`.
    fn of_path(&self, path: &str) -> Need {
        let mut segs = path.split('.');
        let head = segs.next().unwrap_or_default();
        let need = |producers: Vec<String>, none: bool, note: Option<String>| Need {
            what: path.to_string(),
            producers,
            none,
            note,
        };
        match head {
            "quest" => {
                let id = segs.next().unwrap_or_default();
                return need(Vec::new(), false, Some(format!("quest `{id}`'s lifecycle")));
            }
            "entry" => {
                let id = segs.next().unwrap_or_default();
                return need(Vec::new(), false, Some(format!("presenting entry `{id}`")));
            }
            "occasion" => {
                return need(Vec::new(), false, Some("the occasion's raise".to_string()));
            }
            "app" | "clock" => return need(Vec::new(), false, Some("the engine".to_string())),
            _ => {}
        }
        let (target, earlier) = match lute_check::cel_paths::is_prev_path(path) {
            true => (path.strip_prefix("prev.").unwrap_or(path).to_string(), true),
            false => (path.to_string(), false),
        };
        let mut writers: BTreeSet<String> = self
            .writes
            .iter()
            .filter(|(_, w)| overlaps(w, &target))
            .map(|(label, _)| label.clone())
            .collect();
        // A body the checker already spliced a component's effects into
        // writes the same path under its own label; the `via` one says why.
        let spliced: Vec<String> = writers
            .iter()
            .filter_map(|l| {
                l.split_once(" via component ")
                    .map(|(own, _)| own.to_string())
            })
            .collect();
        for own in spliced {
            writers.remove(&own);
        }
        let decl = self.decl(&target);
        let engine = decl.is_some_and(|d| d.owner.is_some());
        let mut note = None;
        if engine {
            note = Some("the engine (`owner: engine`)".to_string());
        }
        if earlier {
            note = Some(match note {
                Some(n) => format!("{n}, in an earlier run"),
                None => "its writers, in an earlier run".to_string(),
            });
        }
        let none = writers.is_empty()
            && !engine
            && matches!(target.split('.').next(), Some("run" | "user"));
        if none {
            let rest = match decl.and_then(|d| d.default.as_ref()) {
                Some(_) => "it keeps its declared default",
                None => "it stays unset",
            };
            note = Some(format!(
                "nothing writes it{} — {rest}",
                if earlier { " in any run" } else { "" }
            ));
        }
        need(writers.into_iter().collect(), none, note)
    }
}

/// `lute scenario <dir> reach --endings[=<occasion>]`, text.
pub(crate) fn run_text(
    out: &mut String,
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    occasion: Option<&str>,
) -> ExitCode {
    let roots = match rows(by_root, file_results, occasion) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let what = match occasion {
        Some(o) => format!("the beats answering `{o}`"),
        None => "the beats whose content can run `::end`".to_string(),
    };
    for r in &roots {
        outln!(out, "project root: {}", r.root.display());
        outln!(out, "endings ({what}):");
        if r.rows.is_empty() {
            outln!(out, "  (none)");
        }
        for row in &r.rows {
            outln!(
                out,
                "  {} ({}, {}): {}",
                row.id,
                row.kind,
                row.file,
                row.bucket.as_str()
            );
            outln!(out, "    after: {}", row.after_text);
            match &row.when {
                Some(w) => outln!(out, "    when: `{w}` — {}", row.when_text),
                None => outln!(out, "    when: {}", row.when_text),
            }
            for n in &row.needs {
                let mut parts = Vec::new();
                if !n.producers.is_empty() {
                    parts.push(format!("written by {}", n.producers.join(", ")));
                }
                if let Some(note) = &n.note {
                    parts.push(note.clone());
                }
                if parts.is_empty() {
                    parts.push("producers not determined".to_string());
                }
                outln!(out, "      {} — {}", n.what, parts.join("; "));
            }
        }
        let count = |b: Bucket| r.rows.iter().filter(|x| x.bucket == b).count();
        outln!(
            out,
            "{} ending(s): {} reachable, {} unreachable, {} unknown",
            r.rows.len(),
            count(Bucket::Reachable),
            count(Bucket::Unreachable),
            count(Bucket::Unknown)
        );
    }
    outln!(
        out,
        "reachable here means nothing static refutes it, not that a run reaches it: a play that \
         presents the ending is the proof — `lute test --coverage` lists the beats no play \
         presents."
    );
    ExitCode::SUCCESS
}

/// `lute scenario <dir> --format json reach --endings[=<occasion>]`.
pub(crate) fn json(
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    occasion: Option<&str>,
) -> Result<Json, ExitCode> {
    let roots = rows(by_root, file_results, occasion)?;
    let roots: Vec<Json> = roots
        .iter()
        .map(|r| {
            let count = |b: Bucket| r.rows.iter().filter(|x| x.bucket == b).count();
            let endings: Vec<Json> = r
                .rows
                .iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "kind": row.kind,
                        "file": row.file,
                        "verdict": row.bucket.as_str(),
                        "after": { "reach": row.after_token, "text": row.after_text },
                        "when": {
                            "text": row.when,
                            "verdict": row.when_token,
                            "detail": row.when_text,
                        },
                        "needs": row.needs.iter().map(|n| json!({
                            "what": n.what,
                            "writers": n.producers,
                            "note": n.note,
                            "nothingProduces": n.none,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "root": r.root.display().to_string(),
                "occasion": occasion,
                "endings": endings,
                "summary": {
                    "endings": r.rows.len(),
                    "reachable": count(Bucket::Reachable),
                    "unreachable": count(Bucket::Unreachable),
                    "unknown": count(Bucket::Unknown),
                },
            })
        })
        .collect();
    Ok(json!({ "roots": roots }))
}
