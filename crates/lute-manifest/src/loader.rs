//! Plugin package loader (plugin §4). Reads a `plugins/<id>/` directory into a
//! `LoadedPlugin`, honoring `exports`, sorting files byte-wise, and rejecting
//! per-package duplicate ids within a kind. Never panics: every failure is a
//! `LoadError` in the returned vec.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::schema::{
    AssetKindDecl, AssetKindsFile, AttrDecl, BridgeCapability, BridgeFile, CastFile, CastMember,
    DefDecl, DefsFile, DirectiveDecl, DirectivesFile, EnumsFile, EventDecl, EventsFile,
    FrontmatterFile, LintsFile, OccasionBody, OccasionDecl, OccasionTarget, OccasionsFile,
    PluginManifest, ProviderDecl, ProvidersFile, RewardKindDecl, RewardKindsFile, StampAttrsFile,
    StateFile, StateShape, StateTemplate,
};
use crate::types::{type_str, Type};

#[derive(Clone, Debug)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub directives: Vec<DirectiveDecl>,
    pub enums: BTreeMap<String, crate::snapshot::Domain>,
    pub state_shapes: Vec<StateShape>,
    pub state_templates: Vec<StateTemplate>,
    pub providers: Vec<ProviderDecl>,
    pub bridge: Vec<BridgeCapability>,
    pub defs: Vec<DefDecl>,
    pub frontmatter: BTreeMap<String, Type>,
    pub asset_kinds: Vec<AssetKindDecl>,
    pub events: Vec<EventDecl>,
    /// plugin §14.1 `stampAttrs` export: stamp attributes, lowered into the
    /// record's stamp rather than its own fields. Each keeps its raw `on:`
    /// names; assembly resolves them (0.50.0 §3.2).
    pub stamp_attrs: Vec<crate::stamps::DeclaredStampAttr>,
    /// dsl 0.16.0 §4 `rewardKinds` export: capability-declared reward
    /// vocabulary. Each entry carries the id (map key), optional `target`
    /// (`providerRef` pattern — assembly rejects a kind whose provider is
    /// absent), and optional extra `attrs` for game-specific slots. Folded
    /// into [`crate::snapshot::CapabilitySnapshot::reward_kinds`] at
    /// assembly, participating in `capabilitySnapshot` via the guarded
    /// hash section.
    pub reward_kinds: Vec<RewardKindDecl>,
    /// dsl 0.21.0 §2 `occasions/*.yaml`: engine occasion vocabulary (name,
    /// `select`, `target`). Folded into
    /// [`crate::snapshot::CapabilitySnapshot::occasions`] at assembly as a
    /// guarded `capabilitySnapshot` section, the `rewardKinds` precedent.
    pub occasions: Vec<OccasionDecl>,
    /// lint-system design §6 `lints/*.yaml`: DECLARATIVE lint rules a
    /// plugin publishes. Stored with RAW ids; a consumer namespaces each
    /// as `<plugin-id>/<id>` via [`crate::lint::namespace_active_lints`].
    /// DELIBERATELY not folded into
    /// [`crate::snapshot::CapabilitySnapshot`] or `capabilitySnapshot` —
    /// lints are advisory and must never change artifact identity
    /// (design §1 non-goals).
    pub lints: Vec<crate::lint::LintRuleDecl>,
    /// dsl 0.23.0 §7 `cast/*.yaml`: the declared speaker ids and display
    /// names. Folded into [`crate::snapshot::CapabilitySnapshot::cast`] at
    /// assembly as a guarded `capabilitySnapshot` section.
    pub cast: Vec<CastMember>,
    /// Where each named declaration sits in the package's files — kind
    /// (`directive`, `event`, `occasion`, `rewardKind`, `cast`, …) → name →
    /// `path:line:column` of its FIRST declaration — so an error assembly
    /// raises about ONE declaration points at its line ([`LoadedPlugin::site`]).
    pub sites: Sites,
    /// 0.41.0 §3.2: declarations dropped from the lists above because they
    /// are invalid (a `type: number`, or a reference to such a
    /// declaration of this package), by `(kind, name)` — kept by name so a
    /// use is attributed to them rather than reported unknown.
    pub invalid: Vec<((&'static str, String), crate::snapshot::InvalidDecl)>,
}

/// [`LoadedPlugin::sites`]: kind → name → `path:line:column`.
pub type Sites = BTreeMap<&'static str, BTreeMap<String, String>>;

impl LoadedPlugin {
    /// `path:line:column` of the `kind` declaration `name`, when it came from
    /// a file.
    pub fn site(&self, kind: &str, name: &str) -> Option<&str> {
        self.sites.get(kind)?.get(name).map(String::as_str)
    }
}

/// One export file being merged into its package.
struct Source<'a> {
    /// The package directory: a duplicate names its earlier declaration
    /// relative to it.
    pkg: &'a Path,
    file: &'a Path,
    text: &'a str,
}

impl Source<'_> {
    /// The `kind` declaration `name` at byte `at` of this file: when `new`,
    /// record its site (when its place is known) and return `true`; else it
    /// is a [`LoadError::DuplicateId`] naming the first declaration as
    /// `file:line` relative to the package, and `false`.
    fn admit(
        &self,
        sites: &mut Sites,
        kind: &'static str,
        name: String,
        at: Option<usize>,
        new: bool,
        errs: &mut Vec<LoadError>,
    ) -> bool {
        if new {
            self.record(sites, kind, name, at);
            return true;
        }
        errs.push(self.duplicate(sites, kind, name, at));
        false
    }

    /// Record the site of the `kind` declaration `name` at byte `at`.
    fn record(&self, sites: &mut Sites, kind: &'static str, name: String, at: Option<usize>) {
        if let Some(site) = self.site_at(at) {
            sites.entry(kind).or_default().insert(name, site);
        }
    }

    /// `path:line:column` of byte `at` of this file.
    fn site_at(&self, at: Option<usize>) -> Option<String> {
        let (line, col) = crate::yaml_text::line_col(self.text, at?);
        Some(format!("{}:{line}:{col}", self.file.display()))
    }

    /// [`LoadError::DuplicateId`] for the `kind` declaration `name` at byte
    /// `at`, naming the first declaration of `name` as `file:line` relative
    /// to the package.
    fn duplicate(
        &self,
        sites: &Sites,
        kind: &'static str,
        name: String,
        at: Option<usize>,
    ) -> LoadError {
        let first = sites.get(kind).and_then(|m| m.get(&name)).cloned();
        self.duplicate_of(kind, name, at, first.as_deref(), Vec::new())
    }

    /// [`LoadError::DuplicateId`] for the `kind` declaration `name` at byte
    /// `at`, naming the declaration it repeats (at `first`, a
    /// `path:line:column`) as `file:line` relative to the package. `shared`
    /// names the surfaces two stamp entries share.
    fn duplicate_of(
        &self,
        kind: &'static str,
        name: String,
        at: Option<usize>,
        first: Option<&str>,
        shared: Vec<String>,
    ) -> LoadError {
        let at = at.map(|o| crate::yaml_text::line_col(self.text, o));
        let first = first.map(|site| {
            let site = site.rsplit_once(':').map_or(site, |(s, _)| s);
            let (path, line) = site.rsplit_once(':').unwrap_or((site, ""));
            let rel = Path::new(path).strip_prefix(self.pkg).map_or_else(
                |_| path.to_string(),
                |rel| {
                    rel.components()
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/")
                },
            );
            format!("{rel}:{line}")
        });
        LoadError::DuplicateId {
            kind: kind.into(),
            id: name,
            file: self.file.display().to_string(),
            at,
            first,
            shared,
        }
    }
}

/// The keys `plugin.yaml` takes (plugin §5).
const MANIFEST_KEYS: &[&str] = &["id", "version", "kind", "depends", "exports", "options"];

/// The export kinds a manifest's `exports:` may name (plugin §4).
pub const EXPORT_KINDS: &[&str] = &[
    "directives",
    "state",
    "providers",
    "bridge",
    "defs",
    "enums",
    "frontmatter",
    "docs",
    "assetKinds",
    "events",
    "stampAttrs",
    "rewardKinds",
    "occasions",
    "lints",
    "cast",
];

/// Export keys renamed in 0.28 (old → new); the old spelling is refused with
/// the new one.
const RENAMED_EXPORTS: &[(&str, &str)] = &[
    ("assetkinds", "assetKinds"),
    ("stampattrs", "stampAttrs"),
    ("rewardkinds", "rewardKinds"),
];

/// A (line, column) in a file, both 1-based, the column in characters.
pub type At = Option<(usize, usize)>;

