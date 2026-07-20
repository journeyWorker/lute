//! `lute loc` — localization export and production word-count reporting.
//!
//! Two read-only surfaces over a project's `.lute` documents, both built on the
//! SAME deterministic file walk `check-project` uses ([`crate::find_lute_files`]
//! — byte-sorted, symlink-deduped) and the SAME syntax-layer parse the checker
//! runs ([`lute_syntax::parse`]). Neither validates: a document that fails to
//! parse (any `Error`-severity parse diagnostic — the exact guard
//! [`lute_check::tag_document`] uses before it will rewrite) is reported to
//! stderr and SKIPPED, never crashed on. `lute_syntax::parse` itself never
//! panics (best-effort AST + diagnostics), so the skip is a policy choice, not a
//! panic guard.
//!
//! ## Translatable units (`export`)
//! Two kinds, both walked in document order (descending into `<branch>`/`<hub>`
//! choice bodies, `<match>` arms, `<objective>`/`<on>` bodies, and quest bodies
//! — mirroring `lute-check`'s own `collect_lines`):
//! - **content lines** (`@speaker: text`, dsl §7.1) — `file`, `line`, the stable
//!   `code` (dsl §12; `null` when the line carries no `code="…"` string attr,
//!   i.e. it has not been through `lute tag`), `speaker`, and `text`.
//! - **choice / hub labels** (dsl §7.3.1/§7.3.2) — `file`, `line`, the `key`
//!   `{branchOrHubId}.{choiceId}` (the SAME join `lute-compile`'s option-label
//!   `lineId` keys on), and the `label` text.
//!
//! The export array is sorted by (`file`, byte offset) so it is byte-identical
//! across runs regardless of directory-iteration order. `--format json`
//! (default) emits a stable JSON array; `--format csv` emits an RFC-4180 file
//! (header row, minimal quoting). An unknown format is a usage error (exit 2).
//! `-o <FILE>` writes the export there; otherwise it goes to stdout. When any
//! exported content line is untagged, a single `N lines untagged — run lute tag`
//! summary is written to stderr (advisory; never changes the exit code).
//!
//! ## Word/line report (`report`)
//! Per-document and per-speaker word counts, total lines, tagged-vs-untagged
//! line counts, and choice-label counts, plus project-wide totals. `--json`
//! emits a stable object; otherwise aligned human tables.
//!
//! ### Word-counting rule
//! A content line's word count is computed from its `text` by first REMOVING the
//! `{{` and `}}` interpolation delimiters (dsl §7.6) — the interior referent
//! text is kept in place — then splitting on Unicode whitespace and counting
//! each maximal run of non-whitespace characters as one word. So
//! `Hello {{@player.name}}!` counts as two words (`Hello`, `@player.name}}`→
//! `@player.name!`). Choice/hub labels are counted as units (their count), not
//! folded into the word totals.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use lute_core_span::Severity;
use lute_syntax::ast::{AttrValue, Attr, Choice, Document, Node, Arm};

/// One translatable unit extracted from a document, carrying the byte offset
/// used to sort the export deterministically.
enum Unit {
    Line {
        file: String,
        line: u32,
        byte: usize,
        code: Option<String>,
        speaker: String,
        text: String,
    },
    Choice {
        file: String,
        line: u32,
        byte: usize,
        key: String,
        label: String,
    },
}

impl Unit {
    fn file(&self) -> &str {
        match self {
            Unit::Line { file, .. } | Unit::Choice { file, .. } => file,
        }
    }
    fn byte(&self) -> usize {
        match self {
            Unit::Line { byte, .. } | Unit::Choice { byte, .. } => *byte,
        }
    }
}

/// A line's authored stable `code` (dsl §12), trimmed to the exact string the
/// addressing pass keys `lineId`/`voiceKey` on — mirrors
/// `lute-check`'s own `authored_code`. `None` when the line has no `code`, or
/// its `code` is not a string literal (an `@ref`/bare value is not a stable
/// code).
fn line_code(attrs: &[Attr]) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.key == "code")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.trim().to_string()),
            _ => None,
        })
}

