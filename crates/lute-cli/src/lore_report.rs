//! `lute lore <dir> [--json]` — the project's world-narrative map (dsl 0.19.0
//! §8).
//!
//! A read-only report over every `.lute` document under `dir`, discovered by
//! the SAME deterministic walk `check-project` uses ([`crate::find_lute_files`]
//! — byte-sorted, symlink-deduped) and parsed by the same syntax-layer parse
//! the checker runs ([`lute_syntax::parse`]). It does not check: a document
//! that parses is reported whatever its diagnostics; one whose parse fails
//! (any `Error`-severity parse diagnostic, `lute loc`'s own guard) is named on
//! stderr and skipped. Three sections:
//!
//! 1. **Entries by target** — every lore `<entry>`, grouped by its `target`
//!    (targets byte-sorted, entries without one last under `(no target)`);
//!    within a group, project order (file order, then declaration order — the
//!    `ProjectIndex.entries` tiebreak, spec §6).
//! 2. **Series** — every `series`, byte-sorted, its entries by `order`
//!    (entries whose `order` is absent or not a non-negative integer follow,
//!    in project order).
//! 3. **Facts** — for every relation with at least one `::assert` anywhere,
//!    each ground fact asserted, and who reveals it: lore entries (their ids),
//!    scenes/quests (their documents), or both. Relations and facts are
//!    byte-sorted; ids and documents too.
//!
//! Document paths are shown relative to `dir`. `--json` emits the same data as
//! one object. Exit `0` on success, `2` on an I/O failure.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use lute_core_span::Severity;
use lute_syntax::ast::{Arm, Document, Node};
use lute_syntax::datalog::{FactPattern, FactTerm};
use serde::Serialize;

/// One lore entry as the report lists it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryRow {
    id: String,
    document: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    /// The RESOLVED series ([`lute_check::resolve_entry_series`], spec
    /// §2.1): the document's `series:` when it declares one, else the
    /// entry's own `series=`.
    #[serde(skip_serializing_if = "Option::is_none")]
    series: Option<String>,
    /// The RESOLVED order: the entry's position under a document-level
    /// `series:`, else its own `order=` when that is a non-negative integer
    /// (spec §3, the checker's own [`lute_check::parse_entry_order`]); a
    /// malformed value (`E-ENTRY-ATTR`) is reported as absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    order: Option<u32>,
}

#[derive(Serialize)]
struct TargetGroup {
    /// `null` for the entries that declare no target.
    target: Option<String>,
    entries: Vec<EntryRow>,
}

#[derive(Serialize)]
struct SeriesGroup {
    series: String,
    entries: Vec<EntryRow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FactRow {
    fact: String,
    /// `"entries"`, `"scenes"` (scenes and quests), or `"both"`.
    revealed_by: &'static str,
    entries: Vec<String>,
    documents: Vec<String>,
}

#[derive(Serialize)]
struct RelationGroup {
    relation: String,
    facts: Vec<FactRow>,
}

#[derive(Serialize)]
struct Report {
    targets: Vec<TargetGroup>,
    series: Vec<SeriesGroup>,
    relations: Vec<RelationGroup>,
}

/// Who asserts one ground fact: entry ids and scene/quest documents.
#[derive(Default)]
struct Sources {
    entries: BTreeSet<String>,
    documents: BTreeSet<String>,
}

/// Every well-formed ground `::assert` pattern in `nodes`, recursing into
/// every body that can hold one (choice, hub choice, match arm, objective,
/// `<on>`). Parse-failed sentinels (empty relation) and patterns with a `_`
/// (a checker error in an assert) are skipped.
fn collect_asserts(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Assert(a) => {
                if let Some(fact) = ground_fact(&a.pattern) {
                    out.push(fact);
                }
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    collect_asserts(&c.body, out);
                }
            }
            Node::Hub(h) => {
                for c in &h.choices {
                    collect_asserts(&c.body, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_asserts(body, out)
                        }
                    }
                }
            }
            Node::Objective(o) => collect_asserts(&o.body, out),
            Node::On(o) => collect_asserts(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Retract(_) => {}
        }
    }
}

/// `rel(a, b)` for a ground pattern; `None` for a parse-failed or wildcard one.
fn ground_fact(p: &FactPattern) -> Option<String> {
    if p.relation.is_empty() {
        return None;
    }
    let mut args = Vec::with_capacity(p.args.len());
    for arg in &p.args {
        match &arg.term {
            FactTerm::Ident(s) => args.push(s.clone()),
            FactTerm::Bool(b) => args.push(b.to_string()),
            FactTerm::Wildcard => return None,
        }
    }
    Some(format!("{}({})", p.relation, args.join(", ")))
}

/// Fold one parsed document into the running entry list and fact table.
fn fold_document(
    document: &str,
    doc: &Document,
    entries: &mut Vec<EntryRow>,
    facts: &mut BTreeMap<String, BTreeMap<String, Sources>>,
) {
    let mut record = |fact: String, into_entry: Option<&str>| {
        let relation = fact[..fact.find('(').unwrap_or(fact.len())].to_string();
        let sources = facts.entry(relation).or_default().entry(fact).or_default();
        match into_entry {
            Some(id) => sources.entries.insert(id.to_string()),
            None => sources.documents.insert(document.to_string()),
        };
    };
    let mut scene_facts = Vec::new();
    for shot in &doc.shots {
        collect_asserts(&shot.body, &mut scene_facts);
    }
    for quest in &doc.quests {
        collect_asserts(&quest.body, &mut scene_facts);
    }
    for fact in scene_facts {
        record(fact, None);
    }
    let doc_series = lute_check::document_series(&doc.meta);
    let resolved = lute_check::resolve_entry_series(doc_series.as_deref(), &doc.entries);
    for (entry, position) in doc.entries.iter().zip(resolved) {
        let mut entry_facts = Vec::new();
        collect_asserts(&entry.body, &mut entry_facts);
        for fact in entry_facts {
            record(fact, Some(&entry.id));
        }
        entries.push(EntryRow {
            id: entry.id.clone(),
            document: document.to_string(),
            target: entry.target.as_ref().map(|(v, _)| v.clone()),
            category: entry.category.as_ref().map(|(v, _)| v.clone()),
            title: entry.title.as_ref().map(|(v, _)| v.clone()),
            series: position.series.map(str::to_string),
            order: position.order,
        });
    }
}

