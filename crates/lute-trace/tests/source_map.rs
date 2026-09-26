//! Runtime-unification design §5 S3 acceptance (1): `lute_compile::compile_mapped`'s
//! [`SourceMap`] carries every span and authored text today's `lute trace`
//! reports — decision spans, `authoredId` / `authoredGuard`, rendered
//! guards, coverage site keys and totals, and re-read `Skipped` texts — so a
//! trace that executes the IR (S4) can rebuild the same report from the map.
//!
//! Corpus: every `docs/examples/**/*.lute` (project-aware when it sits under
//! a `lute.project.yaml`) and every inline document of this crate's own
//! tests. Each document is traced with the default mock, once per branch /
//! hub option forced by `choose`, and (lore) once per entry re-read and per
//! bundle beat; each report is then checked against the document's map.
//!
//! Lives here, not in `lute-compile`: the D1 quarantine forbids
//! `lute-compile` from naming `lute-trace`, even as a dev-dependency.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lute_check::{CheckInput, Mode};
use lute_compile::source_map::SourceMap;
use lute_core_span::Span;
use lute_manifest::project::{load_project, resolve_document_snapshot, IdentityTemplates};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_trace::{
    report::site_key, trace_beat, trace_document, trace_entry, MockSet, Step, TraceExit,
    TraceReport,
};
use serde_json::Value as Json;

const ZERO: Span = Span {
    byte_start: 0,
    byte_end: 0,
    line: 0,
    column: 0,
    utf16_range: (0, 0),
};

/// Assemble a [`CheckInput`] the way `lute trace --project <root>` does.
fn input_for(text: &str, uri: &str, base: &Path, root: Option<&Path>) -> CheckInput {
    let project = root.and_then(|r| load_project(r).ok().flatten());
    let defaults = project
        .as_ref()
        .map(|p| p.defaults.clone())
        .unwrap_or_default();
    let (doc, _) = lute_syntax::parse(text);
    let (meta, _) = lute_check::meta::parse_meta_kind_with_defaults(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Scene,
        &defaults,
    );
    let (snapshot, _) =
        resolve_document_snapshot(project.as_ref(), meta.profile.as_deref(), &meta.plugins);
    CheckInput {
        text: text.to_string(),
        uri: uri.to_string(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta.uses, &meta.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &meta.components, doc.meta.span),
        defaults,
    }
}

/// One compiled document: its map and its commands by `addr`.
struct Mapped {
    map: SourceMap,
    commands: BTreeMap<String, Json>,
}

