use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lute_model::{ModelOptions, ProjectModel};
use serde::Serialize;
use serde_yaml::Value as Yaml;

const MIN_SAMPLE: Duration = Duration::from_millis(200);
const TIERS: [(&str, &str); 3] = [
    ("tiny", "ledger"),
    ("medium", "drowned-crown"),
    ("large", "monster-league"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    ColdLoad,
    Resolution,
    Analysis,
    Serialization,
    Playback,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::ColdLoad => "cold-load",
            Self::Resolution => "project-resolution",
            Self::Analysis => "analysis",
            Self::Serialization => "serialization",
            Self::Playback => "playback",
        }
    }

    fn all() -> [Self; 5] {
        [
            Self::ColdLoad,
            Self::Resolution,
            Self::Analysis,
            Self::Serialization,
            Self::Playback,
        ]
    }
}

#[derive(Debug)]
struct Config {
    root: PathBuf,
    tiers: Vec<(&'static str, &'static str)>,
    phases: Vec<Phase>,
    samples: usize,
    json: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Runner {
    os: String,
    arch: String,
    cpu: String,
}

#[derive(Serialize)]
struct BinaryInfo {
    label: String,
    profile: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CorpusEntry {
    project: String,
    project_revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sample {
    tier: String,
    project: String,
    phase: String,
    sample: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    iterations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    per_iteration_us: Option<f64>,
    exit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: &'static str,
    commit: String,
    runner: Runner,
    binary: BinaryInfo,
    corpus: BTreeMap<String, CorpusEntry>,
    samples: Vec<Sample>,
}

struct TestCase {
    path: PathBuf,
    text: String,
}
struct PreparedTest {
    path: PathBuf,
    document_index: usize,
    mocks: lute_trace::MockSet,
    presentation: TestPresentation,
}
enum TestPresentation {
    Document,
    Entry(String),
    Entries(Vec<String>),
    Beat(String),
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("lute-bench: {error}");
            eprintln!("usage: lute-bench --root <checkout> --tier tiny|medium|large|all --phase <name>|all --samples <n> --json <path>|-");
            std::process::exit(2);
        }
    };

    let destination = config.json.clone();
    let report = run(config);
    let mut output = match serde_json::to_string_pretty(&report) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("lute-bench: cannot serialize report: {error}");
            std::process::exit(2);
        }
    };
    output.push('\n');
    let write_result = if destination == "-" {
        print!("{output}");
        Ok(())
    } else {
        std::fs::write(&destination, output)
            .map_err(|error| format!("cannot write {}: {error}", destination))
    };
    if let Err(error) = write_result {
        eprintln!("lute-bench: {error}");
        std::process::exit(2);
    }
}