/// A string-valued attribute (used for a `<hub>`'s `id`, which — unlike a
/// `<branch>` — has no dedicated AST field, dsl §7.3.2).
fn attr_str<'a>(attrs: &'a [Attr], key: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.as_str()),
        _ => None,
    })
}

/// Push one choice/hub label unit plus recurse into its body.
fn walk_choice(file: &str, group_id: &str, choice: &Choice, out: &mut Vec<Unit>) {
    out.push(Unit::Choice {
        file: file.to_string(),
        line: choice.span.line,
        byte: choice.span.byte_start,
        key: format!("{group_id}.{}", choice.id),
        label: choice.label.clone(),
    });
    walk_nodes(file, &choice.body, out);
}

/// Recursively collect translatable units from a node stream in document order
/// (mirrors `lute-check`'s `collect_lines` descent).
fn walk_nodes(file: &str, nodes: &[Node], out: &mut Vec<Unit>) {
    for node in nodes {
        match node {
            Node::Line(l) => out.push(Unit::Line {
                file: file.to_string(),
                line: l.span.line,
                byte: l.span.byte_start,
                code: line_code(&l.attrs),
                speaker: l.speaker.clone(),
                text: l.text.clone(),
            }),
            Node::Branch(b) => {
                for choice in &b.choices {
                    walk_choice(file, &b.id, choice, out);
                }
            }
            Node::Hub(h) => {
                let id = attr_str(&h.attrs, "id").unwrap_or("");
                for choice in &h.choices {
                    walk_choice(file, id, choice, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            walk_nodes(file, body, out)
                        }
                    }
                }
            }
            Node::Objective(o) => walk_nodes(file, &o.body, out),
            Node::On(o) => walk_nodes(file, &o.body, out),
            Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// Collect every translatable unit from one parsed document.
fn document_units(file: &str, doc: &Document, out: &mut Vec<Unit>) {
    for shot in &doc.shots {
        walk_nodes(file, &shot.body, out);
    }
    for quest in &doc.quests {
        walk_nodes(file, &quest.body, out);
    }
}

/// Parse every `.lute` under `dir` (skipping — with a stderr note — any file
/// whose parse produces an `Error`-severity diagnostic) and collect all
/// translatable units, sorted by (`file`, byte offset). `Err(2)` on a walk or
/// read I/O failure. `String` display paths are the byte-sorted walk paths, so
/// the whole result is deterministic.
fn collect_units(dir: &Path) -> Result<Vec<Unit>, ExitCode> {
    let files = crate::find_lute_files(dir).map_err(|e| {
        eprintln!("lute loc: cannot walk {}: {e}", dir.display());
        ExitCode::from(2)
    })?;
    let mut units = Vec::new();
    for path in &files {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("lute loc: cannot read {}: {e}", path.display());
                return Err(ExitCode::from(2));
            }
        };
        let (doc, diags) = lute_syntax::parse(&text);
        let errors = diags.iter().filter(|d| d.severity == Severity::Error).count();
        if errors > 0 {
            eprintln!(
                "lute loc: skipping {} — parse failed ({errors} error(s))",
                path.display()
            );
            continue;
        }
        document_units(&path.display().to_string(), &doc, &mut units);
    }
    units.sort_by(|a, b| a.file().cmp(b.file()).then(a.byte().cmp(&b.byte())));
    Ok(units)
}

/// Extract translatable lines to a localization export. See [`crate::LocCommand::Export`].
pub fn run_export(dir: &Path, format: &str, out: Option<&Path>) -> ExitCode {
    if format != "json" && format != "csv" {
        eprintln!("lute loc export: unknown format `{format}` (expected `json` or `csv`)");
        return ExitCode::from(2);
    }
    let units = match collect_units(dir) {
        Ok(u) => u,
        Err(code) => return code,
    };

    let untagged = units
        .iter()
        .filter(|u| matches!(u, Unit::Line { code: None, .. }))
        .count();

    let rendered = match format {
        "json" => render_json(&units),
        _ => render_csv(&units),
    };

    let write_result = match out {
        Some(path) => std::fs::write(path, rendered.as_bytes()).map_err(|e| {
            eprintln!("lute loc export: cannot write {}: {e}", path.display());
        }),
        None => crate::write_stdout(&rendered).map_err(|_| {}),
    };
    if write_result.is_err() {
        return ExitCode::from(2);
    }

    if untagged > 0 {
        eprintln!("{untagged} lines untagged — run lute tag");
    }
    ExitCode::SUCCESS
}