#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    /// `plugin.yaml` is missing, unreadable, not YAML, or lacks a required
    /// key. `file` is its path; `at`, where in it.
    Manifest {
        file: String,
        at: At,
        msg: String,
    },
    /// An export file does not parse or declares something inconsistent.
    Parse {
        file: String,
        at: At,
        msg: String,
    },
    /// A key `plugin.yaml` or an export file does not take (unknown, or an
    /// old spelling), or a `kind:` other than `capability`.
    Key {
        file: String,
        at: At,
        msg: String,
    },
    /// A name declared twice within one package, or two packages with one
    /// id. The first declaration is kept and the later one ignored, so the
    /// package still loads. `file`/`at` locate the later declaration;
    /// `first` names the earlier one as `file:line`, relative to the package
    /// (for a package id, to the plugins directory). `shared` names the
    /// `on:` surfaces two `stampAttrs` entries of one name share (0.50.0
    /// §3.2); it is empty when the whole name is declared twice.
    DuplicateId {
        kind: String,
        id: String,
        file: String,
        at: At,
        first: Option<String>,
        shared: Vec<String>,
    },
    /// 0.41.0 §3.2: a declaration typed `number`. The package still loads;
    /// `entry` — the export entry the type sits in, as `(kind, name)` — is
    /// kept by name as invalid ([`LoadedPlugin::invalid`]).
    TypeNumber {
        file: String,
        at: At,
        msg: String,
        entry: Option<(&'static str, String)>,
    },
    /// 0.41.1 §3.5: a `layer: bridge` + `bridgeCall` directive with no
    /// `effects.writes` reading a bridge result, at its `name:`. The
    /// package still loads — the declaration is otherwise sound, so its
    /// uses are checked — but the error fails the run, so nothing compiles.
    BridgeNoResultWrite {
        file: String,
        at: At,
        msg: String,
    },
    MissingExportDir {
        export: String,
        path: String,
    },
    /// An I/O or encoding failure reading a listed file/dir.
    Io {
        path: String,
        msg: String,
    },
    /// An export key outside [`EXPORT_KINDS`] (plugin §4), at its place in
    /// `plugin.yaml` (`file`).
    UnknownExport {
        file: String,
        at: At,
        export: String,
    },
    /// plugin §7: an asset-kind segment (`assetKinds` export, `AssetSegment.ty`)
    /// declared a `Type` outside the closed set a segment position admits.
    /// `AssetSegment.ty` is the SAME shared `Type` enum every other typed
    /// position uses (`crates/lute-manifest/src/schema.rs`), so every `Type`
    /// variant parses syntactically in a segment position; only four are
    /// semantically admitted there (`enum`, `int`, `string`,
    /// `providerRef` — the set `asset.rs::validate_segments` actually
    /// enforces, and the only ones the plugin-system closed `Type ::=`
    /// production (0.0.1 §7) leaves unrestricted AND representable as the
    /// single delimited string token a decomposed segment always is). Every
    /// other variant is rejected here, at load, before an inadmissible
    /// declaration can reach assembly or silently validate nothing against
    /// an authored id (see `check_asset_segment_types` for the full
    /// per-variant reasoning).
    AssetSegmentType {
        file: String,
        kind: String,
        segment: String,
        found: String,
    },
}

impl LoadError {
    /// Stable, machine-readable code per variant (plugin §11); the `E-*` family
    /// mirrors the checker's diagnostic codes so a consumer (CLI/LSP) can key on
    /// it instead of parsing the rendered [`std::fmt::Display`] text below.
    pub fn code(&self) -> &'static str {
        match self {
            LoadError::Manifest { .. } => "E-PLUGIN-MANIFEST",
            LoadError::Parse { .. } => "E-PLUGIN-PARSE",
            LoadError::Key { .. } => "E-PLUGIN-KEY",
            LoadError::DuplicateId { .. } => "E-PLUGIN-DUP-ID",
            LoadError::TypeNumber { .. } => crate::types::E_TYPE_NUMBER,
            LoadError::BridgeNoResultWrite { .. } => crate::validate::E_BRIDGE_NO_RESULT_WRITE,
            LoadError::MissingExportDir { .. } => "E-PLUGIN-MISSING-EXPORT",
            LoadError::Io { .. } => "E-PLUGIN-IO",
            LoadError::UnknownExport { .. } => "E-PLUGIN-UNKNOWN-EXPORT",
            LoadError::AssetSegmentType { .. } => "E-PLUGIN-ASSET-SEGMENT-TYPE",
        }
    }
}

/// `path:line:column: ` when the place is known, else `path: `.
fn place(f: &mut std::fmt::Formatter<'_>, file: &str, at: At) -> std::fmt::Result {
    match at {
        Some((line, col)) => write!(f, "{file}:{line}:{col}: "),
        None => write!(f, "{file}: "),
    }
}

