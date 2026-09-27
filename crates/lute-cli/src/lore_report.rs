//! `lute lore <dir> [--json]` — the project's world-narrative map (dsl 0.19.0
//! §8).
//!
//! A read-only report over every `.lute` document under `dir`, discovered by
//! the SAME deterministic walk `check-project` uses ([`crate::find_lute_files`]
//! — byte-sorted, symlink-deduped) and parsed by the same syntax-layer parse
//! the checker runs ([`lute_syntax::parse`]). It does not check: a document
//! that parses is reported whatever its diagnostics; one whose parse fails
//! (any `Error`-severity parse diagnostic, `lute loc`'s own guard) is named on
//! stderr and skipped. Each entry and beat row carries its `when` as
//! authored, and its `for` when it is presented once per member. Sections:
//!
//! 1. **Entries and beats by target** — every lore `<entry>` and every beat
//!    (a lore `<beat>` bundle under its canonical `<document id>.<beat id>`,
//!    labelled `beat`, dsl 0.23.0 §4, and a scene beat under its scene key,
//!    labelled `scene`, dsl 0.21.0 §4), grouped by its `target` (targets
//!    byte-sorted, rows without one last under `(no target)`); within a
//!    group, project order (file order, then declaration order — the
//!    `ProjectIndex.entries` tiebreak, spec §6).
//! 2. **Series** — every `series`, byte-sorted, its entries by `order`
//!    (entries whose `order` is absent or not a non-negative integer follow,
//!    in project order).
//! 3. **Facts** — for every relation with at least one `::assert` anywhere,
//!    each ground fact asserted, and who reveals it: lore entries (their
//!    ids), lore `<beat>` bundles (their canonical ids, listed as `beats`),
//!    scenes/quests (their documents), components (each `::use` of one, its
//!    params bound by the call: ``keepsake (via scene `k`)``), or more than
//!    one of those (`both`). Relations and facts are byte-sorted; ids and
//!    documents too. A component document reveals nothing on its own.
//! 4. **Derived** (dsl 0.24.0 T3-2, only when a relation is `derive`d) —
//!    every derived atom the may set holds (the conclusions the rules can
//!    reach from what the project asserts), each with the rule instances
//!    that conclude it, the evidence it rests on, and the fact-guarded
//!    conditions it gates — over the same per-root collection `lute
//!    scenario knowledge` reads ([`crate::knowledge`]).
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

/// One lore entry, or one beat, as the report lists it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntryRow {
    /// `Some("beat")` for a bundle beat, `Some("scene")` for a scene beat;
    /// absent for an entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    id: String,
    document: String,
    /// A beat's occasion.
    #[serde(skip_serializing_if = "Option::is_none")]
    on: Option<String>,
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
    /// The eligibility guard as authored (entry `when=`, beat `when`).
    #[serde(skip_serializing_if = "Option::is_none")]
    when: Option<String>,
    /// A `for="kind:<kind>"` beat's `for`: presented once per member.
    #[serde(rename = "for", skip_serializing_if = "Option::is_none")]
    for_kind: Option<String>,
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
    /// `"entries"` (lore entries), `"beats"` (bundle beats), `"scenes"`
    /// (scenes and quests), `"components"` (a component at a `::use`), or
    /// `"both"` (more than one of those).
    revealed_by: &'static str,
    entries: Vec<String>,
    beats: Vec<String>,
    documents: Vec<String>,
    components: Vec<ComponentSite>,
}

/// A component asserting a fact where it is used.
#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct ComponentSite {
    component: String,
    /// The use: ``scene `k` ``, ``quest `q` ``, ``entry `e` ``, ``beat `d.b` ``.
    via: String,
}

#[derive(Serialize)]
struct RelationGroup {
    relation: String,
    facts: Vec<FactRow>,
}

#[derive(Serialize)]
struct DerivedGroup {
    relation: String,
    facts: Vec<crate::knowledge::DerivedFact>,
}

#[derive(Serialize)]
struct Report {
    targets: Vec<TargetGroup>,
    series: Vec<SeriesGroup>,
    relations: Vec<RelationGroup>,
    /// Every derived relation's reachable atoms (dsl 0.24.0 T3-2); absent
    /// when the project declares no derived relation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    derived: Vec<DerivedGroup>,
}

/// Who asserts one ground fact: entry ids, bundle beat ids, scene/quest
/// documents and components at their uses.
#[derive(Default)]
struct Sources {
    entries: BTreeSet<String>,
    beats: BTreeSet<String>,
    documents: BTreeSet<String>,
    components: BTreeSet<ComponentSite>,
}