fn parse_args() -> Result<Config, String> {
    let mut args = std::env::args().skip(1);
    let mut root = PathBuf::from(".");
    let mut tier = "all".to_string();
    let mut phase = "all".to_string();
    let mut samples = 7usize;
    let mut json = "-".to_string();

    while let Some(arg) = args.next() {
        let value = |name: &str, args: &mut std::iter::Skip<std::env::Args>| {
            args.next().ok_or_else(|| format!("{name} requires a value"))
        };
        match arg.as_str() {
            "--root" => root = PathBuf::from(value("--root", &mut args)?),
            "--tier" => tier = value("--tier", &mut args)?,
            "--phase" => phase = value("--phase", &mut args)?,
            "--samples" => {
                samples = value("--samples", &mut args)?
                    .parse()
                    .map_err(|_| "--samples must be a positive integer".to_string())?;
                if samples == 0 {
                    return Err("--samples must be a positive integer".to_string());
                }
            }
            "--json" => json = value("--json", &mut args)?,
            "--help" | "-h" => {
                println!("usage: lute-bench --root <checkout> --tier tiny|medium|large --phase <name>|all --samples <n> --json <path>|-");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument `{other}`")),
        }
    }

    let tiers = match tier.as_str() {
        "all" => TIERS.to_vec(),
        "tiny" => vec![TIERS[0]],
        "medium" => vec![TIERS[1]],
        "large" => vec![TIERS[2]],
        other => return Err(format!("invalid tier `{other}`")),
    };
    let phases = match phase.as_str() {
        "all" => Phase::all().to_vec(),
        "cold-load" | "cold_load" | "cold" => vec![Phase::ColdLoad],
        "project-resolution" | "project_resolution" | "resolution" => vec![Phase::Resolution],
        "analysis" => vec![Phase::Analysis],
        "serialization" | "serialize" => vec![Phase::Serialization],
        "playback" | "test" => vec![Phase::Playback],
        other => return Err(format!("invalid phase `{other}`")),
    };
    Ok(Config {
        root,
        tiers,
        phases,
        samples,
        json,
    })
}

fn run(config: Config) -> Report {
    let mut corpus = BTreeMap::new();
    let mut samples = Vec::new();
    for (tier, project) in config.tiers {
        let project_root = config.root.join("docs/examples/games").join(project);
        let revision = revision_for(&project_root);
        corpus.insert(
            tier.to_string(),
            CorpusEntry {
                project: project.to_string(),
                project_revision: revision,
            },
        );
        for phase in &config.phases {
            for sample in 0..config.samples {
                samples.push(run_sample(
                    tier,
                    project,
                    &project_root,
                    *phase,
                    sample,
                ));
            }
        }
    }

    Report {
        schema_version: "0.36.0.bench",
        commit: commit_for(&config.root),
        runner: Runner {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu: std::env::var("LUTE_BENCH_CPU").unwrap_or_else(|_| "unknown".to_string()),
        },
        binary: BinaryInfo {
            label: std::env::var("LUTE_BENCH_LABEL").unwrap_or_else(|_| "head".to_string()),
            profile: if cfg!(debug_assertions) {
                "dev".to_string()
            } else {
                "release".to_string()
            },
        },
        corpus,
        samples,
    }
}

fn run_sample(
    tier: &str,
    project: &str,
    root: &Path,
    phase: Phase,
    sample: usize,
) -> Sample {
    let test_count = match count_tests(root) {
        Ok(count) => Some(count),
        Err(error) => {
            return failed_sample(tier, project, phase, sample, None, error);
        }
    };
    let mut iterations = 0u64;
    let result = match phase {
        Phase::ColdLoad => loop_until(root, || {
            let model = build_model(root, false)?;
            black_box(model);
            Ok(())
        }, &mut iterations),
        // Resolution intentionally includes ProjectModel's discovery/read,
        // fold, and reconcile boundary; cold-load repeats that construction
        // with a fresh input cache to expose uncached filesystem behavior.
        Phase::Resolution => loop_until(root, || {
            let model = build_model(root, false)?;
            black_box(model.reconciled());
            Ok(())
        }, &mut iterations),
        Phase::Analysis => {
            let model = match build_model(root, false) {
                Ok(model) => model,
                Err(error) => {
                    return failed_sample(tier, project, phase, sample, test_count, error);
                }
            };
            loop_until(
                root,
                || {
                    let graph = lute_model::SemanticGraph::build(&model);
                    black_box(graph);
                    Ok(())
                },
                &mut iterations,
            )
        }
        Phase::Serialization => {
            let model = match build_model(root, true) {
                Ok(model) => model,
                Err(error) => {
                    return failed_sample(tier, project, phase, sample, test_count, error);
                }
            };
            loop_until(
                root,
                || {
                    for document in model.documents() {
                        if let Some(artifact) = &document.artifact {
                            let bytes = serde_json::to_vec(artifact).map_err(|e| e.to_string())?;
                            black_box(bytes);
                        }
                    }
                    if let Some(index) = model.index() {
                        let text = index.to_json().map_err(|e| e.to_string())?;
                        black_box(text);
                    }
                    Ok(())
                },
                &mut iterations,
            )
        }
        Phase::Playback => {
            let model = match build_model(root, true) {
                Ok(model) => model,
                Err(error) => {
                    return failed_sample(tier, project, phase, sample, test_count, error);
                }
            };
            let tests = match prepare_tests(&model, root) {
                Ok(tests) => tests,
                Err(error) => {
                    return failed_sample(tier, project, phase, sample, test_count, error);
                }
            };
            let project_asserts = model
                .reconciled()
                .scenarios
                .get(model.root())
                .map(|scenario| {
                    lute_check::connectivity::live_assert_relations(
                        &scenario.docs,
                        &scenario.reach,
                        &scenario.ambiguous_quests,
                        &scenario.unreachable_quests,
                        &scenario.rel_vocab.effect_directives,
                    )
                });
            loop_until(
                root,
                || run_tests(&model, &tests, project_asserts.as_ref()),
                &mut iterations,
            )
        }
    };
    match result {
        Ok(elapsed) => {
            let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
            let per_iteration_us = elapsed.as_secs_f64() * 1_000_000.0 / iterations as f64;
            Sample {
                tier: tier.to_string(),
                project: project.to_string(),
                phase: phase.name().to_string(),
                sample,
                test_count,
                iterations: Some(iterations),
                elapsed_ms: Some(round3(elapsed_ms)),
                per_iteration_us: Some(round3(per_iteration_us)),
                exit: "ok".to_string(),
                error: None,
            }
        }
        Err(error) => failed_sample(tier, project, phase, sample, test_count, error),
    }
}

fn failed_sample(
    tier: &str,
    project: &str,
    phase: Phase,
    sample: usize,
    test_count: Option<usize>,
    error: String,
) -> Sample {
    Sample {
        tier: tier.to_string(),
        project: project.to_string(),
        phase: phase.name().to_string(),
        sample,
        test_count,
        iterations: None,
        elapsed_ms: None,
        per_iteration_us: None,
        exit: "error".to_string(),
        error: Some(error),
    }
}

fn loop_until<F>(root: &Path, mut operation: F, iterations: &mut u64) -> Result<Duration, String>
where
    F: FnMut() -> Result<(), String>,
{
    let _ = root;
    let start = Instant::now();
    while start.elapsed() < MIN_SAMPLE || *iterations == 0 {
        operation()?;
        *iterations += 1;
    }
    Ok(start.elapsed())
}

fn build_model(root: &Path, compile: bool) -> Result<ProjectModel, String> {
    ProjectModel::build_single_root(
        root,
        &ModelOptions {
            compile,
            ..ModelOptions::default()
        },
    )
    .map_err(|error| error.to_string())
}

fn count_tests(root: &Path) -> Result<usize, String> {
    Ok(load_test_paths(root)?.len())
}

fn load_test_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    visit_files(root, &mut |path| {
        if path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with(".test.yaml")) {
            paths.push(path.to_path_buf());
        }
        Ok(())
    })?;
    paths.sort();
    Ok(paths)
}

