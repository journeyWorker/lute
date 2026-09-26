//! `lute refs` (dsl 0.26.0 §2.5): every value of a directive attribute
//! (`--attr give.item`) or every target of a reward kind (`--reward ITEM`)
//! across a project, with the documents and lines using it — so a lead sees
//! "who gives what" before a merge. Read-only; documents need not check
//! clean (a file that fails to parse is skipped with a note). Exit `0` on
//! success, `2` on an I/O or usage failure.
//!
//! A value passed through a component (prerelease N2) is listed at the
//! `::use` binding it: `::use{component="shop" stock="ashgrove"}` over a body
//! `::shop{stock=@stock}` is a use of `ashgrove` for `shop.stock`, at the
//! `::use` line, `via` the component. An omitted argument's literal
//! `default:` is bound there too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_syntax::ast::{Arm, AttrValue, ClipNode, Directive, Document, Node, Reward};
use rayon::prelude::*;
use serde::Serialize;

/// One `--attr` / `--reward` query and what it found.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Query {
    /// `attr` (`<directive>.<attr>`) or `reward` (a reward kind).
    kind: &'static str,
    /// The query as written: `give.item`, `ITEM`.
    name: String,
    /// Every value, sorted; a reward with no `target=` is `value: null`.
    values: Vec<ValueUses>,
}

#[derive(Serialize)]
struct ValueUses {
    value: Option<String>,
    uses: Vec<Use>,
}

#[derive(Serialize, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Use {
    document: String,
    line: u32,
    /// The component a `::use` passes the value through (prerelease N2).
    #[serde(skip_serializing_if = "Option::is_none")]
    via: Option<String>,
}

#[derive(Serialize)]
struct Report {
    queries: Vec<Query>,
}

/// What one query collects.
enum Target<'q> {
    Attr { directive: &'q str, attr: &'q str },
    Reward { kind: &'q str },
}

