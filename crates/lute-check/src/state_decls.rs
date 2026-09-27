//! dsl 0.26.0 §2.1 (T1-1): every declaration of one state path across the
//! project agrees. A document's frontmatter `state:` is checked against its
//! own import graph only (`E-STATE-REDECLARE`); two documents — or a document
//! and a schema another document imports — that declare the same path with a
//! different `type`, `default`, `per` or `owner` would silently share one
//! runtime value. `E-STATE-DECL-CONFLICT` names both declarations. A
//! declaration refining one it `extends:` (a schema or document overriding
//! its base's default) is the language's own refinement, not a conflict.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_manifest::types::{type_str, Literal, Owner, Type};
use lute_syntax::ast::Document;

use crate::check::FoldedEnv;
use crate::meta::StateDecl;
use crate::rel_schema::{meta_position, state_key_span};

pub const E_STATE_DECL_CONFLICT: &str = "E-STATE-DECL-CONFLICT";

/// One declaration of a path: where it is and what it says.
struct Decl<'a> {
    file: PathBuf,
    /// `file`'s `extends:` bases, transitively (canonical paths).
    bases: BTreeSet<PathBuf>,
    span: Span,
    line: usize,
    ty: &'a Type,
    default: Option<&'a Literal>,
    per: Option<&'a str>,
    owner: Option<&'a Owner>,
}

impl Decl<'_> {
    fn shape(&self) -> String {
        let mut s = type_str(self.ty);
        if let Some(d) = self.default {
            let shown = match d {
                Literal::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
                other => serde_json::to_string(other).unwrap_or_default(),
            };
            s.push_str(&format!(", default {shown}"));
        }
        if let Some(p) = self.per {
            s.push_str(&format!(", per {p}"));
        }
        if self.owner.is_some() {
            s.push_str(", owner engine");
        }
        s
    }

    fn agrees(&self, other: &Decl<'_>) -> bool {
        self.ty == other.ty
            && self.default == other.default
            && self.per == other.per
            && self.owner == other.owner
    }

    fn at(&self) -> String {
        format!("{}:{}", self.file.display(), self.line)
    }
}

/// Every `state:` declaration the project's documents make or import,
/// grouped by path; one `E-STATE-DECL-CONFLICT` per declaration that
/// disagrees with the path's first declaration (file path, then position),
/// reported at the disagreeing declaration with the first as `related`.
/// `scene.*` paths are local to each scene (reset at every scene boundary)
/// and never shared, so they are exempt. `foldeds` is index-aligned with
/// `docs`.
pub fn check_project_state_decls(
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let mut by_path: BTreeMap<&str, Vec<Decl<'_>>> = BTreeMap::new();
    let mut extends: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    for ((file, doc), folded) in docs.iter().zip(foldeds) {
        let typed = &folded.typed;
        if let Ok(canon) = std::fs::canonicalize(file) {
            let dir = canon.parent().map(Path::to_path_buf).unwrap_or_default();
            let bases = typed
                .extends
                .iter()
                .filter_map(|e| std::fs::canonicalize(dir.join(e)).ok());
            extends.insert(canon, bases.collect());
        }
        let vocab = &folded.env.rel_vocab;
        let inline = typed.state.decls.iter().map(|(path, decl)| {
            let span = meta_position(&doc.meta, state_key_span(&doc.meta, path));
            (
                path,
                decl,
                typed.state_index.get(path),
                (file.clone(), span),
            )
        });
        let imported = vocab.origins.state.iter().filter_map(|(path, origin)| {
            let decl = folded.env.state.decls.get(path)?;
            let at = (origin.file.clone(), origin.span);
            Some((path, decl, vocab.indexed_state.get(path), at))
        });
        for (path, decl, per, (file, span)) in inline.chain(imported) {
            let line = span.line as usize;
            if path.starts_with("scene.") {
                continue;
            }
            let decls = by_path.entry(path.as_str()).or_default();
            if decls
                .iter()
                .any(|d| d.file == file && d.span.byte_start == span.byte_start)
            {
                continue;
            }
            decls.push(new_decl(file, span, line, decl, per));
        }
    }
    for decls in by_path.values_mut() {
        for d in decls.iter_mut() {
            d.bases = extends_closure(&d.file, &mut extends);
        }
    }

    let mut out = Vec::new();
    for (path, mut decls) in by_path {
        decls.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.span.byte_start.cmp(&b.span.byte_start))
        });
        // A declaration whose file `extends:` another declaring file refines
        // it; the reference is the first declaration that refines none.
        let canon: Vec<Option<PathBuf>> = decls
            .iter()
            .map(|d| std::fs::canonicalize(&d.file).ok())
            .collect();
        let refines = |d: &Decl<'_>| canon.iter().flatten().any(|f| d.bases.contains(f));
        let mut plain = decls.iter().filter(|d| !refines(d));
        let Some(first) = plain.next() else {
            continue;
        };
        for d in plain.filter(|d| !d.agrees(first)) {
            let message = format!(
                "state path `{path}` is declared as {} at `{}` but as {} at `{}`; every \
                 declaration of one path must agree on type, default, per and owner — they \
                 share one runtime value (declare it once in a schema both documents import) \
                 (dsl 0.26.0 §2.1)",
                first.shape(),
                first.at(),
                d.shape(),
                d.at(),
            );
            let mut diag = error(message.clone(), d.span);
            diag.related = vec![RelatedDiagnostic {
                file: first.file.display().to_string(),
                diagnostic: error(message, first.span),
            }];
            out.push((d.file.clone(), diag));
        }
    }
    out
}