/// Which kind of source revealed a fact.
#[derive(Clone)]
enum Source<'a> {
    Entry(&'a str),
    Beat(&'a str),
    Document,
    /// A component, at a use labelled `via`.
    Component(String, &'a str),
}

/// Every well-formed ground `::assert` pattern in `nodes` — and (dsl 0.27.0
/// §4) every directive call's declared `effects.asserts` — recursing into
/// every body that can hold one (choice, hub choice, match arm, objective,
/// `<on>`). Parse-failed sentinels (empty relation) and patterns with a `_`
/// (a checker error in an assert) are skipped.
fn collect_asserts(
    nodes: &[Node],
    effects: &lute_check::directive_facts::EffectDirectives,
    out: &mut Vec<String>,
) {
    for node in nodes {
        match node {
            Node::Assert(a) => {
                if let Some(fact) = ground_fact(&a.pattern) {
                    out.push(fact);
                }
            }
            Node::Directive(d) => {
                let facts = lute_check::directive_facts::lookup(effects, d);
                out.extend(
                    facts
                        .iter()
                        .flat_map(|f| &f.asserts)
                        .filter_map(ground_fact),
                );
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    collect_asserts(&c.body, effects, out);
                }
            }
            Node::Hub(h) => {
                for b in h.bodies() {
                    collect_asserts(b, effects, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_asserts(body, effects, out)
                        }
                    }
                }
            }
            Node::Objective(o) => collect_asserts(&o.body, effects, out),
            Node::On(o) => collect_asserts(&o.body, effects, out),
            Node::Line(_) | Node::Set(_) | Node::Timeline(_) | Node::Retract(_) => {}
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
            FactTerm::Wildcard | FactTerm::Param(_) | FactTerm::Target => return None,
        }
    }
    Some(format!("{}({})", p.relation, args.join(", ")))
}

/// Fold one parsed document into the running entry list and fact table;
/// `components` are the project's component documents, by name.
fn fold_document(
    document: &str,
    doc: &Document,
    effects: &lute_check::directive_facts::EffectDirectives,
    components: &BTreeMap<&str, &Document>,
    entries: &mut Vec<EntryRow>,
    facts: &mut BTreeMap<String, BTreeMap<String, Sources>>,
) {
    let meta = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml).ok();
    let field = |k: &str| {
        meta.as_ref()?
            .get(serde_yaml::Value::String(k.to_string()))?
            .as_str()
            .map(str::to_string)
    };
    // A component asserts where it is used, with the call's arguments.
    if field("component").is_some() {
        return;
    }
    let mut record = |fact: String, source: Source<'_>| {
        let relation = fact[..fact.find('(').unwrap_or(fact.len())].to_string();
        let sources = facts.entry(relation).or_default().entry(fact).or_default();
        match source {
            Source::Entry(id) => sources.entries.insert(id.to_string()),
            Source::Beat(id) => sources.beats.insert(id.to_string()),
            Source::Document => sources.documents.insert(document.to_string()),
            Source::Component(component, via) => sources.components.insert(ComponentSite {
                component,
                via: via.to_string(),
            }),
        };
    };
    // Every fact `nodes` asserts, and the components it uses assert there.
    let mut reveal = |nodes: &[Node], source: Source<'_>, via: &str| {
        let mut own = Vec::new();
        collect_asserts(nodes, effects, &mut own);
        let used = crate::knowledge::component_asserts(nodes, components, effects);
        for fact in own {
            record(fact, source.clone());
        }
        for (component, p) in used {
            if let Some(fact) = ground_fact(&p) {
                record(fact, Source::Component(component, via));
            }
        }
    };
    let scene = lute_check::connectivity::scene_key(doc);
    let via = scene
        .as_ref()
        .map_or_else(|| document.to_string(), |k| format!("scene `{k}`"));
    for shot in &doc.shots {
        reveal(&shot.body, Source::Document, &via);
    }
    for quest in &doc.quests {
        reveal(
            &quest.body,
            Source::Document,
            &format!("quest `{}`", quest.id),
        );
    }
    // dsl 0.21.0 §4: a scene beat (frontmatter `on:`) is listed under its
    // target beside the bundle beats; its asserts stay the scene's.
    if let (Some(key), Some(on)) = (scene, field("on")) {
        let mut row = beat_row(
            "scene",
            key,
            document,
            Some(on),
            field("target"),
            field("title"),
        );
        row.when = field("when").map(|w| w.trim().to_string());
        row.for_kind = field("for");
        entries.push(row);
    }
    // Entries and bundle beats in declaration order (the rows interleave by
    // source position, as their compiled records do).
    let mut rows: Vec<(usize, EntryRow)> = Vec::new();
    let doc_series = lute_check::document_series(&doc.meta);
    let resolved = lute_check::resolve_entry_series(doc_series.as_deref(), &doc.entries);
    for (entry, position) in doc.entries.iter().zip(resolved) {
        reveal(
            &entry.body,
            Source::Entry(&entry.id),
            &format!("entry `{}`", entry.id),
        );
        rows.push((
            entry.span.byte_start,
            EntryRow {
                kind: None,
                id: entry.id.clone(),
                document: document.to_string(),
                on: None,
                target: entry.target.as_ref().map(|(v, _)| v.clone()),
                category: entry.category.as_ref().map(|(v, _)| v.clone()),
                title: entry.title.as_ref().map(|(v, _)| v.clone()),
                series: position.series.map(str::to_string),
                order: position.order,
                when: authored(&entry.when),
                for_kind: entry.for_kind.as_ref().map(|(v, _)| v.clone()),
            },
        ));
    }
    // dsl 0.23.0 §4: a bundle beat is listed and reveals its asserts under
    // its canonical id; without a well-formed document `id:` it has none (its
    // own `E-BEAT-ATTR`), so the bare beat id stands in.
    let doc_id = (!doc.beats.is_empty())
        .then(|| lute_check::connectivity::bundle_id(doc))
        .flatten();
    for beat in &doc.beats {
        let id = match &doc_id {
            Some(doc_id) => lute_check::bundle_beat_key(doc_id, &beat.id),
            None => beat.id.clone(),
        };
        reveal(&beat.body, Source::Beat(&id), &format!("beat `{id}`"));
        let value = |v: &Option<(String, lute_core_span::Span)>| v.as_ref().map(|(s, _)| s.clone());
        let mut row = beat_row(
            "beat",
            id,
            document,
            value(&beat.on),
            value(&beat.target),
            value(&beat.title),
        );
        row.when = authored(&beat.when);
        row.for_kind = value(&beat.for_kind);
        rows.push((beat.span.byte_start, row));
    }
    rows.sort_by_key(|(at, _)| *at);
    entries.extend(rows.into_iter().map(|(_, row)| row));
}

