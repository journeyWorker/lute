use crate::envelope::{self, CliError};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::Mode;
use lute_core_span::Diagnostic;
use lute_model::{constraints::ConstraintResult, ModelMemo, ModelOptions, ProjectModel};
use lute_semantic::{NodeKey, NodeKind};

/// The play/test scripts `--script` / `--all-scripts` select (0.48.0 §3.4):
/// a named script, a directory selecting every script under it, or every
/// script of the project.
pub(crate) struct ScriptSelection<'a> {
    pub(crate) scripts: &'a [PathBuf],
    pub(crate) all: bool,
    /// `--providers` / `--permission-profile`: the witness scripts are
    /// compiled and traced under the same project options as the model.
    pub(crate) providers: Option<&'a Path>,
    pub(crate) permission_profile: Option<&'a str>,
}

impl ScriptSelection<'_> {
    /// The build options the selected scripts run under.
    pub(crate) fn build(&self) -> crate::play::Build<'_> {
        crate::play::Build {
            providers: self.providers,
            permission_profile: self.permission_profile,
            policy: crate::SeverityPolicy::default(),
        }
    }

    /// Whether any script is selected at all.
    pub(crate) fn any(&self) -> bool {
        self.all || !self.scripts.is_empty()
    }

    /// Whether `path` (a script found in the project) is selected.
    pub(crate) fn selects(&self, path: &Path) -> bool {
        if self.all {
            return true;
        }
        let canonical = std::fs::canonicalize(path).ok();
        self.scripts.iter().any(|selector| {
            if selector == path {
                return true;
            }
            match (std::fs::canonicalize(selector).ok(), &canonical) {
                (Some(selector), Some(path)) => path == &selector || path.starts_with(&selector),
                _ => false,
            }
        })
    }
}

/// The models a `constraints` run reads. Without scripts it is the
/// analysis-only model of each root. A witness-bearing run builds one
/// compiled model per root, which the evaluation and every witness play and
/// test read (spec 0.49.0 §9.6); its analysis-only results are what the
/// analysis-only model reports.
pub(crate) fn constraint_models(
    providers: Option<&Path>,
    witnesses: bool,
) -> (ModelMemo, ModelOptions) {
    let analysis = ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: None,
        mode: Mode::Author,
        compile: false,
        wip: false,
    };
    if !witnesses {
        return (ModelMemo::default(), analysis);
    }
    let selected = ModelOptions {
        mode: Mode::Ci,
        compile: true,
        ..analysis
    };
    (ModelMemo::selecting(selected.clone()), selected)
}

/// The manifest's `E-CONSTRAINT-DECL` diagnostics of `model`'s root.
pub(crate) fn declaration_errors(model: &ProjectModel) -> impl Iterator<Item = &Diagnostic> {
    model
        .analysis_diagnostics()
        .iter()
        .filter(|(_, d)| d.code == "E-CONSTRAINT-DECL")
        .map(|(_, d)| d)
}

/// Every constraint of `model`'s manifest (`None` without one), with the
/// witnesses of the scripts `selection` selects under `root` (the root as
/// the command spells it), each run over `memo`'s models.
pub(crate) fn evaluate_root(
    memo: &ModelMemo,
    root: &Path,
    model: &ProjectModel,
    selection: &ScriptSelection<'_>,
) -> Option<Vec<ConstraintResult>> {
    let project = model.manifest()?;
    let docs: Vec<_> = model
        .documents()
        .iter()
        .map(|d| (d.path.clone(), d.doc.clone()))
        .collect();
    let project_docs = model.project_docs();
    let folded_refs: Vec<_> = model.documents().iter().map(|d| &d.folded).collect();
    let slots =
        lute_check::clock_positions::project_objective_slot_results(&project_docs, &folded_refs);
    let mut results = lute_model::constraints::evaluate_constraints_with_foldeds(
        model.root(),
        project,
        &docs,
        &folded_refs,
        model
            .reconciled()
            .scenarios
            .get(model.root())
            .expect("model scenario"),
        &slots,
    );
    if selection.any() {
        add_script_witnesses(memo, root, &mut results, selection);
    }
    Some(results)
}

