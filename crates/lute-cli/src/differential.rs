//! The differential test (docs/design/runtime-unification.md §4): every
//! document of the corpus runs through both runtimes with concrete inputs —
//! trace (`lute_trace::trace_*_with_check`, the AST walker) and run (the
//! reference Machine over the compiled IR) — and the two [`Observation`]s
//! must agree on the transcript, the final state, the holding facts, the
//! quest states and the exit.
//!
//! Corpus (§4.2): `docs/examples/**`, `conformance/*/`, and
//! `crates/lute-cli/tests/fixtures/**` (the round-5 repros under
//! `fixtures/diff/`) always; the game projects under every directory of the
//! colon-separated `LUTE_DIFF_CORPUS` on request. Inputs (§4.3): per
//! document the empty mock, every `*.test.yaml` and every `mocks/*.yaml`
//! naming it (a conformance fixture's `mock.yaml`); a lore document is cased
//! per `<entry>` and per bundle `<beat>`. Trace runs first: an incomplete
//! walk, a refused mock, an unanswered bridge or a pick forced past an
//! unknown guard is not a concrete input and is skipped; otherwise trace's
//! branch/hub picks are copied into `choose:` and both runtimes run that
//! mock. Play level: every presentation of every `*.play.yaml` is replayed
//! through trace from the world the play presented it in.
//!
//! Known divergences are pinned, one per `(case, field)`, in
//! `differential/allowlist.txt`. The test fails on an unlisted divergence
//! and on a listed one that no longer diverges, so the list only shrinks.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_check::{CheckInput, CheckResult};
use lute_core_span::Severity;
use lute_trace::{MockSet, Step, TraceExit, TraceReport, Value};
use rayon::prelude::*;
use serde_json::Value as Json;

use lute_trace::datalog::Fact;
use lute_trace::exec::{Carry, Machine, Seed};

use crate::runner::{run_machine, RunDriver};

/// The allowlist of known divergences (§4.5): `<case id> <field> <class>
/// <slice> <reason…>`, whitespace-separated, `#` comments.
const ALLOWLIST: &str = include_str!("differential/allowlist.txt");

/// The in-repo cases the first S1 run compared (§4.2). A run comparing fewer
/// than 90% of this fails: a broken enumerator must not pass vacuously.
const COMPARED_FLOOR: usize = 201;

/// What one runtime observed for one case (§4.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Observation {
    /// Canonical transcript lines, `@speaker: text`. Attribute blocks join
    /// once both runtimes report them (design §8, S1 notes).
    pub(crate) said: Vec<String>,
    /// Every state path the runtime reports a value for (quest `state` paths
    /// live in `quests`). [`compare`] fills a path only one side reports
    /// from the declared default.
    pub(crate) state: BTreeMap<String, String>,
    /// Holding facts, base and derived, rendered `rel(a, b)`.
    pub(crate) facts: BTreeSet<String>,
    /// Quest id -> `unset | active | complete | failed`.
    pub(crate) quests: BTreeMap<String, String>,
    /// `complete | ended | incomplete | refused:<code> | fatal`.
    pub(crate) exit: String,
}

/// One comparison (§4.3).
pub(crate) enum Case {
    /// A document under one concrete input.
    Doc(DocCase),
    /// One presentation a play made, replayed through trace.
    Presentation(PresentationCase),
}

pub(crate) struct DocCase {
    pub(crate) id: String,
    subject: Arc<Subject>,
    present: Present,
    mock: MockSet,
}

pub(crate) struct PresentationCase {
    pub(crate) id: String,
    subject: Arc<Subject>,
    present: Present,
    /// The trace mock rebuilt from the world the play presented in.
    mock: MockSet,
    /// What the play itself observed, already restricted to `keys`.
    played: Observation,
    /// The state paths a presentation is judged on: the document's declared
    /// non-`scene.*` paths and its entries' `entry.<id>.everRead`.
    keys: BTreeSet<String>,
    /// Relations whose facts a presentation is judged on (base relations —
    /// the play records base facts only).
    base_relations: BTreeSet<String>,
    /// Why no trace mock can express the world the play presented in.
    unrepresentable: Option<String>,
}

impl Case {
    pub(crate) fn id(&self) -> &str {
        match self {
            Case::Doc(c) => &c.id,
            Case::Presentation(c) => &c.id,
        }
    }
}

/// What the walk presents.
#[derive(Clone, Debug)]
enum Present {
    Document,
    Entries(Vec<String>),
    Beat(String),
}

