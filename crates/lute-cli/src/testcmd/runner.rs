use super::*;
use lute_load::build_input;
pub fn run_test(
    dir: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    coverage: bool,
    no_derive: bool,
) -> ExitCode {
    let single = dir.is_file().then(|| {
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        (name.ends_with(".test.yaml"), name.ends_with(".play.yaml"))
    });
    let (test_files, play_files) = match single {
        Some((true, _)) => (vec![dir.to_path_buf()], Vec::new()),
        Some((_, true)) => (Vec::new(), vec![dir.to_path_buf()]),
        Some(_) => {
            eprintln!(
                "lute: {} is not a directory, a `*.test.yaml` or a `*.play.yaml`",
                dir.display()
            );
            return ExitCode::from(2);
        }
        None => match (
            find_files_with_suffix(dir, ".test.yaml"),
            find_files_with_suffix(dir, ".play.yaml"),
        ) {
            (Ok(t), Ok(p)) => (t, p),
            (Err(e), _) | (_, Err(e)) => {
                let e = lute_manifest::io_reason(&e);
                eprintln!("lute: cannot walk {}: {e}", dir.display());
                return ExitCode::from(2);
            }
        },
    };

    // T2-1: the project is loaded and analysed once for the whole run —
    // every test's project producer set / quest ids before the tests, the
    // plays' compiled project before the plays — and the tests, then the
    // plays, run in parallel (`RAYON_NUM_THREADS` respected). Each one's
    // stderr is captured and replayed, and its result folded, in the order
    // the sequential run reported them; the first usage/I-O failure still
    // stops the run with nothing after it printed.
    let mut results = Vec::new();
    let mut cov = CoverageAccum::default();
    let mut noted: BTreeSet<PathBuf> = BTreeSet::new();
    // Every model this run builds, once: the tests' analysis, the gates,
    // quest ids, producer sets and the plays' compile read the same project.
    let memo = lute_model::ModelMemo::default();
    let mut shared = match Shared::for_tests(&memo, &test_files, project, providers) {
        Ok(shared) => shared,
        Err(code) => return code,
    };
    // — then the run is refused, instead of one failed test (and one
    // `<doc>:1:1` copy) per document that imports it.
    let play_roots: BTreeSet<PathBuf> = play_files
        .iter()
        .filter_map(|p| project_dir_of(p, project))
        .collect();
    for root in play_roots {
        if !shared.gates.contains_key(&root) {
            let rec = lute_model::reconciled_project_results_in(&memo, &root, providers).ok();
            shared.gates.insert(root, rec);
        }
    }
    let faults: Vec<String> = shared
        .gates
        .iter()
        .filter_map(|(root, rec)| Some(rec.as_ref()?.schema_faults(root)))
        .flatten()
        .collect();
    if !faults.is_empty() {
        for line in &faults {
            println!("{line}");
        }
        eprintln!(
            "lute test: {} schema or plugin error(s) every importing document shares; \
             refusing to run the tests",
            faults.len()
        );
        return ExitCode::from(1);
    }

    let runs: Vec<_> = test_files
        .par_iter()
        .map(|test_file| {
            let mut local = CoverageAccum::default();
            let (r, lines) = captured(|| {
                run_one_test(
                    test_file,
                    providers,
                    project,
                    no_derive,
                    &shared,
                    coverage.then_some(&mut local),
                )
            });
            (r, local, lines)
        })
        .collect();
    for (r, local, lines) in runs {
        replay(lines, &mut noted);
        match r {
            Ok(r) => {
                results.push(r);
                cov.merge(local);
            }
            // A malformed test yaml or an unreadable referenced document is a
            // usage/I-O failure (exit 2) — never a silent skip that would let
            // a broken suite report "all passed".
            Err(code) => return code,
        }
    }

    shared.compile_plays(&memo, &play_files, project);
    let runs: Vec<_> = play_files
        .par_iter()
        .map(|play_file| {
            let mut local = CoverageAccum::default();
            let (r, lines) = captured(|| {
                run_one_play(
                    play_file,
                    project,
                    no_derive,
                    &shared,
                    coverage.then_some(&mut local),
                )
            });
            (r, local, lines)
        })
        .collect();
    for (r, local, lines) in runs {
        replay(lines, &mut noted);
        if let Some(r) = r {
            results.push(r);
            cov.merge(local);
        }
    }

    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.len() - passed;

    // #24's denominator, at beat granularity since T3-20: every coverage
    // unit ([`coverage_units`]) under the PROJECT root that no test
    // presented and no play presented, MINUS the component documents. The
    // root is `--project`, else the nearest `lute.project.yaml` above `dir`,
    // else `dir` itself — the
    // tests conventionally live in `tests/`, and measuring against the walk
    // root there made `every testable document … is named` vacuously true
    // (T1-13). `find_lute_files` is the SAME byte-sorted, symlink-deduped
    // walk `check-project` uses, so the two surfaces agree about what a
    // document is. Compared on canonical paths, printed as the walk spelled
    // them.
    //
    // A component is filtered out because it is UNTESTABLE, not untested: it
    // is reached only by `::use` from an importer, it is never the `file:` of
    // a `*.test.yaml`, and it produces no artifact anyone can execute
    // (`compile_all::is_component_file`'s own doc makes the identical call for
    // `--all`). Listing it would print a line no author can ever discharge,
    // which is the exact shape of the wound this release exists to close. The
    // component case already has its own honest surface, and it is not this
    // one: `W-COMPONENT-UNVERIFIED` (dsl 0.10.0 §9 rule 4, D-W) says the
    // component's contract was not verified, and says who decides.
    let walk_root = if single.is_some() {
        dir.parent().unwrap_or_else(|| Path::new("."))
    } else {
        dir
    };
    let coverage_root: PathBuf = match project {
        Some(p) => p.to_path_buf(),
        None => match nearest_manifest_dir(dir) {
            Some(root) if canonical_key(&root) != canonical_key(walk_root) => root,
            _ => walk_root.to_path_buf(),
        },
    };
    let units: Vec<CoverageUnit> = if coverage {
        match coverage_units(&coverage_root) {
            Ok(units) => units,
            Err(e) => {
                let e = lute_manifest::io_reason(&e);
                eprintln!(
                    "lute: cannot walk {} for the untested set: {e}",
                    coverage_root.display()
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // T3-15: the whole report is rendered first and written once, so a
    // closed pipe (`lute test … | head`) is an I/O exit rather than a panic.
    let text = if json {
        render_json(
            &results,
            coverage.then_some((&cov, coverage_root.as_path())),
            &units,
        )
    } else {
        render_human(
            dir,
            &results,
            coverage.then_some((&cov, coverage_root.as_path())),
            &units,
        )
    };
    if crate::write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }

    if failed > 0 {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// The project state every test and play of one run shares (T2-1),
/// computed before they run so the parallel tests and plays only read it.
struct Shared {
    /// Retained project models, keyed by canonical project root. Every
    /// project test borrows its document input and analysis from one of these
    /// models for the entire run.
    models: BTreeMap<String, std::sync::Arc<ProjectModel>>,
    /// The project May producer set per project root (T1-14), for every
    /// root a test mocking `facts:` resolves against — it costs a full
    /// project collection.
    producers: BTreeMap<PathBuf, Option<BTreeSet<String>>>,
    /// Every quest id of the project per root (dsl 0.26.0 §7, T3-5), for
    /// every root a test with `accepts:` resolves against.
    quests: BTreeMap<PathBuf, Option<BTreeSet<String>>>,
    /// The compiled project per play project directory.
    plays: BTreeMap<PathBuf, crate::play::PlayProject>,
    /// The reconciled project analysis per project root a test resolves
    /// its document against (`None` when it failed): every test gates its
    /// document on its project's verdict, as `lute trace --project` and
    /// `lute play` do — one envelope (round-5 `test-project-envelope`).
    gates: BTreeMap<PathBuf, Option<crate::ReconciledProject>>,
}

/// The root whose producer set judges `lute_path`'s mocked facts:
/// `--project` when given (every file resolves against it, as the trace
/// gate does), else the nearest `lute.project.yaml` — with whether it is a
/// single root ([`crate::project_assert_relations`]).
fn producer_root(lute_path: &Path, project: Option<&Path>) -> Option<(PathBuf, bool)> {
    match project {
        Some(p) => Some((p.to_path_buf(), true)),
        None => nearest_manifest_dir(lute_path).map(|root| (root, false)),
    }
}

/// The project `file` belongs to: `--project`, else the nearest
/// `lute.project.yaml` above it.
fn project_dir_of(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    project
        .map(Path::to_path_buf)
        .or_else(|| nearest_manifest_dir(file))
}

impl Shared {
    /// Collect, once per root, what `test_files` read of their projects,
    /// each model built through `memo`.
    fn for_tests(
        memo: &lute_model::ModelMemo,
        test_files: &[PathBuf],
        project: Option<&Path>,
        providers: Option<&Path>,
    ) -> Result<Self, ExitCode> {
        let mut model_roots: BTreeSet<PathBuf> = BTreeSet::new();
        let mut producer_roots: BTreeSet<(PathBuf, bool)> = BTreeSet::new();
        let mut quest_roots: BTreeSet<PathBuf> = BTreeSet::new();
        let mut gate_roots: BTreeSet<PathBuf> = BTreeSet::new();
        for test_file in test_files {
            let Ok(text) = std::fs::read_to_string(test_file) else {
                continue;
            };
            let (Ok(mocks), Ok(Some(rel))) =
                (parse_mock_surfaces(&text), lute_trace::mock_subject(&text))
            else {
                continue;
            };
            let lute_path = test_file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&rel);
            if let Some(root) = project_dir_of(&lute_path, project) {
                model_roots.insert(root);
            }
            if !mocks.facts.is_empty() {
                producer_roots.extend(producer_root(&lute_path, project));
            }
            if !mocks.accepts.is_empty() {
                quest_roots.extend(project_dir_of(&lute_path, project));
            }
            gate_roots.extend(project_dir_of(&lute_path, project));
        }
        let opts = ModelOptions {
            providers: providers.map(Path::to_path_buf),
            permission_profile: None,
            mode: lute_check::Mode::Ci,
            compile: false,
            wip: false,
        };
        let mut shared = Shared {
            models: BTreeMap::new(),
            producers: BTreeMap::new(),
            quests: BTreeMap::new(),
            plays: BTreeMap::new(),
            gates: BTreeMap::new(),
        };
        for root in model_roots {
            let model = memo.single_root(&root, &opts).map_err(|error| {
                eprintln!("lute: cannot build project {}: {error}", root.display());
                match *error {
                    ModelError::Io(_) | ModelError::Input(_) => ExitCode::from(2),
                    ModelError::Resolve(_)
                    | ModelError::Compile { .. }
                    | ModelError::Index(_) => ExitCode::from(1),
                }
            })?;
            if model.has_resolution_errors() {
                for document in model.documents() {
                    for diagnostic in &document.resolve_diags {
                        eprintln!(
                            "lute test: {} [{}] {}",
                            document.path.display(),
                            diagnostic.code,
                            diagnostic.message
                        );
                    }
                }
                eprintln!("lute test: project resolution failed; refusing to run the tests");
                return Err(ExitCode::from(1));
            }
            shared.models.insert(canonical_key(&root), model);
        }
        for (root, single_root) in producer_roots {
            let set = crate::project::project_assert_relations_in(memo, &root, single_root, providers);
            shared.producers.insert(root, set);
        }
        for root in quest_roots {
            let ids = crate::project::project_quest_ids_in(memo, &root, providers);
            shared.quests.insert(root, ids);
        }
        for root in gate_roots {
            let rec = lute_model::reconciled_project_results_in(memo, &root, providers).ok();
            shared.gates.insert(root, rec);
        }
        Ok(shared)
    }

    /// Find a project-model document by canonical file identity.
    fn document(&self, path: &Path) -> Option<&ModelDocument> {
        let key = canonical_key(path);
        self.models
            .values()
            .flat_map(|model| model.documents())
            .find(|document| canonical_key(&document.path) == key)
    }

    /// Compile, once per project, what the expect-carrying `play_files`
    /// run over, from `memo`'s models.
    fn compile_plays(&mut self, memo: &lute_model::ModelMemo, play_files: &[PathBuf], project: Option<&Path>) {
        for play_file in play_files {
            if !matches!(scan_play(play_file), PlayScan::Expect { .. }) {
                continue;
            }
            if let Some(dir) = project_dir_of(play_file, project) {
                if !self.plays.contains_key(&dir) {
                    let compiled = crate::play::PlayProject::compile_in(memo, &dir);
                    self.plays.insert(dir, compiled);
                }
            }
        }
    }

    /// The producer set of the project `lute_path` belongs to
    /// ([`producer_root`]). `None` when there is no project to consult or it
    /// could not be collected — the trace then judges the document alone
    /// and its note says so.
    fn for_document(
        &self,
        lute_path: &Path,
        project: Option<&Path>,
        providers: Option<&Path>,
    ) -> Option<BTreeSet<String>> {
        let (root, single_root) = producer_root(lute_path, project)?;
        match self.producers.get(&root) {
            Some(set) => set.clone(),
            None => crate::project_assert_relations(&root, single_root, providers),
        }
    }

    /// Every quest id of the project at `root`.
    fn project_quests(&self, root: &Path, providers: Option<&Path>) -> Option<BTreeSet<String>> {
        match self.quests.get(root) {
            Some(ids) => ids.clone(),
            None => crate::project_quest_ids(root, providers),
        }
    }

    /// `lute_path`'s gate verdict in the project at `root` (the spec §5
    /// gate `lute trace --project` applies); `None` when the project could
    /// not be analysed or does not hold the document — the standalone
    /// check then decides, as `lute trace` without a project does.
    fn schema_faults(&self, root: &Path) -> Option<Vec<String>> {
        Some(self.gates.get(root)?.as_ref()?.schema_faults(root))
    }

    /// `lute_path`'s gate verdict in the project at `root` (the spec §5
    /// gate `lute trace --project` applies); `None` when the project could
    /// not be analysed or does not hold the document — the standalone
    /// check then decides, as `lute trace` without a project does.
    fn gate(&self, root: &Path, lute_path: &Path) -> Option<lute_check::CheckResult> {
        self.gates.get(root)?.as_ref()?.gate(lute_path)
    }
}

/// Trace one test file and evaluate its expectations. `Err(code)` is an I/O /
/// malformed-yaml failure (exit 2). `Ok` is a decided pass/fail verdict —
/// including a refused trace, which is a test FAILURE (semantic), not an I/O
/// error. When `cov` is `Some`, the produced report is folded into it.
///
/// `project` resolves the traced document EXACTLY as `lute trace --project`
/// does (module docs): threaded straight into [`lute_load::build_input`], never
/// substituted for `None`. A project-resolution `E-` diagnostic
/// (`resolve_error`) is therefore a build-failing error here too — `Err(1)`,
/// never folded into a per-test `TestResult` where a caller filtering on
/// `passed` could mistake a broken manifest for a failing assertion.
/// Run one scenario test in-process and report only a quest completion
/// transition observed by its trace. Seeded `complete` status is not enough.
pub(crate) fn run_test_for_constraint(root: &Path, script: &Path, quest: &str) -> bool {
    let files = [script.to_path_buf()];
    let Ok(shared) = Shared::for_tests(&lute_model::ModelMemo::default(), &files, Some(root), None) else {
        return false;
    };
    let Ok(result) = run_one_test(script, None, Some(root), false, &shared, None) else {
        return false;
    };
    result.completed_quests.contains(quest)
}

/// Execute one scenario test in-process and report whether its trace visited a
/// graph target. This deliberately uses the same trace runner as
/// `constraints --run`; static YAML mentions are never treated as witnesses.
pub(crate) fn run_test_for_context(
    root: &Path,
    script: &Path,
    target: &NodeKey,
) -> bool {
    let files = [script.to_path_buf()];
    let Ok(shared) = Shared::for_tests(&lute_model::ModelMemo::default(), &files, Some(root), None) else {
        return false;
    };
    let Ok(result) = run_one_test(script, None, Some(root), false, &shared, None) else {
        return false;
    };
    result_visits_target(&result, target)
}

fn result_visits_target(result: &TestResult, target: &NodeKey) -> bool {
    if target.kind == NodeKind::Quest
        && result.completed_quests.contains(&target.key)
    {
        return true;
    }
    if target.kind == NodeKind::Choice {
        let Some((document, tail)) = target.key.rsplit_once(':') else {
            return false;
        };
        let Some((parent, option)) = tail.rsplit_once('.') else {
            return false;
        };
        let Some(result_document) = traced_document_id(&result.lute_file) else {
            return false;
        };
        if result_document != document {
            return false;
        }
        let suffix = format!(":{parent}:{option}");
        return result.visited.iter().any(|entry| entry.ends_with(&suffix));
    }
    let suffix = format!(":{}", target.key);
    result.visited.iter().any(|entry| {
        entry == &target.canonical() || entry.ends_with(&suffix)
    })
}

fn traced_document_id(path: &str) -> Option<String> {
    let source = std::fs::read_to_string(path).ok()?;
    let (document, _) = lute_syntax::parse(&source);
    serde_yaml::from_str::<serde_yaml::Value>(&document.meta.raw_yaml)
        .ok()?
        .get("id")
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_string)
}

fn run_one_test(
    test_file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    no_derive: bool,
    shared: &Shared,
    cov: Option<&mut CoverageAccum>,
) -> Result<TestResult, ExitCode> {
    let text = match std::fs::read_to_string(test_file) {
        Ok(t) => t,
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot read {}: {e}", test_file.display());
            return Err(ExitCode::from(2));
        }
    };

    // The mock surfaces reuse `lute trace --mock`'s EXACT parser, in its
    // OPEN form: this family's legal set adds [`HARNESS_KEYS`], and a key
    // violation here is a per-test failure (exit 1, below) rather than the
    // mock family's parse error (exit 2). `parse_mock_surfaces` therefore
    // skips the closed-key gate and `closed_key_violations` supplies it.
    let mut mocks = match parse_mock_surfaces(&text) {
        Ok(m) => m,
        Err(d) => {
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.text());
            return Err(ExitCode::from(2));
        }
    };
    // `--no-derive` wins over the test's own `derive:` key (dsl 0.22.0 §6).
    if no_derive {
        mocks.derive = Some(false);
    }

    // Parse `file:` and `expect:` from the same document as a YAML value,
    // mirroring `parse_mock_yaml`'s hand-rolled navigation (no serde derive
    // dependency added to this crate).
    let top: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("lute: {}: malformed test yaml: {e}", test_file.display());
            return Err(ExitCode::from(2));
        }
    };
    let map = match top.as_mapping() {
        Some(m) => m,
        None => {
            eprintln!(
                "lute: {}: a test file must be a YAML mapping with a `file:` key",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };

    // #2(a) / D-B: the key set is closed at BOTH levels. A violation is a
    // per-test FAILURE (exit 1), not an I/O error (exit 2) — every offending
    // file must be named in one run, and the suite must keep going. T9.8's
    // acceptance test asks for exit 1 by name.
    let key_violations = closed_key_violations(map, &text, test_file);
    if !key_violations.is_empty() {
        // The document the test names, when `file:` says, as a run of it would.
        let lute_display = match lute_trace::mock_subject(&text) {
            Ok(Some(rel)) => {
                let base = test_file.parent().unwrap_or_else(|| Path::new("."));
                fold_parent_dirs(&base.join(rel)).display().to_string()
            }
            _ => String::new(),
        };
        return Ok(TestResult::refused(
            test_file,
            lute_display,
            "invalid",
            key_violations,
        ));
    }
    // 0.27 prerelease OT-F-3: every string `expect.state` value and where it
    // was written — member-checked once the document's schema is folded.
    let expect_state: Vec<(String, String, String)> = map
        .get("expect")
        .and_then(|e| e.get("state"))
        .and_then(|v| v.as_mapping())
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| {
            use lute_trace::YamlStep::{Key, Value};
            let (path, want) = (k.as_str()?, v.as_str()?);
            let at = lute_trace::yaml_span(&text, &[Key("expect"), Key("state"), Key(path), Value])
                .map_or_else(
                    || test_file.display().to_string(),
                    |s| format!("{}:{}:{}", test_file.display(), s.line, s.column),
                );
            Some((lute_trace::state_key(path), want.to_string(), at))
        })
        .collect();
    let rel = match lute_trace::mock_subject(&text) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!(
                "lute: {}: missing required `file:` (path to the `.lute` under test)",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
        Err(d) => {
            eprintln!("lute: {}: [{}] {}", test_file.display(), d.code, d.text());
            return Err(ExitCode::from(2));
        }
    };
    // dsl 0.22.0 §5: `entry: <id>` / `entries: [ids]` — the lore entries to
    // present, in order. 0.23.1: `beat: <id>` — one bundle `<beat>` (local
    // or canonical `<document id>.<beat id>`), as `lute trace --beat`.
    let entries: Vec<String> = match (map.get("entry"), map.get("entries")) {
        (None, None) => Vec::new(),
        (Some(serde_yaml::Value::String(id)), None) => vec![id.clone()],
        (None, Some(serde_yaml::Value::Sequence(ids))) if ids.iter().all(|i| i.is_string()) => ids
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        (Some(_), Some(_)) => {
            eprintln!(
                "lute: {}: name the entries to present with ONE of `entry: <id>` or \
                 `entries: [ids]`, not both",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
        _ => {
            eprintln!(
                "lute: {}: `entry:` must be one entry id and `entries:` a list of entry ids",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    let beat: Option<String> = match map.get("beat") {
        None => None,
        Some(serde_yaml::Value::String(id)) if !id.trim().is_empty() => Some(id.clone()),
        Some(_) => {
            eprintln!(
                "lute: {}: `beat:` must be one bundle beat id (`<beat id>` or \
                 `<document id>.<beat id>`)",
                test_file.display()
            );
            return Err(ExitCode::from(2));
        }
    };
    if beat.is_some() && !entries.is_empty() {
        eprintln!(
            "lute: {}: a test presents ONE of `beat:` or `entry:` / `entries:`, not both",
            test_file.display()
        );
        return Err(ExitCode::from(2));
    }

    let base = test_file.parent().unwrap_or_else(|| Path::new("."));
    let lute_path = base.join(&rel);
    // Shown folded (`tests/spine/../../lore/a.lute` → `lore/a.lute`), so a
    // report names one document one way wherever the test sits (ML-F9).
    let lute_display = fold_parent_dirs(&lute_path).display().to_string();

    // One test naming a document that no longer exists is that test's
    // failure, never the whole suite's abort.
    /// A test whose `file:` names no document (0.23.1).
    const E_TEST_FILE: &str = "E-TEST-FILE";
    /// A transcript needle naming what no presented line can carry (0.27).
    const E_TEST_NEEDLE: &str = "E-TEST-NEEDLE";
    if !lute_path.is_file() {
        // `file:` is relative to the test file: name the `../` spelling
        // when the path resolves from a directory above it.
        let hint = base
            .ancestors()
            .skip(1)
            .take(4)
            .enumerate()
            .find(|(_, dir)| dir.join(&rel).is_file())
            .map(|(up, _)| {
                format!(
                    " — `file:` is relative to the test file; did you mean `{}{rel}`?",
                    "../".repeat(up + 1)
                )
            })
            .unwrap_or_default();
        return Ok(TestResult::refused(
            test_file,
            lute_display.clone(),
            "invalid",
            vec![format!(
                "error [{E_TEST_FILE}] `file: {rel}` names no document ({lute_display} does not \
                 exist){hint}"
            )],
        ));
    }

    // 0.23.1: like `lute check <file>` and like the plays of this run, a
    // scenario test without `--project` resolves its document against the
    // nearest `lute.project.yaml` — announced once per project root.
    let discovered = match project {
        Some(_) => None,
        None => nearest_manifest_dir(&lute_path),
    };
    if let Some(root) = &discovered {
        let shown = crate::cwd_relative(&root.display().to_string());
        stderr_line(StderrLine::Note(
            root.clone(),
            format!(
                "lute: note: scenario tests use project {} (nearest lute.project.yaml); pass \
                 --project to choose another",
                if shown.is_empty() {
                    "."
                } else {
                    shown.as_str()
                }
            ),
        ));
    }
    let resolve_with = project.or(discovered.as_deref());
    if let Some(root) = resolve_with {
        if let Some(faults) = shared.schema_faults(root).filter(|faults| !faults.is_empty()) {
            return Ok(TestResult::refused(
                test_file,
                display_path(root),
                "invalid",
                faults,
            ));
        }
    }

    let model_doc = resolve_with.and_then(|_| shared.document(&lute_path));
    let standalone = if model_doc.is_none() {
        if resolve_with.is_some() {
            eprintln!(
                "lute: project model has no document {}",
                lute_path.display()
            );
            return Err(ExitCode::from(1));
        }
        let Some(built) = build_input(&lute_path, providers, None, None) else {
            return Err(ExitCode::from(2));
        };
        built.report_project_diags();
        Some(built)
    } else {
        None
    };
    let mut standalone_doc = None;
    let mut standalone_folded = None;
    if let Some(built) = standalone.as_ref() {
        let (mut doc, _) = lute_syntax::parse(&built.input.text);
        let _ = lute_check::desugar_document(&mut doc, &built.input);
        let (folded, _, _) = lute_check::fold_env(&doc, &built.input);
        standalone_doc = Some(doc);
        standalone_folded = Some(folded);
    }
    let (input, doc, folded, meta, resolve_error) = match model_doc {
        Some(document) => (
            &document.input,
            &document.doc,
            &document.folded,
            &document.folded.typed,
            document.resolve_error,
        ),
        None => {
            let built = standalone.as_ref().expect("standalone input is present");
            (
                &built.input,
                standalone_doc.as_ref().expect("standalone document is present"),
                standalone_folded.as_ref().expect("standalone fold is present"),
                &standalone_folded.as_ref().expect("standalone fold is present").typed,
                built.resolve_error,
            )
        }
    };
    if let Some(document) = model_doc {
        for diag in &document.resolve_diags {
            eprintln!(
                "lute: [{}] {}",
                diag.code,
                lute_core_span::plain_message(&diag.message)
            );
        }
    }
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return Err(ExitCode::from(1));
    }

    // 0.27 prerelease OT-F-2 / OT N-2: a needle speaker or attribute no
    // presented line can carry makes `transcriptContains` a sure miss and
    // `transcriptLacks` a vacuous pass — refused like a misspelt key, before
    // anything is walked, located at the needle.
    let item_at = |key: &str, i: usize| {
        use lute_trace::YamlStep::{Item, Key};
        lute_trace::yaml_span(&text, &[Key("expect"), Key(key), Item(i)]).map_or_else(
            || test_file.display().to_string(),
            |s| format!("{}:{}:{}", test_file.display(), s.line, s.column),
        )
    };
    let expect_items = |key: &'static str| {
        map.get("expect")
            .and_then(|e| e.get(key))
            .and_then(|v| v.as_sequence())
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(move |(i, v)| Some((key, i, v.as_str()?)))
    };
    let needles = lute_runtime::NeedleVocab::of(&input, &meta);
    let needle_problems: Vec<String> = expect_items("transcriptContains")
        .chain(expect_items("transcriptLacks"))
        .filter_map(|(key, i, n)| {
            let why = lute_trace::exec::record::needle_problem(n, &needles)?;
            Some(format!(
                "{}: error [{E_TEST_NEEDLE}] `expect.{key}` {why}",
                item_at(key, i)
            ))
        })
        .collect();
    if !needle_problems.is_empty() {
        return Ok(TestResult::refused(
            test_file,
            lute_display,
            "invalid",
            needle_problems,
        ));
    }

    // 0.27 prerelease OT N-2: an `expect.facts` / `expect.notFacts` atom
    // naming an undeclared relation, the wrong arity or a foreign argument
    // names no fact that can hold — a `notFacts` one would pass vacuously.
    // Refused as a seed `facts:` entry is ([`lute_check::check_atom`], a
    // derived relation included), located at the atom.
    let expect_atoms: Vec<(&str, usize, &str)> = expect_items("facts")
        .chain(expect_items("notFacts"))
        .collect();
    if !expect_atoms.is_empty() {
        let folded = folded;
        let problems: Vec<String> = expect_atoms
            .iter()
            .flat_map(|&(key, i, atom)| {
                let whys: Vec<String> = match lute_syntax::datalog::parse_fact(
                    &crate::play_expect::canonical_atom(atom),
                ) {
                    Err(_) => Vec::new(),
                    Ok(pattern) => lute_check::check_atom(
                        &folded.env.rel_vocab,
                        &folded.env.domains,
                        &pattern.relation,
                        &pattern.args,
                        false,
                        lute_core_span::Span {
                            byte_start: 0,
                            byte_end: 0,
                            line: 0,
                            column: 0,
                            utf16_range: (0, 0),
                        },
                    )
                    .into_iter()
                    .map(|d| d.message)
                    .collect(),
                };
                whys.into_iter().map(move |why| {
                    format!(
                        "{}: error [{}] `expect.{key}` entry `{atom}` names no fact that can \
                         hold: {why}",
                        item_at(key, i),
                        lute_trace::E_TRACE_MOCK_FACT
                    )
                })
            })
            .collect();
        if !problems.is_empty() {
            return Ok(TestResult::refused(
                test_file,
                lute_display,
                "invalid",
                problems,
            ));
        }
    }

    // 0.27 prerelease OT-F-3: an `expect.state` value outside its path's
    // closed domain (`{ domain: K }` / `{ entity: K }`, an inline enum) can
    // never hold — refused like a seed outside it, with the members and the
    // nearest one, not reported as a miss. `unset` is the spelling of "no
    // value" (an implicit choice slot, a quest), never a typo.
    if !expect_state.is_empty() {
        let folded = folded;
        let problems: Vec<String> = expect_state
            .iter()
            .filter(|(_, want, _)| want != "unset")
            .filter_map(|(path, want, at)| {
                let why = lute_trace::mock::state_member_problem(&folded.env.state, path, want)?;
                Some(format!(
                    "{at}: error [E-TRACE-MOCK-TYPE] `expect.state.{path}: {want}` can never \
                     hold: {why}"
                ))
            })
            .collect();
        if !problems.is_empty() {
            return Ok(TestResult::refused(
                test_file,
                lute_display,
                "invalid",
                problems,
            ));
        }
    }

    // `expect.eligible`, a map key naming an entry by its `<document
    // id>.<entry id>` alias resolved to the entry id (dsl 0.26.0 §7, T3-10).
    let eligible_want: Option<serde_yaml::Value> = map
        .get("expect")
        .and_then(|e| e.get("eligible"))
        .map(|want| match want {
            serde_yaml::Value::Mapping(m) => {
                let doc_id = serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
                    .ok()
                    .and_then(|v: serde_yaml::Value| {
                        v.get("id").and_then(serde_yaml::Value::as_str).map(str::to_string)
                    });
                serde_yaml::Value::Mapping(
                    m.iter()
                        .map(|(k, v)| {
                            let k = match k.as_str() {
                                Some(id) => serde_yaml::Value::String(
                                    lute_trace::entry_local_id(&doc, doc_id.as_deref(), id)
                                        .to_string(),
                                ),
                                None => k.clone(),
                            };
                            (k, v.clone())
                        })
                        .collect(),
                )
            }
            other => other.clone(),
        });
    // A map-form `expect.eligible` judges every entry / bundle beat of the
    // file it names under the test's mocks, presented or not (dsl 0.24.0,
    // T3-5) — so a lore test may carry it alone, presenting nothing.
    let judges_eligibility_by_id = eligible_want
        .as_ref()
        .is_some_and(serde_yaml::Value::is_mapping);
    let mut lore_lookup_only = false;
    // dsl 0.26.0 §7 (T1-7): a test asserting `eligible:` judges the
    // presentation, not a walk the engine would never make — an entry,
    // bundle beat or scene whose `when` is false under the mocks is shown on
    // its head and its body is not walked (no bridge answer it would need).
    mocks.gate_eligibility = eligible_want.is_some();

    // T1-13: a lore document is looked up, not played — `lute trace` refuses
    // one without `--entry`/`--beat`, and a test naming it without `entry:`
    // / `entries:` / `beat:` would walk nothing and PASS `end: complete`.
    // Say so instead of asserting against nothing — unless it judges the
    // entries by id (map-form `eligible:`) or the seeded world (`facts:` /
    // `notFacts:`), which presenting nothing answers.
    let judges_world = map
        .get("expect")
        .is_some_and(|e| e.get("facts").is_some() || e.get("notFacts").is_some());
    if entries.is_empty() && beat.is_none() {
        if folded.doc_kind == lute_check::DocKind::Lore
            && (judges_eligibility_by_id || judges_world)
        {
            lore_lookup_only = true;
        } else if folded.doc_kind == lute_check::DocKind::Lore {
            let ids: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
            let beats: Vec<&str> = doc.beats.iter().map(|b| b.id.as_str()).collect();
            return Ok(TestResult::refused(
                test_file,
                lute_display,
                "invalid",
                vec![format!(
                    "error [E-TEST-LORE] `file: {rel}` is a lore document — a lore document is \
                     looked up, not played, so there is no walk to assert against until the \
                     test names what to present: `entry: <id>` or `entries: [ids]` (declared: \
                     {}), or `beat: <id>` (declared: {}), or judges them with `expect: \
                     {{ eligible: {{ <id>: true|false }} }}` or the seeded world with `expect: \
                     {{ facts: [...] }}`",
                    if ids.is_empty() {
                        "none".to_string()
                    } else {
                        ids.join(", ")
                    },
                    if beats.is_empty() {
                        "none".to_string()
                    } else {
                        beats.join(", ")
                    }
                )],
            ));
        }
    }

    // T1-14: mocked facts are judged against the project's producers.
    let project_asserts = if mocks.facts.is_empty() {
        None
    } else {
        shared.for_document(&lute_path, project, providers)
    };
    // dsl 0.26.0 §7 (T3-5), round-5 T3-24: an `accepts:`, a `quests:` seed or
    // an `expect.quests` naming a quest this document does not declare
    // resolves against every quest of the project.
    let expected_quests: Vec<&str> = map
        .get("expect")
        .and_then(|v| v.get("quests"))
        .and_then(|v| v.as_mapping())
        .map(|m| m.keys().filter_map(|k| k.as_str()).collect())
        .unwrap_or_default();
    let named_quests: Vec<&str> = mocks
        .accepts
        .iter()
        .map(String::as_str)
        .chain(mocks.state.iter().filter_map(|(p, _, _)| seeded_quest(p)))
        .chain(expected_quests.iter().copied())
        .collect();
    if !named_quests.is_empty() {
        let foreign = named_quests
            .iter()
            .any(|id| !doc.quests.iter().any(|q| q.id == *id));
        if foreign {
            mocks.project_quests =
                resolve_with.and_then(|root| shared.project_quests(root, providers));
        }
    }
    // What a quest another document declares starts the walk as (T3-24).
    let foreign_start = ForeignQuestStart {
        project: mocks.project_quests.clone(),
        seeds: mocks
            .state
            .iter()
            .filter_map(|(p, v, _)| seeded_quest(p).map(|id| (id.to_string(), v.clone())))
            .collect(),
        accepted: mocks.accepts.clone(),
    };
    // One envelope: the document is gated on its project's verdict (what
    // `lute trace --project` and `lute play` gate on), else the standalone
    // check. An unpresented entry / beat is judged under the same mocks and
    // verdict (T3-5).
    let checked = match resolve_with.and_then(|root| shared.gate(root, &lute_path)) {
        Some(check) => check,
        None => model_doc.map_or_else(|| lute_check::check(input), |d| d.check.clone()),
    };
    let eligibility_mocks = judges_eligibility_by_id.then(|| (mocks.clone(), checked.clone()));
    let (report, exit) = if let Some(beat) = &beat {
        trace_beat_with_check(&input, checked, mocks, beat, project_asserts.as_ref())
    } else if lore_lookup_only {
        trace_entries_with_check(&input, checked, mocks, &[], project_asserts.as_ref())
    } else if entries.is_empty() {
        trace_with_check(&input, checked, mocks, project_asserts.as_ref())
    } else {
        let ids: Vec<&str> = entries.iter().map(String::as_str).collect();
        trace_entries_with_check(&input, checked, mocks, &ids, project_asserts.as_ref())
    };

    // A refused trace (document check errors or invalid mocks) cannot be
    // asserted against — mark the whole test failed and print every
    // diagnostic the refusal is holding. The harness used to inspect these
    // codes only to choose between two canned strings and then drop the
    // vector, so a stale `choose:` id, a stale branch id and a stale
    // `state:` path were indistinguishable (#25, T9.11).
    if let TraceExit::Refused(diags) = &exit {
        // A `bridges:` answer's diagnostic is anchored in THIS file's text
        // (dsl 0.24.0 §5), every other one in the traced document.
        let test_display = test_file.display().to_string();
        let lines: Vec<String> = diags
            .iter()
            .map(|d| {
                // A diagnostic with no place in the document is about the
                // test's own input (every mock here comes from this file):
                // a refused raise at its `occasions:`, anything else at the
                // file, never at a made-up `:0:0` of the document.
                let at = if d.provenance.as_deref() == Some(lute_trace::MOCK_TEXT) {
                    format!("{test_display}:{}:{}", d.span.line, d.span.column)
                } else if d.span.line > 0 {
                    format!("{lute_display}:{}:{}", d.span.line, d.span.column)
                } else {
                    let key = (d.code == lute_manifest::semantics::gates::E_OCCASION_GATE).then_some("occasions");
                    match key
                        .and_then(|k| lute_trace::yaml_span(&text, &[lute_trace::YamlStep::Key(k)]))
                    {
                        Some(s) => format!("{test_display}:{}:{}", s.line, s.column),
                        None => test_display.clone(),
                    }
                };
                let severity = crate::output::severity_str(d.severity);
                format!(
                    "{at}: {severity} [{}] {}",
                    d.code,
                    yaml_key_spelling(&d.text())
                )
            })
            .collect();
        return Ok(TestResult::refused(
            test_file,
            lute_display,
            "refused",
            lines,
        ));
    }

    let exit_str = match exit {
        TraceExit::Complete => "complete",
        TraceExit::Incomplete => "incomplete",
        TraceExit::Refused(_) => unreachable!("handled above"),
    };
    // How the walk ended: `terminal` when it completed with the project's
    // `terminal:` holding.
    let ended = match exit_str {
        "complete" if report.terminal == Some(true) => "terminal",
        e => e,
    };

    if let Some(cov) = cov {
        accumulate_coverage(cov, &report);
    }

    // §4.4's auto-pick is legal and deliberate, but it was silent — and
    // silence is what let a dropped selection green a test against the arm
    // the file excluded (#2(d), T9.8).
    let autopicked: Vec<String> = report
        .decisions
        .iter()
        .filter(|d| d.auto && matches!(d.construct.as_str(), "branch" | "hub"))
        .map(|d| format!("{} -> {}", d.id, d.outcome))
        .collect();

    let expect = map.get("expect").and_then(|v| v.as_mapping());
    let mut expectations = Vec::new();

    if let Some(expect) = expect {
        // end: complete | terminal | incomplete | error — how the walk
        // ended.
        if let Some(want) = expect.get("end").and_then(|v| v.as_str()) {
            expectations.push(ExpectResult {
                not_raised: None,
                why: None,
                kind: "end",
                subject: String::new(),
                expected: want.to_string(),
                actual: Some(ended.to_string()),
                passed: want == ended,
            });
        }

        // transcriptContains / transcriptLacks: [substrings] — against the
        // presented content lines in the one canonical transcript form
        // (`lute_trace::exec::said_line`, dsl 0.24.0 T1-2), the form `lute
        // play` matches too; never the trace's human rendering (headers,
        // decisions, staging). A needle's attribute block binds the line it
        // lands on (0.27, T1-11).
        let transcript = if expect.contains_key("transcriptContains")
            || expect.contains_key("transcriptLacks")
        {
            report.said()
        } else {
            String::new()
        };
        for (kind, want_present) in [("transcriptContains", true), ("transcriptLacks", false)] {
            let Some(list) = expect.get(kind).and_then(|v| v.as_sequence()) else {
                continue;
            };
            for sub in list.iter().filter_map(|i| i.as_str()) {
                // A scene walk is one step.
                let miss = lute_trace::exec::record::judge(&transcript, &[], sub, want_present);
                expectations.push(ExpectResult {
                    not_raised: None,
                    why: None,
                    kind,
                    subject: String::new(),
                    expected: sub.to_string(),
                    passed: miss.is_none(),
                    actual: Some(miss.unwrap_or_else(|| {
                        if want_present { "present" } else { "absent" }.to_string()
                    })),
                });
            }
        }

        // options: { <choice id>: [option ids] } (dsl 0.22.0 §5) — the
        // options the walk actually offered at that branch/hub, as a set:
        // every presentation's eligible choices, unioned (a hub is offered
        // once per visit). `None` when the walk never presented it.
        if let Some(options) = expect.get("options").and_then(|v| v.as_mapping()) {
            for (k, v) in options {
                let Some(id) = k.as_str() else { continue };
                let want: Option<BTreeSet<&str>> = v
                    .as_sequence()
                    .and_then(|s| s.iter().map(|i| i.as_str()).collect());
                let presented: Vec<&lute_trace::Decision> = report
                    .decisions
                    .iter()
                    .filter(|d| matches!(d.construct.as_str(), "branch" | "hub") && d.id == id)
                    .collect();
                let actual: Option<BTreeSet<&str>> = (!presented.is_empty()).then(|| {
                    presented
                        .iter()
                        .flat_map(|d| d.eligible.iter().map(String::as_str))
                        .collect()
                });
                let set_text = |s: &BTreeSet<&str>| {
                    format!("[{}]", s.iter().copied().collect::<Vec<_>>().join(", "))
                };
                expectations.push(ExpectResult {
                    not_raised: None,
                    why: None,
                    kind: "options",
                    subject: id.to_string(),
                    expected: match &want {
                        Some(w) => set_text(w),
                        None => "a list of choice ids".to_string(),
                    },
                    actual: actual.as_ref().map(set_text),
                    passed: want.is_some() && want == actual,
                });
            }
        }

        // state: { path: literal } — against the FINAL effective state (T2-5):
        // the last write, else the test's own seed, else the declared
        // `default:` — the same read order every guard in the walk used. A
        // path the walk never wrote used to report "never written" even when
        // its default (or the test's seed) was exactly the expected value.
        // A reserved path of the document's own quests reads its engine
        // default before any write, as in a play (T3-65).
        if let Some(state) = expect.get("state").and_then(|v| v.as_mapping()) {
            let final_state = &report.final_state;
            for (k, v) in state {
                let Some(path) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                let key = lute_trace::state_key(path);
                let actual = final_state
                    .get(&key)
                    .cloned()
                    .or_else(|| reserved_quest_default(&doc, &key).map(str::to_string));
                expectations.push(ExpectResult {
                    not_raised: None,
                    why: None,
                    kind: "state",
                    subject: path.to_string(),
                    expected: want.clone(),
                    // T9.9: the absent case stays absent all the way to the
                    // renderer. Substituting a display string here is what
                    // let a miss line print its two sides identically.
                    passed: actual.as_deref() == Some(want.as_str()),
                    actual,
                });
            }
        }

        // quests: { id: unset|active|complete|failed } — against the
        // lifecycle the trace ran (dsl 0.21.0 §7a.4); a quest another
        // document of the project declares, against where the walk left it
        // (round-5 T3-24, [`ForeignQuestStart::judge`]).
        if let Some(quests) = expect.get("quests").and_then(|v| v.as_mapping()) {
            let final_quests = final_quests(&report, doc);
            for (k, v) in quests {
                let Some(id) = k.as_str() else { continue };
                let want = yaml_scalar_text(v).unwrap_or_default();
                let (actual, why) = match final_quests.get(id) {
                    Some(state) => (Some(state.clone()), None),
                    None => foreign_start.judge(&report, id, &final_quests),
                };
                expectations.push(ExpectResult {
                    not_raised: None,
                    why,
                    kind: "quests",
                    subject: id.to_string(),
                    passed: QUEST_STATES.contains(&want.as_str())
                        && actual.as_deref() == Some(want.as_str()),
                    expected: want,
                    actual,
                });
            }
        }

        // facts / notFacts: [atoms] — every fact that holds when the walk
        // ends, after derivation (0.23.1), judged as `lute play`'s
        // end-of-play expectation judges it. A fact of a relation whose
        // derivation read undecided state neither holds nor fails to hold.
        for (kind, want_held) in [("facts", true), ("notFacts", false)] {
            let Some(list) = expect.get(kind).and_then(|v| v.as_sequence()) else {
                continue;
            };
            for atom in list.iter().filter_map(yaml_scalar_text) {
                let Some((rel, _)) = crate::play_expect::parse_atom(&atom) else {
                    expectations.push(ExpectResult {
                        not_raised: None,
                        why: None,
                        kind,
                        subject: atom.clone(),
                        expected: "a ground atom `rel(a, b)`".to_string(),
                        actual: Some("not a ground atom".to_string()),
                        passed: false,
                    });
                    continue;
                };
                let canonical = crate::play_expect::canonical_atom(&atom);
                let held = if report.final_facts.contains(&canonical) {
                    "holds"
                } else if report.final_undecided.contains(&rel) {
                    "unknown"
                } else {
                    "does not hold"
                };
                let want = if want_held { "holds" } else { "does not hold" };
                expectations.push(ExpectResult {
                    not_raised: None,
                    why: None,
                    kind,
                    subject: atom.clone(),
                    expected: want.to_string(),
                    actual: Some(held.to_string()),
                    passed: held == want,
                });
            }
        }

        // eligible: bool | { <id>: bool } (0.23.1) — the `when` verdict of
        // the presented entry/beat. Trace presents it either way (the
        // engine's gate, shown, not enforced); this asserts the verdict. A
        // map key naming an entry / bundle beat of the file this test did
        // NOT present is judged on its own under the same mocks (dsl 0.24.0,
        // T3-5) — `eligible:` covers the whole file, not only what played.
        if let Some(want) = &eligible_want {
            let presented = presented_eligibility(&report);
            // dsl 0.28.0 (T1-25): `{ <id>: { false: <reason> } }` also
            // names the premise that must close it (`spentBy`, `when`, …) —
            // so a beat closed by something else does not pass vacuously.
            let parse = |v: &serde_yaml::Value| -> (Option<bool>, Option<String>) {
                match v {
                    serde_yaml::Value::Mapping(m) if m.len() == 1 => {
                        match m.get(serde_yaml::Value::Bool(false)).map(yaml_scalar_text) {
                            Some(reason) => (Some(false), Some(reason.unwrap_or_default())),
                            None => (None, None),
                        }
                    }
                    other => (other.as_bool(), None),
                }
            };
            let wants: Vec<(Option<String>, Option<bool>, Option<String>)> = match want {
                serde_yaml::Value::Mapping(m) => m
                    .iter()
                    .map(|(k, v)| {
                        let (b, reason) = parse(v);
                        (k.as_str().map(str::to_string), b, reason)
                    })
                    .collect(),
                other => vec![(None, other.as_bool(), None)],
            };
            for (id, want, reason) in wants {
                let mut matched: Vec<(String, Option<bool>)> = presented
                    .iter()
                    .filter(|(p, _)| id.as_deref().is_none_or(|id| names_presented(p, id)))
                    .cloned()
                    .collect();
                let mut alone = None;
                if matched.is_empty() {
                    if let (Some(id), Some((mocks, checked))) = (id.as_deref(), &eligibility_mocks)
                    {
                        alone =
                            eligibility_alone(doc, input, checked, mocks, id, project_asserts.as_ref());
                        matched = alone
                            .as_ref()
                            .map(presented_eligibility)
                            .unwrap_or_default();
                    }
                }
                // Round-5 T3-12: an `eligible: true` miss names the false
                // premise, as the implicit miss does. OT-F-10: a key naming
                // nothing the document declares gets a did-you-mean.
                let why = if matched.is_empty() {
                    id.as_deref().and_then(|id| {
                        let names = doc
                            .entries
                            .iter()
                            .map(|e| e.id.as_str())
                            .chain(doc.beats.iter().map(|b| b.id.as_str()));
                        lute_manifest::suggest::nearest(id, names, 3)
                            .map(|k| format!("did you mean `{k}`?"))
                    })
                } else {
                    (want == Some(true))
                        .then(|| matched.iter().find(|(_, e)| *e == Some(false)))
                        .flatten()
                        .map(|(p, _)| ineligible_why(alone.as_ref().unwrap_or(&report), p))
                };
                let judged = alone.as_ref().unwrap_or(&report);
                let by = |p: &str| judged.ineligible_by.get(p).copied();
                let actual = (!matched.is_empty()).then(|| {
                    matched
                        .iter()
                        .map(|(p, e)| match (e, by(p).filter(|_| reason.is_some())) {
                            (Some(true), _) => "true".to_string(),
                            (Some(false), Some(kind)) => format!("false ({kind})"),
                            (Some(false), None) => "false".to_string(),
                            (None, _) => "unknown".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                let known = |r: &str| lute_runtime::session::PREMISE_KINDS.contains(&r);
                let expected = match (want, &reason) {
                    (Some(_), Some(r)) if known(r) => format!("false ({r})"),
                    (Some(_), Some(r)) => format!(
                        "false for a reason, and `{r}` is none (one of: {})",
                        lute_runtime::session::PREMISE_KINDS.join(", ")
                    ),
                    (Some(b), None) => b.to_string(),
                    (None, _) => "true, false or `{ false: <reason> }`".to_string(),
                };
                let reason_holds = reason.as_deref().is_none_or(|r| {
                    known(r)
                        && matched
                            .iter()
                            .all(|(p, e)| *e != Some(false) || by(p) == Some(r))
                });
                expectations.push(ExpectResult {
                    not_raised: (want == Some(true))
                        .then(|| matched.iter().find(|(_, e)| *e == Some(false)))
                        .flatten()
                        .and_then(|(p, _)| {
                            alone.as_ref().unwrap_or(&report).not_raised.get(p).cloned()
                        }),
                    why,
                    kind: "eligible",
                    subject: id.unwrap_or_default(),
                    passed: want.is_some()
                        && !matched.is_empty()
                        && matched.iter().all(|(_, e)| *e == want)
                        && reason_holds,
                    expected,
                    actual,
                });
            }
        }

        // accepts: [quest ids] (dsl 0.24.0, T3-5) — the quests the walk's
        // `::accept{quest=…}` accepted, as a set. A scene never holds the
        // quest, so its lifecycle (`quests:`) cannot show the accept.
        if let Some(v) = expect.get("accepts") {
            let want: Option<BTreeSet<&str>> = v
                .as_sequence()
                .and_then(|s| s.iter().map(|i| i.as_str()).collect());
            let actual: BTreeSet<&str> = report
                .steps
                .iter()
                .filter_map(|s| match s {
                    lute_trace::Step::Accept { quest, .. } => Some(quest.as_str()),
                    _ => None,
                })
                .collect();
            let set_text = |s: &BTreeSet<&str>| {
                format!("[{}]", s.iter().copied().collect::<Vec<_>>().join(", "))
            };
            expectations.push(ExpectResult {
                not_raised: None,
                why: None,
                kind: "accepts",
                subject: String::new(),
                expected: match &want {
                    Some(w) => set_text(w),
                    None => "a list of quest ids".to_string(),
                },
                actual: Some(set_text(&actual)),
                passed: want.as_ref() == Some(&actual),
            });
        }
    }

    // #2(d) / D-B: `all()` over an empty vector is `true`, so a test that
    // recognised no expectation reported PASS. A test that asserts nothing is
    // not a passing test.
    if expectations.is_empty() {
        let mut r = TestResult::refused(
            test_file,
            lute_display,
            exit_str,
            vec![format!(
                "error [E-TEST-NO-EXPECT] this test declares no recognised expectation \
                 (legal `expect:` keys: {}); a test that asserts nothing cannot pass",
                TEST_EXPECT_KEYS.join(", ")
            )],
        );
        r.autopicked = autopicked;
        r.end = Some(ended.to_string());
        return Ok(r);
    }

    // T1-13: an incomplete trace halted at an unknown guard, so everything
    // after it — including whatever the expectations above were written
    // about — was never walked. It passed whenever the test did not mention
    // `end:`. Now it fails unless the test opts in with `end: incomplete`.
    let declares_exit = expect.is_some_and(|e| e.contains_key("end"));
    if exit_str == "incomplete" && !declares_exit {
        expectations.push(ExpectResult {
            not_raised: None,
            why: None,
            kind: "end",
            subject: IMPLICIT_EXIT.to_string(),
            expected: "complete".to_string(),
            actual: Some(exit_str.to_string()),
            passed: false,
        });
    }

    // dsl 0.26.0 §7 (T1-7): a walk of an entry, bundle beat or scene the
    // engine would never present — its `when` false under these mocks —
    // proves nothing about play (a contract test of another author's file
    // silently went hollow). It fails unless the test asserts `eligible:`.
    let asserted = eligible_want.as_ref();
    for (id, eligible) in presented_eligibility(&report) {
        if eligible == Some(false) && !eligibility_asserted(&id, asserted) {
            expectations.push(ExpectResult {
                not_raised: report.not_raised.get(&id).cloned(),
                why: Some(ineligible_why(&report, &id)),
                kind: "eligible",
                subject: id,
                expected: IMPLICIT_ELIGIBLE.to_string(),
                actual: Some("false".to_string()),
                passed: false,
            });
        }
    }

    let passed = expectations.iter().all(|e| e.passed);
    // Prerelease N3: a scene decided ineligible is judged by `eligible:` (or
    // fails on it above) — the trace's "shows it as if it had been presented"
    // note would contradict both.
    let scene_ineligible = report
        .scene_eligible
        .as_ref()
        .is_some_and(|(_, e)| *e == Some(false));

    Ok(TestResult {
        test_file: test_file.to_path_buf(),
        kind: "test",
        lute_file: lute_display,
        exit: exit_str.to_string(),
        end: Some(ended.to_string()),
        passed,
        expectations,
        misses: Vec::new(),
        autopicked,
        refusal: None,
        unresolved: report.unresolved.clone(),
        forced_unknown: report.forced_unknown.clone(),
        visited: report
            .decisions
            .iter()
            .flat_map(|decision| {
                [
                    format!("{}:{}", decision.construct, decision.id),
                    format!("{}:{}:{}", decision.construct, decision.id, decision.outcome),
                ]
            })
            .collect(),
        completed_quests: report
            .decisions
            .iter()
            .filter(|decision| {
                decision.construct == "quest"
                    && decision.outcome == "complete"
                    && decision.guard.as_deref() != Some("seeded")
            })
            .map(|decision| decision.id.clone())
            .collect(),
        notes: report
            .notes
            .iter()
            .filter(|n| {
                (n.starts_with(lute_trace::NOTE_BEAT_WHEN) && !scene_ineligible)
                    || n.starts_with(lute_trace::NOTE_ACCEPT_SPENT)
            })
            .cloned()
            .chain(ineligible_notes(&report, asserted))
            .collect(),
    })
}

/// What `lute test` needs to know about a `*.play.yaml` before running it.
enum PlayScan {
    /// No `expect:` anywhere — not a test; `lute play` runs it, `lute test`
    /// does not.
    NoExpect,
    /// Carries an `expect:`; `declares_exit` when the top-level one names
    /// `exit:` (the opt-in for a halted play).
    Expect { declares_exit: bool },
    /// Unreadable or not YAML — reported, never skipped in silence.
    Broken(String),
}

/// Does this play carry any `expect:` — top-level or on any step?
fn scan_play(play_file: &Path) -> PlayScan {
    let text = match std::fs::read_to_string(play_file) {
        Ok(t) => t,
        Err(e) => {
            return PlayScan::Broken(format!(
                "cannot read the play: {}",
                lute_manifest::io_reason(&e)
            ))
        }
    };
    let top: serde_yaml::Value = match serde_yaml::from_str(&text) {
        Ok(v) => v,
        Err(e) => return PlayScan::Broken(format!("malformed play YAML: {e}")),
    };
    let top_expect = top.get("expect");
    let step_expect = top
        .get("steps")
        .and_then(|s| s.as_sequence())
        .is_some_and(|steps| steps.iter().any(|s| s.get("expect").is_some()));
    if top_expect.is_none() && !step_expect {
        return PlayScan::NoExpect;
    }
    PlayScan::Expect {
        declares_exit: top_expect.is_some_and(|e| e.get("end").is_some()),
    }
}

/// Run one expect-carrying play (dsl 0.22.0 §4) through the SAME walk and
/// judge `lute play` uses. `None` for a play without `expect:`. The project
/// is `--project`, else the nearest `lute.project.yaml` above the play. A
/// play that could not run (usage error, no project) is a FAILURE naming
/// why; a play that halted fails unless its top-level `expect:` declares the
/// exit — the rule an incomplete trace follows (T1-13). When `cov` is `Some`,
/// every document, beat and entry the play presented counts as covered, and
/// every branch/hub it answered joins the choice rows (T3-20).
fn run_one_play(
    play_file: &Path,
    project: Option<&Path>,
    no_derive: bool,
    shared: &Shared,
    cov: Option<&mut CoverageAccum>,
) -> Option<TestResult> {
    // One `error:` line per usage error, each plain text.
    let refused = |lute_file: String, why: &str| {
        let lines = why
            .lines()
            .map(|l| format!("error: {}", lute_core_span::plain_message(l)))
            .collect();
        let mut r = TestResult::refused(play_file, lute_file, "invalid", lines);
        r.kind = "play";
        Some(r)
    };
    let declares_exit = match scan_play(play_file) {
        PlayScan::NoExpect => return None,
        PlayScan::Expect { declares_exit } => declares_exit,
        PlayScan::Broken(why) => return refused(String::new(), &why),
    };
    let Some(project_dir) = project_dir_of(play_file, project) else {
        return refused(
            String::new(),
            "no `lute.project.yaml` above this play — a play runs a project; pass \
             `--project <dir>`",
        );
    };
    let project_display = display_path(&project_dir);
    let Some(play_project) = shared.plays.get(&project_dir) else {
        return refused(project_display, "the project's compiled play input is unavailable");
    };
    let run = match crate::play::run_play_for_test(play_project, play_file, !no_derive) {
        Ok(run) => run,
        Err(why) => return refused(project_display, &why),
    };
    if let Some(cov) = cov {
        cov.plays += 1;
        let mut canonical: BTreeMap<String, String> = BTreeMap::new();
        let mut canon = |doc: &str| -> String {
            canonical
                .entry(doc.to_string())
                .or_insert_with(|| canonical_key(&project_dir.join(doc)))
                .clone()
        };
        for doc in &run.presented_docs {
            let c = canon(doc);
            cov.traced_files.insert(c.clone());
            cov.play_files.insert(c);
        }
        for (doc, id) in &run.presented {
            let unit = (canon(doc), id.clone());
            cov.units.insert(unit.clone());
            cov.play_units.insert(unit);
        }
        for c in &run.choices {
            let file = display_path(&project_dir.join(&c.document));
            let row = cov.choice_row(&canon(&c.document), &file, &c.id);
            row.chosen.extend(c.chose.iter().cloned());
            row.eligible.extend(c.offered.iter().cloned());
            row.total = row.total.max(c.total);
        }
    }
    let mut misses = run.misses;
    if run.exit != "complete" && !declares_exit {
        misses.push(ExpectMiss {
            step: None,
            label: None,
            occasion: None,
            repetition: None,
            key: "end".to_string(),
            expected: "complete (a halted play fails unless its top-level `expect:` declares \
                       how it ends, e.g. `expect: { end: incomplete }`)"
                .to_string(),
            actual: run.exit.to_string(),
        });
    }
    Some(TestResult {
        test_file: play_file.to_path_buf(),
        kind: "play",
        lute_file: project_display,
        exit: run.exit.to_string(),
        end: Some(run.end.to_string()),
        passed: misses.is_empty(),
        expectations: Vec::new(),
        misses,
        autopicked: Vec::new(),
        refusal: None,
        unresolved: Vec::new(),
        forced_unknown: Vec::new(),
        visited: run
            .presented
            .iter()
            .flat_map(|(document, id)| {
                [
                    format!("document:{document}"),
                    format!("branch:{document}:{id}"),
                    format!("hub:{document}:{id}"),
                ]
            })
            .chain(run.choices.iter().flat_map(|choice| {
                choice
                    .chose
                    .as_ref()
                    .map(|option| {
                        [
                            format!("choice:{}:{}.{}", choice.document, choice.id, option),
                            format!("branch:{}:{}.{}", choice.document, choice.id, option),
                            format!("hub:{}:{}.{}", choice.document, choice.id, option),
                        ]
                    })
                    .into_iter()
                    .flatten()
            }))
            .collect(),
        completed_quests: run.completed_quests,
        notes: run.notes,
    })
}

/// The `subject` of the `exit` expectation every test carries implicitly
/// when it declares none: an incomplete trace fails (T1-13).
pub(super) const IMPLICIT_EXIT: &str = "implicit";

/// One unresolved atom's mock hint (`lute-trace` renders it as a `lute
/// trace` flag, `--state p=<value>` / `--fact "f"`) in the spelling a
/// `*.test.yaml` can actually use — the T9.11 rule `yaml_key_spelling`
/// applies to refusals, applied to the hint an author acts on next.
pub(super) fn yaml_atom_hint(atom: &str) -> String {
    if let Some((path, value)) = atom
        .strip_prefix("--state ")
        .and_then(|rest| rest.split_once('='))
    {
        return format!("`state: {{ {path}: {value} }}`");
    }
    if let Some(fact) = atom.strip_prefix("--fact ") {
        return format!("`facts: [{fact}]`");
    }
    if let Some((id, list)) = atom
        .strip_prefix("--choose ")
        .and_then(|rest| rest.split_once('='))
    {
        let list = list.split(',').collect::<Vec<_>>().join(", ");
        return format!("`choose: {{ {id}: [{list}] }}`");
    }
    atom.to_string()
}

/// The hint list for one unresolved entry, `, `-joined.
pub(super) fn yaml_atom_hints(u: &UnresolvedEntry) -> String {
    u.atoms
        .iter()
        .map(|a| yaml_atom_hint(a))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `expected` text of the `eligible` expectation a test carries
/// implicitly for a presented entry / beat / scene whose `when` is false
/// (dsl 0.26.0 §7, T1-7).
pub(super) const IMPLICIT_ELIGIBLE: &str = "an eligible presentation, or an `eligible:` assertion";

/// Prerelease N3, round-5 T3-12: the premise that makes presented `id`
/// ineligible, named for the miss line — an entry an earlier read spent
/// (which read), else the verdict the session's eligibility rule gave it
/// ([`TraceReport::premises`]: its `when`, its `after:` / `after=` with the
/// mocks it needs, a spent `once`, a holding `spentBy`), else its `when`.
fn ineligible_why(report: &TraceReport, id: &str) -> String {
    let entry_heads: Vec<Option<&String>> = report
        .steps
        .iter()
        .filter_map(|s| match s {
            lute_trace::Step::Entry { id: e, spent, .. } if e == id => Some(spent.as_ref()),
            _ => None,
        })
        .collect();
    if let Some(at) = entry_heads.iter().position(Option::is_some) {
        let once = entry_heads[at].map_or("", String::as_str);
        let flag = if once == "run" { "read" } else { "everRead" };
        let by = if at > 0 {
            "an earlier presentation in this test read it".to_string()
        } else if once == "run" {
            format!("the mocked `entriesRead: {{ run: [{id}] }}` read it")
        } else {
            format!("the mocked `entriesRead:` (`{{ user: [{id}] }}` or `run:`) read it")
        };
        return format!("it is `once=\"{once}\"` and already spent — {by} (`entry.{id}.{flag}`)");
    }
    match report.premises.get(id) {
        Some(why) => why.clone(),
        None => "its `when` is false".to_string(),
    }
}

/// Every presented lore entry / bundle beat — and the traced scene itself
/// (dsl 0.26.0 §7, T1-7) — with its eligibility verdict, in presentation
/// order: `Some(true)` eligible (or no `when`), `Some(false)` not, `None`
/// undecided under the mocks.
fn presented_eligibility(report: &TraceReport) -> Vec<(String, Option<bool>)> {
    report
        .scene_eligible
        .iter()
        .cloned()
        .chain(report.steps.iter().filter_map(|s| match s {
            lute_trace::Step::Entry { id, eligible, .. }
            | lute_trace::Step::Beat { id, eligible, .. } => Some((id.clone(), *eligible)),
            _ => None,
        }))
        .collect()
}

/// Does the test's `expect.eligible` assert the verdict of the presented
/// `id`? The scalar form covers every presented one, the map form the ids
/// it names (dsl 0.24.0, T3-5).
fn eligibility_asserted(id: &str, asserted: Option<&serde_yaml::Value>) -> bool {
    match asserted {
        None => false,
        Some(serde_yaml::Value::Mapping(m)) => m
            .keys()
            .filter_map(serde_yaml::Value::as_str)
            .any(|k| names_presented(id, k)),
        Some(_) => true,
    }
}

/// Does the `expect.eligible` key `id` name the presented entry / beat `p`
/// (a local id matches its canonical `<document id>.<id>`)?
fn names_presented(p: &str, id: &str) -> bool {
    p == id || p.ends_with(&format!(".{id}"))
}


/// `checked` (dsl 0.24.0, T3-5): the entry / bundle beat of `input`'s
/// document is presented alone, from the mocked start, and its head's
/// verdict read back. Empty when the document declares no such entry or
/// beat.
fn eligibility_alone(
    doc: &lute_syntax::ast::Document,
    input: &lute_check::CheckInput,
    checked: &lute_check::CheckResult,
    mocks: &lute_trace::mock::MockSet,
    id: &str,
    project_asserts: Option<&BTreeSet<String>>,
) -> Option<TraceReport> {
    let checked = checked.clone();
    let (report, _) = if doc.entries.iter().any(|e| e.id == id) {
        trace_entries_with_check(input, checked, mocks.clone(), &[id], project_asserts)
    } else if doc
        .beats
        .iter()
        .any(|b| names_presented(id, &b.id) || b.id == id)
    {
        trace_beat_with_check(input, checked, mocks.clone(), id, project_asserts)
    } else {
        return None;
    };
    Some(report)
}

/// A note for every presented entry / bundle beat / scene whose `when` is
/// undecided under the test's mocks — trace presents it anyway — unless the
/// test's `expect.eligible` asserts that verdict. A `when` that is false is
/// no note: the test fails on it (dsl 0.26.0 §7, T1-7).
fn ineligible_notes(report: &TraceReport, asserted: Option<&serde_yaml::Value>) -> Vec<String> {
    presented_eligibility(report)
        .into_iter()
        // A scene's undecided `when` is the trace's own beat-`when` note.
        .filter(|(id, _)| report.scene_eligible.as_ref().is_none_or(|(s, _)| s != id))
        .filter(|(id, eligible)| eligible.is_none() && !eligibility_asserted(id, asserted))
        .map(|(id, _)| {
            let local = id.rsplit('.').next().unwrap_or(&id);
            format!(
                "`{id}` may not be eligible under these mocks (its `when` is undecided); the \
                 test presents it anyway — assert it with `expect: {{ eligible: {{ {local}: \
                 true|false }} }}` or supply what its `when` reads"
            )
        })
        .collect()
}

/// Every quest the traced document declares, mapped to where the walk left
/// it (dsl 0.21.0 §7a.4): the LAST `quest` decision's `active`/`complete`/
/// `failed` outcome, else `unset` — a quest whose `start` never held, that
/// awaited an accept, or that was never decided at all never left `unset`.
/// Read off the transcript's decisions, the same record the human report
/// prints, never a second lifecycle model.
fn final_quests(report: &TraceReport, doc: &lute_syntax::ast::Document) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = doc
        .quests
        .iter()
        .filter(|q| !q.id.is_empty())
        .map(|q| (q.id.clone(), "unset".to_string()))
        .collect();
    for d in &report.decisions {
        if d.construct != "quest" {
            continue;
        }
        if let Some(slot) = out.get_mut(&d.id) {
            if matches!(d.outcome.as_str(), "active" | "complete" | "failed") {
                *slot = d.outcome.clone();
            }
        }
    }
    out
}

/// T3-65: what a reserved path of one of `doc`'s own quests reads before the
/// engine writes it, as a play reads it — `quest.<q>.state` / `.failedBy`
/// `unset`, `quest.<q>.objectives.<o>.done` / `.failed` `false`. `None` for
/// any other path.
fn reserved_quest_default(doc: &lute_syntax::ast::Document, path: &str) -> Option<&'static str> {
    let segs: Vec<&str> = path.split('.').collect();
    let quest = |id: &str| doc.quests.iter().find(|q| q.id == id);
    match segs.as_slice() {
        ["quest", q, "state" | "failedBy"] => quest(q).map(|_| "unset"),
        ["quest", q, "objectives", o, "done" | "failed"] => quest(q)?
            .body
            .iter()
            .any(|n| matches!(n, lute_syntax::ast::Node::Objective(obj) if obj.id == *o))
            .then_some("false"),
        _ => None,
    }
}

/// The quest id a `quest.<id>.state` seed (a `quests:` entry, or a `state:`
/// seed of the reserved path) names.
fn seeded_quest(path: &str) -> Option<&str> {
    path.strip_prefix("quest.")?.strip_suffix(".state")
}

/// Round-5 T3-24: what `expect.quests` needs to judge a quest another
/// document of the project declares. The walk holds no such quest — it
/// never judges its `start` or objectives — so the quest ends the walk as
/// it began (its `quests:` / `state:` seed, else `unset`), except that an
/// accept — `accepts:`, or a `::accept` the walk ran that is not queued for
/// the next run — activates it while `unset`, as the engine's settle does.
struct ForeignQuestStart {
    /// Every quest id of the resolved project (`None`: no project resolved,
    /// or the test names no quest outside the traced document).
    project: Option<BTreeSet<String>>,
    seeds: BTreeMap<String, String>,
    accepted: Vec<String>,
}

impl ForeignQuestStart {
    /// `(observed state, why)` for `id`, which the traced document does not
    /// declare (`own` is [`final_quests`]). The state is `None` when no
    /// quest of the project — or, with no project, of the document —
    /// declares `id`; `why` then says so, with the nearest id.
    fn judge(
        &self,
        report: &TraceReport,
        id: &str,
        own: &BTreeMap<String, String>,
    ) -> (Option<String>, Option<String>) {
        if !self.project.as_ref().is_some_and(|p| p.contains(id)) {
            let (unknown, candidates): (String, Vec<&str>) = match &self.project {
                Some(p) => (
                    format!("no document of the project declares quest `{id}`"),
                    p.iter().map(String::as_str).collect(),
                ),
                None => (
                    format!("the traced document declares no quest `{id}`"),
                    own.keys().map(String::as_str).collect(),
                ),
            };
            let hint = lute_manifest::suggest::nearest(id, candidates, 2)
                .map(|c| format!(" — did you mean `{c}`?"))
                .unwrap_or_default();
            return (None, Some(format!("{unknown}{hint}")));
        }
        let seeded = self
            .seeds
            .get(id)
            .map(|s| s.trim_matches(['\'', '"']).to_string());
        let accepted = self.accepted.iter().any(|a| a == id)
            || report.steps.iter().any(
                |s| matches!(s, lute_trace::Step::Accept { quest, next_run: false } if quest == id),
            );
        let state = match seeded {
            Some(s) if s != "unset" => s,
            _ if accepted => "active".to_string(),
            _ => "unset".to_string(),
        };
        let why = format!(
            "quest `{id}` is declared in another document of the project, so the walk does not \
             judge its `start` or objectives: it is its state at the start of the walk (a \
             `quests:` seed, else `unset`), made `active` by an accept"
        );
        (Some(state), Some(why))
    }
}

/// T9.11's second half. `lute-trace` composes its mock diagnostics for
/// `lute trace`'s command line (`--choose id=arm`, `--state path=value`,
/// `--fact`, `--event`, `--accept`, `--entry`), but in a `*.test.yaml` the same input
/// arrived as a YAML KEY. Printing the message verbatim names a syntax the
/// file cannot use. This rewrites the flag spelling to the key spelling and
/// nothing else — the codes, ids, values and clause citations are
/// `lute-trace`'s and stay exactly as written.
fn yaml_key_spelling(message: &str) -> String {
    // Round-5 T3-12: a backticked mock hint (`` `--fact "f"` ``, `` `--state
    // p=<value>` ``, a refused `--choose`'s premise) becomes the YAML entry
    // a test writes, as an unresolved atom's hint does ([`yaml_atom_hint`]).
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(start) = rest.find("`--") {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let inner = &after[..end];
        out.push_str(&rest[..start]);
        if inner.starts_with("--state ") || inner.starts_with("--fact ") {
            out.push_str(&yaml_atom_hint(inner));
        } else {
            out.push('`');
            out.push_str(inner);
            out.push('`');
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    for (flag, key) in [
        ("--choose ", "choose: "),
        ("--state ", "state: "),
        ("--fact ", "facts: "),
        ("--event ", "events: "),
        ("--accept ", "accepts: "),
        ("--entry ", "entry: "),
        ("--beat ", "beat: "),
    ] {
        out = out.replace(flag, key);
    }
    out
}


/// Render a YAML scalar to its literal TEXT form, matching the shape
/// `lute-trace`'s mock parser coerces `state:` values through (bool/number/
/// string). A non-scalar yields `None`.
fn yaml_scalar_text(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}