impl std::fmt::Display for LoadError {
    /// Human-readable rendering, surfaced by `project.rs` as the resolver's
    /// `ResolveDiag` message. Mirrors [`crate::assemble::AssembleError`]'s
    /// `Display` (`assemble.rs`), which made the identical argument when
    /// assembly errors stopped leaking `Debug` prose: since an `E-`-severity
    /// diagnostic here gates the CLI exit code, this text is the whole of
    /// what a failing plugin author sees — a Rust struct dump or serde's own
    /// wording is not an acceptable answer. A located error leads with
    /// `path:line:column:`, as a document diagnostic does.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Manifest { file, at, msg }
            | LoadError::Parse { file, at, msg }
            | LoadError::Key { file, at, msg }
            | LoadError::TypeNumber { file, at, msg, .. }
            | LoadError::BridgeNoResultWrite { file, at, msg } => {
                place(f, file, *at)?;
                f.write_str(msg)
            }
            LoadError::DuplicateId {
                kind,
                id,
                file,
                at,
                first,
                shared,
            } => {
                place(f, file, *at)?;
                if !shared.is_empty() {
                    let names: Vec<String> = shared.iter().map(|s| format!("`{s}`")).collect();
                    let (noun, names) = match names.split_last() {
                        Some((last, rest)) if !rest.is_empty() => {
                            ("surfaces", format!("{} and {last}", rest.join(", ")))
                        }
                        _ => ("surface", names.join("")),
                    };
                    write!(f, "{kind} `{id}` is declared twice on the {noun} {names}")?;
                    if let Some(first) = first {
                        write!(f, " (first at {first})")?;
                    }
                    return f.write_str(" — give each entry disjoint `on:` surfaces");
                }
                match first {
                    Some(first) => write!(f, "{kind} `{id}` is already declared at {first}")?,
                    None => write!(f, "{kind} `{id}` is declared more than once")?,
                }
                f.write_str("; this declaration is ignored — rename or remove one")
            }
            LoadError::MissingExportDir { export, path } => {
                write!(f, "export `{export}` names `{path}`, which does not exist")
            }
            LoadError::Io { path, msg } => write!(f, "`{path}` could not be read: {msg}"),
            LoadError::UnknownExport { file, at, export } => {
                place(f, file, *at)?;
                match RENAMED_EXPORTS.iter().find(|(old, _)| old == export) {
                    Some((_, new)) => write!(
                        f,
                        "export `{export}` is now spelled `{new}` — write `{new}:` in `exports:`"
                    ),
                    None => write!(
                        f,
                        "export `{export}` is not an export kind{} (kinds: {})",
                        crate::suggest::did_you_mean(export, EXPORT_KINDS.iter().copied()),
                        EXPORT_KINDS.join(", ")
                    ),
                }
            }
            LoadError::AssetSegmentType {
                file,
                kind,
                segment,
                found,
            } => write!(
                f,
                "`{file}`: assetKind `{kind}` segment `{segment}` declares type `{found}`, \
                 but a segment position admits only `enum`, `int`, `string`, or \
                 `providerRef`"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Read one plugin package. `dir` MUST contain `plugin.yaml`. Any error —
/// even a duplicate name [`load_plugins_dir`] loads past — is `Err`.
pub fn load_plugin_dir(dir: &Path) -> Result<LoadedPlugin, Vec<LoadError>> {
    match load_package(dir) {
        Ok((loaded, dups)) if dups.is_empty() => Ok(loaded),
        Ok((_, dups)) => Err(dups),
        Err((_, errs)) => Err(errs),
    }
}

/// [`load_plugin_dir`], but a package whose only faults are duplicate names
/// still loads — the first declaration of each is kept — with the
/// [`LoadError::DuplicateId`]s beside it; and a failure also carries the
/// manifest id when the manifest named one (so assembly says the plugin
/// failed to load rather than that it is not installed).
#[expect(
    clippy::type_complexity,
    reason = "the error side carries the manifest id with the errors, so assembly can tell a failed load from a missing plugin; the one caller matches it in place"
)]
fn load_package(
    dir: &Path,
) -> Result<(LoadedPlugin, Vec<LoadError>), (Option<String>, Vec<LoadError>)> {
    let mut errs = Vec::new();
    let manifest_path = dir.join("plugin.yaml");
    let (manifest, manifest_text) = read_manifest(&manifest_path)?;

    let mut out = LoadedPlugin {
        manifest: manifest.clone(),
        directives: Vec::new(),
        enums: BTreeMap::new(),
        state_shapes: Vec::new(),
        state_templates: Vec::new(),
        providers: Vec::new(),
        bridge: Vec::new(),
        defs: Vec::new(),
        frontmatter: BTreeMap::new(),
        asset_kinds: Vec::new(),
        events: Vec::new(),
        stamp_attrs: Vec::new(),
        reward_kinds: Vec::new(),
        occasions: Vec::new(),
        lints: Vec::new(),
        cast: Vec::new(),
        sites: BTreeMap::new(),
        invalid: Vec::new(),
    };

    // Read each declared export. A relative export path resolves under `dir`.
    for (export, rel) in &manifest.exports {
        if !EXPORT_KINDS.contains(&export.as_str()) {
            errs.push(LoadError::UnknownExport {
                file: manifest_path.display().to_string(),
                at: key_at(&manifest_text, &["exports", export]),
                export: export.clone(),
            });
            continue;
        }
        let path = dir.join(rel);
        if !path.exists() {
            errs.push(LoadError::MissingExportDir {
                export: export.clone(),
                path: path.display().to_string(),
            });
            continue;
        }
        // Where the `n`th entry declaring `id` sits in a file: by its
        // `name:` (or other id field) value in a list, by its key in a map.
        let field = |field: &'static str| {
            move |src: &Source, id: &str, n: usize| field_value_at(src.text, field, id, n)
        };
        let keyed = |section: &'static str| {
            move |src: &Source, id: &str, _: usize| key_offset(src.text, &[section, id])
        };
        let sites = &mut out.sites;
        match export.as_str() {
            "directives" => read_kind::<DirectivesFile, _>(dir, &path, &mut errs, |f, src, e| {
                let (file, text) = (src.file, src.text);
                for (i, d) in f.directives.iter().enumerate() {
                    // dsl 0.26.0 §4: `when=` is the core directive condition on
                    // every plugin passthrough directive; an attribute of that
                    // name could never be written.
                    if d.attrs.iter().any(|a| a.name == "when") {
                        e.push(LoadError::Parse {
                            file: file.display().to_string(),
                            at: field_value_at(text, "name", &d.name, 0)
                                .map(|o| crate::yaml_text::line_col(text, o)),
                            msg: format!(
                                "directive `{}` declares an attribute `when`, which is reserved: \
                                 `when=\"<condition>\"` is the core condition every directive \
                                 takes — rename the attribute",
                                d.name
                            ),
                        });
                    }
                    // dsl 0.27.0 §4: a declared effect that reads an attr the
                    // directive lacks is this file's fault, at its line.
                    for me in crate::validate::validate_effects(d) {
                        e.push(LoadError::Parse {
                            file: file.display().to_string(),
                            at: effect_error_line(text, &d.name, &me.message()),
                            msg: me.message(),
                        });
                    }
                    if let Some(msg) = crate::validate::bridge_without_result_write(d) {
                        // At this declaration's own `name:` when a name repeats.
                        let nth = f.directives[..i]
                            .iter()
                            .filter(|o| o.name == d.name)
                            .count();
                        e.push(LoadError::BridgeNoResultWrite {
                            file: file.display().to_string(),
                            at: field_value_at(text, "name", &d.name, nth)
                                .map(|o| crate::yaml_text::line_col(text, o)),
                            msg,
                        });
                    }
                }
                let (dst, key) = (&mut out.directives, |d: &DirectiveDecl| d.name.clone());
                merge_named(
                    dst,
                    sites,
                    f.directives,
                    "directive",
                    key,
                    field("name"),
                    src,
                    e,
                );
            }),
            "state" => read_kind::<StateFile, _>(dir, &path, &mut errs, |f, src, e| {
                if f.state_shapes.is_none() && f.state_templates.is_none() {
                    e.push(LoadError::Parse {
                        file: src.file.display().to_string(),
                        at: None,
                        msg: "not a state file: declare `stateShapes:` and/or `stateTemplates:`"
                            .into(),
                    });
                }
                let shapes = f.state_shapes.unwrap_or_default();
                let key = |s: &StateShape| s.name.clone();
                merge_named(
                    &mut out.state_shapes,
                    sites,
                    shapes,
                    "shape",
                    key,
                    field("name"),
                    src,
                    e,
                );
                let templates = f.state_templates.unwrap_or_default();
                let key = |t: &StateTemplate| t.name.clone();
                let dst = &mut out.state_templates;
                merge_named(
                    dst,
                    sites,
                    templates,
                    "template",
                    key,
                    field("name"),
                    src,
                    e,
                );
            }),
            "providers" => read_kind::<ProvidersFile, _>(dir, &path, &mut errs, |f, src, e| {
                let key = |p: &ProviderDecl| p.name.clone();
                merge_named(
                    &mut out.providers,
                    sites,
                    f.providers,
                    "provider",
                    key,
                    field("name"),
                    src,
                    e,
                );
            }),
            // One operation of one service; located by its `operation:`.
            "bridge" => read_kind::<BridgeFile, _>(dir, &path, &mut errs, |f, src, e| {
                let key = |b: &BridgeCapability| format!("{}.{}", b.service, b.operation);
                let at = |src: &Source, id: &str, n: usize| {
                    let op = id.rsplit_once('.').map_or(id, |(_, op)| op);
                    field_value_at(src.text, "operation", op, n)
                };
                merge_named(&mut out.bridge, sites, f.bridge, "bridge", key, at, src, e);
            }),
            "defs" => read_kind::<DefsFile, _>(dir, &path, &mut errs, |f, src, e| {
                for d in &f.defs {
                    let at = field_value_at(src.text, "name", &d.name, 0);
                    check_name(crate::ident::ident_fault("def", &d.name, "@"), at, src, e);
                    for param in &d.params {
                        let what = format!("def `{}` param", d.name);
                        let at = field_value_at(src.text, "name", &param.name, 0);
                        check_name(
                            crate::ident::ident_fault(&what, &param.name, ""),
                            at,
                            src,
                            e,
                        );
                    }
                }
                let key = |d: &DefDecl| d.name.clone();
                merge_named(
                    &mut out.defs,
                    sites,
                    f.defs,
                    "def",
                    key,
                    field("name"),
                    src,
                    e,
                );
            }),
            "enums" => read_kind::<EnumsFile, _>(dir, &path, &mut errs, |f, src, e| {
                let mut reported = std::collections::BTreeSet::new();
                for (name, decl) in &f.enums {
                    let at = key_offset(src.text, &["enums", name]);
                    check_name(crate::ident::name_fault("enum", name), at, src, e);
                    for member in decl.members() {
                        if !crate::ident::is_name(member) && reported.insert(member.as_str()) {
                            let at = at.and_then(|from| word_offset(src.text, from, member));
                            check_name(crate::ident::name_fault("enum member", member), at, src, e);
                        }
                    }
                }
                let items = f.enums.into_iter().map(|(k, v)| (k, v.into_domain()));
                merge_keyed(&mut out.enums, sites, items, "enum", keyed("enums"), src, e);
            }),
            "frontmatter" => read_kind::<FrontmatterFile, _>(dir, &path, &mut errs, |f, src, e| {
                let items = f.frontmatter.into_iter().map(|d| (d.key, d.schema));
                merge_keyed(
                    &mut out.frontmatter,
                    sites,
                    items,
                    "frontmatter",
                    field("key"),
                    src,
                    e,
                );
            }),
            "docs" => { /* non-normative (plugin §6.7); skip */ }
            // Anchored at the INDIVIDUAL declaration file `read_kind` just
            // parsed `f` from, not the export dir `path` names — the same
            // anchor `LoadError::Parse` already uses for a sibling failure in
            // the same file (plugin §7 fix, defect 2).
            "assetKinds" => read_kind::<AssetKindsFile, _>(dir, &path, &mut errs, |f, src, e| {
                check_asset_segment_types(&f.asset_kinds, &src.file.display().to_string(), e);
                let key = |k: &AssetKindDecl| k.kind.clone();
                let dst = &mut out.asset_kinds;
                merge_named(
                    dst,
                    sites,
                    f.asset_kinds,
                    "assetKind",
                    key,
                    field("kind"),
                    src,
                    e,
                );
            }),
            "events" => read_kind::<EventsFile, _>(dir, &path, &mut errs, |f, src, e| {
                for ev in &f.events {
                    let at = field_value_at(src.text, "name", &ev.name, 0);
                    check_name(crate::ident::name_fault("event", &ev.name), at, src, e);
                }
                let key = |ev: &EventDecl| ev.name.clone();
                merge_named(
                    &mut out.events,
                    sites,
                    f.events,
                    "event",
                    key,
                    field("name"),
                    src,
                    e,
                );
            }),
            "stampAttrs" => read_kind::<StampAttrsFile, _>(dir, &path, &mut errs, |f, src, e| {
                merge_stamp_attrs(&mut out.stamp_attrs, f.stamp_attrs, src, e);
            }),
            "rewardKinds" => read_kind::<RewardKindsFile, _>(dir, &path, &mut errs, |f, src, e| {
                let decls: Vec<RewardKindDecl> = f
                    .reward_kinds
                    .into_iter()
                    .map(|(name, body)| RewardKindDecl {
                        name,
                        target: body.target,
                        attrs: body.attrs,
                        credits: body.credits,
                    })
                    .collect();
                let key = |r: &RewardKindDecl| r.name.clone();
                let at = keyed("rewardKinds");
                merge_named(
                    &mut out.reward_kinds,
                    sites,
                    decls,
                    "rewardKind",
                    key,
                    at,
                    src,
                    e,
                );
            }),
            "occasions" => read_kind::<OccasionsFile, _>(dir, &path, &mut errs, |f, src, e| {
                for name in f.occasions.keys() {
                    let at = key_offset(src.text, &["occasions", name]);
                    check_name(crate::ident::name_fault("occasion", name), at, src, e);
                }
                check_occasion_members(&f.occasions, src.file, src.text, e);
                check_gate_types(src.text, src.file, e);
                let decls: Vec<OccasionDecl> = f
                    .occasions
                    .into_iter()
                    .map(|(name, body)| OccasionDecl {
                        name,
                        select: body.select,
                        target: body.target,
                        description: body.description,
                        judge: body.judge,
                        raised_when: body.raised_when,
                        payload: body.payload,
                        outside_run: body.outside_run,
                    })
                    .collect();
                let key = |o: &OccasionDecl| o.name.clone();
                let at = keyed("occasions");
                merge_named(
                    &mut out.occasions,
                    sites,
                    decls,
                    "occasion",
                    key,
                    at,
                    src,
                    e,
                );
            }),
            "lints" => read_kind::<LintsFile, _>(dir, &path, &mut errs, |f, src, e| {
                let key = |r: &crate::lint::LintRuleDecl| r.id.clone();
                merge_named(
                    &mut out.lints,
                    sites,
                    f.lints,
                    "lint",
                    key,
                    field("id"),
                    src,
                    e,
                );
            }),
            "cast" => read_kind::<CastFile, _>(dir, &path, &mut errs, |f, src, e| {
                let decls: Vec<CastMember> = f
                    .cast
                    .into_iter()
                    .map(|(id, body)| body.into_member(id))
                    .collect();
                let key = |c: &CastMember| c.id.clone();
                merge_named(
                    &mut out.cast,
                    sites,
                    decls,
                    "cast",
                    key,
                    keyed("cast"),
                    src,
                    e,
                );
            }),
            _ => unreachable!("every EXPORT_KINDS entry has an arm"),
        }
    }

    drop_invalid(dir, &mut out, &errs);
    if errs.iter().all(|e| {
        matches!(
            e,
            LoadError::DuplicateId { .. }
                | LoadError::TypeNumber { .. }
                | LoadError::BridgeNoResultWrite { .. }
        )
    }) {
        Ok((out, errs))
    } else {
        Err((Some(out.manifest.id), errs))
    }
}

