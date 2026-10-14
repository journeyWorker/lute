use crate::envelope::{self, CliError};
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
            return envelope::fail(
                CliError::usage(format!("lute impact: {e}")).with_argument(target),
            );
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
            return envelope::fail(CliError::io(dir, format!("lute impact: {e}")));
        }
    };
    let report = impact::query(&model, &target);
    let rendered = if json {
        let payload = match serde_json::to_value(&report) {
            Ok(value) => value,
            Err(e) => {
                return envelope::fail(CliError::io_without_path(format!(
                    "lute impact: cannot serialize report: {e}"
                )));
            }
        };
        let value = envelope::report(crate::envelope::version::IMPACT, 0, payload);
        match serde_json::to_string_pretty(&value) {
            Ok(s) => format!("{s}\n"),
            Err(e) => {
                return envelope::fail(CliError::io_without_path(format!(
                    "lute impact: cannot serialize report: {e}"
                )));
            }
        }
    } else {
        impact::human(&report)
    };
    if crate::output::write_stdout(&rendered).is_err() {
        return envelope::fail_quietly(CliError::io_without_path(
            "lute impact: cannot write stdout",
        ));
    }
    ExitCode::SUCCESS
}