/// One document, gated and compiled once for all its cases.
struct Subject {
    /// Corpus-relative display path — the case id's prefix.
    rel: String,
    input: CheckInput,
    gate: CheckResult,
    /// The compiled artifact, or the first error code compile refused with.
    artifact: Result<Json, String>,
    lore: bool,
    entries: Vec<String>,
    beats: Vec<String>,
    /// Declared state path -> default, rendered (the artifact's `state[]`).
    defaults: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// The two observations.
// ---------------------------------------------------------------------------

/// Trace's observation (today `lute_trace::trace_*_with_check`).
pub(crate) fn observe_trace(case: &Case) -> Observation {
    match case {
        Case::Doc(c) => trace_observation(&trace(&c.subject, &c.present, &c.mock)),
        Case::Presentation(c) => {
            let mut obs = trace_observation(&trace(&c.subject, &c.present, &c.mock));
            restrict(&mut obs, &c.keys, &c.base_relations);
            obs
        }
    }
}

/// Run's observation (`lute run`'s Machine + [`RunDriver`] over
/// `compile_with_check`'s artifact, presenting entries / a bundle beat);
/// for a presentation, what the play recorded.
pub(crate) fn observe_run(case: &Case) -> Observation {
    let c = match case {
        Case::Doc(c) => c,
        Case::Presentation(c) => return c.played.clone(),
    };
    let art = match &c.subject.artifact {
        Ok(art) => art,
        Err(code) => {
            return Observation {
                exit: format!("refused:{code}"),
                ..Observation::default()
            }
        }
    };
    let mut transcript: Vec<Json> = Vec::new();
    let (state, facts, quests, flags) = match &c.present {
        Present::Entries(ids) if ids.len() > 1 => {
            // Each later entry resumes the world the previous one left,
            // `lute test`'s `entries:` rule; the mock seeds apply once.
            let mut carry: Option<(BTreeMap<String, Value>, BTreeSet<_>, BTreeMap<_, _>)> = None;
            let mut last = None;
            for id in ids {
                let m = match carry.take() {
                    None => run_machine(art, &c.mock, None, None),
                    Some((s, f, q)) => {
                        let mut mock = c.mock.clone();
                        mock.state.clear();
                        mock.facts.clear();
                        let driver = RunDriver::from_mock(&mock);
                        Machine::resume(art, Seed::from(&mock), Carry::world(s, f, q), driver)
                    }
                };
                let r = run_one(m.with_entry(id));
                transcript.extend(r.transcript.iter().cloned());
                let stop = r.flags.stops();
                carry = Some((r.state.clone(), r.base_facts.clone(), r.quests.clone()));
                last = Some(r);
                if stop {
                    break;
                }
            }
            let r = last.expect("at least two entries");
            (r.state, r.facts, r.quests, r.flags)
        }
        present => {
            let (entry, beat) = match present {
                Present::Document => (None, None),
                Present::Entries(ids) => (Some(ids[0].as_str()), None),
                Present::Beat(id) => (None, Some(id.as_str())),
            };
            let r = run_one(run_machine(art, &c.mock, entry, beat));
            transcript = r.transcript;
            (r.state, r.facts, r.quests, r.flags)
        }
    };
    let mut obs = Observation {
        said: transcript.iter().filter_map(record_line).collect(),
        facts,
        exit: flags.exit(&transcript),
        ..Observation::default()
    };
    for (path, v) in &state {
        match quest_of_state_path(path) {
            Some(id) => {
                obs.quests.insert(id.to_string(), value_text(v));
            }
            None => {
                obs.state.insert(path.clone(), value_text(v));
            }
        }
    }
    obs.quests.extend(quests);
    obs.state.remove(lute_check::beats::OCCASION_TARGET);
    obs
}

struct RunFlags {
    error: Option<String>,
    refused: bool,
    incomplete: bool,
}

impl RunFlags {
    fn stops(&self) -> bool {
        self.error.is_some() || self.incomplete
    }

    fn exit(&self, transcript: &[Json]) -> String {
        if let Some(msg) = &self.error {
            return if self.refused {
                format!("refused:{}", bracketed_code(msg).unwrap_or("?"))
            } else {
                "fatal".to_string()
            };
        }
        if self.incomplete {
            "incomplete".to_string()
        } else if transcript
            .iter()
            .any(|r| r.get("kind").and_then(Json::as_str) == Some("end"))
        {
            "ended".to_string()
        } else {
            "complete".to_string()
        }
    }
}

struct RunResult {
    transcript: Vec<Json>,
    state: BTreeMap<String, Value>,
    base_facts: BTreeSet<Fact>,
    facts: BTreeSet<String>,
    quests: BTreeMap<String, String>,
    flags: RunFlags,
}

fn run_one(mut m: Machine<RunDriver>) -> RunResult {
    let result = m.run();
    let facts = m.all_facts().iter().map(render_fact).collect();
    let (out, driver) = m.into_carry();
    RunResult {
        transcript: driver.transcript,
        state: out.state,
        base_facts: out.base_facts,
        facts,
        quests: out.quest_status,
        flags: RunFlags {
            error: result.err(),
            refused: out.refused,
            incomplete: out.incomplete,
        },
    }
}

fn trace(s: &Subject, present: &Present, mock: &MockSet) -> (TraceReport, TraceExit) {
    let (input, gate, mock) = (&s.input, s.gate.clone(), mock.clone());
    match present {
        Present::Document => lute_trace::trace_with_check(input, gate, mock, None),
        Present::Entries(ids) => {
            let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
            lute_trace::trace_entries_with_check(input, gate, mock, &ids, None)
        }
        Present::Beat(id) => lute_trace::trace_beat_with_check(input, gate, mock, id, None),
    }
}

fn trace_observation((report, exit): &(TraceReport, TraceExit)) -> Observation {
    let mut obs = Observation {
        said: report
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Line { speaker, text, .. } => Some(said_line(speaker, text)),
                _ => None,
            })
            .collect(),
        facts: report.final_facts.clone(),
        exit: match exit {
            TraceExit::Complete if report.disposition == "ended" => "ended".to_string(),
            TraceExit::Complete => "complete".to_string(),
            TraceExit::Incomplete => "incomplete".to_string(),
            TraceExit::Refused(diags) => format!("refused:{}", first_error(diags)),
        },
        ..Observation::default()
    };
    for (path, v) in &report.final_state {
        match quest_of_state_path(path) {
            Some(id) => {
                obs.quests.insert(id.to_string(), v.clone());
            }
            None => {
                obs.state.insert(path.clone(), v.clone());
            }
        }
    }
    obs.state.remove(lute_check::beats::OCCASION_TARGET);
    obs
}

/// A presentation is judged on its declared persistent paths and base facts
/// only; quests belong to the play's quest advance, not the presentation.
fn restrict(obs: &mut Observation, keys: &BTreeSet<String>, base: &BTreeSet<String>) {
    obs.state.retain(|k, _| keys.contains(k));
    obs.facts.retain(|f| base.contains(relation_of(f)));
    obs.quests.clear();
}