/// 0.41.0 §3.2: move each export entry a `type: number` sits in — and each
/// entry of this package that references one (a directive calling an
/// invalid bridge or declaring an invalid shape, a template of an invalid
/// shape) — out of the loaded lists into [`LoadedPlugin::invalid`], so the
/// package's valid entries still assemble.
fn drop_invalid(dir: &Path, out: &mut LoadedPlugin, errs: &[LoadError]) {
    use crate::snapshot::InvalidDecl;
    let base = dir.parent().unwrap_or(dir);
    for e in errs {
        let LoadError::TypeNumber {
            entry: Some((kind, name)),
            ..
        } = e
        else {
            continue;
        };
        if out.invalid.iter().any(|((k, n), _)| k == kind && n == name) {
            continue;
        }
        out.invalid.push((
            (kind, name.clone()),
            InvalidDecl {
                cause: name.clone(),
                code: crate::types::E_TYPE_NUMBER.to_string(),
                site: e.site_in(base),
                seen: Vec::new(),
            },
        ));
    }
    if out.invalid.is_empty() {
        return;
    }
    let find = |invalid: &[((&'static str, String), InvalidDecl)], kind: &str, name: &str| {
        invalid
            .iter()
            .find(|((k, n), _)| *k == kind && n == name)
            .map(|(_, d)| d.clone())
    };
    let mut dependants = Vec::new();
    for d in &out.directives {
        if find(&out.invalid, "directive", &d.name).is_some() {
            continue;
        }
        let bridge = d.bridge.as_ref().and_then(|b| {
            find(
                &out.invalid,
                "bridge",
                &format!("{}.{}", b.service, b.operation),
            )
        });
        let shape = || {
            d.state
                .iter()
                .flat_map(|s| &s.declares)
                .find_map(|slot| find(&out.invalid, "shape", &slot.shape))
        };
        if let Some(cause) = bridge.or_else(shape) {
            dependants.push((("directive", d.name.clone()), cause));
        }
    }
    for t in &out.state_templates {
        if let Some(cause) = find(&out.invalid, "shape", &t.shape) {
            dependants.push((("template", t.name.clone()), cause));
        }
    }
    out.invalid.extend(dependants);
    let invalid = &out.invalid;
    let gone = |kind: &str, name: &str| invalid.iter().any(|((k, n), _)| *k == kind && n == name);
    out.directives.retain(|d| !gone("directive", &d.name));
    out.state_shapes.retain(|s| !gone("shape", &s.name));
    out.state_templates.retain(|t| !gone("template", &t.name));
    out.bridge
        .retain(|b| !gone("bridge", &format!("{}.{}", b.service, b.operation)));
    out.defs.retain(|d| !gone("def", &d.name));
    out.occasions.retain(|o| !gone("occasion", &o.name));
    // One name may have several (disjoint) stamp entries: each is dropped by
    // its own identity, so a sibling entry of the name still loads.
    out.stamp_attrs
        .retain(|a| !gone("stampAttr", &a.identity()));
    out.reward_kinds.retain(|r| !gone("rewardKind", &r.name));
    out.asset_kinds.retain(|a| !gone("assetKind", &a.kind));
    out.frontmatter.retain(|key, _| !gone("frontmatter", key));
}

/// 0.41.0 §3.2: report every `type: number` of the export file `text` as
/// [`LoadError::TypeNumber`] and return the text with each one read as
/// `double` (same length, so every other error keeps its position), so the
/// file's other entries still load.
fn patch_numbers(file: &Path, text: String, errs: &mut Vec<LoadError>) -> String {
    let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(&text) else {
        return text;
    };
    let sites = crate::types::number_type_sites(&value);
    if sites.is_empty() {
        return text;
    }
    let mut patched = text.clone();
    for site in &sites {
        let range = site.range_in(&text);
        if let Some(r) = &range {
            patched.replace_range(r.clone(), "double");
        }
        errs.push(LoadError::TypeNumber {
            file: file.display().to_string(),
            at: range.map(|r| crate::yaml_text::line_col(&text, r.start)),
            msg: crate::types::number_type_message(site.default.as_slice()),
            entry: export_entry(&value, &site.path),
        });
    }
    patched
}

/// The export entry `path` lies in, as the `(kind, name)` the package's
/// [`Sites`] use: `directives[0]` → `("directive", <its name>)`,
/// `occasions.dive` → `("occasion", "dive")`.
fn export_entry(
    value: &yaml_serde::Value,
    path: &[crate::types::YamlKey],
) -> Option<(&'static str, String)> {
    use crate::types::YamlKey;
    let [YamlKey::Key(section), entry, ..] = path else {
        return None;
    };
    let kind = match section.as_str() {
        "directives" => "directive",
        "stateShapes" => "shape",
        "stateTemplates" => "template",
        "bridgeCapabilities" => "bridge",
        "defs" => "def",
        "occasions" => "occasion",
        "stampAttrs" => "stampAttr",
        "rewardKinds" => "rewardKind",
        "assetKinds" => "assetKind",
        "frontmatter" => "frontmatter",
        _ => return None,
    };
    let name = match entry {
        YamlKey::Key(name) => name.clone(),
        YamlKey::Item(i) => {
            let e = value.get(section.as_str())?.get(*i)?;
            let field = |f: &str| e.get(f).and_then(yaml_serde::Value::as_str);
            match kind {
                "bridge" => format!("{}.{}", field("service")?, field("operation")?),
                "assetKind" => field("kind")?.to_string(),
                "frontmatter" => field("key")?.to_string(),
                // A stamp entry by name and `on:` names, as
                // `DeclaredStampAttr::identity` spells it.
                "stampAttr" => {
                    let on = e
                        .get("on")
                        .and_then(yaml_serde::Value::as_sequence)
                        .map(|items| {
                            items
                                .iter()
                                .filter_map(yaml_serde::Value::as_str)
                                .map(str::to_string)
                                .collect::<BTreeSet<String>>()
                        });
                    crate::stamps::entry_identity(field("name")?, on.as_ref())
                }
                _ => field("name")?.to_string(),
            }
        }
    };
    Some((kind, name))
}

/// The `Type` variants a segment position actually enforces
/// (`crate::asset::validate_segments`'s non-catch-all arms): `enum`
/// (membership), `int` (parses as integer), `string` (accepts anything),
/// and `providerRef` (resolved against a provider snapshot — plugin-system
/// 0.0.1 §7 names this the asset-kind-segment case explicitly, alongside
/// attribute and frontmatter positions). Nothing else is admitted; see
/// [`check_asset_segment_types`] for why each remaining `Type` variant is
/// excluded.
fn segment_type_admitted(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Enum(_) | Type::Int | Type::Str | Type::ProviderRef(_)
    )
}

/// Reject every asset-kind segment in `kinds` whose declared `Type` is not
/// [`segment_type_admitted`], pushing one [`LoadError::AssetSegmentType`] per
/// offending segment, anchored at `file` (the INDIVIDUAL `assetkinds/*.yaml`
/// declaration file `kinds` was deserialized from — matching how
/// [`LoadError::Parse`] anchors at the file it failed to parse; spanned YAML
/// is not in scope here, same as there). Runs before [`merge_named`] folds
/// `kinds` into the loaded package, so a rejected declaration still contributes its `errs` entry
/// (which fails the whole package, `load_plugin_dir`'s `Err` path) but never
/// reaches assembly or a document check.
///
/// `AssetSegment.ty` (`crate::schema::AssetSegment`) is the SAME shared
/// `Type` enum every typed position uses, so EVERY variant parses
/// syntactically here; the plugin-system closed `Type ::=` production
/// (0.0.1 §7, `docs/proposals/plugin-system/0.0.1.md:379-390`) plus its own
/// per-variant notes settle which ones are semantically legitimate in a
/// segment position:
///
/// - `domain`: absent from the closed production entirely. 0.0.3 §2 later
///   admits `{ domain: … }` only for directive attributes and content-line
///   slots (dsl 0.9.0 §2) — never for a segment. This is the reported
///   defect: `AssetSegment.ty` accepted it anyway, purely because it shares
///   the general `Type` enum, and `validate_segments`'s catch-all arm then
///   enforced nothing against it.
/// - `enumFromOption`, `slotId`: the production itself scopes both to
///   "attribute types only" (§7 lines 385/387) — never a segment.
/// - `narrativeTime`: also absent from the closed production. `types.rs`
///   documents it as opaque and "NEVER author-declarable" — no `Literal`,
///   and by the same reasoning no segment string, ever inhabits it.
/// - `list`, `record`, `map`: admitted by the production for SOME typed
///   position, but structurally incompatible with a segment specifically —
///   §6.9 defines a segment's value as the single string token produced by
///   splitting the composed id on `sep` (`crate::asset::decompose`); there is
///   no serialization of a compound/multi-field value into one token.
/// - `assetKind`: the production's own elaboration (§7 lines 398-401)
///   describes it as validating a value AS a complete authored id, itself
///   decomposed against ANOTHER kind's segment grammar — the inverse of
///   being one token WITHIN an id. Nesting a whole nested-kind decomposition
///   inside a single split token has no defined meaning.
/// - `bool`: a single token (`"true"`/`"false"`) like `number`, so not
///   excluded by shape alone — but §6.9's own segment example
///   (0.0.1.md:277-280, `const`/`providerRef`/`string` only) and the four
///   types `validate_segments` actually enforces never extend to it.
///   Admitting it here would recreate the exact "declared and enforces
///   nothing" shape as `domain`, just for a different variant — the second
///   hole this function exists to close.
fn check_asset_segment_types(kinds: &[AssetKindDecl], file: &str, errs: &mut Vec<LoadError>) {
    for kind in kinds {
        for seg in &kind.segments {
            if let Some(ty) = &seg.ty {
                if !segment_type_admitted(ty) {
                    errs.push(LoadError::AssetSegmentType {
                        file: file.to_string(),
                        kind: kind.kind.clone(),
                        segment: seg.name.clone(),
                        found: type_str(ty),
                    });
                }
            }
        }
    }
}

/// A name a plugin declares — an occasion, an event, an enum and its
/// members, a def — follows the one name rule, and a def's param is an
/// identifier its body reads bare: `fault` (the slot's verdict) is refused
/// at `offset` (its declaration in `src`).
fn check_name(
    fault: Option<String>,
    offset: Option<usize>,
    src: &Source,
    errs: &mut Vec<LoadError>,
) {
    if let Some(msg) = fault {
        errs.push(LoadError::Parse {
            file: src.file.display().to_string(),
            at: offset.map(|o| crate::yaml_text::line_col(src.text, o)),
            msg,
        });
    }
}

/// Byte offset of the first whole-word occurrence of `word` in `text` at or
/// after `from` (a word ends at anything but a letter, digit, `_`, `-`).
fn word_offset(text: &str, from: usize, word: &str) -> Option<usize> {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let mut at = from;
    while let Some(i) = text.get(at..)?.find(word) {
        let start = at + i;
        let end = start + word.len();
        let before = text[..start].chars().next_back().is_some_and(is_word);
        let after = text[end..].chars().next().is_some_and(is_word);
        if !before && !after {
            return Some(start);
        }
        at = end;
    }
    None
}

/// An occasion target domain's `members:` list (the member subset) must name
/// at least one member, each once. Whether each listed member belongs to the
/// domain's entity kind is the checker's to judge: entity kinds are project
/// vocabulary (`entities:`), unknown to a plugin package. Reported as a
/// [`LoadError::Parse`] at the list in `file` (whose text is `text`).
fn check_occasion_members(
    occasions: &BTreeMap<String, OccasionBody>,
    file: &Path,
    text: &str,
    errs: &mut Vec<LoadError>,
) {
    for (name, body) in occasions {
        let OccasionTarget::Domain {
            entity,
            members: Some(members),
            ..
        } = &body.target
        else {
            continue;
        };
        let msg = if members.is_empty() {
            format!(
                "occasion `{name}`'s `target.members` is empty; list at least one member of \
                 entity kind `{entity}`, or drop `members` to draw targets from the whole kind"
            )
        } else if let Some(dup) = members
            .iter()
            .enumerate()
            .find_map(|(i, m)| members[..i].contains(m).then_some(m))
        {
            format!("occasion `{name}`'s `target.members` lists `{dup}` more than once")
        } else {
            continue;
        };
        errs.push(LoadError::Parse {
            file: file.display().to_string(),
            at: key_at(text, &["occasions", name, "target", "members"]),
            msg,
        });
    }
}

/// dsl 0.27.0 §4: an occasion's `raisedWhen:` is a condition string. A YAML
/// scalar that is not a string (`raisedWhen: true`, a number) would be read
/// as its text; it is refused like a non-string `terminal:`.
fn check_gate_types(text: &str, file: &Path, errs: &mut Vec<LoadError>) {
    let Ok(doc) = yaml_serde::from_str::<yaml_serde::Value>(text) else {
        return;
    };
    let Some(occasions) = doc.get("occasions").and_then(|o| o.as_mapping()) else {
        return;
    };
    for (name, body) in occasions {
        let Some(gate) = body.get("raisedWhen") else {
            continue;
        };
        if gate.is_string() || gate.is_null() {
            continue;
        }
        let name = name.as_str().unwrap_or_default();
        let shown = yaml_serde::to_string(gate).unwrap_or_default();
        let shown = shown.trim();
        errs.push(LoadError::Parse {
            file: file.display().to_string(),
            at: key_at(text, &["occasions", name, "raisedWhen"]),
            msg: format!(
                "occasion `{name}`'s `raisedWhen: {shown}` is not a condition string — quote it \
                 (`raisedWhen: \"{shown}\"`), or drop `raisedWhen` for an occasion the engine \
                 may always raise"
            ),
        });
    }
}

/// Byte offset of the mapping key at `path` in YAML `text`.
fn key_offset(text: &str, path: &[&str]) -> Option<usize> {
    crate::yaml_text::key_span(text, path).map(|r| r.start)
}

/// (line, column) of the mapping key at `path` in YAML `text`.
fn key_at(text: &str, path: &[&str]) -> At {
    key_offset(text, path).map(|o| crate::yaml_text::line_col(text, o))
}

/// Byte offset of the value `value` of the `n`th (0-based) `field:` key
/// holding it in `text` — a list entry `- name: use` or a flow entry
/// `{ name: use, … }`, quoted or not.
fn field_value_at(text: &str, field: &str, value: &str, n: usize) -> Option<usize> {
    let key = format!("{field}:");
    let mut from = 0;
    let mut seen = 0;
    while let Some(i) = text[from..].find(&key) {
        let at = from + i;
        from = at + key.len();
        if text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        let rest = &text[from..];
        let pad = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let v = &rest[pad..];
        let quote = v.chars().next().filter(|c| *c == '"' || *c == '\'');
        let body = if quote.is_some() { &v[1..] } else { v };
        let Some(after) = body.strip_prefix(value) else {
            continue;
        };
        let closed = match quote {
            Some(q) => after.starts_with(q),
            None => {
                after.is_empty() || after.starts_with([',', '}', ']', ' ', '\t', '\r', '\n', '#'])
            }
        };
        if closed {
            if seen == n {
                return Some(from + pad + usize::from(quote.is_some()));
            }
            seen += 1;
        }
    }
    None
}

/// `(line, column)` (1-based) of what an effect error names — its first
/// backticked token after the directive's name, e.g. `holding(@itm)` —
/// inside directive `directive`'s entry of `text`; `None` when not found.
fn effect_error_line(text: &str, directive: &str, message: &str) -> Option<(usize, usize)> {
    let entry = text.lines().scan(0usize, |off, line| {
        let at = *off;
        *off += line.len() + 1;
        Some((at, line))
    });
    let from = entry
        .filter(|(_, l)| {
            let t = l.trim_start().trim_start_matches("- ").trim_start();
            t.strip_prefix("name:")
                .is_some_and(|r| r.trim().trim_matches(['"', '\'']) == directive)
        })
        .map(|(at, _)| at)
        .next()?;
    // Skip the leading `directive `::give` effects.asserts:` tokens.
    let detail = message.split_once(": ").map_or(message, |(_, d)| d);
    let needle = detail.split('`').nth(1).filter(|n| !n.is_empty())?;
    let at = from + text[from..].find(needle)?;
    Some(crate::yaml_text::line_col(text, at))
}

/// Scan `dir` for plugin packages (each immediate subdirectory containing a
/// `plugin.yaml`), in sorted order, and index by manifest id. A duplicate id
/// across packages is a `LoadError::DuplicateId { kind: "plugin", .. }` (the
/// later package is dropped); a duplicate name within a package is reported
/// and the package still installed, its first declaration kept. A package
/// whose manifest parsed but whose exports failed lands in
/// [`crate::resolve::InstalledPlugins::failed`], so assembly can say it
/// failed to load rather than that it is not installed. A missing `dir`
/// yields an empty registry.
pub fn load_plugins_dir(dir: &Path) -> (crate::resolve::InstalledPlugins, Vec<LoadError>) {
    use crate::resolve::{InstalledPlugin, InstalledPlugins};
    let mut reg = InstalledPlugins::default();
    let mut errs = Vec::new();
    let mut subs: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => return (reg, errs),
    };
    subs.sort();
    // The manifest each installed id came from, for a later duplicate.
    let mut homes: BTreeMap<String, std::path::PathBuf> = BTreeMap::new();
    for sub in subs {
        if !sub.join("plugin.yaml").is_file() {
            continue;
        }
        match load_package(&sub) {
            Ok((loaded, mut dups)) => {
                errs.append(&mut dups);
                let id = loaded.manifest.id.clone();
                let manifest = sub.join("plugin.yaml");
                let id_at = |file: &Path| {
                    let text = std::fs::read_to_string(file).unwrap_or_default();
                    key_at(&text, &["id"])
                };
                match reg.by_id.entry(id) {
                    std::collections::btree_map::Entry::Occupied(e) => {
                        let first = homes.get(e.key()).map(|home| {
                            let line = id_at(home).map_or(String::new(), |(l, _)| format!(":{l}"));
                            let rel = home.strip_prefix(dir).unwrap_or(home);
                            format!("{}{line}", rel.display())
                        });
                        errs.push(LoadError::DuplicateId {
                            kind: "plugin".into(),
                            id: e.key().clone(),
                            at: id_at(&manifest),
                            file: manifest.display().to_string(),
                            first,
                            shared: Vec::new(),
                        });
                    }
                    std::collections::btree_map::Entry::Vacant(e) => {
                        homes.insert(e.key().clone(), manifest);
                        e.insert(InstalledPlugin { loaded });
                    }
                }
            }
            Err((id, mut e)) => {
                if let Some(id) = id {
                    let codes = reg.failed.entry(id.clone()).or_default();
                    for err in &e {
                        if !codes.contains(&err.code()) {
                            codes.push(err.code());
                        }
                    }
                    if let Some(first) = e.first() {
                        reg.failed_causes.insert(
                            id.clone(),
                            crate::snapshot::InvalidDecl {
                                cause: id,
                                code: first.code().to_string(),
                                site: first.site_in(dir),
                                seen: Vec::new(),
                            },
                        );
                    }
                }
                errs.append(&mut e);
            }
        }
    }
    (reg, errs)
}

