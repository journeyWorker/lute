//! `lute.project.yaml` `diagnostics:` validation (dsl 0.50.0 §6.1): the one
//! function the CLI manifest pass and the LSP both call, so the two report
//! the same `E-DIAGNOSTICS-CONFIG` faults.
//!
//! The entries' shape (a level, `{ level, paths }`) is read with the manifest
//! (`lute_manifest::diagnostic_levels`); this adds the code faults, which
//! need the toolchain's code table ([`crate::evidence::classification`]).
//! A refused entry never applies: `DiagnosticLevels::level_for` answers only
//! `W-*` codes, and an undefined code names no diagnostic.

use lute_manifest::diagnostic_levels::E_DIAGNOSTICS_CONFIG;
use lute_manifest::project::{ProjectConfig, ResolveDiag};

/// Every `E-DIAGNOSTICS-CONFIG` fault of `project`'s `diagnostics:` block —
/// its shape faults and its code faults — in manifest order.
pub fn diagnostics_config_diags(project: &ProjectConfig) -> Vec<ResolveDiag> {
    let mut out = project.diagnostics_diags.clone();
    for (code, entry) in project.diagnostics.entries() {
        if let Some(message) = code_fault(code) {
            out.push(ResolveDiag {
                code: E_DIAGNOSTICS_CONFIG.to_string(),
                message,
                span: entry.span.clone(),
            });
        }
    }
    out.sort_by_key(|d| d.span.as_ref().map_or(0, |s| s.start));
    out
}

/// Why `code` cannot be re-levelled, or `None` for a defined `W-*` code.
fn code_fault(code: &str) -> Option<String> {
    if code.starts_with("L-") {
        return Some(format!(
            "`{code}` is a lint rule; set its level in `lute.lint.yaml` `rules:`"
        ));
    }
    if crate::evidence::classification(code).is_none() {
        return Some(format!(
            "`{code}` is not a diagnostic code{}",
            lute_manifest::suggest::did_you_mean(
                code,
                crate::evidence::DIAGNOSTIC_CLASSIFICATIONS
                    .iter()
                    .map(|(c, _)| *c)
            )
        ));
    }
    code.starts_with("E-").then(|| {
        format!(
            "`{code}` is an error: an error is a contract violation and cannot be re-levelled; \
             fix the reported source instead"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::code_fault;

    #[test]
    fn each_code_fault_has_its_message() {
        assert_eq!(code_fault("W-STAGE-ABSENT"), None);
        assert_eq!(
            code_fault("E-UNKNOWN-ATTR").as_deref(),
            Some(
                "`E-UNKNOWN-ATTR` is an error: an error is a contract violation and cannot be \
                 re-levelled; fix the reported source instead"
            )
        );
        assert_eq!(
            code_fault("L-LINE-LENGTH").as_deref(),
            Some("`L-LINE-LENGTH` is a lint rule; set its level in `lute.lint.yaml` `rules:`")
        );
        // A typo; lower-case so the registry scan (`every_emitted_code_is_
        // registered`) does not read it as an emitted code. Case folds in
        // the suggestion.
        assert_eq!(
            code_fault("w-stage-absnt").as_deref(),
            Some("`w-stage-absnt` is not a diagnostic code — did you mean `W-STAGE-ABSENT`?")
        );
        assert_eq!(
            code_fault("nonsense").as_deref(),
            Some("`nonsense` is not a diagnostic code")
        );
    }
}