// ---------------------------------------------------------------------------
// Comparison.
// ---------------------------------------------------------------------------

/// One `(case, field)` on which the runtimes disagree.
#[derive(Debug)]
struct Divergence {
    case: String,
    field: &'static str,
    detail: String,
}

/// The fields of `trace` and `run` that differ. A state path only one side
/// reports is filled from `defaults` (an `entry.<id>.read` / `.everRead`
/// from its reserved `false`, else `<unset>`); a quest only one side reports
/// is `unset`.
fn compare(
    case: &str,
    trace: &Observation,
    run: &Observation,
    defaults: &BTreeMap<String, String>,
) -> Vec<Divergence> {
    let mut out = Vec::new();
    let mut push = |field, detail: String| {
        out.push(Divergence {
            case: case.to_string(),
            field,
            detail,
        })
    };
    if trace.said != run.said {
        push("said", said_diff(&trace.said, &run.said));
    }
    // `entry.<id>.read` / `.everRead` are engine-written, default `false`.
    let fill = |m: &BTreeMap<String, String>, k: &str, dflt: Option<&String>| {
        m.get(k).or(dflt).cloned().unwrap_or_else(|| {
            let entry_flag =
                k.starts_with("entry.") && (k.ends_with(".read") || k.ends_with(".everRead"));
            if entry_flag { "false" } else { "<unset>" }.to_string()
        })
    };
    let state: Vec<String> = trace
        .state
        .keys()
        .chain(run.state.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|k| {
            let (t, r) = (
                fill(&trace.state, k, defaults.get(k)),
                fill(&run.state, k, defaults.get(k)),
            );
            (t != r).then(|| format!("{k}: trace={t} run={r}"))
        })
        .collect();
    if !state.is_empty() {
        push("state", state.join("; "));
    }
    if trace.facts != run.facts {
        let only = |a: &BTreeSet<String>, b: &BTreeSet<String>| {
            a.difference(b).cloned().collect::<Vec<_>>().join(", ")
        };
        push(
            "facts",
            format!(
                "trace only [{}]; run only [{}]",
                only(&trace.facts, &run.facts),
                only(&run.facts, &trace.facts)
            ),
        );
    }
    let unset = "unset".to_string();
    let quests: Vec<String> = trace
        .quests
        .keys()
        .chain(run.quests.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|k| {
            let (t, r) = (
                trace.quests.get(k).unwrap_or(&unset),
                run.quests.get(k).unwrap_or(&unset),
            );
            (t != r).then(|| format!("{k}: trace={t} run={r}"))
        })
        .collect();
    if !quests.is_empty() {
        push("quests", quests.join("; "));
    }
    if trace.exit != run.exit {
        push("exit", format!("trace={} run={}", trace.exit, run.exit));
    }
    out
}

fn said_diff(trace: &[String], run: &[String]) -> String {
    let at = trace
        .iter()
        .zip(run)
        .position(|(a, b)| a != b)
        .unwrap_or(trace.len().min(run.len()));
    let show = |v: &[String]| v.get(at).map_or("<end>".to_string(), |l| format!("{l:?}"));
    format!(
        "first difference at line {}: trace {} / run {} ({} vs {} lines)",
        at + 1,
        show(trace),
        show(run),
        trace.len(),
        run.len()
    )
}

// ---------------------------------------------------------------------------
// Concretization (§4.3).
// ---------------------------------------------------------------------------

/// Why a case was not compared.
enum Skip {
    /// The document does not pass its check gate: neither runtime runs it.
    CheckRefused,
    /// The input does not decide the walk (§4.3 step 1).
    NotConcrete(String),
}

/// Make every decision of a document case explicit, or say why the input is
/// not concrete.
fn concretize(c: &DocCase) -> Result<MockSet, Skip> {
    if !c.subject.gate.ok {
        return Err(Skip::CheckRefused);
    }
    let (report, exit) = trace(&c.subject, &c.present, &c.mock);
    match &exit {
        TraceExit::Incomplete => return Err(Skip::NotConcrete("trace incomplete".into())),
        TraceExit::Refused(diags) => {
            let code = first_error(diags);
            // A mock the document rejects is not an input to compare under,
            // and neither is a scripted pick of an option that is not offered:
            // what a runtime does with one is its driver's `forced` policy,
            // which differs by design (§3.3). A walk-time refusal of another
            // kind (`E-FACT-EXCLUSIVE`) is compared.
            if code == lute_trace::E_TRACE_CHOICE {
                return Err(Skip::NotConcrete("a scripted pick is not offered".into()));
            }
            if code.starts_with("E-TRACE-") {
                return Err(Skip::NotConcrete(format!("mock refused ({code})")));
            }
        }
        TraceExit::Complete => {}
    }
    if !report.forced_unknown.is_empty() {
        return Err(Skip::NotConcrete(
            "a pick forced past an unknown guard".into(),
        ));
    }
    if report
        .steps
        .iter()
        .any(|s| matches!(s, Step::Bridge { answered: None, .. }))
    {
        return Err(Skip::NotConcrete("an unanswered bridge".into()));
    }
    let mut mock = c.mock.clone();
    let mut choose: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for d in &report.decisions {
        if d.construct == "branch" || d.construct == "hub" {
            choose
                .entry(d.id.clone())
                .or_default()
                .push(d.outcome.clone());
        }
    }
    mock.choose = choose;
    Ok(mock)
}

// ---------------------------------------------------------------------------
// Corpus enumeration (§4.2).
// ---------------------------------------------------------------------------