/// `file:line` (or `file`) with `file` relative to `base`, `/`-separated.
fn rel_site(base: &Path, file: &str, at: At) -> String {
    let rel = Path::new(file).strip_prefix(base).map_or_else(
        |_| file.to_string(),
        |rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        },
    );
    match at {
        Some((line, _)) => format!("{rel}:{line}"),
        None => rel,
    }
}

impl LoadError {
    /// Where this error sits, as [`rel_site`] relative to `base`.
    pub fn site_in(&self, base: &Path) -> Option<String> {
        match self {
            LoadError::Manifest { file, at, .. }
            | LoadError::Parse { file, at, .. }
            | LoadError::Key { file, at, .. }
            | LoadError::TypeNumber { file, at, .. }
            | LoadError::BridgeNoResultWrite { file, at, .. }
            | LoadError::DuplicateId { file, at, .. }
            | LoadError::UnknownExport { file, at, .. } => Some(rel_site(base, file, *at)),
            LoadError::MissingExportDir { path, .. } | LoadError::Io { path, .. } => {
                Some(rel_site(base, path, None))
            }
            LoadError::AssetSegmentType { file, .. } => Some(rel_site(base, file, None)),
        }
    }
}

/// Read `plugin.yaml` at `path`: every key it holds is one it takes, its
/// `kind:` is `capability`, and each `depends:`/`options:` entry holds only
/// its own keys. Returns the manifest and its text; a failure carries the
/// manifest id when the file names one.
fn read_manifest(
    path: &Path,
) -> Result<(PluginManifest, String), (Option<String>, Vec<LoadError>)> {
    use yaml_serde::Value;
    let file = path.display().to_string();
    let fail = |at: At, msg: String| {
        vec![LoadError::Manifest {
            file: file.clone(),
            at,
            msg,
        }]
    };
    let text = std::fs::read_to_string(path).map_err(|e| {
        (
            None,
            fail(None, format!("`plugin.yaml` could not be read: {e}")),
        )
    })?;
    let doc: Value = yaml_serde::from_str(&text).map_err(|e| {
        let fault = crate::yaml_text::yaml_fault(&text, &e);
        let at = crate::yaml_text::line_col(&text, fault.offset);
        (None, fail(Some(at), fault.message))
    })?;
    let shape = "`plugin.yaml` declares `id: <plugin id>`, `version: <version>`, \
                 `kind: capability` and `exports: { <kind>: <path> }`";
    let Some(map) = doc.as_mapping() else {
        return Err((None, fail(None, shape.to_string())));
    };
    let id = map.get("id").and_then(Value::as_str).map(str::to_string);
    let shown = |v: &Value| {
        yaml_serde::to_string(v)
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let mut errs = Vec::new();
    let mut key_error = |at: At, msg: String| {
        errs.push(LoadError::Key {
            file: file.clone(),
            at,
            msg,
        });
    };
    for k in map.keys() {
        let key = shown(k);
        if !MANIFEST_KEYS.contains(&key.as_str()) {
            key_error(
                key_at(&text, &[&key]),
                format!(
                    "`plugin.yaml` has no key `{key}`{} (its keys: {})",
                    crate::suggest::did_you_mean(&key, MANIFEST_KEYS.iter().copied()),
                    MANIFEST_KEYS.join(", ")
                ),
            );
        }
    }
    if let Some(kind) = map.get("kind").filter(|k| k.as_str() != Some("capability")) {
        key_error(
            key_at(&text, &["kind"]),
            format!(
                "`kind: {}` is not a plugin kind — a plugin package declares `kind: capability`",
                shown(kind)
            ),
        );
    }
    for (list, keys, entry) in [
        (
            "depends",
            &["id", "range"][..],
            "`{ id: <plugin id>, range: <version range> }`",
        ),
        (
            "options",
            &["name", "type", "default"][..],
            "`{ name: <option>, type: <type>, default: <value> }`",
        ),
    ] {
        let items = map.get(list).and_then(Value::as_sequence);
        for item in items.into_iter().flatten().filter_map(Value::as_mapping) {
            for k in item.keys() {
                let key = shown(k);
                if !keys.contains(&key.as_str()) {
                    key_error(
                        key_at(&text, &[list, &key]),
                        format!(
                            "a `{list}:` entry has no key `{key}`{} — an entry is {entry}",
                            crate::suggest::did_you_mean(&key, keys.iter().copied())
                        ),
                    );
                }
            }
        }
    }
    if !errs.is_empty() {
        return Err((id, errs));
    }
    for (key, example) in [
        ("id", "id: <plugin id>"),
        ("version", "version: 0.1.0"),
        ("kind", "kind: capability"),
        ("exports", "exports: { directives: directives/ }"),
    ] {
        if map.get(key).is_none() {
            errs.extend(fail(
                None,
                format!("`plugin.yaml` names no `{key}:` — add `{example}`"),
            ));
        }
    }
    if !errs.is_empty() {
        return Err((id, errs));
    }
    // 0.41.0 §3.2: an option typed `number` is this manifest's own error,
    // and a package whose `plugin.yaml` fails does not load.
    let numbers = crate::types::number_type_sites(&doc);
    if !numbers.is_empty() {
        let errs = numbers
            .iter()
            .map(|site| LoadError::TypeNumber {
                file: file.clone(),
                at: site
                    .range_in(&text)
                    .map(|r| crate::yaml_text::line_col(&text, r.start)),
                msg: crate::types::number_type_message(site.default.as_slice()),
                entry: None,
            })
            .collect();
        return Err((id, errs));
    }
    match yaml_serde::from_str::<PluginManifest>(&text) {
        Ok(m) => Ok((m, text)),
        Err(e) => {
            let (at, msg) = data_error(&text, &e);
            Err((id, fail(at, format!("`plugin.yaml`: {msg}"))))
        }
    }
}

/// Read a single YAML file OR every `*.yaml`/`*.yml` in a dir (sorted byte-wise),
/// deserialize each to `F`, and hand it — with its [`Source`] in package `pkg` —
/// to `merge`.
fn read_kind<F, M>(pkg: &Path, path: &Path, errs: &mut Vec<LoadError>, mut merge: M)
where
    F: serde::de::DeserializeOwned,
    M: FnMut(F, &Source, &mut Vec<LoadError>),
{
    for file in yaml_files(path, errs) {
        let s = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                errs.push(LoadError::Io {
                    path: file.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        let s = patch_numbers(&file, s, errs);
        match yaml_serde::from_str::<F>(&s) {
            Ok(f) => {
                let src = Source {
                    pkg,
                    file: &file,
                    text: &s,
                };
                merge(f, &src, errs);
            }
            Err(e) => errs.push(export_error(&file.display().to_string(), &s, &e)),
        }
    }
}

/// The load error for an export file `src` that failed to deserialize: a
/// YAML syntax fault in plain words ([`crate::yaml_text::yaml_fault`]); an
/// unknown key as [`LoadError::Key`] with a did-you-mean and — when the
/// offending mapping holds a key with no value — the hint that an unquoted
/// flow-map value ended at a comma (`{ description: Pick one, the player
/// picks one }` parses as `description: Pick one` plus a null-valued key `the
/// player picks one`); anything else as [`LoadError::Parse`], reworded by
/// [`data_error`].
fn export_error(file: &str, src: &str, e: &yaml_serde::Error) -> LoadError {
    let file = file.to_string();
    let doc = match yaml_serde::from_str::<yaml_serde::Value>(src) {
        Ok(doc) => doc,
        Err(syntax) => {
            let fault = crate::yaml_text::yaml_fault(src, &syntax);
            return LoadError::Parse {
                file,
                at: Some(crate::yaml_text::line_col(src, fault.offset)),
                msg: fault.message,
            };
        }
    };
    let at = e
        .location()
        .map(|l| crate::yaml_text::line_col(src, l.index()));
    let (path, detail) = serde_parts(e);
    if let Some((key, expected)) = unknown_field(&detail) {
        let owner = match &path {
            Some(p) => format!("`{p}`"),
            None => "this file".to_string(),
        };
        let mut msg = format!(
            "{owner} has no {}key `{key}`{}",
            if path.is_none() { "top-level " } else { "" },
            crate::suggest::did_you_mean(&key, expected.iter().map(String::as_str))
        );
        if !expected.is_empty() {
            msg.push_str(&format!(" (its keys: {})", expected.join(", ")));
        }
        if let Some(hint) = null_key_hint(&doc, &key, &expected) {
            msg.push_str("; ");
            msg.push_str(&hint);
        }
        return LoadError::Key { file, at, msg };
    }
    LoadError::Parse {
        file,
        at,
        msg: plain_data_error(path.as_deref(), &detail),
    }
}

/// A `yaml_serde` data error (the text parsed as YAML) of `src`, reworded:
/// its place and [`plain_data_error`]'s sentence.
fn data_error(src: &str, e: &yaml_serde::Error) -> (At, String) {
    let at = e
        .location()
        .map(|l| crate::yaml_text::line_col(src, l.index()));
    let (path, detail) = serde_parts(e);
    (at, plain_data_error(path.as_deref(), &detail))
}

/// yaml_serde's error text split into the key path it names (`occasions.talk`,
/// `directives[0]`; `None` at the top level) and the problem, without the
/// ` at line N column M` mark (the location carries it).
fn serde_parts(e: &yaml_serde::Error) -> (Option<String>, String) {
    let mut text = e.to_string();
    if let Some(l) = e.location() {
        let mark = format!(" at line {} column {}", l.line(), l.column());
        if let Some(i) = text.rfind(&mark) {
            text.replace_range(i..i + mark.len(), "");
        }
    }
    match text.split_once(": ") {
        Some((path, rest)) if !path.is_empty() && !path.contains(' ') => {
            (Some(path.to_string()), rest.to_string())
        }
        _ => (None, text),
    }
}

/// serde's data-error sentence in an author's words: no Rust type names
/// (`untagged enum Literal`, `struct AttrDecl`), YAML's own vocabulary (a
/// list, a mapping), and — for a top-level key of the wrong shape — what
/// that key holds. Messages Lute itself raised while parsing (a write
/// value's shape, a `target:`) pass through after the path.
fn plain_data_error(path: Option<&str>, detail: &str) -> String {
    let at = |s: String| match path {
        Some(p) => format!("`{p}` {s}"),
        None => format!("this file {s}"),
    };
    let example = path
        .and_then(top_level_shape)
        .map_or(String::new(), |ex| format!(" — {ex}"));
    if let Some(rest) = detail.strip_prefix("invalid type: ") {
        let (found, expected) = rest
            .split_once(", expected ")
            .unwrap_or((rest, "another shape"));
        return at(format!(
            "is {}, where {} is expected{example}",
            plain_found(found),
            plain_expected(expected)
        ));
    }
    if let Some(rest) = detail.strip_prefix("invalid value: ") {
        let (found, expected) = rest
            .split_once(", expected ")
            .unwrap_or((rest, "another value"));
        return at(format!(
            "is {}, where {} is expected",
            plain_found(found),
            plain_expected(expected)
        ));
    }
    if let Some(rest) = detail.strip_prefix("unknown variant `") {
        if let Some((value, tail)) = rest.split_once('`') {
            let legal: Vec<&str> = tail.split('`').skip(1).step_by(2).collect();
            return at(format!(
                "is `{value}`, which is not one of {}{}",
                legal.join(", "),
                crate::suggest::did_you_mean(value, legal.iter().copied())
            ));
        }
    }
    if let Some(field) = detail
        .strip_prefix("missing field `")
        .and_then(|r| r.strip_suffix('`'))
    {
        return at(format!("needs `{field}:`"));
    }
    if let Some(field) = detail
        .strip_prefix("duplicate field `")
        .and_then(|r| r.strip_suffix('`'))
    {
        return at(format!("writes `{field}:` twice — keep one"));
    }
    if let Some(name) = detail.strip_prefix("data did not match any variant of untagged enum ") {
        let shapes = match name {
            "Literal" => "a value: `true`/`false`, a number, text, a list or a mapping",
            "PathSegment" => "a path segment: a name, or `{ fromAttr: { name: <attr> } }`",
            _ => "one of the shapes this key takes",
        };
        return at(format!("is not {shapes}"));
    }
    match path {
        Some(p) => format!("{p}: {detail}"),
        None => detail.to_string(),
    }
}

/// What a top-level key of an export file holds, for a wrong-shape error.
fn top_level_shape(key: &str) -> Option<&'static str> {
    Some(match key {
        "occasions" => "`occasions:` maps each occasion name to its declaration: `occasions: { talk: { select: first } }`",
        "rewardKinds" => "`rewardKinds:` maps each kind to its declaration: `rewardKinds: { GOLD: {} }`",
        "cast" => "`cast:` maps each speaker id to its declaration: `cast: { mira: { name: Mira } }`",
        "enums" => "`enums:` maps each enum name to its members: `enums: { mood: [calm, tense] }`",
        "events" => "`events:` is a list: `events: [ { name: combatEnd } ]`",
        "directives" => "`directives:` is a list: `directives: [ { name: give, attrs: [ { name: item, type: string } ] } ]`",
        "stampAttrs" => "`stampAttrs:` is a list: `stampAttrs: [ { name: bonusId, type: string } ]`",
        "assetKinds" => "`assetKinds:` is a list of asset-kind declarations",
        "providers" | "bridge" | "defs" | "frontmatter" | "lints" | "stateShapes" | "stateTemplates" => {
            "this key holds a list of declarations"
        }
        _ => return None,
    })
}

/// serde's "found" half (`sequence`, `string "x"`, `unit value`) in YAML words.
fn plain_found(found: &str) -> String {
    match found {
        "sequence" => "a list".into(),
        "map" => "a mapping".into(),
        "unit value" => "empty".into(),
        _ => {
            for (prefix, word) in [
                ("string ", "the text"),
                ("boolean ", ""),
                ("integer ", "the number"),
                ("floating point ", "the number"),
            ] {
                if let Some(v) = found.strip_prefix(prefix) {
                    let v = v.trim_matches(['"', '`']);
                    return if word.is_empty() {
                        format!("`{v}`")
                    } else {
                        format!("{word} `{v}`")
                    };
                }
            }
            found.to_string()
        }
    }
}

/// serde's "expected" half (`a sequence`, `struct AttrDecl`, `u32`) in YAML
/// words — never a Rust type name.
fn plain_expected(expected: &str) -> String {
    match expected {
        "a sequence" => "a list".into(),
        "a map" | "a mapping" => "a mapping".into(),
        "a boolean" => "`true` or `false`".into(),
        "a string" => "text".into(),
        e if e.starts_with("struct ") || e.starts_with("a map ") => "a mapping".into(),
        e if e.starts_with("enum ") => "one of its names".into(),
        e if matches!(
            e,
            "f64" | "f32" | "u8" | "u16" | "u32" | "u64" | "usize" | "i32" | "i64"
        ) || e.starts_with("a number") =>
        {
            "a number".into()
        }
        e => e.to_string(),
    }
}

/// Split serde's "unknown field `k`, expected one of `a`, `b`" (or "expected
/// `a`", or "there are no fields") into the key and the accepted names.
fn unknown_field(msg: &str) -> Option<(String, Vec<String>)> {
    let rest = msg.strip_prefix("unknown field `")?;
    let end = rest.find('`')?;
    let key = rest[..end].to_string();
    let expected = rest[end + 1..]
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    Some((key, expected))
}

/// The first mapping in `doc` holding `key`; if it also holds keys with no
/// value that are not accepted fields, the "quote it" hint naming them.
fn null_key_hint(doc: &yaml_serde::Value, key: &str, expected: &[String]) -> Option<String> {
    fn find<'a>(v: &'a yaml_serde::Value, key: &str) -> Option<&'a yaml_serde::Mapping> {
        match v {
            yaml_serde::Value::Mapping(m) => {
                if m.contains_key(key) {
                    return Some(m);
                }
                m.values().find_map(|v| find(v, key))
            }
            yaml_serde::Value::Sequence(s) => s.iter().find_map(|v| find(v, key)),
            _ => None,
        }
    }
    let map = find(doc, key)?;
    let stray: Vec<&str> = map
        .iter()
        .filter(|(_, v)| v.is_null())
        .filter_map(|(k, _)| k.as_str())
        .filter(|k| !expected.iter().any(|e| e == k))
        .collect();
    if stray.is_empty() {
        return None;
    }
    let named = stray
        .iter()
        .map(|k| format!("`{k}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let fix = match map.get("description").and_then(|d| d.as_str()) {
        Some(desc) => format!(
            "quote the description: `description: \"{desc}, {}\"`",
            stray.join(", ")
        ),
        None => "quote the value that contains the comma".to_string(),
    };
    Some(format!(
        "{named} has no value — in a flow map `{{ … }}` an unquoted value ends at the \
         first comma, so the rest became a key; {fix}"
    ))
}

/// Every `*.yaml`/`*.yml` under `path` (a dir), sorted byte-wise; or `[path]`
/// itself if `path` is a file (plugin §4 sort determinism). A `read_dir` failure
/// or any per-entry error is surfaced as `LoadError::Io` rather than silently
/// dropped, so a listed-but-unreadable export dir never loads as empty.
///
/// NOTE: the dir-enumeration failure paths (`read_dir` Err, per-entry Err) are
/// not portably testable — they require an inaccessible/racing directory — so
/// they are hardened by construction with no dedicated test.
fn yaml_files(path: &Path, errs: &mut Vec<LoadError>) -> Vec<std::path::PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            errs.push(LoadError::Io {
                path: path.display().to_string(),
                msg: e.to_string(),
            });
            return Vec::new();
        }
    };
    let mut v = Vec::new();
    for entry in entries {
        let p = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                errs.push(LoadError::Io {
                    path: path.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        if p.is_file() && matches!(p.extension().and_then(|x| x.to_str()), Some("yaml" | "yml")) {
            v.push(p);
        }
    }
    v.sort();
    v
}

/// Merge a `stampAttrs` file's entries into its package (0.50.0 §3.2): each
/// entry's `on:` shape is checked (`E-PLUGIN-PARSE` at the entry), then it
/// is admitted unless it overlaps an entry of its name already declared in
/// the package (`E-PLUGIN-DUP-ID` naming the shared surfaces and the entry
/// it overlaps). Two entries may share a name only when both have `on:` and
/// their surfaces are disjoint. Each admitted entry keeps its own site,
/// where assembly reports a fault of that entry.
fn merge_stamp_attrs(
    dst: &mut Vec<crate::stamps::DeclaredStampAttr>,
    items: Vec<crate::schema::StampAttrEntry>,
    src: &Source,
    errs: &mut Vec<LoadError>,
) {
    const KIND: &str = "stampAttr";
    let mut nth: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        let (decl, on) = item.into_parts();
        let n = nth.entry(decl.name.clone()).or_default();
        let offset = field_value_at(src.text, "name", &decl.name, *n);
        *n += 1;
        let on = match stamp_surface_names(&decl, on) {
            Ok(on) => on,
            Err(msg) => {
                errs.push(LoadError::Parse {
                    file: src.file.display().to_string(),
                    at: offset.map(|o| crate::yaml_text::line_col(src.text, o)),
                    msg,
                });
                continue;
            }
        };
        if let Some((held, shared)) = dst
            .iter()
            .filter(|d| d.decl.name == decl.name)
            .find_map(|d| Some((d, crate::stamps::overlap(d.on.as_ref(), on.as_ref())?)))
        {
            let first = held.site.as_deref();
            errs.push(src.duplicate_of(KIND, decl.name, offset, first, shared));
            continue;
        }
        dst.push(crate::stamps::DeclaredStampAttr {
            decl,
            on,
            site: src.site_at(offset),
        });
    }
}