/// Serialize the export as a stable JSON array (object keys are emitted in
/// `serde_json`'s sorted order — deterministic).
fn render_json(units: &[Unit]) -> String {
    let arr: Vec<serde_json::Value> = units
        .iter()
        .map(|u| match u {
            Unit::Line {
                file,
                line,
                code,
                speaker,
                text,
                ..
            } => serde_json::json!({
                "kind": "line",
                "file": file,
                "line": line,
                "code": code,
                "speaker": speaker,
                "text": text,
            }),
            Unit::Choice {
                file,
                line,
                key,
                label,
                ..
            } => serde_json::json!({
                "kind": "choice",
                "file": file,
                "line": line,
                "key": key,
                "label": label,
            }),
        })
        .collect();
    let mut s = serde_json::to_string_pretty(&serde_json::Value::Array(arr))
        .expect("Value -> JSON serialization is infallible");
    s.push('\n');
    s
}

/// One RFC-4180 field: quote when it contains a comma, quote, CR, or LF;
/// escape an embedded quote by doubling it.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Serialize the export as RFC-4180 CSV — one shared column schema covers both
/// unit kinds (`kind,file,line,code,speaker,key,text`); a line row leaves `key`
/// empty, a choice row leaves `code`/`speaker` empty and carries its label in
/// `text`.
fn render_csv(units: &[Unit]) -> String {
    let mut s = String::from("kind,file,line,code,speaker,key,text\r\n");
    for u in units {
        let row = match u {
            Unit::Line {
                file,
                line,
                code,
                speaker,
                text,
                ..
            } => [
                "line".to_string(),
                file.clone(),
                line.to_string(),
                code.clone().unwrap_or_default(),
                speaker.clone(),
                String::new(),
                text.clone(),
            ],
            Unit::Choice {
                file,
                line,
                key,
                label,
                ..
            } => [
                "choice".to_string(),
                file.clone(),
                line.to_string(),
                String::new(),
                String::new(),
                key.clone(),
                label.clone(),
            ],
        };
        let cells: Vec<String> = row.iter().map(|c| csv_field(c)).collect();
        s.push_str(&cells.join(","));
        s.push_str("\r\n");
    }
    s
}

/// Count words in a content line's `text` per the module's documented rule:
/// remove the `{{`/`}}` interpolation delimiters, then count whitespace-split
/// non-empty tokens.
fn word_count(text: &str) -> usize {
    text.replace("{{", "").replace("}}", "").split_whitespace().count()
}

/// Per-speaker accumulator within one document (or project-wide).
#[derive(Default, Clone)]
struct SpeakerStat {
    lines: usize,
    words: usize,
}

/// One document's aggregated report row.
#[derive(Default)]
struct DocStat {
    lines: usize,
    tagged: usize,
    untagged: usize,
    words: usize,
    choices: usize,
    speakers: BTreeMap<String, SpeakerStat>,
}

