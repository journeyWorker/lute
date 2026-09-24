//! Cast (dsl 0.23.0 §7): a plugin (`cast/*.yaml`) or a schema document
//! (`cast:`) MAY declare the speaker ids a project uses. Once any cast is
//! declared, a content line whose speaker is outside it is
//! [`E_CAST_UNKNOWN`] with a did-you-mean; without one, speakers stay
//! shape-only. `narrator` is always a speaker.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Document, Line};

use crate::schema_import::SchemaImports;

/// A content line's speaker is not in the declared cast (dsl 0.23.0 §7).
pub const E_CAST_UNKNOWN: &str = "E-CAST-UNKNOWN";

/// The cast a document is checked against: the active plugins' `cast`
/// exports, the import-reachable schemas' `cast:` and — for a schema
/// document itself — its own `cast:`. A plugin entry wins a same-id clash
/// (it carries the engine's display name).
pub fn declared_cast(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    own: &[CastMember],
) -> BTreeMap<String, CastMember> {
    let mut cast = snapshot.cast.clone();
    for c in imports.cast.values().chain(own) {
        cast.entry(c.id.clone()).or_insert_with(|| c.clone());
    }
    cast
}

/// `E-CAST-UNKNOWN` for every line of `doc` (scene shots, quest bodies, lore
/// entries and bundle beats) whose speaker is neither `narrator` nor a cast
/// member. Silent when `cast` is empty (shape-only).
pub fn check_speakers(doc: &Document, cast: &BTreeMap<String, CastMember>) -> Vec<Diagnostic> {
    if cast.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&Line> = Vec::new();
    let bodies = doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body));
    for body in bodies {
        crate::match_check::collect_lines(body, &mut lines);
    }
    lines
        .into_iter()
        .filter(|l| l.speaker != "narrator" && !cast.contains_key(&l.speaker))
        .map(|l| unknown(l, cast))
        .collect()
}

fn unknown(line: &Line, cast: &BTreeMap<String, CastMember>) -> Diagnostic {
    let mut message = format!(
        "speaker `{}` is not in the declared cast (dsl 0.23.0 §7)",
        line.speaker
    );
    let candidates = cast.keys().map(String::as_str).chain(["narrator"]);
    if let Some(near) = lute_manifest::suggest::nearest(&line.speaker, candidates, 2) {
        message.push_str(&format!(" — did you mean `{near}`?"));
    }
    // The speaker id sits just past the line's leading `@`; line/column are
    // recomputed from the bytes when the checker normalizes spans.
    let start = line.span.byte_start + 1;
    Diagnostic {
        code: E_CAST_UNKNOWN.to_string(),
        severity: Severity::Error,
        message,
        span: Span {
            byte_start: start,
            byte_end: start + line.speaker.len(),
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        },
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
