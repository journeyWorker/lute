use super::*;
use super::diagnostic_helpers::unrepresentable_yaml;

/// dsl 0.15.0 §3: lift one authored `extra:` YAML value into the JSON-ready
/// `extra_block` on `TypedMeta`. The value MUST be a mapping with string keys
/// whose values are either scalars (string/int/float/bool) or FLAT sequences
/// of scalars — anything else (nested mapping, mixed list, non-string key,
/// non-mapping top-level) stays out of the block and draws one `E-META-VALUE`
/// anchored at the offending inner key (or at `extra:` itself for the
/// non-mapping case).
///
/// Never panics; a malformed entry is simply skipped so a partly-valid block
/// still contributes its clean keys downstream.
fn lift_extra_block(
    meta: &Meta,
    value: &serde_yaml::Value,
    into: &mut BTreeMap<String, serde_json::Value>,
    diags: &mut Vec<Diagnostic>,
) {
    let map = match value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => return,
        _ => {
            diags.push(Diagnostic {
                code: "E-META-VALUE".to_string(),
                severity: Severity::Error,
                message: "`extra:` must be a YAML mapping of descriptive keys; each value \
                          is a scalar (string/int/float/bool) or a flat list of scalars \
                          (dsl 0.15.0 §3)"
                    .to_string(),
                evidence: None,
                span: meta_key_span(meta, "extra"),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
            return;
        }
    };
    for (k, v) in map {
        let Some(key) = k.as_str() else {
            diags.push(Diagnostic {
                code: "E-META-VALUE".to_string(),
                severity: Severity::Error,
                message: "`extra:` mapping keys must be strings (dsl 0.15.0 §3)".to_string(),
                evidence: None,
                span: meta_key_span(meta, "extra"),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
            continue;
        };
        match scalar_or_flat_seq_to_json(v) {
            Some(json) => {
                into.insert(key.to_string(), json);
            }
            None => diags.push(Diagnostic {
                code: "E-META-VALUE".to_string(),
                severity: Severity::Error,
                message: format!(
                    "`meta.{key}` must be a scalar (string/int/float/bool) or a flat list \
                     of scalars; nested mappings and non-scalar list entries are not allowed \
                     (dsl 0.15.0 §3)"
                ),
                evidence: None,
                span: meta_key_span(meta, key),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }),
        }
    }
}

/// dsl 0.15.0 §3: convert an `extra:` value to `serde_json::Value` if and only
/// if it is a scalar or a flat sequence of scalars. `None` for a nested
/// mapping, a sequence containing a non-scalar, a `!tag`, or a mapping with
/// non-string keys. `null` is a scalar (it round-trips to JSON `null`).
fn scalar_or_flat_seq_to_json(value: &serde_yaml::Value) -> Option<serde_json::Value> {
    fn scalar_to_json(value: &serde_yaml::Value) -> Option<serde_json::Value> {
        match value {
            serde_yaml::Value::Null => Some(serde_json::Value::Null),
            serde_yaml::Value::Bool(b) => Some(serde_json::Value::Bool(*b)),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Some(serde_json::Value::Number(i.into()))
                } else if let Some(u) = n.as_u64() {
                    Some(serde_json::Value::Number(u.into()))
                } else if let Some(f) = n.as_f64() {
                    serde_json::Number::from_f64(f).map(serde_json::Value::Number)
                } else {
                    None
                }
            }
            serde_yaml::Value::String(s) => Some(serde_json::Value::String(s.clone())),
            _ => None,
        }
    }
    match value {
        serde_yaml::Value::Sequence(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(scalar_to_json(item)?);
            }
            Some(serde_json::Value::Array(out))
        }
        other => scalar_to_json(other),
    }
}

/// Whether `meta`'s frontmatter lifts to a mapping (an empty frontmatter
/// included) — exactly when [`parse_meta_kind_with_defaults`] does NOT emit
/// `E-META-PARSE`, including its duplicate-block-key retry. The project-wide
/// passes, which read raw frontmatter without a `TypedMeta`, use this to tell
/// "this document declares no such id" from "this document's ids are
/// unreadable" (0.21.1 T3-8).
pub fn frontmatter_parses(meta: &Meta) -> bool {
    let value = serde_yaml::from_str::<serde_yaml::Value>(&meta.raw_yaml)
        .or_else(|_| serde_yaml::from_str(&sanitize_dup_block_keys(&meta.raw_yaml)));
    matches!(
        value,
        Ok(serde_yaml::Value::Mapping(_) | serde_yaml::Value::Null)
    )
}