/// Run `lute refs`. See the module doc.
pub fn run_refs(dir: &Path, attrs: &[String], rewards: &[String], json: bool) -> ExitCode {
    if attrs.is_empty() && rewards.is_empty() {
        eprintln!(
            "lute refs: name what to list with `--attr <directive>.<attr>` or `--reward <KIND>`"
        );
        return ExitCode::from(2);
    }
    let mut targets = Vec::new();
    for a in attrs {
        match a.split_once('.') {
            Some((directive, attr)) if !directive.is_empty() && !attr.is_empty() => {
                let directive = directive.strip_prefix("::").unwrap_or(directive);
                targets.push((a.clone(), Target::Attr { directive, attr }));
            }
            _ => {
                eprintln!("lute refs: `--attr {a}` is not `<directive>.<attr>` (e.g. `give.item`)");
                return ExitCode::from(2);
            }
        }
    }
    for kind in rewards {
        targets.push((kind.clone(), Target::Reward { kind }));
    }

    let files = match crate::find_lute_files(dir) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("lute refs: cannot walk {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };
    let parsed: Vec<std::io::Result<Document>> = files
        .par_iter()
        .map(|path| std::fs::read_to_string(path).map(|text| lute_syntax::parse(&text).0))
        .collect();
    let mut docs = Vec::with_capacity(files.len());
    for (path, doc) in files.iter().zip(parsed) {
        match doc {
            Ok(d) => docs.push((path, d)),
            Err(e) => {
                eprintln!("lute refs: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        }
    }
    let comps = components(&docs);
    // query index -> value -> uses
    let mut found: Vec<BTreeMap<Option<String>, Vec<Use>>> =
        targets.iter().map(|_| BTreeMap::new()).collect();
    for (path, doc) in &docs {
        let document = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        for ((_, target), out) in targets.iter().zip(found.iter_mut()) {
            let mut record = |value: Option<String>, line: u32, via: Option<&str>| {
                out.entry(value).or_default().push(Use {
                    document: document.clone(),
                    line,
                    via: via.map(str::to_string),
                });
            };
            match target {
                Target::Attr { directive, attr } => {
                    let sink = Sink {
                        directive,
                        attr,
                        comps: &comps,
                    };
                    for_each_body(doc, &mut |nodes| attr_values(nodes, &sink, &mut record));
                }
                Target::Reward { kind } => {
                    for quest in &doc.quests {
                        let objective_rewards = quest.body.iter().flat_map(|n| match n {
                            Node::Objective(o) => o.rewards.as_slice(),
                            _ => &[],
                        });
                        for r in quest.rewards.iter().chain(objective_rewards) {
                            if r.kind.trim() == *kind {
                                record(r.target.clone(), reward_line(r), None);
                            }
                        }
                    }
                }
            }
        }
    }

    let report = Report {
        queries: targets
            .iter()
            .zip(found)
            .map(|((name, target), values)| Query {
                kind: match target {
                    Target::Attr { .. } => "attr",
                    Target::Reward { .. } => "reward",
                },
                name: name.clone(),
                values: values
                    .into_iter()
                    .map(|(value, mut uses)| {
                        uses.sort();
                        ValueUses { value, uses }
                    })
                    .collect(),
            })
            .collect(),
    };
    let text = if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => format!("{s}\n"),
            Err(e) => {
                eprintln!("lute refs: cannot serialize the report: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        render_text(&report)
    };
    if crate::write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn reward_line(r: &Reward) -> u32 {
    r.target_span.map_or(r.span.line, |s| s.line)
}

fn render_text(r: &Report) -> String {
    let mut out = String::new();
    for q in &r.queries {
        let head = match q.kind {
            "reward" => format!("reward {}", q.name),
            _ => format!("::{}", q.name),
        };
        if q.values.is_empty() {
            out.push_str(&format!("{head}: no uses\n"));
            continue;
        }
        out.push_str(&format!("{head}: {} value(s)\n", q.values.len()));
        for v in &q.values {
            let value = v
                .value
                .as_deref()
                .map_or("(no target)".to_string(), |s| format!("`{s}`"));
            let docs = {
                let mut d: Vec<&str> = v.uses.iter().map(|u| u.document.as_str()).collect();
                d.dedup();
                d.len()
            };
            out.push_str(&format!(
                "  {value} — {} use(s) in {docs} document(s)\n",
                v.uses.len()
            ));
            for u in &v.uses {
                match &u.via {
                    Some(c) => out.push_str(&format!(
                        "    {}:{} (via component `{c}`)\n",
                        u.document, u.line
                    )),
                    None => out.push_str(&format!("    {}:{}\n", u.document, u.line)),
                }
            }
        }
    }
    out
}

/// Every top-level node body of `doc`: shots, quests, entries, bundle beats.
fn for_each_body(doc: &Document, f: &mut dyn FnMut(&[Node])) {
    for shot in &doc.shots {
        f(&shot.body);
    }
    for quest in &doc.quests {
        f(&quest.body);
    }
    for entry in &doc.entries {
        f(&entry.body);
    }
    for beat in &doc.beats {
        f(&beat.body);
    }
}

/// One component the project declares: its params in order, each literal
/// `default:`, and its body.
struct Component<'d> {
    params: Vec<String>,
    defaults: BTreeMap<String, String>,
    body: &'d Document,
}

/// Every document declaring `component:`, by name (the first file in path
/// order wins a duplicate name, as `E-COMPONENT-DUP` reports).
fn components<'d>(docs: &'d [(&PathBuf, Document)]) -> BTreeMap<String, Component<'d>> {
    let mut out = BTreeMap::new();
    for (_, doc) in docs {
        if !doc
            .meta
            .raw_yaml
            .lines()
            .any(|l| l.trim_end().starts_with("component:"))
        {
            continue;
        }
        let (tm, _) = lute_check::parse_meta_kind(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
            lute_check::MetaKind::Component,
        );
        let Some(name) = tm.component else {
            continue;
        };
        out.entry(name).or_insert_with(|| Component {
            params: tm.params.iter().map(|p| p.name.clone()).collect(),
            defaults: tm
                .param_defaults
                .iter()
                .filter_map(|(p, v)| match v {
                    AttrValue::Str(s) => Some((p.clone(), s.clone())),
                    _ => None,
                })
                .collect(),
            body: doc,
        });
    }
    out
}

/// What an `--attr <directive>.<attr>` query lists, with the project's
/// components to see through.
struct Sink<'q> {
    directive: &'q str,
    attr: &'q str,
    comps: &'q BTreeMap<String, Component<'q>>,
}

impl Sink<'_> {
    /// Does component `comp` pass its param `param` whole to the queried
    /// attribute — directly, or through a nested `::use`? `seen` guards a
    /// cycle.
    fn reached(&self, comp: &str, param: &str, seen: &mut Vec<(String, String)>) -> bool {
        if seen.iter().any(|(c, p)| c == comp && p == param) {
            return false;
        }
        seen.push((comp.to_string(), param.to_string()));
        let Some(c) = self.comps.get(comp) else {
            return false;
        };
        let mut hit = false;
        for_each_body(c.body, &mut |nodes| {
            for_each_directive(nodes, &mut |d| {
                if hit {
                    return;
                }
                let passed = |key: &str| {
                    d.attrs.iter().any(|a| {
                        a.key == key
                            && matches!(&a.value, AttrValue::Ref(s) if s.raw.trim().strip_prefix('@') == Some(param))
                    })
                };
                if d.tag == self.directive && passed(self.attr) {
                    hit = true;
                } else if let Some(inner) = use_component(d) {
                    let keys: Vec<String> = d
                        .attrs
                        .iter()
                        .filter(|a| a.key != "component" && passed(&a.key))
                        .map(|a| a.key.clone())
                        .collect();
                    hit = keys.iter().any(|k| self.reached(inner, k, seen));
                }
            })
        });
        hit
    }
}

/// The component a `::use` names, as written.
fn use_component(d: &Directive) -> Option<&str> {
    (d.tag == "use")
        .then(|| {
            d.attrs.iter().find_map(|a| match &a.value {
                AttrValue::Str(s) if a.key == "component" => Some(s.as_str()),
                _ => None,
            })
        })
        .flatten()
}

/// Every `::directive` in `nodes`, descending into choice, arm, handler and
/// objective bodies and timeline clips.
fn for_each_directive<'n>(nodes: &'n [Node], f: &mut dyn FnMut(&'n Directive)) {
    for node in nodes {
        match node {
            Node::Directive(d) => f(d),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        f(d);
                    }
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    for_each_directive(body, f);
                }
            }
            Node::Branch(b) => b
                .choices
                .iter()
                .for_each(|c| for_each_directive(&c.body, f)),
            Node::Hub(h) => h
                .choices
                .iter()
                .for_each(|c| for_each_directive(&c.body, f)),
            Node::On(o) => for_each_directive(&o.body, f),
            Node::Objective(o) => for_each_directive(&o.body, f),
            Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// Every literal value of the queried attribute in `nodes`: a
/// `::directive{attr="…"}` written in place, and a `::use` argument (or
/// literal param `default:`) its component passes whole to that attribute,
/// recorded at the `::use` with the component. A CEL (`@…`) value names no
/// literal id and is not listed.
fn attr_values(
    nodes: &[Node],
    sink: &Sink<'_>,
    record: &mut dyn FnMut(Option<String>, u32, Option<&str>),
) {
    for_each_directive(nodes, &mut |d| {
        if d.tag == sink.directive {
            for a in d.attrs.iter().filter(|a| a.key == sink.attr) {
                if let AttrValue::Str(s) = &a.value {
                    record(Some(s.clone()), a.value_span.line, None);
                }
            }
        }
        let Some((name, comp)) = use_component(d).and_then(|n| sink.comps.get(n).map(|c| (n, c)))
        else {
            return;
        };
        for param in &comp.params {
            let bound = match d.attrs.iter().find(|a| &a.key == param) {
                Some(a) => match &a.value {
                    AttrValue::Str(s) => Some((s.as_str(), a.value_span.line)),
                    _ => None,
                },
                None => comp.defaults.get(param).map(|s| (s.as_str(), d.span.line)),
            };
            if let Some((value, line)) = bound {
                if sink.reached(name, param, &mut Vec::new()) {
                    record(Some(value.to_string()), line, Some(name));
                }
            }
        }
    });
}
