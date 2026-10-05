//! A document that writes its own `components:` or `uses:` replaces the
//! manifest's `defaults.components` / `defaults.uses` whole (a key the
//! document contains at all wins entire, [`crate::meta::parse_meta_kind_with_defaults`]).
//! The replacement is legal; what this module does is name it on the errors
//! it causes, so an author who added one import to a document is not left
//! chasing "unknown" names the defaults used to supply.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Severity};
use lute_syntax::ast::Document;

use crate::rel_schema::origin_display;
use crate::CheckInput;

/// The manifest's `defaults.<key>` entries (canonical paths) when the
/// document writes `key:` itself — the list its own one replaced. `None`
/// when the document does not write `key:`, or the manifest has no default.
pub(crate) fn replaced(
    frontmatter: Option<&serde_yaml::Value>,
    input: &CheckInput,
    key: &str,
) -> Option<Vec<String>> {
    frontmatter?.get(key)?;
    let paths: Vec<String> = match input.defaults.get(key)? {
        serde_yaml::Value::String(s) => vec![s.clone()],
        serde_yaml::Value::Sequence(v) => v
            .iter()
            .filter_map(|p| p.as_str().map(str::to_string))
            .collect(),
        _ => return None,
    };
    (!paths.is_empty()).then_some(paths)
}

/// The hint on `<beat use="name">` when `name` is a component only the
/// replaced `defaults.components` imports, from `src`.
pub(crate) fn component_hint(src: &Path) -> String {
    format!(
        " — this document's `components:` replaces `defaults.components` (from \
         lute.project.yaml), which imports it from `{}`; list that file here too",
        origin_display(src)
    )
}

/// Append the replaced-`uses:` note to every error that names a relation,
/// entity kind, domain, def, state path or cast member which the manifest's
/// `defaults.uses` declares but this document's own `uses:` does not reach.
/// An imported schema's own error (a `related` diagnostic in a schema file,
/// which `check-project` reports once at that file) carries the note too.
pub(crate) fn note_replaced_uses(
    doc: &Document,
    typed: &crate::meta::TypedMeta,
    input: &CheckInput,
    mut diags: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    if input.defaults.get("uses").is_none() || !diags.iter().any(|d| d.severity == Severity::Error)
    {
        return diags;
    }
    let Some(paths) = replaced(typed.yaml(), input, "uses") else {
        return diags;
    };
    let defaults =
        crate::schema_import::resolve_imports(Path::new("."), &paths, &[], doc.meta.span);
    let missing = missing_names(&defaults, &input.imports);
    if missing.is_empty() {
        return diags;
    }
    for d in &mut diags {
        annotate(d, &missing);
        // An imported schema's own error travels as a `related` diagnostic
        // in that file, where `check-project` reports it once; one in a
        // component only repeats its outer line.
        for r in &mut d.related {
            if Path::new(&r.file).extension().and_then(|e| e.to_str()) != Some("lute") {
                annotate(&mut r.diagnostic, &missing);
            }
        }
    }
    diags
}

/// Every name the defaults' imports declare that the document's own imports
/// do not, with the schema file that declares it.
fn missing_names(
    defaults: &crate::SchemaImports,
    own: &crate::SchemaImports,
) -> BTreeMap<String, PathBuf> {
    let mut have: BTreeSet<&str> = BTreeSet::new();
    have.extend(own.rel.relations.keys().map(String::as_str));
    have.extend(own.rel.kinds.keys().map(String::as_str));
    have.extend(own.rel.enums.keys().map(String::as_str));
    have.extend(own.defs.keys().map(String::as_str));
    have.extend(own.domains.keys().map(String::as_str));
    have.extend(own.state.decls.keys().map(String::as_str));
    have.extend(own.cast.keys().map(String::as_str));
    let o = &defaults.rel.origins;
    [
        &o.relations,
        &o.kinds,
        &o.domains,
        &o.defs,
        &o.state,
        &o.cast,
    ]
    .into_iter()
    .flatten()
    .filter(|(name, _)| !have.contains(name.as_str()))
    .map(|(name, origin)| (name.clone(), origin.file.clone()))
    .collect()
}

fn annotate(d: &mut Diagnostic, missing: &BTreeMap<String, PathBuf>) {
    if d.severity == Severity::Error {
        let hit = d
            .message
            .split('`')
            .skip(1)
            .step_by(2)
            .find_map(|t| missing.get_key_value(t.strip_prefix('@').unwrap_or(t)));
        if let Some((name, file)) = hit {
            let note = format!(
                " — this document's `uses:` replaces `defaults.uses` (from lute.project.yaml), \
                 and `{name}` is declared in `{}`, which it no longer imports; list that schema \
                 here too",
                origin_display(file)
            );
            d.message.push_str(&note);
        }
    }
}
