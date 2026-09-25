//! Plugin package loader (plugin §4). Reads a `plugins/<id>/` directory into a
//! `LoadedPlugin`, honoring `exports`, sorting files byte-wise, and rejecting
//! per-package duplicate ids within a kind. Never panics: every failure is a
//! `LoadError` in the returned vec.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::schema::*;
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
    /// plugin §14.1 `stampattrs/*.yaml`: CROSS-CUTTING attrs admissible on
    /// every directive and content line, lowered into the record's stamp
    /// rather than its own fields.
    pub stamp_attrs: Vec<AttrDecl>,
    /// dsl 0.16.0 §4 `rewardkinds/*.yaml`: capability-declared reward
    /// vocabulary. Each entry carries the id (map key), optional `target`
    /// (`providerRef` pattern — assembly rejects a kind whose provider is
    /// absent), and optional extra `attrs` for game-specific slots. Folded
    /// into [`crate::snapshot::CapabilitySnapshot::reward_kinds`] at
    /// assembly, participating in `capabilityVersion` via the guarded
    /// hash section.
    pub reward_kinds: Vec<RewardKindDecl>,
    /// dsl 0.21.0 §2 `occasions/*.yaml`: engine occasion vocabulary (name,
    /// `select`, `target`). Folded into
    /// [`crate::snapshot::CapabilitySnapshot::occasions`] at assembly as a
    /// guarded `capabilityVersion` section, the `rewardKinds` precedent.
    pub occasions: Vec<OccasionDecl>,
    /// lint-system design §6 `lints/*.yaml`: DECLARATIVE lint rules a
    /// plugin publishes. Stored with RAW ids; a consumer namespaces each
    /// as `<plugin-id>/<id>` via [`crate::lint::namespace_active_lints`].
    /// DELIBERATELY not folded into
    /// [`crate::snapshot::CapabilitySnapshot`] or `capabilityVersion` —
    /// lints are advisory and must never change artifact identity
    /// (design §1 non-goals).
    pub lints: Vec<crate::lint::LintRuleDecl>,
    /// dsl 0.23.0 §7 `cast/*.yaml`: the declared speaker ids and display
    /// names. Folded into [`crate::snapshot::CapabilitySnapshot::cast`] at
    /// assembly as a guarded `capabilityVersion` section.
    pub cast: Vec<CastMember>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    Manifest {
        dir: String,
        msg: String,
    },
    Parse {
        file: String,
        msg: String,
    },
    DuplicateId {
        kind: String,
        id: String,
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
    /// An export key outside the closed set (plugin §4).
    UnknownExport {
        export: String,
    },
    /// plugin §7: an asset-kind segment (`assetkinds/*.yaml`, `AssetSegment.ty`)
    /// declared a `Type` outside the closed set a segment position admits.
    /// `AssetSegment.ty` is the SAME shared `Type` enum every other typed
    /// position uses (`crates/lute-manifest/src/schema.rs`), so every `Type`
    /// variant parses syntactically in a segment position; only four are
    /// semantically admitted there (`enum`, `number`, `string`,
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
            LoadError::DuplicateId { .. } => "E-PLUGIN-DUP-ID",
            LoadError::MissingExportDir { .. } => "E-PLUGIN-MISSING-EXPORT",
            LoadError::Io { .. } => "E-PLUGIN-IO",
            LoadError::UnknownExport { .. } => "E-PLUGIN-UNKNOWN-EXPORT",
            LoadError::AssetSegmentType { .. } => "E-PLUGIN-ASSET-SEGMENT-TYPE",
        }
    }
}

