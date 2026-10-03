use lute_model::{impact, ModelOptions, ProjectModel};
use std::path::Path;
use std::process::ExitCode;

pub(crate) fn run_impact(
    dir: &Path,
    target: &str,
    json: bool,
    providers: Option<&Path>,
) -> ExitCode {
    let target = match impact::ImpactTarget::parse(target) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute impact: {e}");
            return ExitCode::from(2);
        }
    };
    let opts = ModelOptions {
        providers: providers.map(|p| p.to_path_buf()),
        compile: true,
        ..ModelOptions::default()
    };
    let model = match ProjectModel::build_single_root(dir, &opts) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lute impact: {e}");
            return ExitCode::from(2);
        }
    };
    let report = impact::query(&model, &target);
    let rendered = if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => format!("{s}\n"),
            Err(e) => {
                eprintln!("lute impact: cannot serialize report: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        impact::human(&report)
    };
    if crate::output::write_stdout(&rendered).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