/// Report every project constraint. `--script` / `--all-scripts` record
/// script witnesses when a selected play/test script contains an observed
/// quest completion transition.
pub(crate) fn run_constraints(
    dir: &Path,
    json: bool,
    scripts: &[PathBuf],
    all_scripts: bool,
    providers: Option<&Path>,
) -> ExitCode {
    let selection = ScriptSelection {
        scripts,
        all: all_scripts,
        providers,
        permission_profile: None,
    };
    let (memo, opts) = constraint_models(providers, selection.any());
    let models = match memo.roots_under(dir, &opts) {
        Ok(models) => models,
        Err(error) => {
            return envelope::fail(CliError::io(dir, format!("lute constraints: {error}")));
        }
    };
    let mut all = Vec::<serde_json::Value>::new();
    let mut diagnostics = Vec::<serde_json::Value>::new();
    let mut failed = false;
    for (root, model) in models {
        let manifest = root.join("lute.project.yaml");
        if model.manifest().is_none() {
            // dsl 0.49.0 §6.5: a manifest that exists but does not load
            // fails the run with the diagnostic whole-project `check` prints.
            let Some((line, diagnostic)) = manifest_load_error(&root) else {
                continue;
            };
            failed = true;
            if json {
                diagnostics.push(diagnostic_json(&root, &manifest, &diagnostic));
            } else {
                println!("{line}");
            }
            continue;
        }
        for diagnostic in declaration_errors(&model) {
            failed = true;
            if json {
                diagnostics.push(diagnostic_json(&root, &manifest, diagnostic));
            } else {
                println!(
                    "{}: {} {}",
                    manifest.display(),
                    diagnostic.code,
                    diagnostic.message
                );
            }
        }
        let results = evaluate_root(&memo, &root, &model, &selection).unwrap_or_default();
        for result in results {
            failed |= matches!(
                result.verdict,
                lute_model::constraints::ConstraintVerdict::Violated
            );
            if !result.declaration_errors.is_empty() {
                failed = true;
            }
            if json {
                all.push(serde_json::json!({
                    "root": root, "id": result.id, "kind": result.kind,
                    "subject": result.subject,
                    "verdict": result.verdict, "evidence": result.evidence,
                    "reason": result.reason,
                    "scope": result.scope, "declaration": result.declaration,
                    "witnesses": result.witnesses,
                    "counterexamples": result.counterexamples, "related": result.related,
                    "declarationErrors": result.declaration_errors,
                }));
            } else {
                for error in &result.declaration_errors {
                    println!("{}: {} {}", result.id, error.code, error.message);
                }
                let evidence = match &result.evidence {
                    lute_core_span::Evidence::Bounded { scope } => format!("bounded: {scope}"),
                    lute_core_span::Evidence::Proven => "proven".into(),
                    lute_core_span::Evidence::Witnessed => "witnessed".into(),
                    lute_core_span::Evidence::Heuristic => "heuristic".into(),
                    lute_core_span::Evidence::Unknown => "unknown".into(),
                };
                println!(
                    "{}: {} {} [{}]",
                    result.id,
                    result.kind,
                    match result.verdict {
                        lute_model::constraints::ConstraintVerdict::Holds => "holds",
                        lute_model::constraints::ConstraintVerdict::Violated => "violated",
                        lute_model::constraints::ConstraintVerdict::Unknown => "unknown",
                    },
                    evidence
                );
                if result.verdict == lute_model::constraints::ConstraintVerdict::Unknown {
                    println!("  reason: {}", result.reason);
                }
                for witness in result.witnesses {
                    println!("  witness: {witness}");
                }
                for counterexample in result.counterexamples {
                    println!("  counterexample: {counterexample}");
                }
            }
        }
    }
    let exit = if failed { 1 } else { 0 };
    if json {
        let payload = serde_json::json!({"diagnostics": diagnostics, "constraints": all});
        let report = envelope::report(crate::envelope::version::CONSTRAINTS, exit, payload);
        match serde_json::to_string_pretty(&report) {
            Ok(text) => println!("{text}"),
            Err(error) => {
                return envelope::fail(CliError::io_without_path(format!(
                    "lute constraints: {error}"
                )));
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// One `diagnostics` entry of the JSON report: a manifest diagnostic of
/// `root`, at `manifest`.
fn diagnostic_json(root: &Path, manifest: &Path, diagnostic: &Diagnostic) -> serde_json::Value {
    serde_json::json!({
        "root": root, "path": manifest, "code": diagnostic.code,
        "severity": diagnostic.severity, "message": diagnostic.message, "span": diagnostic.span,
    })
}

/// The error of a `lute.project.yaml` under `root` that exists but does not
/// load: the line whole-project `check` prints, and the diagnostic. `None`
/// when it loads or does not exist.
fn manifest_load_error(root: &Path) -> Option<(String, Diagnostic)> {
    use lute_manifest::project::{ProjectLoadError, E_MANIFEST};
    let error = lute_manifest::project::load_project(root).err()?;
    let text = std::fs::read_to_string(root.join("lute.project.yaml")).unwrap_or_default();
    let (message, line, column) = match &error {
        ProjectLoadError::Io { .. } => (error.to_string(), 1, 1),
        ProjectLoadError::Yaml {
            line,
            column,
            message,
            ..
        }
        | ProjectLoadError::Invalid {
            line,
            column,
            reason: message,
            ..
        } => (message.clone(), *line, *column),
    };
    let offset = text
        .split_inclusive('\n')
        .take(line.saturating_sub(1))
        .map(str::len)
        .sum::<usize>();
    let offset = text[offset..]
        .char_indices()
        .nth(column.saturating_sub(1))
        .map_or(text.len(), |(i, _)| offset + i);
    let index = lute_core_span::TextIndex::new(&text);
    let diagnostic = Diagnostic {
        code: E_MANIFEST.to_string(),
        severity: lute_core_span::Severity::Error,
        message,
        span: lute_core_span::Span::from_bytes(&index, offset, offset),
        layer: lute_core_span::Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
        evidence: None,
    };
    Some((error.to_string(), diagnostic))
}

fn add_script_witnesses(
    memo: &ModelMemo,
    root: &Path,
    results: &mut [ConstraintResult],
    selection: &ScriptSelection<'_>,
) {
    let mut scripts = lute_model::scripts::collect_scripts(root);
    scripts.retain(|script| selection.selects(script));
    for result in results.iter_mut().filter(|r| r.kind == "completable") {
        let quest = result
            .subject
            .strip_prefix("quest:")
            .unwrap_or_default()
            .to_string();
        let witness = scripts
            .iter()
            .find(|script| script_witness(memo, root, script, &quest, &selection.build()));
        match witness {
            Some(script) => {
                result
                    .witnesses
                    .push(format!("{}: {} -> complete", script.display(), quest));
                result.verdict = lute_model::constraints::ConstraintVerdict::Holds;
                result.evidence = lute_core_span::Evidence::Witnessed;
                result.reason = format!("{} observed quest `{quest}` complete", script.display());
                result.counterexamples.clear();
                result.related.clear();
            }
            None if result.reason == lute_model::constraints::NO_SCRIPT_REASON => {
                result.reason = format!(
                    "{} script(s) ran; none completed quest `{quest}`",
                    scripts.len()
                );
            }
            None => {}
        }
    }
}

/// Execute one script through the in-process test/play runner over `memo`'s
/// models and accept only an observed completion transition, never a seeded
/// complete status.
fn script_witness(
    memo: &ModelMemo,
    root: &Path,
    script: &Path,
    quest: &str,
    build: &crate::play::Build<'_>,
) -> bool {
    let Some(name) = script.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.ends_with(".test.yaml") {
        return crate::testcmd::run_test_for_constraint(memo, script, quest, build);
    }
    if !name.ends_with(".play.yaml") {
        return false;
    }
    let project = crate::play::PlayProject::compile_in(memo, root, build);
    let Ok(run) = crate::play::run_play_for_test(&project, script, true) else {
        return false;
    };
    run.completed_quests.contains(quest)
}

/// Run one discovered script through the same in-process machinery used by
/// `constraints --all-scripts`, over `memo`'s models, and report whether it
/// observed the target node.
pub(crate) fn run_script_for_context(
    memo: &ModelMemo,
    root: &Path,
    script: &Path,
    target: &NodeKey,
    build: &crate::play::Build<'_>,
) -> bool {
    let Some(name) = script.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.ends_with(".test.yaml") {
        return crate::testcmd::run_test_for_context(memo, script, target, build);
    }
    if !name.ends_with(".play.yaml") {
        return false;
    }
    let project = crate::play::PlayProject::compile_in(memo, root, build);
    let Ok(run) = crate::play::run_play_for_test(&project, script, true) else {
        return false;
    };
    if target.kind == NodeKind::Quest && run.completed_quests.contains(&target.key) {
        return true;
    }
    if target.kind == NodeKind::Choice {
        let Some((document, tail)) = target.key.rsplit_once(':') else {
            return false;
        };
        let Some((parent, option)) = tail.rsplit_once('.') else {
            return false;
        };
        return run.choices.iter().any(|choice| {
            choice.document == document
                && choice.id == parent
                && choice.chose.as_deref() == Some(option)
        });
    }
    run.presented
        .iter()
        .any(|(document, id)| format!("{document}:{id}") == target.key)
}

#[cfg(test)]
mod one_model_tests {
    //! Spec 0.49.0 §9.6: a witness-bearing `constraints` run reads one
    //! compiled `Ci` model per root. These tests show it reports exactly what
    //! the analysis-only `Author` model reports (the equality test that
    //! precedes the switch), and that one build serves every witness.

    use super::*;

    fn workspace() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn manifest_roots(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path.join("lute.project.yaml").is_file() {
                out.push(path.clone());
            }
            manifest_roots(&path, out);
        }
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    const SCHEMA: &str =
        "state:\n  run.flag: { type: bool, default: false }\ncast:\n  mira: { name: Mira }\n";
    const FIRST: &str = "<quest id=\"first\" tier=\"run\" title=\"First\" start=\"true\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n";

    /// The reproductions of #183 (an invalid manifest), #184 (`unknown`
    /// verdicts) and #185 (a holding `reachable`), a manifest whose
    /// constraint declarations are invalid, and a two-profile project whose
    /// compiled model alone reports an index error (`E-CAPABILITY-MISMATCH`).
    fn issue_fixtures(scratch: &Path) -> Vec<PathBuf> {
        let constraints = "constraints:\n  - id: first-reachable\n    kind: reachable\n    node: quest:first\n  - id: first-completes\n    kind: completable\n    quest: first\n  - id: second-reachable\n    kind: reachable\n    node: quest:second\n";
        let profiles = "profiles:\n  game:\n    plugins: {}\ndefaults:\n  contentLang: en\n  uses: [schema/world.schema.yaml]\n";
        let quests = format!(
            "---\nkind: quest\nid: acme.quests\n---\n\n{FIRST}\n<quest id=\"second\" tier=\"run\" title=\"Second\" start=\"quest.first.state == 'complete'\">\n  <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n"
        );
        let mut roots = Vec::new();
        for (name, manifest) in [
            ("i183", format!("{profiles}{constraints}")),
            ("i184", format!("defaultProfile: game\n{profiles}{constraints}")),
            (
                "decl",
                format!("defaultProfile: game\n{profiles}constraints:\n  - id: bad\n    kind: reachable\n  - id: worse\n    kind: nope\n    node: quest:first\n"),
            ),
        ] {
            let root = scratch.join(name);
            write(&root, "lute.project.yaml", &manifest);
            write(&root, "schema/world.schema.yaml", SCHEMA);
            write(&root, "quests/q.lute", &quests);
            roots.push(root);
        }
        let root = scratch.join("profiles");
        write(
            &root,
            "lute.project.yaml",
            &format!("pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: {{}}\n  withAnnounce:\n    plugins: {{ demo.plugin: true }}\ndefaults:\n  uses: [schema/world.schema.yaml]\n{constraints}"),
        );
        write(&root, "schema/world.schema.yaml", SCHEMA);
        write(&root, "quests/q.lute", &quests);
        write(
            &root,
            "plugins/demo.plugin/plugin.yaml",
            "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
        );
        write(
            &root,
            "plugins/demo.plugin/directives/d.yaml",
            "directives:\n  - { name: announce, attrs: [ { name: text, type: string } ] }\n",
        );
        write(
            &root,
            "a.lute",
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nprofile: withAnnounce\n---\n## Shot 1.\n::announce{text=\"hi\"}\n@narrator: a.\n",
        );
        roots.push(root);
        roots
    }

    /// The play-hub fixture with one `.play.yaml` and two `.test.yaml`
    /// witnesses (one completing the quest, one refused), and a constraint
    /// no script witnesses, so every script runs.
    fn witness_fixture(scratch: &Path) -> PathBuf {
        fn copy(from: &Path, to: &Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap().flatten() {
                let dest = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &dest);
                } else {
                    std::fs::copy(entry.path(), dest).unwrap();
                }
            }
        }
        let root = scratch.join("witnesses");
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/play-hub"),
            &root,
        );
        let manifest = root.join("lute.project.yaml");
        let mut text = std::fs::read_to_string(&manifest).unwrap();
        text.push_str("constraints:\n  - id: escape\n    kind: completable\n    quest: firstEscape\n  - id: never\n    kind: completable\n    quest: never\n");
        std::fs::write(&manifest, text).unwrap();
        write(
            &root,
            "quests/never.lute",
            "---\nkind: quest\nid: never\nstate:\n  run.never: { type: bool, default: false }\n---\n<quest id=\"never\" start=\"true\">\n  <objective id=\"o\" done=\"run.never\"/>\n</quest>\n",
        );
        write(
            &root,
            "tests/escape.test.yaml",
            "file: ../quests/first-escape.lute\nstate: { run.hubVisits: 2 }\nexpect:\n  quests: { firstEscape: complete }\n",
        );
        write(
            &root,
            "tests/stale.test.yaml",
            "file: ../quests/first-escape.lute\nstate: { run.noSuchPath: 2 }\nexpect:\n  quests: { firstEscape: complete }\n",
        );
        root
    }

    /// Everything `run_constraints` reports for `dir`: per root, the
    /// declaration errors and every `ConstraintResult` field.
    fn report(
        memo: &ModelMemo,
        opts: &ModelOptions,
        dir: &Path,
        selection: &ScriptSelection<'_>,
    ) -> String {
        let models = match memo.roots_under(dir, opts) {
            Ok(models) => models,
            Err(error) => return format!("error: {error}"),
        };
        let mut out = String::new();
        for (root, model) in models {
            let errors: Vec<_> = declaration_errors(&model).collect();
            let results = evaluate_root(memo, &root, &model, selection);
            out.push_str(&format!(
                "{}\n{errors:?}\n{}\n",
                root.display(),
                serde_json::to_string(&results).unwrap()
            ));
        }
        out
    }

    /// A play witness run, for comparing two runs.
    fn play_record(memo: &ModelMemo, root: &Path, script: &Path) -> String {
        let project =
            crate::play::PlayProject::compile_in(memo, root, &crate::play::Build::default());
        match crate::play::run_play_for_test(&project, script, true) {
            Err(error) => format!("error: {error}"),
            Ok(run) => format!(
                "exit={} end={} completed={:?} presented={:?} docs={:?} notes={:?} misses={}",
                run.exit,
                run.end,
                run.completed_quests,
                run.presented,
                run.presented_docs,
                run.notes,
                run.misses.len()
            ),
        }
    }

    fn scripts_of(root: &Path) -> Vec<PathBuf> {
        lute_model::scripts::collect_scripts(root)
    }

    #[test]
    fn the_selected_model_reports_what_the_analysis_model_reports() {
        let scratch = lute_scratch::ScratchDir::named("constraints-one-model");
        let mut roots = Vec::new();
        manifest_roots(&workspace().join("docs/examples"), &mut roots);
        manifest_roots(&workspace().join("conformance"), &mut roots);
        roots.sort();
        assert!(roots.len() > 10, "corpus roots: {roots:?}");
        roots.extend(issue_fixtures(&scratch));
        let none = ScriptSelection {
            scripts: &[],
            all: false,
            providers: None,
            permission_profile: None,
        };
        // Roots whose compiled model reports more than its analysis.
        let mut diverged = Vec::new();
        for root in &roots {
            let (analysis_memo, analysis) = constraint_models(None, false);
            let (selected_memo, selected) = constraint_models(None, true);
            assert_eq!(
                report(&analysis_memo, &analysis, root, &none),
                report(&selected_memo, &selected, root, &none),
                "{}",
                root.display()
            );
            for (_, plain) in analysis_memo
                .roots_under(root, &analysis)
                .unwrap_or_default()
            {
                let model = selected_memo.single_root(plain.root(), &analysis).unwrap();
                if model.project_diagnostics() != model.analysis_diagnostics() {
                    diverged.push(plain.root().to_path_buf());
                }
                assert_eq!(
                    plain.project_diagnostics(),
                    model.analysis_diagnostics(),
                    "{}",
                    plain.root().display()
                );
                let json = |checks: Vec<(&PathBuf, &lute_check::CheckResult)>| {
                    serde_json::to_value(checks).unwrap()
                };
                assert_eq!(
                    json(plain.analysis_checks().collect()),
                    json(model.analysis_checks().collect()),
                    "{}",
                    plain.root().display()
                );
            }
        }
        assert!(
            diverged.iter().any(|root| root.ends_with("profiles")),
            "{diverged:?}"
        );
    }

    #[test]
    fn one_build_serves_constraints_and_every_witness() {
        let scratch = lute_scratch::ScratchDir::named("constraints-witnesses");
        let root = witness_fixture(&scratch);
        let all = ScriptSelection {
            scripts: &[],
            all: true,
            providers: None,
            permission_profile: None,
        };
        let (analysis_memo, analysis) = constraint_models(None, false);
        let (selected_memo, selected) = constraint_models(None, true);
        let expected = report(&analysis_memo, &analysis, &root, &all);
        let actual = report(&selected_memo, &selected, &root, &all);
        assert_eq!(expected, actual);
        assert!(
            actual.contains("plays/tour.play.yaml: firstEscape -> complete"),
            "{actual}"
        );
        // dsl 0.49.0 §6.4: an unwitnessed `completable` names the scripts run.
        assert!(
            actual.contains("3 script(s) ran; none completed quest `never`"),
            "{actual}"
        );
        // Every script ran inside the one `constraints` evaluation (the
        // `never` constraint is witnessed by none of them) over one build.
        assert_eq!(selected_memo.builds(), 1);
        // The analysis-only path builds an `Author` model for the evaluation
        // and `Ci` ones (analysis and compiled) for the witnesses.
        assert!(analysis_memo.builds() >= 3, "{}", analysis_memo.builds());

        let scripts = scripts_of(&root);
        assert_eq!(scripts.len(), 3, "{scripts:?}");
        let build = crate::play::Build::default();
        let mut records = Vec::new();
        for script in &scripts {
            let (old, new) = if script.to_string_lossy().ends_with(".play.yaml") {
                (
                    play_record(&ModelMemo::default(), &root, script),
                    play_record(&selected_memo, &root, script),
                )
            } else {
                (
                    crate::testcmd::witness_record(&ModelMemo::default(), script, &build),
                    crate::testcmd::witness_record(&selected_memo, script, &build),
                )
            };
            assert_eq!(old, new, "{}", script.display());
            records.push(new);
        }
        assert!(records[0].contains("firstEscape"), "{records:?}");
        assert!(records[1].contains("passed=true"), "{records:?}");
        assert!(records[2].contains("refusal=Some"), "{records:?}");
        assert_eq!(selected_memo.builds(), 1);
    }
}