impl std::fmt::Display for LoadError {
    /// Human-readable rendering, surfaced by `project.rs` as the resolver's
    /// `ResolveDiag` message. Mirrors [`crate::assemble::AssembleError`]'s
    /// `Display` (`assemble.rs`), which made the identical argument when
    /// assembly errors stopped leaking `Debug` prose: since an `E-`-severity
    /// diagnostic here gates the CLI exit code, this text is the whole of
    /// what a failing plugin author sees — a Rust struct dump is not an
    /// acceptable answer (0.10.1: the toolchain says what it knows).
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Manifest { dir, msg } => {
                write!(
                    f,
                    "plugin package `{dir}` has no readable `plugin.yaml`: {msg}"
                )
            }
            LoadError::Parse { file, msg } => write!(f, "`{file}` failed to parse: {msg}"),
            LoadError::DuplicateId { kind, id } => {
                write!(f, "{kind} `{id}` is declared more than once")
            }
            LoadError::MissingExportDir { export, path } => {
                write!(f, "export `{export}` names `{path}`, which does not exist")
            }
            LoadError::Io { path, msg } => write!(f, "`{path}` could not be read: {msg}"),
            LoadError::UnknownExport { export } => write!(
                f,
                "export `{export}` is not one of the plugin manifest's known kinds \
                 (directives, state, providers, bridge, defs, enums, frontmatter, docs, \
                 assetkinds, events, stampattrs, rewardkinds, occasions, lints, cast)"
            ),
            LoadError::AssetSegmentType {
                file,
                kind,
                segment,
                found,
            } => write!(
                f,
                "`{file}`: assetKind `{kind}` segment `{segment}` declares type `{found}`, \
                 but a segment position admits only `enum`, `number`, `string`, or \
                 `providerRef` (plugin §7)"
            ),
        }
    }
}

/// Read one plugin package. `dir` MUST contain `plugin.yaml`.
pub fn load_plugin_dir(dir: &Path) -> Result<LoadedPlugin, Vec<LoadError>> {
    load_package(dir).map_err(|(_, errs)| errs)
}