/// Parse the peeled YAML frontmatter (dsl §6.1) into typed form plus the inline
/// `state:` schema (dsl §9.3). Never panics on malformed YAML: a parse failure
/// surfaces `E-META-PARSE` and yields a best-effort (empty) `TypedMeta`.
///
/// This performs the §6.1 required-key and unknown-key checks and records each
/// `state:` path's `Namespace` from its leading segment. App-write read-only
/// enforcement (§9.5, Task 4.5) and def-assignment (§8.1, Task 4.4) are NOT done
/// here.
pub fn parse_meta(meta: &Meta, snapshot: &CapabilitySnapshot) -> (TypedMeta, Vec<Diagnostic>) {
    parse_meta_kind(meta, snapshot, MetaKind::Scene)
}

/// Kind-aware variant of [`parse_meta`]. Schema docs (`MetaKind::Schema`) skip
/// the §6.1 required-key check (they carry no character/season/episode); every
/// other check (unknown-key, `state:`, `defs:`, `uses:` lift) is identical.
pub fn parse_meta_kind(
    meta: &Meta,
    snapshot: &CapabilitySnapshot,
    kind: MetaKind,
) -> (TypedMeta, Vec<Diagnostic>) {
    parse_meta_kind_with_defaults(
        meta,
        snapshot,
        kind,
        &lute_manifest::project::MetaDefaults::default(),
    )
}

