use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::Mode;
use lute_model::{constraints::ConstraintResult, ModelOptions};

/// Report every project constraint. `--run` records script witnesses when a
/// play/test script contains an observed quest completion transition.
pub(crate) fn run_constraints(
    dir: &Path,
    json: bool,
    run: bool,
    providers: Option<&Path>,
) -> ExitCode {
    let opts = ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: None,
        mode: Mode::Author,
        compile: false,
        wip: false,
    };
    let models = match lute_model::ProjectModel::roots_under(dir, &opts) {
        Ok(models) => models,
        Err(error) => {
            eprintln!("lute constraints: {error}");
            return ExitCode::from(2);
        }
    };
    let mut all = Vec::<serde_json::Value>::new();
    let mut failed = false;
    for model in models {
        let Some(project) = model.manifest() else {
            continue;
        };
        for (_, diagnostic) in model
            .project_diagnostics()
            .iter()
            .filter(|(_, d)| d.code == "E-CONSTRAINT-DECL")
        {
            failed = true;
            if json {
                all.push(serde_json::json!({"root": model.root(), "code": diagnostic.code, "message": diagnostic.message, "span": diagnostic.span}));
            } else {
                println!(
                    "{}: {} {}",
                    model.root().join("lute.project.yaml").display(),
                    diagnostic.code,
                    diagnostic.message
                );
            }
        }
        let docs: Vec<_> = model
            .documents()
            .iter()
            .map(|d| (d.path.clone(), d.doc.clone()))
            .collect();
        let slots = lute_check::clock_positions::project_objective_slot_results(
            &docs,
            &model
                .documents()
                .iter()
                .map(|d| &d.folded)
                .collect::<Vec<_>>(),
        );
        let mut results = lute_model::constraints::evaluate_constraints_with_foldeds(
            model.root(),
            project,
            &docs,
            &model
                .documents()
                .iter()
                .map(|d| &d.folded)
                .collect::<Vec<_>>(),
            model
                .reconciled()
                .scenarios
                .get(model.root())
                .expect("model scenario"),
            &slots,
        );
        if run {
            add_script_witnesses(model.root(), &mut results);
        }
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
                    "root": model.root(), "id": result.id, "kind": result.kind,
                    "verdict": result.verdict, "evidence": result.evidence,
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
                for witness in result.witnesses {
                    println!("  witness: {witness}");
                }
                for counterexample in result.counterexamples {
                    println!("  counterexample: {counterexample}");
                }
            }
        }
    }
    if json {
        match serde_json::to_string_pretty(&all) {
            Ok(text) => println!("{text}"),
            Err(error) => {
                eprintln!("lute constraints: {error}");
                return ExitCode::from(2);
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn add_script_witnesses(root: &Path, results: &mut [ConstraintResult]) {
    let mut scripts = Vec::new();
    collect_scripts(root, &mut scripts);
    for result in results.iter_mut().filter(|r| r.kind == "completable") {
        let quest = result.counterexamples.first().cloned().unwrap_or_default();
        for script in &scripts {
            if !script_witness(root, script, &quest) {
                continue;
            }
            result
                .witnesses
                .push(format!("{}: {} -> complete", script.display(), quest));
            result.verdict = lute_model::constraints::ConstraintVerdict::Holds;
            result.evidence = lute_core_span::Evidence::Witnessed;
            result.counterexamples.clear();
            break;
        }
    }
}

/// Execute one script through the in-process test/play runner and accept only
/// an observed completion transition, never a seeded complete status.
fn script_witness(root: &Path, script: &Path, quest: &str) -> bool {
    let Some(name) = script.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.ends_with(".test.yaml") {
        return crate::testcmd::run_test_for_constraint(root, script, quest);
    }
    if !name.ends_with(".play.yaml") {
        return false;
    }
    let project = crate::play::PlayProject::compile(root);
    let Ok(run) = crate::play::run_play_for_test(&project, script, true) else {
        return false;
    };
    run.completed_quests.contains(quest)
}

/// Run one discovered script through the same in-process machinery used by
/// `constraints --run`, and report whether it observed the target node.
pub(crate) fn run_script_for_context(
    root: &Path,
    script: &Path,
    target: &lute_model::NodeKey,
) -> bool {
    let Some(name) = script.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.ends_with(".test.yaml") {
        return crate::testcmd::run_test_for_context(root, script, target);
    }
    if !name.ends_with(".play.yaml") {
        return false;
    }
    let project = crate::play::PlayProject::compile(root);
    let Ok(run) = crate::play::run_play_for_test(&project, script, true) else {
        return false;
    };
    if target.kind == lute_model::NodeKind::Quest
        && run.completed_quests.contains(&target.key)
    {
        return true;
    }
    if target.kind == lute_model::NodeKind::Choice {
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
    run.presented.iter().any(|(document, id)| {
        format!("{document}:{id}") == target.key
    })
}

fn collect_scripts(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_scripts(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("yaml")
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".play.yaml") || n.ends_with(".test.yaml"))
        {
            out.push(path);
        }
    }
    out.sort();
}