/// The surface names of a `stampAttrs` entry's `on:` (`None` without one),
/// or the message of its shape fault (0.50.0 §3.2).
fn stamp_surface_names(
    decl: &AttrDecl,
    on: Option<yaml_serde::Value>,
) -> Result<Option<BTreeSet<String>>, String> {
    use yaml_serde::Value;
    let name = &decl.name;
    let Some(on) = on else {
        if decl.required {
            return Err(format!(
                "stamp attribute `{name}` is `required: true` but has no `on:`; a stamp required \
                 on every record is not a column — name its surfaces with `on: […]`"
            ));
        }
        return Ok(None);
    };
    let items = match on {
        Value::Sequence(items) if !items.is_empty() => items,
        Value::Sequence(_) | Value::Null => {
            return Err(format!(
                "stamp attribute `{name}` has an empty `on:` — list the surfaces it applies to, \
                 or omit `on:` for every directive and content line"
            ))
        }
        Value::String(one) => {
            return Err(format!(
                "stamp attribute `{name}` has `on: {one}`, which is not a list — write \
                 `on: [{one}]`"
            ))
        }
        _ => return Err(not_surface_names(name)),
    };
    let mut names = BTreeSet::new();
    for item in items {
        let Value::String(surface) = item else {
            return Err(not_surface_names(name));
        };
        if names.contains(&surface) {
            return Err(format!(
                "stamp attribute `{name}` lists `{surface}` twice in `on:`"
            ));
        }
        names.insert(surface);
    }
    Ok(Some(names))
}