/// [`load_plugin_dir`], but a failure also carries the manifest id when the
/// manifest itself parsed (only an export failed).
fn load_package(dir: &Path) -> Result<LoadedPlugin, (Option<String>, Vec<LoadError>)> {
    let mut errs = Vec::new();

    let manifest_path = dir.join("plugin.yaml");
    let manifest: PluginManifest = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => match serde_yaml::from_str(&s) {
            Ok(m) => m,
            Err(e) => {
                return Err((
                    None,
                    vec![LoadError::Manifest {
                        dir: dir.display().to_string(),
                        msg: e.to_string(),
                    }],
                ))
            }
        },
        Err(e) => {
            return Err((
                None,
                vec![LoadError::Manifest {
                    dir: dir.display().to_string(),
                    msg: e.to_string(),
                }],
            ))
        }
    };

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
    };

    // Read each declared export. A relative export path resolves under `dir`.
    for (export, rel) in &manifest.exports {
        let path = dir.join(rel);
        if !path.exists() {
            errs.push(LoadError::MissingExportDir {
                export: export.clone(),
                path: path.display().to_string(),
            });
            continue;
        }
        match export.as_str() {
            "directives" => read_kind::<DirectivesFile, _>(&path, &mut errs, |f, _file, e| {
                merge_directives(&mut out.directives, f.directives, e)
            }),
            "state" => read_state(&path, &mut out, &mut errs),
            "providers" => read_kind::<ProvidersFile, _>(&path, &mut errs, |f, _file, e| {
                merge_named(
                    &mut out.providers,
                    f.providers,
                    "provider",
                    |p| p.name.clone(),
                    e,
                )
            }),
            "bridge" => read_kind::<BridgeFile, _>(&path, &mut errs, |f, _file, e| {
                merge_bridge(&mut out.bridge, f.bridge, e)
            }),
            "defs" => read_kind::<DefsFile, _>(&path, &mut errs, |f, _file, e| {
                merge_named(&mut out.defs, f.defs, "def", |d| d.name.clone(), e)
            }),
            "enums" => read_enums(&path, &mut out.enums, &mut errs),
            "frontmatter" => read_kind::<FrontmatterFile, _>(&path, &mut errs, |f, _file, e| {
                merge_frontmatter(&mut out.frontmatter, f.frontmatter, e)
            }),
            "docs" => { /* non-normative (plugin §6.7); skip */ }
            // Anchored at the INDIVIDUAL declaration file `read_kind` just
            // parsed `f` from, not the `assetkinds/` export dir `path` names
            // — the same anchor `LoadError::Parse` already uses for a sibling
            // failure in the same file (plugin §7 fix, defect 2).
            "assetkinds" => read_kind::<AssetKindsFile, _>(&path, &mut errs, |f, file, e| {
                check_asset_segment_types(&f.asset_kinds, &file.display().to_string(), e);
                merge_named(
                    &mut out.asset_kinds,
                    f.asset_kinds,
                    "assetKind",
                    |k| k.kind.clone(),
                    e,
                )
            }),
            "events" => read_kind::<EventsFile, _>(&path, &mut errs, |f, _file, e| {
                merge_named(&mut out.events, f.events, "event", |ev| ev.name.clone(), e)
            }),
            "stampattrs" => read_kind::<StampAttrsFile, _>(&path, &mut errs, |f, _file, e| {
                merge_named(
                    &mut out.stamp_attrs,
                    f.stamp_attrs,
                    "stampAttr",
                    |a| a.name.clone(),
                    e,
                )
            }),
            "rewardkinds" => read_kind::<RewardKindsFile, _>(&path, &mut errs, |f, _file, e| {
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
                merge_named(
                    &mut out.reward_kinds,
                    decls,
                    "rewardKind",
                    |r| r.name.clone(),
                    e,
                )
            }),
            "occasions" => read_kind::<OccasionsFile, _>(&path, &mut errs, |f, file, e| {
                check_occasion_members(&f.occasions, &file.display().to_string(), e);
                let decls: Vec<OccasionDecl> = f
                    .occasions
                    .into_iter()
                    .map(|(name, body)| OccasionDecl {
                        name,
                        select: body.select,
                        target: body.target,
                        description: body.description,
                        judge: body.judge,
                    })
                    .collect();
                merge_named(&mut out.occasions, decls, "occasion", |o| o.name.clone(), e)
            }),
            "lints" => read_kind::<LintsFile, _>(&path, &mut errs, |f, _file, e| {
                merge_named(&mut out.lints, f.lints, "lint", |r| r.id.clone(), e)
            }),
            "cast" => read_kind::<CastFile, _>(&path, &mut errs, |f, _file, e| {
                let decls: Vec<CastMember> = f
                    .cast
                    .into_iter()
                    .map(|(id, body)| body.into_member(id))
                    .collect();
                merge_named(&mut out.cast, decls, "cast", |c| c.id.clone(), e)
            }),
            other => errs.push(LoadError::UnknownExport {
                export: other.to_string(),
            }),
        }
    }

    if errs.is_empty() {
        Ok(out)
    } else {
        Err((Some(out.manifest.id), errs))
    }
}

/// The `Type` variants a segment position actually enforces
/// (`crate::asset::validate_segments`'s non-catch-all arms): `enum`
/// (membership), `number` (parses as `f64`), `string` (accepts anything),
/// and `providerRef` (resolved against a provider snapshot — plugin-system
/// 0.0.1 §7 names this the asset-kind-segment case explicitly, alongside
/// attribute and frontmatter positions). Nothing else is admitted; see
/// [`check_asset_segment_types`] for why each remaining `Type` variant is
/// excluded.
fn segment_type_admitted(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Enum(_) | Type::Number | Type::Str | Type::ProviderRef(_)
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

/// An occasion target domain's `members:` list (the member subset) must name
/// at least one member, each once. Whether each listed member belongs to the
/// domain's entity kind is the checker's to judge: entity kinds are project
/// vocabulary (`entities:`), unknown to a plugin package. Reported as a
/// [`LoadError::Parse`] of `file`, the declaration file the list came from.
fn check_occasion_members(
    occasions: &BTreeMap<String, OccasionBody>,
    file: &str,
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
            file: file.to_string(),
            msg,
        });
    }
}