impl Mapped {
    fn of(input: &CheckInput) -> Option<Self> {
        let (artifact, map) = lute_compile::compile_mapped(
            input,
            lute_check::check(input),
            &IdentityTemplates::default(),
        )
        .ok()?;
        // The map rides next to the artifact, never in it.
        let plain = lute_compile::compile(input).ok()?;
        assert_eq!(
            serde_json::to_string(&artifact).unwrap(),
            serde_json::to_string(&plain).unwrap(),
            "{}: compile_mapped changed the artifact",
            input.uri
        );
        let json = serde_json::to_value(&artifact).unwrap();
        let commands = json["commands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| (c["addr"].as_str().unwrap().to_string(), c.clone()))
            .collect();
        for addr in map.by_addr.keys() {
            assert!(
                json["commands"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["addr"] == addr.as_str()),
                "{}: mapped addr {addr} is no record",
                input.uri
            );
        }
        assert_eq!(
            map.by_addr.len(),
            json["commands"].as_array().unwrap().len(),
            "{}: every record is mapped",
            input.uri
        );
        Some(Mapped { map, commands })
    }

    /// Records of `kind` whose map entry satisfies `f`.
    fn find(
        &self,
        kind: &str,
        f: impl Fn(&Json, &lute_compile::source_map::SourceInfo) -> bool,
    ) -> Vec<&lute_compile::source_map::SourceInfo> {
        self.map
            .by_addr
            .iter()
            .filter(|(addr, info)| {
                let cmd = &self.commands[*addr];
                cmd["kind"] == kind && f(cmd, info)
            })
            .map(|(_, info)| info)
            .collect()
    }
}

#[derive(Default)]
struct Tally {
    docs: usize,
    reports: usize,
    decisions: usize,
    sites: usize,
    skipped: usize,
    sugar: usize,
    failures: Vec<String>,
}

impl Tally {
    fn fail(&mut self, uri: &str, what: String) {
        self.failures.push(format!("{uri}: {what}"));
    }
}

/// Every source fact `report` states, checked against `m`.
fn check_report(uri: &str, report: &TraceReport, m: &Mapped, t: &mut Tally) {
    t.reports += 1;
    for d in &report.decisions {
        t.decisions += 1;
        let ok = match d.construct.as_str() {
            "match" => {
                let hits = m.find("match", |cmd, info| {
                    info.span == d.span && cmd["subject"] == d.id.as_str()
                });
                hits.iter().any(|info| {
                    let arm_ok = match d.outcome.strip_prefix("arm ") {
                        Some(n) => {
                            let n: usize = n.parse().unwrap();
                            info.arms.get(n - 1).is_some_and(|a| {
                                a.guard == d.guard && a.authored_guard == d.authored_guard
                            })
                        }
                        None => d.guard.is_none() && d.authored_guard.is_none(),
                    };
                    arm_ok && info.authored_id == d.authored_id
                })
            }
            "branch" | "hub" => {
                let (kind, key) = if d.construct == "branch" {
                    ("choice", "branchId")
                } else {
                    ("hub", "id")
                };
                let hits = m.find(kind, |cmd, _| cmd[key] == d.id.as_str());
                d.authored_id.is_none()
                    && hits.iter().any(|info| {
                        info.arms.iter().any(|a| {
                            a.span == d.span
                                && a.guard == d.guard
                                && a.authored_guard == d.authored_guard
                        })
                    })
            }
            "quest" => m.map.quests.get(&d.id).is_some_and(|q| q.span == d.span),
            "objective" => m
                .map
                .quests
                .values()
                .any(|q| q.objectives.get(&d.id).is_some_and(|o| o.span == d.span)),
            "on" => m
                .map
                .quests
                .values()
                .any(|q| q.handlers.values().any(|s| *s == d.span)),
            other => {
                t.fail(uri, format!("unexpected decision construct `{other}`"));
                true
            }
        };
        if !ok {
            t.fail(uri, format!("decision not in the map: {d:?}"));
        }
    }
    for u in &report.unresolved {
        let known = m.map.by_addr.values().any(|i| i.span == u.span)
            || m.map.quests.values().any(|q| {
                q.span == u.span
                    || q.objectives.values().any(|o| o.span == u.span)
                    || q.handlers.values().any(|s| *s == u.span)
            })
            || m.map.entries.values().any(|s| *s == u.span)
            || m.map.beats.values().any(|s| *s == u.span);
        if !known {
            t.fail(uri, format!("unresolved span not in the map: {u:?}"));
        }
    }
    for (key, count) in &report.coverage.arms {
        t.sites += 1;
        let hits = m.find("match", |cmd, info| {
            site_key(&info.span) == *key && cmd["subject"] == count.label.as_str()
        });
        if !hits
            .iter()
            .any(|info| info.arms.len() == count.total && info.authored_id == count.authored_label)
        {
            t.fail(
                uri,
                format!("arm coverage site {key} not in the map: {count:?}"),
            );
        }
    }
    for (id, count) in &report.coverage.choices {
        t.sites += 1;
        let hits: Vec<_> = m
            .find("choice", |cmd, _| cmd["branchId"] == id.as_str())
            .into_iter()
            .chain(m.find("hub", |cmd, _| cmd["id"] == id.as_str()))
            .collect();
        if !hits.iter().any(|info| info.arms.len() == count.total) {
            t.fail(
                uri,
                format!("choice coverage `{id}` not in the map: {count:?}"),
            );
        }
    }
    for step in &report.steps {
        match step {
            Step::Skipped { effect, text } => {
                t.skipped += 1;
                if m.find(effect, |_, info| info.write_text.as_deref() == Some(text))
                    .is_empty()
                {
                    t.fail(uri, format!("skipped `{effect} {text}` not in the map"));
                }
            }
            Step::Set {
                path, sugar: true, ..
            } => {
                t.sugar += 1;
                if m.find("set", |cmd, info| {
                    cmd["path"] == path.as_str() && info.sugar
                })
                .is_empty()
                {
                    t.fail(uri, format!("into-sugar set of `{path}` not in the map"));
                }
            }
            _ => {}
        }
    }
}

fn traced(exit: &TraceExit) -> bool {
    !matches!(exit, TraceExit::Refused(_))
}

/// Trace `input` every way the corpus runs it and check each report.
fn check_document(input: &CheckInput, t: &mut Tally) {
    let Some(m) = Mapped::of(input) else { return };
    t.docs += 1;
    let (doc, _) = lute_syntax::parse(&input.text);
    let uri = input.uri.as_str();
    if doc.entries.is_empty() && doc.beats.is_empty() {
        let (report, exit) = trace_document(input, MockSet::default());
        if traced(&exit) {
            check_report(uri, &report, &m, t);
        }
        // Force every option of every branch / hub once, so each choice's
        // span and guard is reported by some trace.
        for cmd in m.commands.values() {
            let (id, options) = match cmd["kind"].as_str() {
                Some("choice") => (&cmd["branchId"], &cmd["options"]),
                Some("hub") => (&cmd["id"], &cmd["options"]),
                _ => continue,
            };
            for opt in options.as_array().unwrap() {
                let mocks = MockSet {
                    choose: BTreeMap::from([(
                        id.as_str().unwrap().to_string(),
                        vec![opt["id"].as_str().unwrap().to_string()],
                    )]),
                    ..Default::default()
                };
                let (report, exit) = trace_document(input, mocks);
                if traced(&exit) {
                    check_report(uri, &report, &m, t);
                }
            }
        }
        return;
    }
    for entry in &doc.entries {
        for reread in [false, true] {
            let mocks = MockSet {
                state: if reread {
                    vec![(format!("entry.{}.read", entry.id), "true".to_string(), ZERO)]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let (report, exit) = trace_entry(input, mocks, &entry.id);
            if traced(&exit) {
                check_report(uri, &report, &m, t);
            }
        }
    }
    for beat in &doc.beats {
        let (report, exit) = trace_beat(input, MockSet::default(), &beat.id);
        if traced(&exit) {
            check_report(uri, &report, &m, t);
        }
    }
}

fn lute_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            lute_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "lute") {
            out.push(p);
        }
    }
}

