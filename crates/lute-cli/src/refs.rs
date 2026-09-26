//! `lute refs` (dsl 0.26.0 §2.5): every value of a directive attribute
//! (`--attr give.item`) or every target of a reward kind (`--reward ITEM`)
//! across a project, with the documents and lines using it — so a lead sees
//! "who gives what" before a merge. Read-only; documents need not check
//! clean (a file that fails to parse is skipped with a note). Exit `0` on
//! success, `2` on an I/O or usage failure.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use lute_syntax::ast::{Arm, AttrValue, ClipNode, Document, Node, Reward};
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
    // query index -> value -> uses
    let mut found: Vec<BTreeMap<Option<String>, Vec<Use>>> =
        targets.iter().map(|_| BTreeMap::new()).collect();
    for (path, doc) in files.iter().zip(parsed) {
        let doc = match doc {
            Ok(d) => d,
            Err(e) => {
                eprintln!("lute refs: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let document = path.strip_prefix(dir).unwrap_or(path).display().to_string();
        for ((_, target), out) in targets.iter().zip(found.iter_mut()) {
            let mut record = |value: Option<String>, line: u32| {
                out.entry(value).or_default().push(Use {
                    document: document.clone(),
                    line,
                });
            };
            match target {
                Target::Attr { directive, attr } => {
                    for_each_body(&doc, &mut |nodes| {
                        attr_values(nodes, directive, attr, &mut record)
                    });
                }
                Target::Reward { kind } => {
                    for quest in &doc.quests {
                        let objective_rewards = quest.body.iter().flat_map(|n| match n {
                            Node::Objective(o) => o.rewards.as_slice(),
                            _ => &[],
                        });
                        for r in quest.rewards.iter().chain(objective_rewards) {
                            if r.kind.trim() == *kind {
                                record(r.target.clone(), reward_line(r));
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
                out.push_str(&format!("    {}:{}\n", u.document, u.line));
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

/// Every literal `attr=` value of a `::directive` in `nodes`, descending into
/// choice, arm, handler and objective bodies and timeline clips. A CEL
/// (`@…`) value names no literal id and is not listed.
fn attr_values(
    nodes: &[Node],
    directive: &str,
    attr: &str,
    record: &mut dyn FnMut(Option<String>, u32),
) {
    let mut directive_value = |d: &lute_syntax::ast::Directive| {
        if d.tag != directive {
            return;
        }
        for a in d.attrs.iter().filter(|a| a.key == attr) {
            if let AttrValue::Str(s) = &a.value {
                record(Some(s.clone()), a.value_span.line);
            }
        }
    };
    let mut nested: Vec<&[Node]> = Vec::new();
    for node in nodes {
        match node {
            Node::Directive(d) => directive_value(d),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        directive_value(d);
                    }
                }
            }
            Node::Match(m) => nested.extend(m.arms.iter().map(|arm| match arm {
                Arm::When { body, .. } | Arm::Otherwise { body, .. } => body.as_slice(),
            })),
            Node::Branch(b) => nested.extend(b.choices.iter().map(|c| c.body.as_slice())),
            Node::Hub(h) => nested.extend(h.choices.iter().map(|c| c.body.as_slice())),
            Node::On(o) => nested.push(&o.body),
            Node::Objective(o) => nested.push(&o.body),
            Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
    for body in nested {
        attr_values(body, directive, attr, record);
    }
}
