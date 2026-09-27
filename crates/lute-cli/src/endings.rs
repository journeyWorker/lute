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
    producers: Vec<Writer>,
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
    /// The occasion's `raisedWhen` gate — a condition of the beat as much
    /// as its `when` — and what it reads.
    gate: Option<(String, Vec<Need>)>,
    /// The beat's content can run `::end`.
    runs_end: bool,
    /// What the beat writes (a state path, `holds(<relation>(…))`) that
    /// the project's `terminal:` reads — it can end the game that way.
    terminal: Vec<String>,
    /// OT-F-13: the errors inside the `when` (the root cause of a
    /// never-holds verdict), as `(code, line, column, message)`.
    causes: Vec<(String, u32, u32, String)>,
    bucket: Bucket,
}

/// One root's rows.
struct RootRows {
    root: PathBuf,
    /// The project's `terminal:` condition, as declared.
    terminal: Option<String>,
    rows: Vec<Row>,
}

/// The project's `terminal:` condition and what it reads: the state paths,
/// and the relations of the facts it needs to hold.
struct Terminal {
    raw: String,
    paths: BTreeSet<String>,
    relations: BTreeSet<String>,
}

impl Terminal {
    fn of(root: &Path, group: &DocGroup, producers: &Producers<'_>) -> Option<Self> {
        let folded = group
            .iter()
            .map(|(_, _, f)| f)
            .find(|f| f.env.terminal.is_some())?;
        let raw = folded.env.terminal.clone()?;
        let expanded = expand(folded, &raw);
        let relations =
            crate::knowledge::condition_atoms(root, group, &expanded, &producers.asserts)
                .into_iter()
                .filter(|(_, negated, _)| !negated)
                .filter_map(|(atom, _, _)| atom.split_once('(').map(|(r, _)| r.trim().to_string()))
                .collect();
        Some(Terminal {
            paths: read_paths(&expanded),
            relations,
            raw,
        })
    }

    /// What beat `b`'s content writes that the condition reads.
    fn written_by(
        &self,
        root: &Path,
        producers: &Producers<'_>,
        b: &ProjectBeat<'_>,
    ) -> Vec<String> {
        let mut out: BTreeSet<String> = BTreeSet::new();
        for (w, path) in &producers.writes {
            if w.origin.is_beat(b, root) {
                out.extend(self.paths.iter().filter(|r| overlaps(path, r)).cloned());
            }
        }
        for (origin, relation) in &producers.asserted {
            if origin.is_beat(b, root) && self.relations.contains(relation) {
                out.insert(format!("holds({relation}(…))"));
            }
        }
        out.into_iter().collect()
    }
}

/// `cond` with its `@def`s expanded in `folded`'s document.
fn expand(folded: &lute_check::FoldedEnv, cond: &str) -> String {
    let defs = lute_check::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    lute_check::cel_expand::expand_cel(cond, &defs, None, &mut Vec::new())
        .unwrap_or_else(|_| cond.to_string())
}

