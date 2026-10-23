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

use crate::ProjectDoc;
use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_manifest::types::{type_str, Literal, Owner, Type};

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
                Literal::Int(n) => n.to_string(),
                Literal::Double(n) if n.fract() == 0.0 && n.abs() < 1e15 => {
                    format!("{}", *n as i64)
                }
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
    docs: &[ProjectDoc<'_>],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let mut by_path: BTreeMap<std::borrow::Cow<'_, str>, Vec<Decl<'_>>> = BTreeMap::new();
    let mut extends: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
    // The relational vocabularies whose imported declarations are already
    // recorded. A document sharing one adds nothing a previous document did
    // not: each of its imported declarations sits at the same origin, and a
    // declaration is recorded once per origin. Skipping them keeps this pass
    // from walking the project's imported state once per document (spec
    // 0.49.0 §9.2).
    let mut seen_vocabs: std::collections::HashSet<*const crate::rel_schema::RelVocab> =
        std::collections::HashSet::new();
    for (item, folded) in docs.iter().zip(foldeds) {
        let file = item.path;
        let doc = item.doc;
        let typed = item.meta;
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
            let span = meta_position(&doc.meta, state_key_span(&doc.meta, &path));
            let per = typed.state_index.get(path.as_ref());
            (path, decl, per, (file.to_path_buf(), span))
        });
        let first_use = seen_vocabs.insert(std::sync::Arc::as_ptr(vocab));
        let imported =
            vocab
                .origins
                .state
                .iter()
                .filter(|_| first_use)
                .filter_map(|(path, origin)| {
                    let decl = folded.env.state.get(path)?;
                    let at = (origin.file.clone(), origin.span);
                    Some((
                        std::borrow::Cow::Borrowed(path.as_str()),
                        decl,
                        vocab.indexed_state.get(path),
                        at,
                    ))
                });
        for (path, decl, per, (file, span)) in inline.chain(imported) {
            let line = span.line as usize;
            if path.starts_with("scene.") {
                continue;
            }
            let decls = by_path.entry(path).or_default();
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
        evidence: None,
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
    let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(yaml) else {
        return Vec::new();
    };
    let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
    let entries: Vec<&str> = match value.get("extends") {
        Some(yaml_serde::Value::String(s)) => vec![s.as_str()],
        Some(yaml_serde::Value::Sequence(v)) => v.iter().filter_map(|e| e.as_str()).collect(),
        _ => Vec::new(),
    };
    entries
        .into_iter()
        .filter_map(|e| std::fs::canonicalize(dir.join(e)).ok())
        .collect()
}

/// The callbacks [`value_prefix_at`] reports through: `declared(path)` — an
/// author declared the path; `results_under(value)` — the directives
/// storing results under `value.`; `locate(path, message)` — the report at
/// `path`'s declaration, `None` for a path no author declared.
pub(crate) struct ValuePrefixSite<'a> {
    pub(crate) declared: &'a dyn Fn(&str) -> bool,
    pub(crate) results_under: &'a dyn Fn(&str) -> Vec<String>,
    pub(crate) locate: &'a dyn Fn(&str, &str) -> Option<Diagnostic>,
}

/// dsl 0.28.0 (T3-45): a declared path that is both a value and the prefix of
/// another declared path — `run.lanterns` beside `run.lanterns.gold`, or a
/// `scene.date` a directive's result state fills as `scene.date.stars.*`.
/// Stored state cannot hold a value and fields under one name, so the pair is
/// an `E-STATE-DECL`, reported once per value path, in path order.
pub(crate) fn check_value_prefix(
    state: &crate::StateView,
    site: &ValuePrefixSite<'_>,
) -> Vec<Diagnostic> {
    state
        .keys()
        .filter_map(|value| value_prefix_at(state, &value, site))
        .collect()
}

/// [`check_value_prefix`] by value path, over every declared path.
pub(crate) fn value_prefix_by_path(
    state: &crate::StateView,
    site: &ValuePrefixSite<'_>,
) -> BTreeMap<String, Diagnostic> {
    state
        .keys()
        .filter_map(|value| {
            let d = value_prefix_at(state, &value, site)?;
            Some((value.into_owned(), d))
        })
        .collect()
}