/// The fault of an `on:` that is neither a list nor a list of names.
fn not_surface_names(name: &str) -> String {
    format!(
        "stamp attribute `{name}` has an `on:` that is not a list of surface names — write \
         `on: [line, bg]`, naming `line`, `branch`, `hub`, `choice` or a directive"
    )
}

/// Merge `items` — `kind` declarations from `src`, named by `key` — into
/// `dst`, first declaration kept. `at(src, id, n)` is the byte offset of the
/// `n`th declaration of `id` in `src`, for [`Source::admit`].
#[expect(
    clippy::too_many_arguments,
    reason = "one generic merge for every declaration kind: `kind`, `key` and `at` are the per-kind parameters, the rest is the merge state and the source"
)]
fn merge_named<T>(
    dst: &mut Vec<T>,
    sites: &mut Sites,
    items: Vec<T>,
    kind: &'static str,
    key: impl Fn(&T) -> String,
    at: impl Fn(&Source, &str, usize) -> Option<usize>,
    src: &Source,
    errs: &mut Vec<LoadError>,
) {
    let mut seen: BTreeSet<String> = dst.iter().map(&key).collect();
    let mut nth: BTreeMap<String, usize> = BTreeMap::new();
    for it in items {
        let id = key(&it);
        let n = nth.entry(id.clone()).or_default();
        let offset = at(src, &id, *n);
        *n += 1;
        let new = seen.insert(id.clone());
        if src.admit(sites, kind, id, offset, new, errs) {
            dst.push(it);
        }
    }
}

/// [`merge_named`] into a keyed map.
fn merge_keyed<V>(
    dst: &mut BTreeMap<String, V>,
    sites: &mut Sites,
    items: impl Iterator<Item = (String, V)>,
    kind: &'static str,
    at: impl Fn(&Source, &str, usize) -> Option<usize>,
    src: &Source,
    errs: &mut Vec<LoadError>,
) {
    let mut nth: BTreeMap<String, usize> = BTreeMap::new();
    for (id, v) in items {
        let n = nth.entry(id.clone()).or_default();
        let offset = at(src, &id, *n);
        *n += 1;
        let new = !dst.contains_key(&id);
        if new {
            dst.insert(id.clone(), v);
        }
        src.admit(sites, kind, id, offset, new, errs);
    }
}