/// Group the folded data into the report's three stable-sorted sections.
fn build_report(
    entries: Vec<EntryRow>,
    facts: BTreeMap<String, BTreeMap<String, Sources>>,
) -> Report {
    let mut by_target: BTreeMap<String, Vec<EntryRow>> = BTreeMap::new();
    let mut untargeted = Vec::new();
    let mut by_series: BTreeMap<String, Vec<EntryRow>> = BTreeMap::new();
    for row in entries {
        if let Some(series) = &row.series {
            by_series.entry(series.clone()).or_default().push(row.clone());
        }
        match &row.target {
            Some(t) => by_target.entry(t.clone()).or_default().push(row),
            None => untargeted.push(row),
        }
    }
    let mut targets: Vec<TargetGroup> = by_target
        .into_iter()
        .map(|(target, entries)| TargetGroup {
            target: Some(target),
            entries,
        })
        .collect();
    if !untargeted.is_empty() {
        targets.push(TargetGroup {
            target: None,
            entries: untargeted,
        });
    }
    let series = by_series
        .into_iter()
        .map(|(series, mut entries)| {
            // Stable: equal keys (and every order-less entry) keep project order.
            entries.sort_by_key(|e| (e.order.is_none(), e.order));
            SeriesGroup { series, entries }
        })
        .collect();
    let relations = facts
        .into_iter()
        .map(|(relation, facts)| RelationGroup {
            relation,
            facts: facts
                .into_iter()
                .map(|(fact, s)| FactRow {
                    fact,
                    revealed_by: match (s.entries.is_empty(), s.documents.is_empty()) {
                        (false, true) => "entries",
                        (true, false) => "scenes",
                        _ => "both",
                    },
                    entries: s.entries.into_iter().collect(),
                    documents: s.documents.into_iter().collect(),
                })
                .collect(),
        })
        .collect();
    Report {
        targets,
        series,
        relations,
    }
}

fn entry_label(e: &EntryRow) -> &str {
    if e.id.is_empty() {
        "(missing id)"
    } else {
        &e.id
    }
}

/// One entry line: `id  [category]  "title"  document`.
fn entry_line(e: &EntryRow, lead: Option<String>) -> String {
    let mut s = String::from("    ");
    if let Some(lead) = lead {
        s.push_str(&lead);
        s.push_str("  ");
    }
    s.push_str(entry_label(e));
    if let Some(c) = &e.category {
        s.push_str(&format!("  [{c}]"));
    }
    if let Some(t) = &e.title {
        s.push_str(&format!("  \"{t}\""));
    }
    s.push_str(&format!("  {}", e.document));
    s
}

fn render_text(r: &Report) -> String {
    let mut out = String::from("Entries by target\n");
    if r.targets.is_empty() {
        out.push_str("  (none)\n");
    }
    for g in &r.targets {
        out.push_str(&format!(
            "  {}\n",
            g.target.as_deref().unwrap_or("(no target)")
        ));
        for e in &g.entries {
            out.push_str(&entry_line(e, None));
            out.push('\n');
        }
    }
    out.push_str("\nSeries\n");
    if r.series.is_empty() {
        out.push_str("  (none)\n");
    }
    for g in &r.series {
        out.push_str(&format!("  {}\n", g.series));
        for e in &g.entries {
            let lead = e.order.map_or_else(|| "-".to_string(), |o| o.to_string());
            out.push_str(&entry_line(e, Some(lead)));
            out.push('\n');
        }
    }
    out.push_str("\nFacts by relation\n");
    if r.relations.is_empty() {
        out.push_str("  (none)\n");
    }
    for g in &r.relations {
        out.push_str(&format!("  {}\n", g.relation));
        for f in &g.facts {
            out.push_str(&format!("    {}  {}\n", f.fact, f.revealed_by));
            if !f.entries.is_empty() {
                out.push_str(&format!("      entries: {}\n", f.entries.join(", ")));
            }
            if !f.documents.is_empty() {
                out.push_str(&format!(
                    "      scenes/quests: {}\n",
                    f.documents.join(", ")
                ));
            }
        }
    }
    out
}

/// Run `lute lore`. See the module doc.
pub fn run_lore(dir: &Path, json: bool) -> ExitCode {
    let files = match crate::find_lute_files(dir) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("lute lore: cannot walk {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };
    let mut entries = Vec::new();
    let mut facts = BTreeMap::new();
    for path in &files {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lute lore: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let (doc, diags) = lute_syntax::parse(&text);
        let errors = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        if errors > 0 {
            eprintln!(
                "lute lore: skipping {} — parse failed ({errors} error(s))",
                path.display()
            );
            continue;
        }
        let document = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        fold_document(&document, &doc, &mut entries, &mut facts);
    }
    let report = build_report(entries, facts);
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute lore: cannot serialize the report: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        print!("{}", render_text(&report));
    }
    ExitCode::SUCCESS
}