/// The nearest `lute.project.yaml` directory at or under `top`.
fn project_root<'a>(file: &'a Path, top: &Path) -> Option<&'a Path> {
    file.ancestors()
        .skip(1)
        .take_while(|d| d.starts_with(top))
        .find(|d| d.join("lute.project.yaml").is_file())
}

/// A Rust string literal's value: `\n`, `\t`, `\"`, `\\`, and a `\` line
/// continuation (which also eats the next line's leading whitespace).
fn unescape(lit: &str) -> String {
    let mut out = String::with_capacity(lit.len());
    let mut chars = lit.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('\'') => out.push('\''),
            Some('\n') => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Every whole document written as one string literal in `src`: a raw
/// `r#"---…"#` or a plain `"---\n…"` literal. A `format!` template is
/// taken when its only braces are `{{` / `}}`.
fn inline_documents(src: &str) -> Vec<String> {
    let mut docs = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find("r#\"---") {
        let body = &rest[at + 3..];
        let Some(end) = body.find("\"#") else { break };
        docs.push(body[..end].to_string());
        rest = &body[end + 2..];
    }
    let mut rest = src;
    while let Some(at) = rest.find("\"---\\n") {
        let body = &rest[at + 1..];
        let mut end = None;
        let mut escaped = false;
        for (i, c) in body.char_indices() {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let text = unescape(&body[..end]);
        let stripped = text.replace("{{", "").replace("}}", "");
        let plain = !stripped.contains('{') && !stripped.contains('}');
        let formatted = rest[..at].trim_end().ends_with("format!(");
        if !formatted || plain {
            let text = if formatted {
                text.replace("{{", "{").replace("}}", "}")
            } else {
                text
            };
            if text.contains("kind:") && text.matches("---").count() >= 2 {
                docs.push(text);
            }
        }
        rest = &body[end + 1..];
    }
    docs
}

#[test]
fn the_source_map_carries_everything_trace_reports() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let examples = crate_dir.join("../../docs/examples");
    let mut t = Tally::default();

    let mut files = Vec::new();
    lute_files(&examples, &mut files);
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        let base = file.parent().unwrap();
        let input = input_for(
            &text,
            &file.display().to_string(),
            base,
            project_root(file, &examples),
        );
        check_document(&input, &mut t);
    }
    let example_docs = t.docs;

    let tests = crate_dir.join("tests");
    let mut inline = 0usize;
    let mut sources: Vec<_> = std::fs::read_dir(&tests)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    sources.sort();
    for src in sources {
        for (i, text) in inline_documents(&std::fs::read_to_string(&src).unwrap())
            .into_iter()
            .enumerate()
        {
            inline += 1;
            let uri = format!("{}#{i}", src.display());
            check_document(&input_for(&text, &uri, &tests, None), &mut t);
        }
    }

    eprintln!(
        "source map: {} documents compiled ({example_docs} docs/examples, {} of {inline} inline), \
         {} reports, {} decisions, {} coverage sites, {} skipped writes, {} into-sugar sets checked",
        t.docs,
        t.docs - example_docs,
        t.reports,
        t.decisions,
        t.sites,
        t.skipped,
        t.sugar
    );
    assert!(
        t.failures.is_empty(),
        "{} mismatch(es):\n{}",
        t.failures.len(),
        t.failures.join("\n")
    );
    // Not vacuous: the corpus exercises every kind of fact the map carries.
    assert!(
        example_docs >= 30,
        "only {example_docs} docs/examples compiled"
    );
    assert!(t.decisions >= 100, "only {} decisions checked", t.decisions);
    assert!(t.skipped > 0 && t.sugar > 0 && t.sites > 0);
}