/// Word/line-count report per document and speaker. See [`crate::LocCommand::Report`].
pub fn run_report(dir: &Path, json: bool) -> ExitCode {
    let units = match collect_units(dir) {
        Ok(u) => u,
        Err(code) => return code,
    };

    // Aggregate per document (BTreeMap keeps documents in stable path order).
    let mut docs: BTreeMap<String, DocStat> = BTreeMap::new();
    let mut totals = DocStat::default();
    for u in &units {
        let stat = docs.entry(u.file().to_string()).or_default();
        match u {
            Unit::Line {
                code, speaker, text, ..
            } => {
                let words = word_count(text);
                stat.lines += 1;
                stat.words += words;
                if code.is_some() {
                    stat.tagged += 1;
                } else {
                    stat.untagged += 1;
                }
                let sp = stat.speakers.entry(speaker.clone()).or_default();
                sp.lines += 1;
                sp.words += words;

                totals.lines += 1;
                totals.words += words;
                if code.is_some() {
                    totals.tagged += 1;
                } else {
                    totals.untagged += 1;
                }
                let tsp = totals.speakers.entry(speaker.clone()).or_default();
                tsp.lines += 1;
                tsp.words += words;
            }
            Unit::Choice { .. } => {
                stat.choices += 1;
                totals.choices += 1;
            }
        }
    }

    let rendered = if json {
        render_report_json(&docs, &totals)
    } else {
        render_report_human(&docs, &totals)
    };
    if crate::write_stdout(&rendered).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// Serialize the report as a stable JSON object.
fn render_report_json(docs: &BTreeMap<String, DocStat>, totals: &DocStat) -> String {
    let speakers_json = |speakers: &BTreeMap<String, SpeakerStat>| -> Vec<serde_json::Value> {
        speakers
            .iter()
            .map(|(name, s)| {
                serde_json::json!({ "speaker": name, "lines": s.lines, "words": s.words })
            })
            .collect()
    };
    let documents: Vec<serde_json::Value> = docs
        .iter()
        .map(|(file, d)| {
            serde_json::json!({
                "file": file,
                "lines": d.lines,
                "tagged": d.tagged,
                "untagged": d.untagged,
                "words": d.words,
                "choices": d.choices,
                "speakers": speakers_json(&d.speakers),
            })
        })
        .collect();
    let value = serde_json::json!({
        "documents": documents,
        "totals": {
            "documents": docs.len(),
            "lines": totals.lines,
            "tagged": totals.tagged,
            "untagged": totals.untagged,
            "words": totals.words,
            "choices": totals.choices,
            "speakers": speakers_json(&totals.speakers),
        },
    });
    let mut s =
        serde_json::to_string_pretty(&value).expect("Value -> JSON serialization is infallible");
    s.push('\n');
    s
}

/// Render the report as aligned human tables: a per-document summary, a
/// per-speaker project-wide summary, and a totals line.
fn render_report_human(docs: &BTreeMap<String, DocStat>, totals: &DocStat) -> String {
    let mut s = String::new();

    // Per-document table.
    let file_w = docs
        .keys()
        .map(|f| f.len())
        .chain(std::iter::once("document".len()))
        .max()
        .unwrap_or(8);
    s.push_str(&format!(
        "{:<file_w$}  {:>6}  {:>6}  {:>8}  {:>6}  {:>7}\n",
        "document", "lines", "words", "untagged", "tagged", "choices"
    ));
    for (file, d) in docs {
        s.push_str(&format!(
            "{:<file_w$}  {:>6}  {:>6}  {:>8}  {:>6}  {:>7}\n",
            file, d.lines, d.words, d.untagged, d.tagged, d.choices
        ));
    }
    s.push_str(&format!(
        "{:<file_w$}  {:>6}  {:>6}  {:>8}  {:>6}  {:>7}\n",
        "TOTAL", totals.lines, totals.words, totals.untagged, totals.tagged, totals.choices
    ));

    // Per-speaker (project-wide) table.
    if !totals.speakers.is_empty() {
        let sp_w = totals
            .speakers
            .keys()
            .map(|n| n.len())
            .chain(std::iter::once("speaker".len()))
            .max()
            .unwrap_or(7);
        s.push('\n');
        s.push_str(&format!("{:<sp_w$}  {:>6}  {:>6}\n", "speaker", "lines", "words"));
        for (name, sp) in &totals.speakers {
            s.push_str(&format!("{:<sp_w$}  {:>6}  {:>6}\n", name, sp.lines, sp.words));
        }
    }

    s.push_str(&format!(
        "\n{} document(s), {} line(s), {} word(s), {} choice(s)\n",
        docs.len(),
        totals.lines,
        totals.words,
        totals.choices
    ));
    s
}