/// [`parse_meta_kind`] with the governing manifest's `defaults:` applied
/// (0.10.0 §6.2).
///
/// The merge happens on the frontmatter MAPPING, before any frontmatter rule
/// runs, which is what makes §6.2's last two bullets free: required-key
/// checking, unknown-key checking, type checking and every lift below all see
/// the resolved map, and a defaulted value draws exactly the diagnostic an
/// authored one would because it IS an authored one by the time they run.
///
/// Per key, whole-value (D-P): a key the document contains AT ALL wins
/// entire. `Mapping::contains_key` is exactly that test, so a present null or
/// an empty sequence is a present key — §6.2's "present-but-empty counts as
/// present", with no special case.
pub fn parse_meta_kind_with_defaults(
    meta: &Meta,
    snapshot: &CapabilitySnapshot,
    kind: MetaKind,
    defaults: &lute_manifest::project::MetaDefaults,
) -> (TypedMeta, Vec<Diagnostic>) {
    let span = meta.span;
    let mut diags = Vec::new();
    let err = |code: &str, message: String| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    // Same Content-layer error, but at a caller-supplied narrow span (used for
    // E-PATH-IDENT, which points at the offending key — not the whole block).
    let err_at = |code: &str, message: String, sp: Span| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span: sp,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };

    let raw_parse = serde_yaml::from_str::<serde_yaml::Value>(&meta.raw_yaml);
    let raw_yaml = raw_parse.as_ref().ok().cloned();
    let value: serde_yaml::Value = match raw_parse {
        Ok(v) => v,
        Err(e) => {
            // A literal duplicate key inside `entities:`/`relations:`
            // (same-block dup, dsl 0.3.0 T5/T7) makes `serde_yaml::from_str`
            // reject the ENTIRE frontmatter — it does NOT silently collapse
            // the repeat, despite an earlier assumption in this codebase.
            // Retry against a text-sanitized copy (every occurrence after the
            // first of a same-indent-level `entities:`/`relations:` child key
            // commented out) so every OTHER frontmatter field still lifts;
            // the authoritative dup NAME still comes from
            // `scan_block_dup_names` against the UNMODIFIED `meta.raw_yaml`
            // below, never this sanitized copy.
            match serde_yaml::from_str(&sanitize_dup_block_keys(&meta.raw_yaml)) {
                Ok(v) => v,
                Err(_) => {
                    let (message, at) = yaml_parse_error(meta, &e);
                    diags.push(err_at("E-META-PARSE", message, at));
                    return (TypedMeta::default(), diags);
                }
            }
        }
    };

    // An empty frontmatter deserializes to Null; treat as an empty mapping so the
    // required-key checks still fire.
    let empty = serde_yaml::Mapping::new();
    let map = match &value {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => &empty,
        _ => {
            diags.push(err(
                "E-META-PARSE",
                "meta frontmatter must be a YAML mapping".to_string(),
            ));
            return (TypedMeta::default(), diags);
        }
    };

    // dsl 0.15.0 §4 (D-C): `W-META-LEGACY` reads the AUTHORED frontmatter,
    // never the defaults-merged view — a manifest-inherited legacy key is not
    // the document author's to move. Snapshot the authored `id:` presence and
    // authored legacy keys BEFORE the defaults overlay below; the required-
    // and unknown-key checks continue to run over the merged map (0.10.0 §6.2).
    let authored_has_id = map.contains_key(yaml_key("id"));
    let authored_legacy: Vec<&'static str> = ["character", "season", "episode", "episodeId"]
        .iter()
        .copied()
        .filter(|k| map.contains_key(yaml_key(k)))
        .collect();

    // 0.10.0 §6.2/§6.3: overlay the governing manifest's `defaults:`, filtered
    // to keys legal on THIS kind. Borrowed when there is nothing to overlay,
    // which is every project written before 0.10.0 — no allocation on the
    // path every existing document takes.
    let map: std::borrow::Cow<'_, serde_yaml::Mapping> = if defaults.is_empty() {
        std::borrow::Cow::Borrowed(map)
    } else {
        let mut merged = map.clone();
        for key in defaults.keys() {
            if !default_key_legal_on(key, kind) {
                continue;
            }
            if merged.contains_key(yaml_key(key)) {
                continue;
            }
            if let Some(v) = defaults.get(key) {
                merged.insert(yaml_key(key), v.clone());
            }
        }
        std::borrow::Cow::Owned(merged)
    };
    let map = map.as_ref();
    let mut typed = TypedMeta::default();
    typed.yaml = raw_yaml;
    // Required-key check (dsl §6.1). Only scenes carry the legacy required
    // core keys; Schema/Component/Quest do not. dsl 0.15.0 §4: authored (or
    // defaults-merged) `id:` IS the canonical scene key — the required-key
    // rule no longer fires on a document that supplies one.
    if kind == MetaKind::Scene && !map.contains_key(yaml_key("id")) {
        let missing: Vec<&str> = REQUIRED_KEYS
            .iter()
            .copied()
            .filter(|k| !map.contains_key(yaml_key(k)))
            .collect();
        if missing.len() == REQUIRED_KEYS.len() {
            // No identity at all (an empty file, a first scene): teach `id:`,
            // not the legacy triple. A `title:` (Yarn's node header, an Ink
            // knot name) is only the displayed name: point at it and offer
            // the id it spells.
            let title = map
                .get(yaml_key("title"))
                .and_then(serde_yaml::Value::as_str)
                .map(|t| ident_from_name(t, "opening"));
            diags.push(match title {
                Some(id) => err_at(
                    "E-META-MISSING",
                    format!(
                        "a scene needs an `id:`, its key in the project — `title:` is only the \
                         name shown for it; write `id: {id}` beside it (dsl 0.15.0 §2/§4)"
                    ),
                    meta_key_span(meta, "title"),
                ),
                None => err(
                    "E-META-MISSING",
                    "a scene needs an `id:`, its key in the project — write `id: opening` in \
                     the frontmatter (dsl 0.15.0 §2/§4)"
                        .to_string(),
                ),
            });
        } else {
            for missing in missing {
                diags.push(err(
                    "E-META-MISSING",
                    format!(
                        "required meta key `{missing}` is missing (authored `id:` also \
                         satisfies scene identity, dsl 0.15.0 §2/§4)"
                    ),
                ));
            }
        }
    }
    // Unknown-key check over the top-level keys (dsl §6.1); applies to every kind.
    // `component:`/`params:` (dsl §13) are allowed ONLY in a component file, so a
    // scene or schema doc that declares them hits the unknown-key diagnostic.
    //
    // A key the CORE owns keeps its bespoke handling below (each is lifted and
    // typed by its own `get_*`/parse step); only a PLUGIN-owned key routes
    // through the declared-schema check (plugin Appendix C2).
    let component_key_allowed = kind == MetaKind::Component;
    for (k, v) in map.iter() {
        let Some(key) = k.as_str() else {
            diags.push(err(
                "E-META-UNKNOWN-KEY",
                "meta keys must be strings".to_string(),
            ));
            continue;
        };
        let core_key = UNIVERSAL_KEYS.contains(&key)
            || (key == "kind" && kind.is_root())
            || kind_keys(kind).contains(&key)
            || (kind.is_root() && key == "extra")
            || (component_key_allowed && COMPONENT_ONLY_KEYS.contains(&key));
        if core_key {
            continue;
        }
        let Some(ty) = snapshot.frontmatter.get(key) else {
            diags.push(err_at(
                "E-META-UNKNOWN-KEY",
                format!(
                    "unknown top-level meta key `{key}` (not a core key and not owned by an active plugin){}",
                    unknown_key_hint(key, kind, component_key_allowed)
                ),
                meta_path_span(meta, &[key]),
            ));
            continue;
        };
        // plugin Appendix C2: "The checker MUST validate a scene's frontmatter
        // block against each active plugin's declared key schema; a value that
        // violates the schema is a static error." A value with no `Literal`
        // representation at all (null, a `!tag`, a non-string mapping key)
        // is the SAME error, never a panic.
        let lit = Literal::from_yaml(v);
        if !lit.as_ref().is_some_and(|l| type_accepts(ty, l)) {
            let found = match &lit {
                Some(l) => lit_str(l),
                None => unrepresentable_yaml(v).to_string(),
            };
            diags.push(err_at(
                "E-FRONTMATTER-SCHEMA",
                format!(
                    "frontmatter key `{key}` expects {} (declared by an active plugin), got {found}",
                    type_str(ty)
                ),
                meta_key_span(meta, key),
            ));
        }
    }

    // Lift the built-in scalar/collection keys.
    typed.character = get_str(map, "character");
    typed.season = get_i64(map, "season");
    typed.episode = get_i64(map, "episode");
    typed.episode_id = get_str(map, "episodeId");
    typed.pov = get_str(map, "pov");
    typed.lute_version = get_str(map, "luteVersion");
    typed.after = get_str(map, "after");

    // dsl 0.15.0 §2 / dsl 0.19.0 §2.1 / dsl 0.30.0 §1: authored document id —
    // a scene's canonical scene key, a quest or lore document's bundle name:
    // names joined by `.`. Anything else is `E-META-ID` and the value
    // stays unlifted (a rejected id must never fall through as a valid key).
    // A `Schema`/`Component` id: was already rejected as
    // `E-META-UNKNOWN-KEY` above and is not lifted.
    if kind_keys(kind).contains(&"id") {
        if let Some(raw) = get_str(map, "id") {
            let what = if kind == MetaKind::Scene {
                "scene `id:`"
            } else {
                "document `id:`"
            };
            match lute_manifest::ident::dotted_name_fault(what, &raw) {
                None => typed.id = Some(raw),
                Some(message) => {
                    diags.push(err_at("E-META-ID", message, meta_key_span(meta, "id")))
                }
            }
        }
    }

    // dsl 0.19.0 §2.1 (D-K): a lore document's `series:` names the one series
    // all of its entries form, ordered by position. The value is a name
    // (the `<entry series=…>` shape); anything else is `E-META-VALUE` — the
    // frontmatter value-shape code — and stays unlifted, so no entry
    // resolves into a malformed series.
    if kind == MetaKind::Lore {
        if let Some(value) = map.get(yaml_key("series")) {
            match value.as_str() {
                Some(series) if lute_manifest::ident::is_name(series) => {
                    typed.series = Some(series.to_string())
                }
                found => diags.push(err_at(
                    "E-META-VALUE",
                    match found {
                        Some(series) => lute_manifest::ident::name_fault("`series:`", series)
                            .unwrap_or_default(),
                        None => "`series:` must be a name, got a non-string value; it \
                                 names the series this document's entries form"
                            .to_string(),
                    },
                    meta_key_span(meta, "series"),
                )),
            }
        }
    }

    // dsl 0.21.0 §3.1: a scene answering an occasion — `on` / `target` /
    // `when` / `priority` / `once`, validated against the resolved occasion
    // vocabulary (`E-BEAT-ATTR` / `E-OCCASION-UNKNOWN`). Scene-only: on any
    // other kind these keys were already `E-META-UNKNOWN-KEY` above.
    if kind == MetaKind::Scene {
        typed.beat = crate::beats::lift_scene_beat(meta, map, &snapshot.occasions, &mut diags);
    }

    // dsl 0.15.0 §3: authored `extra:` descriptive block. Legal on Scene and
    // Quest roots (both artifact meta kinds carry it); on Schema/Component
    // documents the top-level unknown-key loop above already rejected it, so
    // the lift silently drops it there.
    if kind.is_root() {
        if let Some(meta_value) = map.get(yaml_key("extra")) {
            lift_extra_block(meta, meta_value, &mut typed.extra_block, &mut diags);
        }
    }

    // dsl 0.15.0 §4/D-C: per-key `W-META-LEGACY` for each authored legacy
    // identity key coexisting with an authored `id:`. Reads the AUTHORED
    // snapshot from BEFORE the defaults merge — a manifest-inherited legacy
    // key on an `id:`-carrying document is silent.
    if authored_has_id && kind == MetaKind::Scene {
        for key in &authored_legacy {
            diags.push(Diagnostic {
                code: "W-META-LEGACY".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "`{key}` no longer carries scene identity (superseded by `id:`); move it \
                     under `extra:` to keep it searchable (dsl 0.15.0 §4)"
                ),
                evidence: None,
                span: meta_key_span(meta, key),
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    }

    typed.profile = get_str(map, "profile");
    typed.uses = get_ref_list(map, "uses");
    typed.extends = get_ref_list(map, "extends");
    typed.plugins = get_sub_map(map, "plugins");
    typed.defs = get_sub_map(map, "defs");
    // Project-authored `enums:`/`entities:`/`relations:` (dsl data-catalog
    // foundation A3; 0.3.0 draft §3.1 kinds, §4 relations, 0.3.0 T4/T5): same
    // YAML-lift discipline as `defs` above, but delegated to
    // `lute_manifest::entities::parse_enums` /
    // `lute_manifest::relations::{parse_entity_kinds, parse_relations,
    // kinds_to_domains}` (the latter owns the `entities:`/`relations:` decl
    // shapes, A2's `Domain` type). `entities:` is folded into `domains` AFTER
    // `enums:` so a same-doc name collision resolves to the `entities:` entry
    // (last-write-wins; not diagnosed here — see `TypedMeta::domains`'s doc
    // comment).
    let enums_val = map
        .get(yaml_key("enums"))
        .unwrap_or(&serde_yaml::Value::Null);
    let project_enums = lute_manifest::entities::parse_enums(enums_val);
    // dsl 0.24.0 §1 / 0.28.0 §1: `parse_enums` is total; the long-form shape
    // mistakes it drops (an unknown key, no `members:`, a non-string label)
    // are reported here, each at its own key (a label for a non-member is
    // `E-ENUM-LABEL-NOT-MEMBER`, from the shared `validate_domain` rules).
    for (message, path) in lute_manifest::entities::enum_shape_errors(enums_val) {
        let path: Vec<&str> = path.iter().map(String::as_str).collect();
        diags.push(err_at("E-META-VALUE", message, meta_path_span(meta, &path)));
    }
    typed.domains = project_enums.clone();
    typed.rel_kinds = lute_manifest::relations::parse_entity_kinds(
        map.get(yaml_key("entities"))
            .unwrap_or(&serde_yaml::Value::Null),
    );
    // dsl 0.28.0 (T3-45): an `enums:` name this document also declares as an
    // entity kind (a clash across documents is `build_rel_vocab`'s).
    for (name, dom) in &project_enums {
        if let Some(kind) = typed.rel_kinds.kinds.get(name) {
            diags.push(err_at(
                crate::rel_schema::E_DOMAIN_NAME_CLASH,
                crate::rel_schema::domain_clash_message(name, &dom.members, &kind.shape),
                meta_path_span(meta, &["entities", name]),
            ));
        }
    }
    typed.rel_relations = lute_manifest::relations::parse_relations(
        map.get(yaml_key("relations"))
            .unwrap_or(&serde_yaml::Value::Null),
    );
    // dsl 0.23.0 §7: `cast: { <id>: { name: "…" } }` (schema documents only;
    // elsewhere the key was already `E-META-UNKNOWN-KEY` above). dsl 0.24.0
    // §4: an entry may add `present: "<condition>"` and `emotions: [...]`,
    // each checked here at the entry's key.
    if kind == MetaKind::Schema {
        if let Some(v) = map.get(yaml_key("cast")) {
            match serde_yaml::from_value::<
                std::collections::BTreeMap<String, lute_manifest::schema::CastBody>,
            >(v.clone())
            {
                Ok(m) => {
                    typed.cast = m
                        .into_iter()
                        .map(|(id, b)| {
                            let mut member = b.into_member(id);
                            let span = meta_key_span(meta, &member.id);
                            diags.extend(crate::cast::validate_member(
                                &mut member,
                                span,
                                &typed.domains,
                            ));
                            member
                        })
                        .collect();
                }
                Err(e) => diags.push(err_at(
                    "E-META-VALUE",
                    format!(
                        "`cast:` must map each speaker id to `{{ name: \"…\", present: \"…\", \
                         emotions: [...] }}` (dsl 0.23.0 §7, 0.24.0 §4): {e}"
                    ),
                    meta_key_span(meta, "cast"),
                )),
            }
        }
    }
    // dsl 0.24.0 §1: `clock:` (schema documents only, like `cast:`).
    if kind == MetaKind::Schema {
        if let Some(v) = map.get(yaml_key("clock")) {
            let (clock, clock_diags) = crate::clock::parse_clock(v, meta);
            typed.state.clock_rejected = clock.is_none();
            typed.clock = clock;
            diags.extend(clock_diags);
        }
        // `terminal: "<condition>"` or `{ when: "<condition>", persists: true }`.
        if let Some(v) = map.get(yaml_key("terminal")) {
            let (terminal, terminal_diags) = crate::gates::parse_terminal(v, meta);
            typed.terminal = terminal;
            diags.extend(terminal_diags);
        }
        // dsl 0.27.0 §5: `seasons: { <name>: { live: "<condition>" } }`.
        if let Some(v) = map.get(yaml_key("seasons")) {
            let (seasons, season_diags) = crate::season::parse_seasons(v, &|path| {
                let full: Vec<&str> = std::iter::once("seasons")
                    .chain(path.iter().copied())
                    .collect();
                meta_path_span(meta, &full)
            });
            typed.seasons = seasons;
            diags.extend(season_diags);
        }
    }
    // Domain projection for the 0.2.2 attr layer (entities win over enums, as before).
    typed
        .domains
        .extend(lute_manifest::relations::kinds_to_domains(
            &typed.rel_kinds.kinds,
        ));

    datalog::lift_datalog(meta, map, &mut typed, &mut diags, span);
    // One name rule: relation names, entity-kind names, `enums:` names, and
    // every enum and entity member are names (E-PATH-IDENT), named at the
    // offending key.
    // A name written twice (a member also listed under `exits:`, or in two
    // enums) is reported once, at its first occurrence.
    let mut reported = BTreeSet::new();
    let mut name_diag = |what: &str, name: &str| {
        if let Some(message) = lute_manifest::ident::name_fault(what, name) {
            if reported.insert(name.to_string()) {
                diags.push(err_at(E_PATH_IDENT, message, meta_key_span(meta, name)));
            }
        }
    };
    for (name, decl) in &typed.rel_kinds.kinds {
        name_diag("entity kind", name);
        if let lute_manifest::relations::KindShape::Members(members) = &decl.shape {
            for member in members {
                name_diag("entity member", member);
            }
        }
    }
    for name in typed.rel_relations.relations.keys() {
        name_diag("relation", name);
    }
    for (name, domain) in &project_enums {
        name_diag("enum", name);
        for member in &domain.members {
            name_diag("enum member", member);
        }
    }

    // Authoritative same-block duplicate detection (0.3.0 T4/T5):
    // `serde_yaml::Mapping` collapses a repeated YAML key before
    // `parse_entity_kinds`/`parse_relations` ever see it, so their own
    // `dups` field is best-effort. This raw-text scan is the authoritative
    // source consumed by the checker (Task 7).
    typed.rel_relations.dups = scan_block_dup_names(&meta.raw_yaml, "relations");
    typed.rel_kinds.dups = scan_block_dup_names(&meta.raw_yaml, "entities");
    // dsl 0.21.0 §7b: lift every `defs:` entry to its canonical long form —
    // the shorthand `name: "<CEL>"` becomes `{ cel: "<CEL>" }` — so no later
    // reader can see a def's name without its body. A malformed entry is
    // `E-DEF-DECL` at its own key; its NAME stays declared, so a `@name` use
    // does not cascade to `E-UNDECLARED-REF`. Imported schemas pass through
    // this same parse (`MetaKind::Schema`), so the rule holds across `uses:`.
    match map.get(yaml_key("defs")) {
        None | Some(serde_yaml::Value::Null) => {}
        Some(serde_yaml::Value::Mapping(defs)) => {
            if defs.keys().any(|k| k.as_str().is_none()) {
                diags.push(err(
                    crate::def_decl::E_DEF_DECL,
                    "def names must be strings".to_string(),
                ));
            }
        }
        Some(_) => diags.push(err(
            crate::def_decl::E_DEF_DECL,
            "`defs` must be a mapping of name to declaration".to_string(),
        )),
    }
    for (name, def) in typed.defs.iter_mut() {
        match crate::def_decl::lift_def(name, def) {
            Ok(lifted) => *def = lifted,
            Err(message) => diags.push(err_at(
                crate::def_decl::E_DEF_DECL,
                message,
                meta_key_span(meta, name),
            )),
        }
    }
    // A def is read bare as `@name` and its params bare in its body, so each
    // is an identifier (E-PATH-IDENT). Imported-schema defs are
    // checked when their own doc is parsed (`MetaKind::Schema`), so both
    // inline and imported defs are covered.
    for (name, def) in &typed.defs {
        if let Some(message) = lute_manifest::ident::ident_fault("def", name, "@") {
            diags.push(err_at(E_PATH_IDENT, message, meta_key_span(meta, name)));
        }
        if let Some(params) = def.get("params").and_then(|p| p.as_mapping()) {
            for pname in params.keys().filter_map(|k| k.as_str()) {
                let what = format!("def `{name}` param");
                if let Some(message) = lute_manifest::ident::ident_fault(&what, pname, "") {
                    diags.push(err_at(E_PATH_IDENT, message, meta_key_span(meta, pname)));
                }
            }
        }
    }
    typed.components = get_ref_list(map, "components");
    typed.component = get_str(map, "component");
    let (params, speakers, defaults, params_malformed) = get_params(map, "params");
    if let Some(raw_params) = map.get(yaml_key("params")).and_then(|v| v.as_mapping()) {
        for (name, value) in raw_params {
            let Some(name) = name.as_str() else { continue };
            let ty = value.get("type").unwrap_or(value);
            if ty.as_str() == Some("number") {
                diags.push(err_at(
                    "E-STATE-DECL",
                    format!(
                        "invalid component parameter `{name}`: {}",
                        lute_manifest::types::NUMBER_TYPE_REMOVED
                    ),
                    meta_path_span(meta, &["params", name]),
                ));
            }
        }
    }
    // A component (and so beat-template) param is read bare as `@name`: an
    // identifier.
    for p in &params {
        if let Some(message) = lute_manifest::ident::ident_fault("component param", &p.name, "@") {
            diags.push(err_at(
                "E-COMPONENT-PARSE",
                message,
                meta_path_span(meta, &["params", &p.name]),
            ));
        }
    }
    typed.params = params;
    typed.speaker_params = speakers;
    typed.param_defaults = defaults;
    typed.params_malformed = params_malformed;
    match map.get(yaml_key("effects")) {
        None => {}
        Some(serde_yaml::Value::Bool(b)) => typed.effects = *b,
        Some(v) => diags.push(err_at(
            "E-META-VALUE",
            format!(
                "`effects:` must be `true` or `false`, got {} (dsl 0.24.0 §4)",
                yaml_shape(v)
            ),
            meta_key_span(meta, "effects"),
        )),
    }
    // dsl 0.27.0 §6: a component's `beat:` header template. dsl 0.28.0 §1:
    // a param named like a key of the use itself can never be passed, and
    // is refused where it is declared (the template is then faulty, so its
    // uses stay silent about it).
    if kind == MetaKind::Component {
        let template = map.get(yaml_key("beat"));
        let reserved = crate::templates::check_param_names(meta, &typed.params, template.is_some());
        if let Some(v) = template {
            let (template, tdiags) = crate::templates::parse_beat_template(meta, v, &typed.params);
            typed.beat_template = template.map(|mut t| {
                t.faulty |= !reserved.is_empty();
                t
            });
            diags.extend(tdiags);
        }
        diags.extend(reserved);
    }

    super::frontmatter_state::parse_state(meta, map, &mut typed, &mut diags);
    // dsl 0.28.0 §1: reserved names are refused where they are declared.
    diags.extend(crate::reserved_names::check_frontmatter(
        meta, map, &mut typed,
    ));

    (typed, diags)
}
