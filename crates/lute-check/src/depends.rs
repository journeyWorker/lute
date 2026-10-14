//! 0.41.0 §3.2: an invalid declaration does not cascade. A declaration that
//! fails to parse but whose name is recoverable is kept by name, marked
//! invalid ([`lute_manifest::snapshot::InvalidDecl`] for a plugin's, the
//! `faulty` / `clock_rejected` marks of [`crate::meta::StateSchema`] for a
//! state row), and reports its own error only. What depends on it is not
//! checked; the first such dependant of each cause carries one
//! [`W_DEPENDS_ON_INVALID`] note instead of a diagnostic per use.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::{CapabilitySnapshot, InvalidDecl};

/// `W-DEPENDS-ON-INVALID` (severity `info`): a use of a declaration that is
/// invalid, so the use is not checked until the declaration is fixed.
pub const W_DEPENDS_ON_INVALID: &str = "W-DEPENDS-ON-INVALID";

/// The text every note shares after its dependant: what it is grouped by.
const DEPENDS_ON: &str = " depends on `";

/// The note for `dependant` (spelled as written, `::check`, `clock:`) of
/// the invalid `cause`, at `span`.
pub fn depends_diag(dependant: &str, cause: &InvalidDecl, span: Span, layer: Layer) -> Diagnostic {
    Diagnostic {
        code: W_DEPENDS_ON_INVALID.to_string(),
        severity: Severity::Info,
        message: format!(
            "`{dependant}` depends on `{}`, which is invalid ({}); it is not checked until that declaration is fixed",
            cause.cause,
            cause.origin()
        ),
        evidence: None,
        span,
        layer,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The invalid plugin declaration of `kind` named `name`, if any.
pub fn invalid<'a>(
    snapshot: &'a CapabilitySnapshot,
    kind: &str,
    name: &str,
) -> Option<&'a InvalidDecl> {
    snapshot.invalid.get(&(kind.to_string(), name.to_string()))
}

/// The cause a document's unknown directive tag `tag` is attributed to: the
/// tag's own invalid declaration, else the first active plugin that failed
/// to load (the tag may be one it declares).
pub fn directive_cause<'a>(snapshot: &'a CapabilitySnapshot, tag: &str) -> Option<&'a InvalidDecl> {
    invalid(snapshot, "directive", tag).or_else(|| snapshot.failed.values().next())
}

/// Turn a document's reports of a name as unknown into a note when the name
/// is an invalid plugin declaration: an occasion a beat answers
/// (`E-OCCASION-UNKNOWN`) and a def a condition references
/// (`E-UNDECLARED-REF`). Directive tags are attributed where they are
/// checked ([`directive_cause`]).
pub fn attribute_unknown(diags: &mut [Diagnostic], snapshot: &CapabilitySnapshot) {
    if snapshot.invalid.is_empty() {
        return;
    }
    for d in diags.iter_mut() {
        let attributed = snapshot.invalid.iter().find_map(|((kind, name), cause)| {
            let (code, written, prefix) = match kind.as_str() {
                "occasion" => (
                    crate::beats::E_OCCASION_UNKNOWN,
                    format!("on: {name}"),
                    format!("occasion `{name}` is not declared"),
                ),
                "def" => (
                    "E-UNDECLARED-REF",
                    format!("@{name}"),
                    format!("`@{name}` is not a declared def"),
                ),
                _ => return None,
            };
            (d.code == code && d.message.starts_with(&prefix)).then_some((written, cause))
        });
        if let Some((written, cause)) = attributed {
            *d = depends_diag(&written, cause, d.span, d.layer);
        }
    }
}

/// Keep one note per cause — the first in document order — and count the
/// dependants it stands for (`(<n> more dependant(s))`).
pub fn fold_dependants(diags: &mut Vec<Diagnostic>) {
    let cause_of = |d: &Diagnostic| -> Option<String> {
        (d.code == W_DEPENDS_ON_INVALID)
            .then(|| {
                d.message
                    .split_once(DEPENDS_ON)
                    .map(|(_, tail)| tail.to_string())
            })
            .flatten()
    };
    let mut first: Vec<(String, usize, usize)> = Vec::new();
    let mut keep = vec![true; diags.len()];
    for (i, d) in diags.iter().enumerate() {
        let Some(cause) = cause_of(d) else { continue };
        match first.iter_mut().find(|(c, ..)| *c == cause) {
            Some((_, _, more)) => {
                *more += 1;
                keep[i] = false;
            }
            None => first.push((cause, i, 0)),
        }
    }
    for (_, i, more) in &first {
        if *more > 0 {
            diags[*i]
                .message
                .push_str(&format!(" ({more} more dependant(s))"));
        }
    }
    let mut i = 0;
    diags.retain(|_| {
        i += 1;
        keep[i - 1]
    });
}

/// [`fold_dependants`] across a project's documents (in walk order): the
/// first document's note of a cause stands for every later document's,
/// which are dropped and counted (0.41.0 §3.2: at most one per cause).
pub fn fold_project_dependants(results: &mut [(std::path::PathBuf, crate::CheckResult)]) {
    let tail = |d: &Diagnostic| -> Option<(String, usize)> {
        if d.code != W_DEPENDS_ON_INVALID {
            return None;
        }
        let (_, rest) = d.message.split_once(DEPENDS_ON)?;
        Some(
            match rest
                .strip_suffix(" more dependant(s))")
                .and_then(|r| r.rsplit_once(" ("))
            {
                Some((cause, n)) => (cause.to_string(), n.parse().unwrap_or(0)),
                None => (rest.to_string(), 0),
            },
        )
    };
    // cause -> (file index, diagnostic index, dependants beyond the first)
    let mut first: Vec<(String, usize, usize, usize)> = Vec::new();
    for (fi, (_, result)) in results.iter_mut().enumerate() {
        let mut keep = Vec::with_capacity(result.diagnostics.len());
        for d in &result.diagnostics {
            let Some((cause, more)) = tail(d) else {
                keep.push(true);
                continue;
            };
            match first.iter_mut().find(|(c, ..)| *c == cause) {
                Some((.., n)) => {
                    *n += more + 1;
                    keep.push(false);
                }
                None => {
                    first.push((cause, fi, keep.len(), more));
                    keep.push(true);
                }
            }
        }
        let mut i = 0;
        result.diagnostics.retain(|_| {
            i += 1;
            keep[i - 1]
        });
    }
    for (cause, fi, di, more) in first {
        let d = &mut results[fi].1.diagnostics[di];
        let (head, _) = d.message.split_once(DEPENDS_ON).expect("a note");
        d.message = if more > 0 {
            format!("{head}{DEPENDS_ON}{cause} ({more} more dependant(s))")
        } else {
            format!("{head}{DEPENDS_ON}{cause}")
        };
    }
}