/// A beat's row — a bundle beat's (`beat`) or a scene beat's (`scene`) —
/// never in a series.
fn beat_row(
    kind: &'static str,
    id: String,
    document: &str,
    on: Option<String>,
    target: Option<String>,
    title: Option<String>,
) -> EntryRow {
    EntryRow {
        kind: Some(kind),
        id,
        document: document.to_string(),
        on,
        target,
        category: None,
        title,
        series: None,
        order: None,
        when: None,
        for_kind: None,
    }
}

/// A guard as authored, whitespace-collapsed; `None` when absent or empty.
fn authored(slot: &Option<lute_syntax::ast::CelSlot>) -> Option<String> {
    let raw = slot
        .as_ref()?
        .raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (!raw.is_empty()).then_some(raw)
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
            by_series
                .entry(series.clone())
                .or_default()
                .push(row.clone());
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
                    revealed_by: match (
                        s.entries.is_empty(),
                        s.beats.is_empty(),
                        s.documents.is_empty(),
                        s.components.is_empty(),
                    ) {
                        (false, true, true, true) => "entries",
                        (true, false, true, true) => "beats",
                        (true, true, false, true) => "scenes",
                        (true, true, true, false) => "components",
                        _ => "both",
                    },
                    entries: s.entries.into_iter().collect(),
                    beats: s.beats.into_iter().collect(),
                    documents: s.documents.into_iter().collect(),
                    components: s.components.into_iter().collect(),
                })
                .collect(),
        })
        .collect();
    Report {
        targets,
        series,
        relations,
        derived: Vec::new(),
    }
}

fn entry_label(e: &EntryRow) -> &str {
    if e.id.is_empty() {
        "(missing id)"
    } else {
        &e.id
    }
}

/// One entry line: `id  [category]  "title"  document`; a beat's reads
/// `beat  id  "title"  document  (on occasion)` (`scene` for a scene beat),
/// and a `for` beat's id says whom it is for: `id (for kind:bonded)`.
fn entry_line(e: &EntryRow, lead: Option<String>) -> String {
    let lead_width = lead.as_ref().map_or(0, |l| l.chars().count() + 2);
    let mut s = String::from("    ");
    if let Some(lead) = lead {
        s.push_str(&lead);
        s.push_str("  ");
    }
    s.push_str(entry_label(e));
    if let Some(f) = &e.for_kind {
        s.push_str(&format!(" (for {f})"));
    }
    if let Some(c) = &e.category {
        s.push_str(&format!("  [{c}]"));
    }
    if let Some(t) = &e.title {
        s.push_str(&format!("  \"{t}\""));
    }
    s.push_str(&format!("  {}", e.document));
    if let Some(on) = &e.on {
        s.push_str(&format!("  (on {on})"));
    }
    if let Some(w) = &e.when {
        s.push_str(&format!("\n{}when: {w}", " ".repeat(6 + lead_width)));
    }
    s
}