/// Whether `cond` decides false in `folded`'s document on its own — the
/// verdict a literal fault inside it (`E-WHEN-LITERAL-DOMAIN`) stands for
/// when that fault is the one report.
fn decides_false(folded: &lute_check::FoldedEnv, cond: &str) -> bool {
    let defs = lute_check::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let ctx = lute_check::DecideCtx {
        schema: &folded.env.state,
        dollar: None,
        params: &BTreeMap::new(),
        facts: None,
    };
    matches!(
        lute_check::decide_slot(cond, &defs, &ctx),
        Some(lute_check::Decided::Bool(false))
    )
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
    // OT-F-13: the errors a verdict follows from — a typo in the `when`
    // itself (`E-WHEN-LITERAL-DOMAIN`) — cited before the verdict.
    let errors: Vec<(&PathBuf, &Diagnostic)> = verdicts
        .iter()
        .copied()
        .filter(|(_, d)| {
            d.severity == lute_core_span::Severity::Error
                && !verdict_codes.contains(&d.code.as_str())
        })
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
        let terminal = Terminal::of(root, group, &producers);
        let mut rows = Vec::new();
        for b in &beats {
            let doc = scenario
                .docs
                .iter()
                .find(|(p, _)| p == b.path)
                .map(|(_, d)| d);
            let runs_end =
                doc.is_some_and(|d| beat_body(d, b).is_some_and(|body| producers.can_end(body, 0)));
            let writes_terminal = terminal
                .as_ref()
                .map(|t| t.written_by(root, &producers, b))
                .unwrap_or_default();
            let is_ending = match occasion {
                Some(o) => b.on == o,
                // An ending runs `::end`, or can make `terminal:` hold.
                None => runs_end || !writes_terminal.is_empty(),
            };
            if !is_ending {
                continue;
            }
            let about: Vec<&Diagnostic> = verdicts
                .iter()
                .filter(|(p, d)| crate::beats_cmd::names_beat(p, d, b))
                .map(|(_, d)| *d)
                .collect();
            let causes: Vec<&Diagnostic> = b
                .when_slot
                .map(|slot| {
                    errors
                        .iter()
                        .filter(|(p, d)| {
                            p.as_path() == b.path.as_path()
                                && d.span.byte_start >= slot.span.byte_start
                                && d.span.byte_start <= slot.span.byte_end
                        })
                        .map(|(_, d)| *d)
                        .collect()
                })
                .unwrap_or_default();
            let mut row = row(root, group, &scenario, &producers, b, &about, &causes);
            row.runs_end = runs_end;
            row.terminal = writes_terminal;
            rows.push(row);
        }
        out.push(RootRows {
            root: root.clone(),
            terminal: terminal.map(|t| t.raw),
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
    causes: &[&Diagnostic],
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
    // The occasion's `raisedWhen`: the beat plays only when it holds.
    let gate = lute_check::gates::gate_of(&b.folded.occasions, b.on);
    let unreachable = about.iter().find(|d| {
        d.code == lute_check::E_BEAT_UNREACHABLE || d.code == lute_check::E_ENTRY_UNREACHABLE
    });
    let shadowed = about.iter().find(|d| d.code == lute_check::W_BEAT_SHADOWED);
    let caused = if causes.is_empty() {
        String::new()
    } else {
        let each: Vec<String> = causes
            .iter()
            .map(|d| {
                format!(
                    "{} at {}:{}: {}",
                    d.code,
                    d.span.line,
                    d.span.column,
                    d.text()
                )
            })
            .collect();
        format!(" — caused by {}", each.join("; "))
    };
    // A literal fault that makes the `when` never hold is that verdict's one
    // report: the row still reads never-holds, caused by it.
    let dead_by_cause = unreachable.is_none()
        && !causes.is_empty()
        && b.when_slot.is_some_and(|s| decides_false(b.folded, &s.raw));
    let (when_token, when_text) = if let Some(d) = unreachable {
        (
            "never-holds",
            format!("never holds{caused} ({}: {})", d.code, d.text()),
        )
    } else if dead_by_cause {
        ("never-holds", format!("never holds{caused}"))
    } else if let Some(d) = shadowed {
        (
            "never-wins",
            format!("never wins ({}: {})", d.code, d.text()),
        )
    } else if b.when.is_none() {
        let always = match gate {
            Some(_) => "no `when` — holds whenever its gate lets the occasion be raised",
            None => "no `when` — always holds",
        };
        ("absent", always.to_string())
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
    // The gate is judged with the `when`: a refuted beat needs nothing.
    let gate = gate.map(|raw| {
        let needs = match when_token {
            "never-holds" | "never-wins" => Vec::new(),
            _ => self::needs(root, group, producers, &expand(b.folded, raw)),
        };
        (raw.to_string(), needs)
    });
    let bucket = if after_token == "unreachable"
        || unreachable.is_some()
        || dead_by_cause
        || shadowed.is_some()
    {
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
        gate,
        // Set by the caller, which decides what makes the beat an ending.
        runs_end: false,
        terminal: Vec::new(),
        causes: causes
            .iter()
            .map(|d| {
                (
                    d.code.clone(),
                    d.span.line,
                    d.span.column,
                    d.text().into_owned(),
                )
            })
            .collect(),
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

/// Where a write or assert sits: the beat (or quest) whose content makes
/// it, and the component it runs inside when a `::use` brought it in.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Origin {
    /// `scene`, `entry`, `beat` or `quest`.
    kind: &'static str,
    id: String,
    /// The document, root-relative.
    file: String,
    /// The `::use`d component.
    via: Option<String>,
}

impl Origin {
    /// ``scene `k` ``, ``scene `k` via component `c` ``.
    fn text(&self) -> String {
        match &self.via {
            Some(c) => format!("{} `{}` via component `{c}`", self.kind, self.id),
            None => format!("{} `{}`", self.kind, self.id),
        }
    }

    /// Whether this is beat `b`'s own content (or a component it uses).
    fn is_beat(&self, b: &ProjectBeat<'_>, root: &Path) -> bool {
        let file = b
            .path
            .strip_prefix(root)
            .unwrap_or(b.path)
            .display()
            .to_string();
        self.file == file
            && match b.kind {
                // One scene per document.
                ProjectBeatKind::Scene => self.kind == "scene",
                ProjectBeatKind::Entry => self.kind == "entry" && self.id == b.id,
                ProjectBeatKind::Bundle => self.kind == "beat" && self.id == b.id,
            }
    }
}

/// How a write is made.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum How {
    /// `::set`.
    Set,
    /// A `<choice into>`.
    ChoiceInto,
    /// A plugin directive's declared `effects.writes`.
    Directive(String),
    /// A reward kind's `credits:`.
    Reward(String),
}

/// One writer of a state path.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Writer {
    origin: Origin,
    how: How,
}

impl Writer {
    /// The writer as the text report names it.
    fn text(&self) -> String {
        let at = self.origin.text();
        match &self.how {
            How::Set => at,
            How::ChoiceInto => format!("{at} (choice into)"),
            How::Directive(tag) => format!("{at} via `::{tag}`"),
            How::Reward(kind) => format!("{at} (reward `{kind}`)"),
        }
    }

    /// `{ kind, id, file, how, via, directive?, reward?, text }`.
    fn json(&self) -> Json {
        let o = &self.origin;
        let mut m = serde_json::Map::new();
        m.insert("kind".into(), json!(o.kind));
        m.insert("id".into(), json!(o.id));
        m.insert("file".into(), json!(o.file));
        let how = match &self.how {
            How::Set => "set",
            How::ChoiceInto => "choice into",
            How::Directive(tag) => {
                m.insert("directive".into(), json!(tag));
                "directive"
            }
            How::Reward(kind) => {
                m.insert("reward".into(), json!(kind));
                "reward"
            }
        };
        m.insert("how".into(), json!(how));
        m.insert("via".into(), json!(o.via));
        m.insert("text".into(), json!(self.text()));
        Json::Object(m)
    }
}

/// One root's writers of state and asserters of facts.
struct Producers<'g> {
    /// `(writer, written path)`.
    writes: Vec<(Writer, String)>,
    /// `(label, relation)` asserting sites the knowledge walk does not see.
    asserts: Vec<(String, String)>,
    /// `(origin, relation)` of every assert, the document's own included.
    asserted: Vec<(Origin, String)>,
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
            asserted: Vec::new(),
            components,
            snapshot,
            group,
        };
        let none = BTreeMap::new();
        for (path, doc, folded) in group {
            if folded.typed.component.is_some() {
                continue;
            }
            let file = path
                .strip_prefix(root)
                .unwrap_or(path)
                .display()
                .to_string();
            let at = |kind: &'static str, id: String| Origin {
                kind,
                id,
                file: file.clone(),
                via: None,
            };
            if let Some(key) = lute_check::connectivity::scene_key(doc) {
                let origin = at("scene", key);
                for shot in &doc.shots {
                    p.walk(&shot.body, &origin, &none, 0);
                }
            }
            for e in &doc.entries {
                p.walk(&e.body, &at("entry", e.id.clone()), &none, 0);
            }
            let bundle = lute_check::connectivity::bundle_id(doc);
            for b in &doc.beats {
                let id = match &bundle {
                    Some(d) => lute_check::bundle_beat_key(d, &b.id),
                    None => b.id.clone(),
                };
                p.walk(&b.body, &at("beat", id), &none, 0);
            }
            for q in &doc.quests {
                let origin = at("quest", q.id.clone());
                p.rewards(&q.rewards, &origin);
                p.walk(&q.body, &origin, &none, 0);
            }
        }
        p
    }

    fn write(&mut self, origin: &Origin, how: How, path: String) {
        let origin = origin.clone();
        self.writes.push((Writer { origin, how }, path));
    }

    fn rewards(&mut self, rewards: &[Reward], origin: &Origin) {
        for r in rewards {
            if let Some(path) = self
                .snapshot
                .reward_kinds
                .get(&r.kind)
                .and_then(|k| k.credits.clone())
            {
                self.write(origin, How::Reward(r.kind.clone()), path);
            }
        }
    }

    /// Record every write / assert `nodes` makes at `origin`; a `::use`
    /// walks its component with the call's arguments bound.
    fn walk(
        &mut self,
        nodes: &[Node],
        origin: &Origin,
        params: &BTreeMap<String, String>,
        depth: u8,
    ) {
        for node in nodes {
            match node {
                Node::Set(s) => self.write(origin, How::Set, normalize(&s.path, params)),
                Node::Directive(d) => self.directive(d, origin, params, depth),
                Node::Branch(b) => {
                    for c in &b.choices {
                        self.choice(&c.attrs, &c.body, origin, params, depth);
                    }
                }
                Node::Hub(h) => {
                    for c in &h.choices {
                        self.choice(&c.attrs, &c.body, origin, params, depth);
                    }
                    if let Some(r) = &h.on_return {
                        self.walk(&r.body, origin, params, depth);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                        self.walk(body, origin, params, depth);
                    }
                }
                Node::On(o) => self.walk(&o.body, origin, params, depth),
                Node::Objective(o) => {
                    self.rewards(&o.rewards, origin);
                    self.walk(&o.body, origin, params, depth);
                }
                Node::Timeline(t) => {
                    for clip in t.tracks.iter().flat_map(|t| &t.clips) {
                        match &clip.node {
                            ClipNode::Set(s) => {
                                self.write(origin, How::Set, normalize(&s.path, params))
                            }
                            ClipNode::Directive(d) => self.directive(d, origin, params, depth),
                        }
                    }
                }
                Node::Assert(a) => {
                    let relation = a.pattern.relation.clone();
                    // Asserts in a document's own body are the knowledge walk's.
                    if depth > 0 {
                        self.asserts.push((origin.text(), relation.clone()));
                    }
                    self.asserted.push((origin.clone(), relation));
                }
                Node::Line(_) | Node::Retract(_) => {}
            }
        }
    }

    fn choice(
        &mut self,
        attrs: &[lute_syntax::ast::Attr],
        body: &[Node],
        origin: &Origin,
        params: &BTreeMap<String, String>,
        depth: u8,
    ) {
        if let Some(AttrValue::Str(into)) = attrs.iter().find(|a| a.key == "into").map(|a| &a.value)
        {
            self.write(origin, How::ChoiceInto, normalize(into, params));
        }
        self.walk(body, origin, params, depth);
    }

    fn directive(
        &mut self,
        d: &Directive,
        origin: &Origin,
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
            // The outermost component names the way in.
            let mut inner = origin.clone();
            if inner.via.is_none() {
                inner.via = Some(name.clone());
            }
            for shot in &doc.shots {
                self.walk(&shot.body, &inner, &args, depth + 1);
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
            let origin = origin.clone();
            self.writes.push((
                Writer {
                    origin,
                    how: How::Directive(d.tag.clone()),
                },
                segs.join("."),
            ));
        }
        for a in &effects.asserts {
            let label = format!("{} via `::{}`", origin.text(), d.tag);
            self.asserts.push((label, a.relation.clone()));
            self.asserted.push((origin.clone(), a.relation.clone()));
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
            Node::Hub(h) => h.bodies().any(|b| self.nodes_end(b, depth)),
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
        let need = |producers: Vec<Writer>, none: bool, note: Option<String>| Need {
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
        let mut writers: BTreeSet<Writer> = self
            .writes
            .iter()
            .filter(|(_, w)| overlaps(w, &target))
            .map(|(writer, _)| writer.clone())
            .collect();
        // A body the checker already spliced a component's effects into
        // writes the same path as its own; the `via` one says why.
        let spliced: Vec<Writer> = writers
            .iter()
            .filter(|w| w.origin.via.is_some())
            .map(|w| Writer {
                origin: Origin {
                    via: None,
                    ..w.origin.clone()
                },
                how: w.how.clone(),
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

/// A need's text line: what, and who produces it.
fn need_line(out: &mut String, n: &Need) {
    let mut parts = Vec::new();
    if !n.producers.is_empty() {
        let by: Vec<String> = n.producers.iter().map(Writer::text).collect();
        parts.push(format!("written by {}", by.join(", ")));
    }
    if let Some(note) = &n.note {
        parts.push(note.clone());
    }
    if parts.is_empty() {
        parts.push("producers not determined".to_string());
    }
    outln!(out, "      {} — {}", n.what, parts.join("; "));
}

fn need_json(n: &Need) -> Json {
    json!({
        "what": n.what,
        "writers": n.producers.iter().map(Writer::json).collect::<Vec<_>>(),
        "note": n.note,
        "nothingProduces": n.none,
    })
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
    for r in &roots {
        let what = match (occasion, &r.terminal) {
            (Some(o), _) => format!("the beats answering `{o}`"),
            (None, Some(t)) => format!(
                "the beats whose content can run `::end` or write what `terminal: {t}` reads"
            ),
            (None, None) => "the beats whose content can run `::end`".to_string(),
        };
        outln!(out, "project root: {}", r.root.display());
        outln!(out, "endings ({what}):");
        if r.rows.is_empty() && occasion.is_none() {
            // OT-F-13: a game that ends by an occasion, not by `::end`.
            let ends = match &r.terminal {
                Some(t) => format!("runs `::end` or writes what `terminal: {t}` reads"),
                None => "runs `::end`".to_string(),
            };
            outln!(
                out,
                "  (none) — no beat's content {ends}; if the game ends on an occasion, name it: \
                 `--endings=<occasion>`"
            );
        } else if r.rows.is_empty() {
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
            if let (false, Some(t)) = (row.terminal.is_empty(), &r.terminal) {
                let what: Vec<String> = row.terminal.iter().map(|w| format!("`{w}`")).collect();
                outln!(
                    out,
                    "    ends: writes {}, which `terminal: {t}` reads",
                    what.join(", ")
                );
            }
            outln!(out, "    after: {}", row.after_text);
            if let Some((gate, needs)) = &row.gate {
                let tail = if needs.is_empty() {
                    ""
                } else {
                    " — it needs:"
                };
                outln!(out, "    gate: `raisedWhen: {gate}`{tail}");
                for n in needs {
                    need_line(out, n);
                }
            }
            match &row.when {
                Some(w) => outln!(out, "    when: `{w}` — {}", row.when_text),
                None => outln!(out, "    when: {}", row.when_text),
            }
            for n in &row.needs {
                need_line(out, n);
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
    unreachable_exit(&roots)
}

/// OT-F-13: exit 1 when a static verdict refutes an ending, so CI can fail
/// on it; `unknown` is no failure.
fn unreachable_exit(roots: &[RootRows]) -> ExitCode {
    if roots
        .iter()
        .flat_map(|r| &r.rows)
        .any(|row| row.bucket == Bucket::Unreachable)
    {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `lute scenario <dir> --format json reach --endings[=<occasion>]`, with
/// its exit code ([`unreachable_exit`]).
pub(crate) fn json(
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    occasion: Option<&str>,
) -> Result<(Json, ExitCode), ExitCode> {
    let roots = rows(by_root, file_results, occasion)?;
    let exit = unreachable_exit(&roots);
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
                        "runsEnd": row.runs_end,
                        "writesTerminal": row.terminal,
                        "after": { "reach": row.after_token, "text": row.after_text },
                        "gate": row.gate.as_ref().map(|(text, needs)| json!({
                            "text": text,
                            "needs": needs.iter().map(need_json).collect::<Vec<_>>(),
                        })),
                        "when": {
                            "text": row.when,
                            "verdict": row.when_token,
                            "detail": row.when_text,
                        },
                        "causes": row.causes.iter().map(|(code, line, column, message)| json!({
                            "code": code,
                            "line": line,
                            "column": column,
                            "message": message,
                        })).collect::<Vec<_>>(),
                        "needs": row.needs.iter().map(need_json).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "root": r.root.display().to_string(),
                "occasion": occasion,
                "terminal": r.terminal,
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
    Ok((json!({ "roots": roots }), exit))
}