fn new_decl<'a>(
    file: PathBuf,
    span: Span,
    line: usize,
    decl: &'a StateDecl,
    per: Option<&'a String>,
) -> Decl<'a> {
    Decl {
        file,
        bases: BTreeSet::new(),
        span,
        line,
        ty: &decl.ty,
        default: decl.default.as_ref(),
        per: per.map(String::as_str),
        owner: decl.owner.as_ref(),
    }
}

fn error(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_STATE_DECL_CONFLICT.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Every file `file` reaches through `extends:`, transitively. A schema's
/// own `extends:` is read from its (front)matter once and cached in `known`.
fn extends_closure(file: &Path, known: &mut BTreeMap<PathBuf, Vec<PathBuf>>) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut todo: Vec<PathBuf> = std::fs::canonicalize(file).into_iter().collect();
    while let Some(f) = todo.pop() {
        let bases = known
            .entry(f.clone())
            .or_insert_with(|| declared_extends(&f))
            .clone();
        for b in bases {
            if out.insert(b.clone()) {
                todo.push(b);
            }
        }
    }
    out
}

/// The `extends:` entries of the schema or document at `file`, canonical.
fn declared_extends(file: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let yaml = match text.strip_prefix("---") {
        Some(rest) => rest.split("\n---").next().unwrap_or_default(),
        None => text.as_str(),
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(yaml) else {
        return Vec::new();
    };
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let entries: Vec<&str> = match value.get("extends") {
        Some(serde_yaml::Value::String(s)) => vec![s.as_str()],
        Some(serde_yaml::Value::Sequence(v)) => v.iter().filter_map(|e| e.as_str()).collect(),
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .filter_map(|e| std::fs::canonicalize(dir.join(e)).ok())
        .collect()
}

/// dsl 0.28.0 (T3-45): a declared path that is both a value and the prefix of
/// another declared path — `run.lanterns` beside `run.lanterns.gold`, or a
/// `scene.date` a directive's result state fills as `scene.date.stars.*`.
/// Stored state cannot hold a value and fields under one name, so the pair is
/// an `E-STATE-DECL`, reported once per value path — `locate(path, message)`
/// builds it at `path`'s declaration (the value's, else the longer path's).
/// A field no author declared (`declared`) is a directive's result: the
/// report names the directives storing results under the value
/// (`results_under`), not one of the fields a call happened to open, so the
/// reports of every document using the value fold into one.
pub(crate) fn check_value_prefix(
    schema: &crate::meta::StateSchema,
    declared: &dyn Fn(&str) -> bool,
    results_under: &dyn Fn(&str) -> Vec<String>,
    locate: &dyn Fn(&str, &str) -> Option<Diagnostic>,
) -> Vec<Diagnostic> {
    let keys: Vec<&str> = schema.decls.keys().map(String::as_str).collect();
    let mut out = Vec::new();
    for (i, value) in keys.iter().enumerate() {
        // A container type legitimately has fields.
        let container = matches!(
            schema.decls[*value].ty,
            Type::Record(_) | Type::Map { .. } | Type::List(_)
        );
        if container || crate::cel_paths::is_engine_owned_path(value) {
            continue;
        }
        let prefix = format!("{value}.");
        let fields: Vec<&str> = keys[i + 1..]
            .iter()
            .take_while(|k| k.starts_with(value))
            .filter(|k| k.starts_with(&prefix))
            .copied()
            .collect();
        let Some(&first) = fields.first() else {
            continue;
        };
        let message = match fields.iter().find(|f| declared(f)) {
            Some(field) => format!(
                "`{value}` is declared as a value and is also the prefix of `{field}` — stored \
                 state cannot hold a value and fields under one name; rename one of them"
            ),
            None => {
                let owners: Vec<String> = results_under(value)
                    .iter()
                    .map(|d| format!("`::{d}`"))
                    .collect();
                let who = match owners.len() {
                    0 => "a directive stores its results".to_string(),
                    1 => format!("{} stores its results", owners[0]),
                    _ => format!("{} store their results", owners.join(", ")),
                };
                format!(
                    "`{value}` is declared as a value, but {who} under `{value}.` — stored state \
                     cannot hold a value and fields under one name; rename `{value}`"
                )
            }
        };
        out.extend(locate(value, &message).or_else(|| locate(first, &message)));
    }
    out
}

/// A `{ domain: K }` / `{ entity: K }` state path whose `K` no enum or entity
/// kind declares (`E-DOMAIN-UNKNOWN`), and one whose `default:` is not one of
/// a closed `K`'s members (`E-STATE-DECL`) — played, a value no `<match>` arm
/// takes. A `per:` family (`families`: family → kind) is reported once, at
/// the family. `locate(path, key, message, code)` builds the report at the
/// path's `key` (`type` / `default`); `None` for a path no author declared.
pub(crate) fn check_domain_types(
    schema: &crate::meta::StateSchema,
    domains: &BTreeMap<String, lute_manifest::snapshot::Domain>,
    families: &BTreeMap<String, String>,
    locate: &dyn Fn(&str, &str, &str, &str) -> Option<Diagnostic>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for (path, decl) in &schema.decls {
        let (Type::Domain(name) | Type::Entity(name)) = &decl.ty else {
            continue;
        };
        let family = path
            .rsplit_once('.')
            .map(|(f, _)| f)
            .filter(|f| families.contains_key(*f));
        let at = family.unwrap_or(path);
        if !seen.insert(at) {
            continue;
        }
        let (form, word) = match &decl.ty {
            Type::Entity(_) => ("entity", "entity kind"),
            _ => ("domain", "enum"),
        };
        match domains.get(name) {
            None => {
                let names: Vec<String> = domains.keys().cloned().collect();
                let message = format!(
                    "`{at}` is typed `{{ {form}: {name} }}`, but `{name}` is not a declared enum \
                     or entity kind{}",
                    crate::rel_schema::member_hint(name, &names)
                );
                out.extend(locate(at, "type", &message, "E-DOMAIN-UNKNOWN"));
            }
            Some(d) if !d.open => {
                let Some(Literal::Str(v)) = &decl.default else {
                    continue;
                };
                if d.members.contains(v) {
                    continue;
                }
                let message = format!(
                    "`{at}`'s `default: {v}` is not a member of {word} `{name}` [{}]{}",
                    d.members.join(", "),
                    crate::rel_schema::member_hint(v, &d.members)
                );
                out.extend(locate(at, "default", &message, "E-STATE-DECL"));
            }
            Some(_) => {}
        }
    }
    out
}