fn render_text(r: &Report) -> String {
    let mut out = String::from("Entries and beats by target\n");
    if r.targets.is_empty() {
        out.push_str("  (none)\n");
    }
    for g in &r.targets {
        out.push_str(&format!(
            "  {}\n",
            g.target.as_deref().unwrap_or("(no target)")
        ));
        for e in &g.entries {
            out.push_str(&entry_line(e, e.kind.map(str::to_string)));
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
            if !f.beats.is_empty() {
                out.push_str(&format!("      beats: {}\n", f.beats.join(", ")));
            }
            if !f.documents.is_empty() {
                out.push_str(&format!(
                    "      scenes/quests: {}\n",
                    f.documents.join(", ")
                ));
            }
            if !f.components.is_empty() {
                let each: Vec<String> = f
                    .components
                    .iter()
                    .map(|c| format!("{} (via {})", c.component, c.via))
                    .collect();
                out.push_str(&format!("      components: {}\n", each.join(", ")));
            }
        }
    }
    if !r.derived.is_empty() {
        out.push_str("\nDerived (what the rules can conclude from what the project asserts)\n");
    }
    for g in &r.derived {
        out.push_str(&format!("  {}\n", g.relation));
        if g.facts.is_empty() {
            out.push_str(
                "    (nothing — no rule instance follows from what the project asserts)\n",
            );
        }
        for f in &g.facts {
            out.push_str(&format!("    {}\n", f.fact));
            for from in &f.from {
                out.push_str(&format!("      ⇐ {from}\n"));
            }
            if !f.evidence.is_empty() {
                out.push_str(&format!("      evidence: {}\n", f.evidence.join(", ")));
            }
            if f.gates.is_empty() {
                out.push_str("      gates: (no condition reads it)\n");
            } else {
                out.push_str(&format!("      gates: {}\n", f.gates.join(", ")));
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
    // Every file reads and parses independently (desugared as every pass
    // reads it, dsl 0.27.0 §6/§8), then folded in walk order, so messages,
    // early exits, and the report are the sequential ones.
    let parsed: Vec<std::io::Result<(Document, usize)>> = crate::parse_project_docs(dir, &files)
        .into_iter()
        .map(|r| {
            r.map(|(doc, diags)| {
                let errors = diags
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .count();
                (doc, errors)
            })
        })
        .collect();
    // dsl 0.24.0 T3-2: the conclusions the rules can reach, over the same
    // per-root collection `lute scenario knowledge` reads — which also
    // resolves each directive call's declared facts (dsl 0.27.0 §4).
    let (_, by_root) = match crate::collect_project_docs(dir, None, false) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let effects =
        lute_check::directive_facts::root_table(by_root.values().flatten().map(|(_, _, f)| f));
    // The project's components, which assert where they are used.
    let mut components: BTreeMap<&str, &Document> = BTreeMap::new();
    for group in by_root.values() {
        for (name, doc) in crate::knowledge::components(group) {
            components.entry(name).or_insert(doc);
        }
    }
    let mut entries = Vec::new();
    let mut facts = BTreeMap::new();
    for (path, parsed) in files.iter().zip(parsed) {
        let (doc, errors) = match parsed {
            Ok(p) => p,
            Err(e) => {
                eprintln!("lute lore: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        if errors > 0 {
            eprintln!(
                "lute lore: skipping {} — parse failed ({errors} error(s))",
                path.display()
            );
            continue;
        }
        let document = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        fold_document(
            &document,
            &doc,
            &effects,
            &components,
            &mut entries,
            &mut facts,
        );
    }
    let mut report = build_report(entries, facts);
    for k in crate::knowledge::collect(&by_root) {
        report.derived.extend(
            crate::knowledge::derived(&k)
                .into_iter()
                .map(|(relation, facts)| DerivedGroup { relation, facts }),
        );
    }
    let text = if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => format!("{s}\n"),
            Err(e) => {
                eprintln!("lute lore: cannot serialize the report: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        render_text(&report)
    };
    // T3-15: one write through `write_stdout`, so `lute lore … | head` exits
    // `2` on the closed pipe instead of panicking inside `println!`.
    if crate::write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