fn load_tests(root: &Path) -> Result<Vec<TestCase>, String> {
    load_test_paths(root)?
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            Ok(TestCase { path, text })
        })
        .collect()
}
fn prepare_tests(model: &ProjectModel, root: &Path) -> Result<Vec<PreparedTest>, String> {
    load_tests(root)?
        .into_iter()
        .map(|test| {
            let yaml: Yaml = serde_yaml::from_str(&test.text)
                .map_err(|error| format!("{}: invalid test YAML: {error}", test.path.display()))?;
            let file = yaml
                .get("file")
                .and_then(Yaml::as_str)
                .ok_or_else(|| format!("{}: test has no file", test.path.display()))?;
            let document_path = test.path.parent().unwrap_or_else(|| Path::new(".")).join(file);
            let canonical = std::fs::canonicalize(&document_path)
                .map_err(|error| format!("{}: cannot resolve {file}: {error}", test.path.display()))?;
            let document_index = model
                .documents()
                .iter()
                .position(|document| std::fs::canonicalize(&document.path).ok().as_ref() == Some(&canonical))
                .ok_or_else(|| format!("{}: document {file} is not in the project", test.path.display()))?;
            let mut mocks = lute_trace::parse_mock_surfaces(&test.text)
                .map_err(|diagnostic| format!("{}: {}", test.path.display(), diagnostic.message))?;
            if yaml.get("expect").and_then(|expect| expect.get("eligible")).is_some() {
                mocks.gate_eligibility = true;
            }
            if mocks.state.iter().any(|(path, _, _)| path.starts_with("quest.")) {
                let quests = model
                    .reconciled()
                    .scenarios
                    .values()
                    .flat_map(|scenario| scenario.quest_ids.iter().cloned())
                    .collect();
                mocks.project_quests = Some(quests);
            }
            let presentation = if let Some(beat) = yaml.get("beat").and_then(Yaml::as_str) {
                TestPresentation::Beat(beat.to_string())
            } else if let Some(entry) = yaml.get("entry").and_then(Yaml::as_str) {
                TestPresentation::Entry(entry.to_string())
            } else if let Some(entries) = yaml.get("entries").and_then(Yaml::as_sequence) {
                TestPresentation::Entries(
                    entries
                        .iter()
                        .map(Yaml::as_str)
                        .collect::<Option<Vec<_>>>()
                        .ok_or_else(|| format!("{}: entries must be strings", test.path.display()))?
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                )
            } else {
                TestPresentation::Document
            };
            Ok(PreparedTest {
                path: test.path,
                document_index,
                mocks,
                presentation,
            })
        })
        .collect()
}