/// The [`check_value_prefix`] report of one declared `value`, when it is a
/// value with fields: the report is built at the value's declaration, else
/// at its first field's. A field no author declared (`declared`) is a
/// directive's result: the report names the directives storing results
/// under the value, not one of the fields a call happened to open, so the
/// reports of every document using the value fold into one.
pub(crate) fn value_prefix_at(
    state: &crate::StateView,
    value: &str,
    site: &ValuePrefixSite<'_>,
) -> Option<Diagnostic> {
    // A container type legitimately has fields.
    let container = matches!(
        state.get(value)?.ty,
        Type::Record(_) | Type::Map { .. } | Type::List(_)
    );
    if container || crate::cel_paths::is_engine_owned_path(value) {
        return None;
    }
    let prefix = format!("{value}.");
    let mut fields = state.with_prefix(&prefix).map(|(k, _)| k);
    let first = fields.next()?;
    let message = match std::iter::once(first.clone())
        .chain(fields)
        .find(|f| (site.declared)(f))
    {
        Some(field) => format!(
            "`{value}` is declared as a value and is also the prefix of `{field}` — stored \
             state cannot hold a value and fields under one name; rename one of them"
        ),
        None => {
            let owners: Vec<String> = (site.results_under)(value)
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
    (site.locate)(value, &message).or_else(|| (site.locate)(&first, &message))
}

/// `locate(path, key, message, code)` for [`check_domain_types`]: the report
/// at the path's `key` (`type` / `default`); `None` for a path no author
/// declared.
pub(crate) type DomainTypeLocate<'a> = &'a dyn Fn(&str, &str, &str, &str) -> Option<Diagnostic>;

/// A `{ domain: K }` / `{ entity: K }` state path whose `K` no enum or entity
/// kind declares (`E-DOMAIN-UNKNOWN`), and one whose `default:` is not one of
/// a closed `K`'s members (`E-STATE-DECL`) — played, a value no `<match>` arm
/// takes. A `per:` family (`families`: family → kind) is reported once, at
/// the family, as its first typed member decides.
pub(crate) fn check_domain_types(
    state: &crate::StateView,
    domains: &BTreeMap<String, lute_manifest::snapshot::Domain>,
    families: &BTreeMap<String, String>,
    locate: DomainTypeLocate<'_>,
) -> Vec<Diagnostic> {
    domain_types_by_at(state, domains, families, locate)
        .into_values()
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

/// Where [`check_domain_types`] reports `path`: its `per:` family, else
/// itself.
pub(crate) fn domain_type_at<'p>(path: &'p str, families: &BTreeMap<String, String>) -> &'p str {
    path.rsplit_once('.')
        .map(|(f, _)| f)
        .filter(|f| families.contains_key(*f))
        .unwrap_or(path)
}

/// [`check_domain_types`]'s reports by reported path (`at`), each with the
/// path that decided it (its order in the report stream).
pub(crate) fn domain_types_by_at(
    state: &crate::StateView,
    domains: &BTreeMap<String, lute_manifest::snapshot::Domain>,
    families: &BTreeMap<String, String>,
    locate: DomainTypeLocate<'_>,
) -> BTreeMap<String, (String, Diagnostic)> {
    let mut out = BTreeMap::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (path, decl) in state.iter() {
        if !matches!(decl.ty, Type::Domain(_) | Type::Entity(_)) {
            continue;
        }
        let at = domain_type_at(&path, families);
        if seen.contains(at) {
            continue;
        }
        seen.insert(at.to_string());
        if let Some(d) = domain_type_report(at, decl, domains, locate) {
            out.insert(at.to_string(), (path.to_string(), d));
        }
    }
    out
}

/// [`check_domain_types`]'s report for the reported path `at`, recomputed
/// over `state`: its first `{ domain: K }` path decides, as in the full
/// scan.
pub(crate) fn domain_type_report_at(
    state: &crate::StateView,
    at: &str,
    domains: &BTreeMap<String, lute_manifest::snapshot::Domain>,
    families: &BTreeMap<String, String>,
    locate: DomainTypeLocate<'_>,
) -> Option<(String, Diagnostic)> {
    let own = state
        .get(at)
        .filter(|_| domain_type_at(at, families) == at)
        .map(|decl| (std::borrow::Cow::Borrowed(at), decl));
    let prefix = format!("{at}.");
    let members = state
        .with_prefix(&prefix)
        .filter(|_| families.contains_key(at))
        .filter(|(p, _)| !p[prefix.len()..].contains('.'));
    let (path, decl) = own
        .into_iter()
        .chain(members)
        .find(|(_, d)| matches!(d.ty, Type::Domain(_) | Type::Entity(_)))?;
    domain_type_report(at, decl, domains, locate).map(|d| (path.to_string(), d))
}

fn domain_type_report(
    at: &str,
    decl: &StateDecl,
    domains: &BTreeMap<String, lute_manifest::snapshot::Domain>,
    locate: DomainTypeLocate<'_>,
) -> Option<Diagnostic> {
    let (Type::Domain(name) | Type::Entity(name)) = &decl.ty else {
        return None;
    };
    let (form, word) = match &decl.ty {
        Type::Entity(_) => ("entity", "entity kind"),
        _ => ("domain", "enum"),
    };
    match domains.get(name) {
        None => {
            let names: Vec<String> = domains.keys().cloned().collect();
            let message = format!(
                "`{at}` is typed `{{ {form}: {name} }}`, but `{name}` is not a declared enum \
                 or entity kind{}{}",
                crate::rel_schema::member_hint(name, &names),
                crate::renames::renamed_domain_hint(name, domains)
            );
            locate(at, "type", &message, "E-DOMAIN-UNKNOWN")
        }
        Some(d) if !d.open => {
            let Some(Literal::Str(v)) = &decl.default else {
                return None;
            };
            if d.members.contains(v) {
                return None;
            }
            let message = format!(
                "`{at}`'s `default: {v}` is not a member of {word} `{name}` [{}]{}",
                d.members.join(", "),
                crate::rel_schema::member_hint(v, &d.members)
            );
            locate(at, "default", &message, "E-STATE-DECL")
        }
        Some(_) => None,
    }
}
