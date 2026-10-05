//! Atomic source patch command.

use std::path::Path;
use std::process::ExitCode;

use lute_model::{apply_patch, PatchRefusal, PatchRequest};
fn refusal_json(refusal: &PatchRefusal) -> serde_json::Value {
    if let PatchRefusal::Io { path, message } = refusal {
        return serde_json::json!({
            "schemaVersion": "0.36.0.patch",
            "ok": false,
            "error": {"kind": "io", "path": path, "message": message},
        });
    }
    let mut value = serde_json::json!({
        "schemaVersion": "0.36.0.patch",
        "ok": false,
        "code": refusal.code(),
        "message": refusal.message(),
    });
    match refusal {
        PatchRefusal::Stale { expected, actual } => {
            value["expected"] = serde_json::Value::String(expected.clone());
            value["actual"] = serde_json::Value::String(actual.clone());
        }
        PatchRefusal::Target { node, reason } => {
            value["target"] = serde_json::Value::String(node.canonical());
            value["reason"] = serde_json::Value::String(reason.clone());
        }
        PatchRefusal::Edit { index, reason } => {
            value["index"] = serde_json::json!(index);
            value["reason"] = serde_json::Value::String(reason.clone());
        }
        PatchRefusal::Check(diagnostics) => {
            value["diagnostics"] =
                serde_json::to_value(diagnostics).unwrap_or(serde_json::Value::Array(Vec::new()))
        }
        PatchRefusal::Preserve(changes) => {
            value["changes"] = serde_json::Value::Array(
                changes
                    .iter()
                    .map(|(preserve, change)| {
                        let mut item =
                            serde_json::to_value(change).unwrap_or(serde_json::json!({}));
                        item["preserve"] = serde_json::Value::String(preserve.clone());
                        item
                    })
                    .collect(),
            )
        }
        PatchRefusal::Io { .. } => unreachable!(),
    }
    value
}

fn report_json(report: &lute_model::PatchReport) -> Result<String, serde_json::Error> {
    let mut value = serde_json::to_value(report)?;
    value["schemaVersion"] = serde_json::Value::String("0.36.0.patch".into());
    value["ok"] = serde_json::Value::Bool(true);
    serde_json::to_string_pretty(&value).map(|text| format!("{text}\n"))
}

/// Run `lute patch` after parsing the strict request schema. A successful
/// patch or dry-run is exit 0 only when semantic changes are empty; accepted
/// semantic changes return exit 1. Refusals and malformed input are exit 2.
pub(crate) fn run(dir: &Path, patch: &Path, dry_run: bool, json: bool) -> ExitCode {
    let input = match std::fs::read_to_string(patch) {
        Ok(input) => input,
        Err(error) => {
            eprintln!("lute patch: cannot read {}: {error}", patch.display());
            return ExitCode::from(2);
        }
    };
    let request: PatchRequest = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            let refusal = serde_json::json!({
                "schemaVersion": "0.36.0.patch",
                "ok": false,
                "code": "E-PATCH-EDIT",
                "message": format!("invalid patch JSON: {error}"),
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&refusal).unwrap());
            } else {
                eprintln!("lute patch: E-PATCH-EDIT: invalid patch JSON: {error}");
            }
            return ExitCode::from(2);
        }
    };
    match apply_patch(dir, request, dry_run) {
        Ok(report) => {
            if json {
                match report_json(&report) {
                    Ok(text) => {
                        if crate::output::write_stdout(&text).is_err() {
                            return ExitCode::from(2);
                        }
                    }
                    Err(error) => {
                        eprintln!("lute patch: cannot serialize report: {error}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                let mut out = String::new();
                out.push_str(&format!("before: sha256:{}\n", report.before.sha256));
                out.push_str(&format!("after: sha256:{}\n", report.after.sha256));
                out.push_str(&format!(
                    "writes: {}\n",
                    report
                        .writes
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                out.push_str(&format!("changes: {}\n", report.diff.changes.len()));
                if crate::output::write_stdout(&out).is_err() {
                    return ExitCode::from(2);
                }
            }
            if report.diff.changes.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(refusal) => {
            if json {
                let value = refusal_json(&refusal);
                let text = format!("{}\n", serde_json::to_string_pretty(&value).unwrap());
                if crate::output::write_stdout(&text).is_err() {
                    return ExitCode::from(2);
                }
            } else {
                eprintln!("lute patch: {refusal}");
            }
            ExitCode::from(2)
        }
    }
}