fn run_tests(
    model: &ProjectModel,
    tests: &[PreparedTest],
    project_asserts: Option<&std::collections::BTreeSet<String>>,
) -> Result<(), String> {
    for test in tests {
        let document = &model.documents()[test.document_index];
        let (_, exit) = match &test.presentation {
            TestPresentation::Document => lute_trace::trace_with_check(
                &document.input,
                document.check.clone(),
                test.mocks.clone(),
                project_asserts,
            ),
            TestPresentation::Entry(entry) => lute_trace::trace_entry_with_check(
                &document.input,
                document.check.clone(),
                test.mocks.clone(),
                entry,
                project_asserts,
            ),
            TestPresentation::Entries(entries) => {
                let entries: Vec<&str> = entries.iter().map(String::as_str).collect();
                lute_trace::trace_entries_with_check(
                    &document.input,
                    document.check.clone(),
                    test.mocks.clone(),
                    &entries,
                    project_asserts,
                )
            }
            TestPresentation::Beat(beat) => lute_trace::trace_beat_with_check(
                &document.input,
                document.check.clone(),
                test.mocks.clone(),
                beat,
                project_asserts,
            ),
        };
        if exit.code() != 0 {
            let reason = match &exit {
                lute_trace::TraceExit::Refused(diags) => diags
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("; "),
                lute_trace::TraceExit::Incomplete => "incomplete trace".to_string(),
                lute_trace::TraceExit::Complete => "unexpected complete exit".to_string(),
            };
            return Err(format!(
                "{}: playback exited {} ({reason})",
                test.path.display(),
                exit.code()
            ));
        }
    }
    Ok(())
}
fn visit_files(root: &Path, callback: &mut impl FnMut(&Path) -> Result<(), String>) -> Result<(), String> {
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("cannot read {}: {error}", root.display()))?;
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot walk {}: {error}", root.display()))?;
    paths.sort();
    for path in paths {
        if path.is_dir() {
            visit_files(&path, callback)?;
        } else {
            callback(&path)?;
        }
    }
    Ok(())
}

fn revision_for(root: &Path) -> String {
    build_model(root, false)
        .map(|model| model.revisions().sha256.clone())
        .unwrap_or_else(|_| "error".to_string())
}

fn commit_for(root: &Path) -> String {
    if let Ok(commit) = std::env::var("LUTE_BENCH_COMMIT") {
        if !commit.is_empty() {
            return commit;
        }
    }
    let git = root.join(".git");
    let git_dir = if git.is_dir() {
        git
    } else if let Ok(text) = std::fs::read_to_string(&git) {
        root.join(text.trim().strip_prefix("gitdir: ").unwrap_or(text.trim()))
    } else {
        return "unknown".to_string();
    };
    let head = match std::fs::read_to_string(git_dir.join("HEAD")) {
        Ok(head) => head.trim().to_string(),
        Err(_) => return "unknown".to_string(),
    };
    if let Some(reference) = head.strip_prefix("ref: ") {
        std::fs::read_to_string(git_dir.join(reference))
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    } else {
        head
    }
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_frozen_schema_and_stable_phase_order() {
        assert_eq!(
            Phase::all().map(Phase::name),
            ["cold-load", "project-resolution", "analysis", "serialization", "playback"]
        );
        let report = Report {
            schema_version: "0.36.0.bench",
            commit: "test".to_string(),
            runner: Runner {
                os: "test".to_string(),
                arch: "test".to_string(),
                cpu: "test".to_string(),
            },
            binary: BinaryInfo {
                label: "head".to_string(),
                profile: "release".to_string(),
            },
            corpus: BTreeMap::new(),
            samples: Vec::new(),
        };
        let json = serde_json::to_value(report).unwrap();
        assert_eq!(json["schemaVersion"], "0.36.0.bench");
        assert!(json["samples"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_count_failure_is_an_explicit_error_without_zero_count() {
        let sample = run_sample(
            "tiny",
            "missing",
            Path::new("/definitely/not/a/lute-benchmark-root"),
            Phase::ColdLoad,
            0,
        );
        assert_eq!(sample.exit, "error");
        assert!(sample.error.is_some());
        assert!(sample.test_count.is_none());
    }
}