/// Scan `dir` for plugin packages (each immediate subdirectory containing a
/// `plugin.yaml`), in sorted order, and index by manifest id. A duplicate id
/// across packages is a `LoadError::DuplicateId { kind: "plugin", .. }` (the
/// later package is dropped). A package whose manifest parsed but whose
/// exports failed lands in [`crate::resolve::InstalledPlugins::failed`], so
/// assembly can say it failed to load rather than that it is not installed.
/// A missing `dir` yields an empty registry.
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
    for sub in subs {
        if !sub.join("plugin.yaml").is_file() {
            continue;
        }
        match load_package(&sub) {
            Ok(loaded) => {
                let id = loaded.manifest.id.clone();
                match reg.by_id.entry(id) {
                    std::collections::btree_map::Entry::Occupied(e) => {
                        errs.push(LoadError::DuplicateId {
                            kind: "plugin".into(),
                            id: e.key().clone(),
                        });
                    }
                    std::collections::btree_map::Entry::Vacant(e) => {
                        e.insert(InstalledPlugin { loaded });
                    }
                }
            }
            Err((id, mut e)) => {
                if let Some(id) = id {
                    let codes = reg.failed.entry(id).or_default();
                    for err in &e {
                        if !codes.contains(&err.code()) {
                            codes.push(err.code());
                        }
                    }
                }
                errs.append(&mut e);
            }
        }
    }
    (reg, errs)
}

/// Read a single YAML file OR every `*.yaml`/`*.yml` in a dir (sorted byte-wise),
/// deserialize each to `F`, and hand it to `merge`.
fn read_kind<F, M>(path: &Path, errs: &mut Vec<LoadError>, mut merge: M)
where
    F: serde::de::DeserializeOwned,
    M: FnMut(F, &Path, &mut Vec<LoadError>),
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
        match serde_yaml::from_str::<F>(&s) {
            Ok(f) => merge(f, &file, errs),
            Err(e) => errs.push(LoadError::Parse {
                file: file.display().to_string(),
                msg: parse_error_msg(&s, &e),
            }),
        }
    }
}

/// The `E-PLUGIN-PARSE` message for a failed export file: serde's own text,
/// plus — for an unknown key — a did-you-mean against the accepted keys, and,
/// when the offending mapping holds a key with no value, the hint that an
/// unquoted flow-map value ended at a comma (`{ description: Pick one, the
/// player picks one }` parses as `description: Pick one` plus a null-valued
/// key `the player picks one`).
fn parse_error_msg(src: &str, e: &serde_yaml::Error) -> String {
    let mut msg = e.to_string();
    let Some((key, expected)) = unknown_field(&msg) else {
        return msg;
    };
    let max = (key.chars().count() / 3).clamp(1, 2);
    if let Some(s) = crate::suggest::nearest(&key, expected.iter().map(String::as_str), max) {
        msg.push_str(&format!("; did you mean `{s}`?"));
    }
    if let Some(hint) = serde_yaml::from_str::<serde_yaml::Value>(src)
        .ok()
        .and_then(|doc| null_key_hint(&doc, &key, &expected))
    {
        msg.push_str("; ");
        msg.push_str(&hint);
    }
    msg
}

/// Split serde's "unknown field `k`, expected one of `a`, `b`" (or "expected
/// `a`", or "there are no fields") into the key and the accepted names.
fn unknown_field(msg: &str) -> Option<(String, Vec<String>)> {
    let rest = &msg[msg.find("unknown field `")? + "unknown field `".len()..];
    let end = rest.find('`')?;
    let key = rest[..end].to_string();
    let tail = &rest[end + 1..];
    let tail = tail.find(" at line ").map_or(tail, |i| &tail[..i]);
    let expected = tail
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();
    Some((key, expected))
}