/// A directory the corpus walks, and how its documents are named.
struct Root {
    dir: PathBuf,
    /// Prefix of every case id under `dir`.
    label: String,
    /// Opt-in corpus: only documents inside a `lute.project.yaml` root count,
    /// named `corpus:<project dir name>/<path in project>`.
    opt_in: bool,
}

#[derive(Default)]
struct Files {
    docs: Vec<PathBuf>,
    inputs: Vec<PathBuf>,
    plays: Vec<PathBuf>,
}

fn walk(dir: &Path, files: &mut Files) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if !name.starts_with('.') && name != "node_modules" && name != "target" {
                walk(&path, files);
            }
        } else if name.ends_with(".lute") && !crate::compile_all::is_component_file(&path) {
            files.docs.push(path);
        } else if name.ends_with(".play.yaml") {
            files.plays.push(path);
        } else if name.ends_with(".test.yaml")
            || (name.ends_with(".yaml")
                && path.parent().and_then(Path::file_name) == Some("mocks".as_ref()))
            || (name == "mock.yaml" && path.with_file_name("source.lute").is_file())
        {
            files.inputs.push(path);
        }
    }
}

/// The nearest directory at or above `from` (never above `root`) holding a
/// `lute.project.yaml`.
fn project_root(from: &Path, root: &Path) -> Option<PathBuf> {
    let mut dir = from.parent();
    while let Some(d) = dir {
        if !d.starts_with(root) {
            return None;
        }
        if d.join("lute.project.yaml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

fn rel_slash(path: &Path, base: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Every document's display name under `root`, or `None` when an opt-in
/// document lies outside every project.
fn display_name(root: &Root, doc: &Path) -> Option<String> {
    if root.opt_in {
        let proj = project_root(doc, &root.dir)?;
        let name = proj.file_name()?.to_string_lossy().into_owned();
        Some(format!("corpus:{name}/{}", rel_slash(doc, &proj)))
    } else {
        Some(format!("{}/{}", root.label, rel_slash(doc, &root.dir)))
    }
}

/// Per project root, the reconciled project analysis (`None` when it
/// failed). Filled up front, outside any parallel section: the analysis runs
/// its own rayon work, and a lock held across it would deadlock a worker.
#[derive(Default)]
struct Gates {
    projects: HashMap<PathBuf, Option<crate::ReconciledProject>>,
}

impl Gates {
    fn add(&mut self, dir: &Path) {
        if !self.projects.contains_key(dir) {
            let rec = crate::reconciled_project_results(dir, None).ok();
            self.projects.insert(dir.to_path_buf(), rec);
        }
    }

    fn project(&self, dir: &Path) -> Option<&crate::ReconciledProject> {
        self.projects.get(dir)?.as_ref()
    }

    /// `file`'s gate: its project's reconciled verdict (`gate_for_doc`, the
    /// verdict `lute play` / `lute compile --project` gate on), else the
    /// standalone check.
    fn gate(&self, file: &Path, project: Option<&Path>, input: &CheckInput) -> CheckResult {
        let canon = std::fs::canonicalize(file).ok();
        if let Some(rec) = project.and_then(|p| self.project(p)) {
            let hit = rec
                .per_doc
                .iter()
                .find(|(path, _)| canon.is_some() && std::fs::canonicalize(path).ok() == canon);
            if let Some((key, base)) = hit {
                return crate::gate_for_doc(rec, key, base);
            }
        }
        lute_check::check(input)
    }
}

fn prepare(file: &Path, rel: String, project: Option<&Path>, gates: &Gates) -> Option<Subject> {
    let built = crate::build_input(file, None, project, None)?;
    let crate::BuiltInput {
        input, identity, ..
    } = built;
    let gate = gates.gate(file, project, &input);
    let artifact = if gate.ok {
        match lute_compile::compile_with_check(&input, gate.clone(), &identity) {
            Ok(a) => Ok(serde_json::to_value(&a).expect("an artifact serializes")),
            Err(diags) => Err(first_error(&diags)),
        }
    } else {
        Err(first_error(&gate.diagnostics))
    };
    let (doc, _) = lute_syntax::parse(&input.text);
    let lore = lute_check::fold_env(&doc, &input).0.doc_kind == lute_check::DocKind::Lore;
    let defaults = artifact
        .as_ref()
        .ok()
        .and_then(|a| a.get("state"))
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let path = e.get("path")?.as_str()?.to_string();
            let d = e.get("default").map(json_text).unwrap_or_default();
            Some((path, d))
        })
        .collect();
    Some(Subject {
        rel,
        entries: doc.entries.iter().map(|e| e.id.clone()).collect(),
        beats: doc.beats.iter().map(|b| b.id.clone()).collect(),
        input,
        gate,
        artifact,
        lore,
        defaults,
    })
}

/// One input read from a `*.test.yaml` / mock file.
struct Input {
    id: String,
    mock: MockSet,
    present: Option<Present>,
}

fn read_input(path: &Path) -> Option<(PathBuf, Input)> {
    let text = std::fs::read_to_string(path).ok()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    // A conformance `mock.yaml` names no `file:`; its subject is the
    // fixture's `source.lute`.
    let subject = match lute_trace::mock_subject(&text).ok()? {
        Some(s) => s,
        None if name == "mock.yaml" => "source.lute".to_string(),
        None => return None,
    };
    let doc = std::fs::canonicalize(path.parent()?.join(&subject)).ok()?;
    let is_test = name.ends_with(".test.yaml");
    let mock = if is_test {
        lute_trace::parse_mock_surfaces(&text).ok()?
    } else {
        lute_trace::parse_mock_yaml(&text).ok()?
    };
    let yaml: serde_yaml::Value = serde_yaml::from_str(&text).ok()?;
    let present = match (yaml.get("entry"), yaml.get("entries"), yaml.get("beat")) {
        (Some(e), _, _) => Some(Present::Entries(vec![e.as_str()?.to_string()])),
        (_, Some(es), _) => Some(Present::Entries(
            es.as_sequence()?
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect(),
        )),
        (_, _, Some(b)) => Some(Present::Beat(b.as_str()?.to_string())),
        _ => None,
    };
    let stem = name
        .trim_end_matches(".yaml")
        .trim_end_matches(".test")
        .to_string();
    let id = if is_test {
        format!("test:{stem}")
    } else if name == "mock.yaml" {
        "mock".to_string()
    } else {
        format!("mock:{stem}")
    };
    Some((doc, Input { id, mock, present }))
}

/// The conformance sidecar: `entry.txt` names the entry `mock.yaml` presents.
fn conformance_entry(doc: &Path) -> Option<String> {
    let text = std::fs::read_to_string(doc.with_file_name("entry.txt")).ok()?;
    Some(text.trim().to_string()).filter(|s| !s.is_empty())
}

struct Corpus {
    cases: Vec<Case>,
    /// Documents whose check gate refuses them (not compared).
    check_refused: usize,
    /// Plays whose project does not compile (its check refuses a document).
    plays_refused: usize,
    /// Plays that could not be loaded for another reason (a usage error).
    plays_failed: Vec<String>,
    /// Every case id enumerated, compared or not.
    enumerated: BTreeSet<String>,
}

fn enumerate(roots: &[Root], only: Option<&str>) -> Corpus {
    let mut gates = Gates::default();
    let mut subjects: BTreeMap<PathBuf, Arc<Subject>> = BTreeMap::new();
    let mut corpus = Corpus {
        cases: Vec::new(),
        check_refused: 0,
        plays_refused: 0,
        plays_failed: Vec::new(),
        enumerated: BTreeSet::new(),
    };
    let mut plays: Vec<(PathBuf, &Root)> = Vec::new();
    for root in roots {
        let mut files = Files::default();
        walk(&root.dir, &mut files);
        for doc in files.docs.iter().chain(&files.plays) {
            if let Some(p) = project_root(doc, &root.dir) {
                gates.add(&p);
            }
        }
        let mut inputs: BTreeMap<PathBuf, Vec<Input>> = BTreeMap::new();
        for p in &files.inputs {
            if let Some((doc, input)) = read_input(p) {
                inputs.entry(doc).or_default().push(input);
            }
        }
        // Prepare every document (check, compile) in parallel.
        let prepared: Vec<(PathBuf, Option<Subject>)> = files
            .docs
            .par_iter()
            .filter_map(|doc| {
                let rel = display_name(root, doc)?;
                let project = project_root(doc, &root.dir);
                Some((doc.clone(), prepare(doc, rel, project.as_deref(), &gates)))
            })
            .collect();
        for (doc, subject) in prepared {
            let Some(subject) = subject else { continue };
            let subject = Arc::new(subject);
            let canon = std::fs::canonicalize(&doc).unwrap_or(doc.clone());
            if !subject.gate.ok {
                corpus.check_refused += 1;
            }
            let mut doc_inputs = vec![Input {
                id: "empty".to_string(),
                mock: MockSet::default(),
                present: None,
            }];
            doc_inputs.extend(inputs.remove(&canon).unwrap_or_default());
            for input in doc_inputs {
                let presents: Vec<(String, Present)> =
                    match input.present {
                        Some(p) => vec![(input.id.clone(), p)],
                        None if input.id == "mock" && conformance_entry(&doc).is_some() => {
                            let e = conformance_entry(&doc).expect("checked");
                            vec![(input.id.clone(), Present::Entries(vec![e]))]
                        }
                        None if subject.lore => subject
                            .entries
                            .iter()
                            .map(|e| {
                                (
                                    format!("{}/entry:{e}", input.id),
                                    Present::Entries(vec![e.clone()]),
                                )
                            })
                            .chain(subject.beats.iter().map(|b| {
                                (format!("{}/beat:{b}", input.id), Present::Beat(b.clone()))
                            }))
                            .collect(),
                        None => vec![(input.id.clone(), Present::Document)],
                    };
                for (id, present) in presents {
                    let id = format!("{}#{id}", subject.rel);
                    if only.is_some_and(|o| !id.contains(o)) {
                        continue;
                    }
                    corpus.enumerated.insert(id.clone());
                    corpus.cases.push(Case::Doc(DocCase {
                        id,
                        subject: subject.clone(),
                        present,
                        mock: input.mock.clone(),
                    }));
                }
            }
            subjects.insert(canon, subject);
        }
        plays.extend(files.plays.into_iter().map(|p| (p, root)));
    }
    for (play, root) in plays {
        match presentation_cases(&play, root, &mut subjects, &gates) {
            Ok(cases) => {
                for c in cases {
                    if only.is_some_and(|o| !c.id().contains(o)) {
                        continue;
                    }
                    corpus.enumerated.insert(c.id().to_string());
                    corpus.cases.push(c);
                }
            }
            // An empty message: the project's compile gate refused it (the
            // diagnostics are printed); its documents count as check-refused.
            Err(e) if e.is_empty() => corpus.plays_refused += 1,
            Err(e) => corpus
                .plays_failed
                .push(format!("{}: {e}", rel_slash(&play, &root.dir))),
        }
    }
    corpus
}

/// Every presentation of the play at `play`, as a case (§4.3 play level).
fn presentation_cases(
    play: &Path,
    root: &Root,
    subjects: &mut BTreeMap<PathBuf, Arc<Subject>>,
    gates: &Gates,
) -> Result<Vec<Case>, String> {
    let project = project_root(play, &root.dir).ok_or("no lute.project.yaml above the play")?;
    let played = crate::play::presentations_for_diff(&project, play)?;
    let stem = play
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .trim_end_matches(".play.yaml")
        .to_string();
    let mut out = Vec::new();
    for (n, pb) in played.into_iter().enumerate() {
        let pr = &pb.presented;
        let doc = project.join(&pr.document);
        let canon = std::fs::canonicalize(&doc).unwrap_or(doc.clone());
        let subject = match subjects.get(&canon) {
            Some(s) => s.clone(),
            None => {
                let rel = display_name(root, &doc).ok_or("document outside the corpus")?;
                let s = Arc::new(
                    prepare(&doc, rel, Some(&project), gates).ok_or("cannot read the document")?,
                );
                subjects.insert(canon, s.clone());
                s
            }
        };
        let Ok(art) = &subject.artifact else { continue };
        let declared: BTreeSet<String> = subject.defaults.keys().cloned().collect();
        let keys: BTreeSet<String> = declared
            .iter()
            .filter(|p| !p.starts_with("scene."))
            .cloned()
            .chain(
                subject
                    .entries
                    .iter()
                    .map(|e| format!("entry.{e}.everRead")),
            )
            .collect();
        let relations: Vec<&Json> = art
            .get("relations")
            .and_then(Json::as_array)
            .map(|v| v.iter().collect())
            .unwrap_or_default();
        let heads: BTreeSet<&str> = art
            .get("rules")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| r.pointer("/head/relation").and_then(Json::as_str))
            .collect();
        let base_relations: BTreeSet<String> = relations
            .iter()
            .filter(|r| r.get("derive").and_then(Json::as_bool) != Some(true))
            .filter_map(|r| r.get("name").and_then(Json::as_str))
            .filter(|n| !heads.contains(n))
            .map(str::to_string)
            .collect();

        // The trace mock: the world the play presented this beat in.
        let span = lute_core_span::Span {
            byte_start: 0,
            byte_end: 0,
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        };
        let mut mock = MockSet::default();
        // The persistent tiers the document declares, and the `prev.` copy
        // of each (what the last `newRun` left, which content may read).
        for (path, v) in &pr.state_before {
            // Another document's `entry.<id>.read` the content reads, too.
            let seeded = keys.contains(path)
                || path
                    .strip_prefix("prev.")
                    .is_some_and(|p| declared.contains(p))
                || (path.starts_with("entry.") && subject.input.text.contains(path.as_str()));
            if seeded {
                if let Some(text) = mock_literal(v) {
                    mock.state.push((path.clone(), text, span));
                }
            }
        }
        for (id, st) in &pr.quests_before {
            if subject.input.text.contains(&format!("quest.{id}.")) {
                mock.state
                    .push((format!("quest.{id}.state"), st.clone(), span));
            }
        }
        if let Some(m) = &pr.member {
            mock.state.push((
                lute_check::beats::OCCASION_TARGET.to_string(),
                m.clone(),
                span,
            ));
        }
        // A mock's facts join the project's seeds; a world that retracted a
        // seed fact has no trace mock.
        let seeds: Vec<Fact> = art
            .get("seedFacts")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|f| {
                let rel = f.get("relation")?.as_str()?.to_string();
                let args = f.get("args")?.as_array()?.iter().map(json_text).collect();
                Some((rel, args))
            })
            .collect();
        let unrepresentable = seeds
            .iter()
            .find(|f| !pr.facts_before.contains(*f))
            .map(|_| "the play's world retracted a seed fact".to_string());
        mock.facts = pr
            .facts_before
            .iter()
            .filter(|(rel, _)| base_relations.contains(rel))
            .map(render_fact)
            .collect();
        mock.visited = pr.visited_before.iter().cloned().collect();
        for rec in &pr.transcript {
            let key = match rec.get("kind").and_then(Json::as_str) {
                Some("choice") => rec.get("branch"),
                Some("hub") => rec.get("hub"),
                _ => None,
            };
            if let (Some(k), Some(chose)) = (
                key.and_then(Json::as_str),
                rec.get("chose").and_then(Json::as_str),
            ) {
                mock.choose
                    .entry(k.to_string())
                    .or_default()
                    .push(chose.to_string());
            }
        }
        mock.bridges = pr.bridges.clone();
        let present = match pr.kind {
            lute_compile::index::BeatKind::Scene => Present::Document,
            lute_compile::index::BeatKind::Entry => Present::Entries(vec![pr.id.clone()]),
            lute_compile::index::BeatKind::Bundle => Present::Beat(pr.id.clone()),
        };

        // What the play observed.
        let mut obs = Observation {
            said: pr.transcript.iter().filter_map(record_line).collect(),
            state: pr
                .state_after
                .iter()
                .map(|(k, v)| (k.clone(), value_text(v)))
                .collect(),
            facts: pr.facts_after.iter().map(render_fact).collect(),
            quests: BTreeMap::new(),
            exit: match pb.halted {
                _ if pb.step_exclusive => {
                    format!("refused:{}", lute_check::fact_check::E_FACT_EXCLUSIVE)
                }
                Some("incomplete") => "incomplete".to_string(),
                Some(_) => "refused:play".to_string(),
                None => RunFlags {
                    error: None,
                    refused: false,
                    incomplete: pr.transcript.iter().any(|r| {
                        matches!(r.get("kind").and_then(Json::as_str), Some("choice" | "hub"))
                            && r.get("chose").is_some_and(Json::is_null)
                    }),
                }
                .exit(&pr.transcript),
            },
        };
        restrict(&mut obs, &keys, &base_relations);
        out.push(Case::Presentation(PresentationCase {
            id: format!("{}#play:{stem}/{}", subject.rel, n + 1),
            subject,
            present,
            mock,
            played: obs,
            keys,
            base_relations,
            unrepresentable,
        }));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Rendering helpers.
// ---------------------------------------------------------------------------

fn said_line(speaker: &str, text: &str) -> String {
    format!("@{speaker}: {text}")
}

fn record_line(rec: &Json) -> Option<String> {
    (rec.get("kind").and_then(Json::as_str) == Some("line")).then(|| {
        said_line(
            rec.get("speaker").and_then(Json::as_str).unwrap_or(""),
            rec.get("text").and_then(Json::as_str).unwrap_or(""),
        )
    })
}

fn render_fact((rel, args): &Fact) -> String {
    format!("{rel}({})", args.join(", "))
}

fn relation_of(fact: &str) -> &str {
    fact.split('(').next().unwrap_or(fact)
}

/// `quest.<id>.state` -> `<id>`.
fn quest_of_state_path(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("quest.")?.strip_suffix(".state")?;
    (!rest.is_empty() && !rest.contains('.')).then_some(rest)
}

/// Trace's rendering of a value (`report::value_text`, `format_num`).
fn value_text(v: &Value) -> String {
    match v {
        Value::Unknown => "unknown".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => s.clone(),
        Value::Num(n) => num_text(*n),
    }
}

fn num_text(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

fn json_text(j: &Json) -> String {
    match j {
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.as_f64().map_or_else(|| n.to_string(), num_text),
        Json::String(s) => s.clone(),
        Json::Null => "<unset>".to_string(),
        other => other.to_string(),
    }
}

/// A mock `state:` literal for a decided value.
fn mock_literal(v: &Value) -> Option<String> {
    (!matches!(v, Value::Unknown)).then(|| value_text(v))
}

fn first_error(diags: &[lute_core_span::Diagnostic]) -> String {
    diags
        .iter()
        .find(|d| d.severity == Severity::Error)
        .or(diags.first())
        .map_or_else(|| "?".to_string(), |d| d.code.clone())
}

/// `[E-TRACE-CHOICE] …` -> `E-TRACE-CHOICE`.
fn bracketed_code(msg: &str) -> Option<&str> {
    let rest = msg.strip_prefix('[')?;
    Some(&rest[..rest.find(']')?])
}

// ---------------------------------------------------------------------------
// Allowlist (§4.5).
// ---------------------------------------------------------------------------

const FIELDS: [&str; 5] = ["said", "state", "facts", "quests", "exit"];

struct Allowed {
    line: usize,
    case: String,
    field: String,
}

fn allowlist() -> Result<Vec<Allowed>, Vec<String>> {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, raw) in ALLOWLIST.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        let n = i + 1;
        if cols.len() < 5 {
            errors.push(format!(
                "allowlist line {n}: `<case> <field> <class> <slice> <reason…>` expected: {line}"
            ));
            continue;
        }
        let (case, field, class, slice) = (cols[0], cols[1], cols[2], cols[3]);
        if !FIELDS.contains(&field) {
            errors.push(format!("allowlist line {n}: unknown field `{field}`"));
        }
        // One divergence may mix classes (`D7,D11`), each with its slice.
        let ids = |text: &str, prefix: char| {
            text.split(',').all(|t| {
                t.strip_prefix(prefix)
                    .is_some_and(|d| d.parse::<u32>().is_ok())
            })
        };
        if !ids(class, 'D') {
            errors.push(format!(
                "allowlist line {n}: class `{class}` is not a (comma-separated) D-id of the \
                 design §2.3"
            ));
        }
        if !ids(slice, 'S') {
            errors.push(format!(
                "allowlist line {n}: slice `{slice}` is not S<n>[,S<n>…]"
            ));
        }
        if !seen.insert((case.to_string(), field.to_string())) {
            errors.push(format!("allowlist line {n}: `{case} {field}` listed twice"));
        }
        out.push(Allowed {
            line: n,
            case: case.to_string(),
            field: field.to_string(),
        });
    }
    if errors.is_empty() {
        Ok(out)
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// The test.
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root exists")
}

fn roots() -> (Vec<Root>, bool) {
    let repo = repo_root();
    let mut roots = vec![
        Root {
            dir: repo.join("docs/examples"),
            label: "docs/examples".into(),
            opt_in: false,
        },
        Root {
            dir: repo.join("conformance"),
            label: "conformance".into(),
            opt_in: false,
        },
        Root {
            dir: repo.join("crates/lute-cli/tests/fixtures"),
            label: "fixtures".into(),
            opt_in: false,
        },
    ];
    let var = std::env::var("LUTE_DIFF_CORPUS").unwrap_or_default();
    let dirs: Vec<&str> = var.split(':').filter(|d| !d.is_empty()).collect();
    for d in &dirs {
        let dir = PathBuf::from(d);
        let dir = dir.canonicalize().unwrap_or(dir);
        roots.push(Root {
            label: "corpus".into(),
            dir,
            opt_in: true,
        });
    }
    (roots, !dirs.is_empty())
}

struct Tally {
    compared: usize,
    compared_in_repo: usize,
    not_concrete: BTreeMap<String, usize>,
    divergences: Vec<Divergence>,
    compared_ids: BTreeSet<String>,
}

fn run_cases(cases: &[Case], verbose: bool) -> Tally {
    enum Outcome {
        Compared(Vec<Divergence>),
        Skipped(Skip),
    }
    let outcomes: Vec<(String, Outcome)> = cases
        .par_iter()
        .map(|case| {
            let outcome = match case {
                Case::Doc(c) => match concretize(c) {
                    Err(skip) => Outcome::Skipped(skip),
                    Ok(mock) => {
                        let concrete = Case::Doc(DocCase {
                            id: c.id.clone(),
                            subject: c.subject.clone(),
                            present: c.present.clone(),
                            mock,
                        });
                        let (t, r) = (observe_trace(&concrete), observe_run(&concrete));
                        if verbose {
                            println!("{}\n  trace: {t:?}\n  run:   {r:?}", c.id);
                        }
                        Outcome::Compared(compare(&c.id, &t, &r, &c.subject.defaults))
                    }
                },
                Case::Presentation(c) => {
                    let t = observe_trace(case);
                    if !c.subject.gate.ok {
                        Outcome::Skipped(Skip::CheckRefused)
                    } else if let Some(why) = &c.unrepresentable {
                        Outcome::Skipped(Skip::NotConcrete(why.clone()))
                    } else if t.exit == "incomplete" || c.played.exit == "incomplete" {
                        Outcome::Skipped(Skip::NotConcrete("presentation incomplete".into()))
                    } else if t.exit.starts_with("refused:E-TRACE-")
                        && t.exit != format!("refused:{}", lute_trace::E_TRACE_CHOICE)
                    {
                        Outcome::Skipped(Skip::NotConcrete(format!(
                            "rebuilt mock refused ({})",
                            &t.exit["refused:".len()..]
                        )))
                    } else {
                        let r = observe_run(case);
                        if verbose {
                            println!(
                                "{}\n  mock:  {:?}\n  trace: {t:?}\n  play:  {r:?}",
                                c.id, c.mock
                            );
                        }
                        Outcome::Compared(compare(&c.id, &t, &r, &c.subject.defaults))
                    }
                }
            };
            (case.id().to_string(), outcome)
        })
        .collect();
    let mut tally = Tally {
        compared: 0,
        compared_in_repo: 0,
        not_concrete: BTreeMap::new(),
        divergences: Vec::new(),
        compared_ids: BTreeSet::new(),
    };
    for (id, outcome) in outcomes {
        match outcome {
            Outcome::Compared(divs) => {
                tally.compared += 1;
                if !id.starts_with("corpus:") {
                    tally.compared_in_repo += 1;
                }
                tally.divergences.extend(divs);
                tally.compared_ids.insert(id);
            }
            Outcome::Skipped(Skip::CheckRefused) => {}
            Outcome::Skipped(Skip::NotConcrete(why)) => {
                if verbose {
                    println!("{id}: skipped, not concrete: {why}");
                }
                *tally.not_concrete.entry(why).or_default() += 1;
            }
        }
    }
    tally
}

#[test]
fn differential_trace_vs_run() {
    let (roots, opt_in) = roots();
    if !opt_in {
        println!(
            "note: LUTE_DIFF_CORPUS is unset; set it to colon-separated directories of game \
             projects (e.g. a copy of lute-dogfood) to add them to the corpus"
        );
    }
    // `LUTE_DIFF_ONLY=<substring>`: only the cases whose id contains it,
    // each printed with both observations (debugging a divergence).
    let only = std::env::var("LUTE_DIFF_ONLY")
        .ok()
        .filter(|o| !o.is_empty());
    let corpus = enumerate(&roots, only.as_deref());
    let tally = run_cases(&corpus.cases, only.is_some());
    let allowed = match allowlist() {
        Ok(a) => a,
        Err(errors) => panic!("{}", errors.join("\n")),
    };

    let listed: BTreeSet<(&str, &str)> = allowed
        .iter()
        .map(|a| (a.case.as_str(), a.field.as_str()))
        .collect();
    let diverged: BTreeSet<(&str, &str)> = tally
        .divergences
        .iter()
        .map(|d| (d.case.as_str(), d.field))
        .collect();
    let allowlisted = tally
        .divergences
        .iter()
        .filter(|d| listed.contains(&(d.case.as_str(), d.field)))
        .count();

    let mut failures: Vec<String> = Vec::new();
    for d in &tally.divergences {
        if !listed.contains(&(d.case.as_str(), d.field)) {
            failures.push(format!(
                "unlisted divergence: {} {}: {}\n    allowlist line: {} {} D? S? <reason>",
                d.case, d.field, d.detail, d.case, d.field
            ));
        }
    }
    for a in &allowed {
        // An opt-in line is judged only when its corpus was enumerated here;
        // under `LUTE_DIFF_ONLY`, only the lines of the selected cases.
        if (a.case.starts_with("corpus:") || only.is_some()) && !corpus.enumerated.contains(&a.case)
        {
            continue;
        }
        if !tally.compared_ids.contains(&a.case) {
            failures.push(format!(
                "allowlist line {}: case `{}` was not compared this run (renamed, removed, or no \
                 longer concrete); delete the line",
                a.line, a.case
            ));
        } else if !diverged.contains(&(a.case.as_str(), a.field.as_str())) {
            failures.push(format!(
                "allowlist entry `{} {}` (line {}) is fixed; delete the line",
                a.case, a.field, a.line
            ));
        }
    }
    for p in &corpus.plays_failed {
        failures.push(format!("play did not load: {p}"));
    }

    let skipped: usize = tally.not_concrete.values().sum();
    println!(
        "differential: {} case(s) compared ({} in-repo), {} skipped as not concrete, {} \
         document(s) and {} play(s) refused by check, {} divergence(s) allowlisted",
        tally.compared,
        tally.compared_in_repo,
        skipped,
        corpus.check_refused,
        corpus.plays_refused,
        allowlisted
    );
    for (why, n) in &tally.not_concrete {
        println!("  skipped (not concrete): {n} × {why}");
    }
    if only.is_none() && tally.compared_in_repo * 10 < COMPARED_FLOOR * 9 {
        failures.push(format!(
            "only {} in-repo case(s) compared, below 90% of the recorded floor {COMPARED_FLOOR}: \
             the corpus enumerator is broken",
            tally.compared_in_repo
        ));
    }
    assert!(
        failures.is_empty(),
        "{} differential failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