/// The first mapping in `doc` holding `key`; if it also holds keys with no
/// value that are not accepted fields, the "quote it" hint naming them.
fn null_key_hint(doc: &serde_yaml::Value, key: &str, expected: &[String]) -> Option<String> {
    fn find<'a>(v: &'a serde_yaml::Value, key: &str) -> Option<&'a serde_yaml::Mapping> {
        match v {
            serde_yaml::Value::Mapping(m) => {
                if m.contains_key(key) {
                    return Some(m);
                }
                m.values().find_map(|v| find(v, key))
            }
            serde_yaml::Value::Sequence(s) => s.iter().find_map(|v| find(v, key)),
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

/// `state/` holds `stateShapes:` and/or `stateTemplates:` files.
fn read_state(path: &Path, out: &mut LoadedPlugin, errs: &mut Vec<LoadError>) {
    read_kind::<StateFile, _>(path, errs, |f, file, e| {
        if f.state_shapes.is_none() && f.state_templates.is_none() {
            e.push(LoadError::Parse {
                file: file.display().to_string(),
                msg: "not a state file: declare `stateShapes:` and/or `stateTemplates:`".into(),
            });
        }
        merge_named(
            &mut out.state_shapes,
            f.state_shapes.unwrap_or_default(),
            "shape",
            |s| s.name.clone(),
            e,
        );
        merge_named(
            &mut out.state_templates,
            f.state_templates.unwrap_or_default(),
            "template",
            |t| t.name.clone(),
            e,
        );
    })
}

fn read_enums(
    path: &Path,
    dst: &mut BTreeMap<String, crate::snapshot::Domain>,
    errs: &mut Vec<LoadError>,
) {
    read_kind::<EnumsFile, _>(path, errs, |f, _file, e| {
        for (k, v) in f.enums {
            if dst.insert(k.clone(), v.into_domain()).is_some() {
                e.push(LoadError::DuplicateId {
                    kind: "enum".into(),
                    id: k,
                });
            }
        }
    })
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
        if p.is_file()
            && matches!(
                p.extension().and_then(|x| x.to_str()),
                Some("yaml") | Some("yml")
            )
        {
            v.push(p);
        }
    }
    v.sort();
    v
}

fn merge_named<T, K: Fn(&T) -> String>(
    dst: &mut Vec<T>,
    items: Vec<T>,
    kind: &str,
    key: K,
    errs: &mut Vec<LoadError>,
) {
    let mut seen: BTreeSet<String> = dst.iter().map(&key).collect();
    for it in items {
        let id = key(&it);
        if !seen.insert(id.clone()) {
            errs.push(LoadError::DuplicateId {
                kind: kind.into(),
                id,
            });
        } else {
            dst.push(it);
        }
    }
}

fn merge_directives(
    dst: &mut Vec<DirectiveDecl>,
    items: Vec<DirectiveDecl>,
    errs: &mut Vec<LoadError>,
) {
    merge_named(dst, items, "directive", |d| d.name.clone(), errs);
}

fn merge_bridge(
    dst: &mut Vec<BridgeCapability>,
    items: Vec<BridgeCapability>,
    errs: &mut Vec<LoadError>,
) {
    let mut seen: BTreeSet<(String, String)> = dst
        .iter()
        .map(|b| (b.service.clone(), b.operation.clone()))
        .collect();
    for b in items {
        let k = (b.service.clone(), b.operation.clone());
        if !seen.insert(k) {
            errs.push(LoadError::DuplicateId {
                kind: "bridge".into(),
                id: format!("{}.{}", b.service, b.operation),
            });
        } else {
            dst.push(b);
        }
    }
}

fn merge_frontmatter(
    dst: &mut BTreeMap<String, Type>,
    items: Vec<FrontmatterDecl>,
    errs: &mut Vec<LoadError>,
) {
    for f in items {
        if dst.insert(f.key.clone(), f.schema).is_some() {
            errs.push(LoadError::DuplicateId {
                kind: "frontmatter".into(),
                id: f.key,
            });
        }
    }
}
