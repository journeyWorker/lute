//! Environment folding: the frontmatter/schema/def/vocabulary fold
//! ([`fold_env`]) plus the implicit `<branch>`/`<hub>` recording decls and
//! plugin directive result slots it folds into the schema.

use super::*;

/// Extract ordered `(param name, Type)` pairs from an inline/imported def's raw
/// YAML value (dsl §8.1). Reads the def's `params:` sub-MAPPING in SOURCE order
/// (`yaml_serde::Mapping` is insertion-ordered), deserializing each value to a
/// `Type` via the same serde path `Type` uses. An absent/non-mapping `params:`
/// or a malformed entry yields no pair — never a panic.
pub fn params_from_yaml(v: &yaml_serde::Value) -> Vec<(String, lute_manifest::types::Type)> {
    let Some(map) = v.get("params").and_then(|p| p.as_mapping()) else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(k, tv)| {
            let name = k.as_str()?.to_string();
            let ty = yaml_serde::from_value::<lute_manifest::types::Type>(tv.clone()).ok()?;
            Some((name, ty))
        })
        .collect()
}

/// The document kind (which body walk applies) and frontmatter kind (which
/// keys the meta parse admits) for `doc`, plus the kind diagnostics (dsl 0.2.0
/// §3.1). Unresolved defaults to `Scene`, the degrade-safe path; a fragment
/// opened standalone takes its import role from its shape. One rule for
/// [`fold_env`] and every caller that lifts frontmatter on its own
/// ([`crate::ProjectDocs::parse`]), so both see the same typed metadata.
pub(crate) fn resolve_kinds(
    doc: &Document,
    defaults: &lute_manifest::project::MetaDefaults,
) -> (crate::meta::DocKind, crate::meta::MetaKind, Vec<Diagnostic>) {
    use crate::meta::{DocKind, MetaKind};
    let (resolved_kind, kind_diags) =
        crate::meta::resolve_doc_kind_with_defaults(&doc.meta, defaults);
    let has_body = !doc.sections.is_empty()
        || !doc.quests.is_empty()
        || !doc.entries.is_empty()
        || !doc.beats.is_empty();
    match resolved_kind {
        Some(DocKind::Scene) => (DocKind::Scene, MetaKind::Scene, kind_diags),
        Some(DocKind::Quest) => (DocKind::Quest, MetaKind::Quest, kind_diags),
        // dsl 0.19.0 §2: a lore document takes the quest-document frontmatter
        // keys (`MetaKind::Lore` mirrors `MetaKind::Quest`'s key set).
        Some(DocKind::Lore) => (DocKind::Lore, MetaKind::Lore, kind_diags),
        None => match crate::meta::infer_meta_kind_from_shape(&doc.meta, has_body) {
            // A fragment opened standalone: validate in its import role, drop the
            // root-only E-KIND-MISSING/E-META-MISSING false positives. Any body
            // (e.g. a component's `## Scene`) still walks as `DocKind::Scene` —
            // the same degrade-safe shape as the genuine-missing-kind default.
            Some(mk) => (DocKind::Scene, mk, Vec::new()),
            None => (DocKind::Scene, MetaKind::Scene, kind_diags), // genuine missing kind
        },
    }
}

/// Fold the analysis environment from an already-parsed document. Returns two
/// diagnostic streams kept SEPARATE so `check()` preserves its exact diagnostic
/// byte-order contract (a stable sort on `(byte_start, code)` makes same-span
/// ties order-sensitive): `.1` = the pre-import fold diags (meta + branch
/// dup/choice-dup) emitted just before import diags, and `.2` = the state-merge
/// diags (`E-EXTENDS-STATE-TYPE`/`E-STATE-REDECLARE`) emitted AFTER component
/// validation. Pure and total; never panics.
///
/// spec 0.49.0 §9.2: the stages that read only project-wide data reuse the
/// vocab's results ([`crate::vocab::BaseFold`]) while this document changes
/// none of their inputs, and the state is the vocab's base state plus this
/// document's overlay. The result equals [`fold_env_full_merge`]'s.
pub fn fold_env(
    doc: &Document,
    input: &CheckInput,
) -> (FoldedEnv, Vec<Diagnostic>, Vec<Diagnostic>) {
    fold_env_in(doc, input, Some(&input.vocab.fold))
}

/// [`fold_env`] with nothing shared: every stage runs over this document's
/// own merged copy of the imports, as 0.48.1 did — the reference the
/// shared fold is proven equal to (spec 0.49.0 §9.2).
#[doc(hidden)]
pub fn fold_env_full_merge(
    doc: &Document,
    input: &CheckInput,
) -> (FoldedEnv, Vec<Diagnostic>, Vec<Diagnostic>) {
    fold_env_in(doc, input, None)
}

/// Copies of `diags` anchored at `at` ([`crate::schema_import::AT_PLACEHOLDER`]).
fn anchored(diags: &[Diagnostic], at: Span) -> Vec<Diagnostic> {
    let mut out = diags.to_vec();
    crate::schema_import::anchor_at(&mut out, at);
    out
}

/// Whether the document declares anything `build_rel_vocab` /
/// `merge_domains` read beside the imports: inline vocabulary, an inline
/// `per:` family over its own kind, or a name whose imported origin it
/// shadows. A document that declares none of them gets the vocab's result.
fn changes_rel(typed: &crate::meta::TypedMeta, imports: &SchemaImports) -> bool {
    let k = &typed.rel_kinds;
    let r = &typed.rel_relations;
    !(k.kinds.is_empty()
        && k.order.is_empty()
        && k.dups.is_empty()
        && k.adds.is_empty()
        && k.add_labels.is_empty()
        && k.label_problems.is_empty()
        && r.relations.is_empty()
        && r.dups.is_empty()
        && typed.domains.is_empty()
        && typed.rel_rules.is_empty()
        && typed.rel_facts.is_empty()
        && typed.rel_rule_failed_heads.is_empty()
        && typed.state_index.is_empty())
        || typed
            .defs
            .keys()
            .any(|n| imports.rel.origins.defs.contains_key(n))
        || typed
            .state
            .decls
            .keys()
            .any(|p| imports.rel.origins.state.contains_key(&p))
}

/// The relational stage (dsl 0.3.0 §3/§4, data-catalog foundation A4): the
/// merged domains and the merged, validated relational vocabulary, with the
/// per-rule Datalog checks and the stratification. Returns the domain map,
/// the vocab (rule guards not yet expanded) and the diagnostics in fold
/// order.
fn rel_stage(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    plugin_origins: &crate::rel_schema::PluginOrigins,
    typed: &crate::meta::TypedMeta,
    meta: &lute_syntax::ast::Meta,
    at: Span,
) -> (
    std::collections::BTreeMap<String, Domain>,
    crate::rel_schema::RelVocab,
    Vec<Diagnostic>,
) {
    // The FULL merged domain vocabulary: `snapshot.domains` (core baseline +
    // active-plugin `enums`) UNION the project's domains — the schema
    // imports AND this document's inline `enums:`/`entities:` projection
    // (`typed.domains`), the same value `build_rel_vocab` gets, so the domain
    // map and `RelVocab` never disagree about a name (A3's `merge_domains`).
    let (mut domains, mut diags) = merge_domains(snapshot, imports, typed, at);
    let (mut vocab, rel_diags) = crate::rel_schema::build_rel_vocab(imports, typed, &domains, meta);
    vocab.effect_directives = crate::directive_facts::effect_directives(snapshot);
    vocab.effect_origins = plugin_origins.effect_facts.clone();
    // dsl 0.26.0 §2.3: a project kind's domain is the kind's final member
    // list — this document's `add:`s and sub-kinds included — and (dsl 0.27.0
    // §7) its final labels, so a `{ domain: K }` path renders them.
    for (name, decl) in &vocab.kinds {
        if let (lute_manifest::relations::KindShape::Members(ms), Some(d)) =
            (&decl.shape, domains.get_mut(name))
        {
            if !snapshot.domains.contains_key(name) {
                if d.members.len() != ms.len() {
                    d.members = ms.clone();
                }
                let labels: std::collections::BTreeMap<String, String> = decl
                    .labels
                    .iter()
                    .map(|(m, l)| (m.clone(), l.text.clone()))
                    .collect();
                if d.labels != labels {
                    d.labels = labels;
                }
            }
        }
    }
    diags.extend(rel_diags);
    // Per-rule Datalog checks (dsl 0.3.0 §7.1/§7.2): heads, body atoms,
    // safety — over the MERGED rule set.
    diags.extend(crate::datalog_check::check_rules(&vocab, &domains));
    // Negation-cycle stratification + the guard-taint closure; fills
    // `guard_tainted` before the vocab is frozen into `Env`'s `Arc`.
    diags.extend(crate::datalog_check::check_stratification(&mut vocab));
    (domains, vocab, diags)
}

/// dsl 0.23.0 §6: the reserved, read-only `prev.run.<path>` mirror of every
/// declared `run.<path>` among `decls` — same type, no default. A `run.*`
/// family's mirror is the `prev.run.*` family over the same members.
fn prev_run_mirrors(decls: &crate::meta::StateDecls) -> crate::meta::StateDecls {
    let mirror = |decl: &crate::meta::StateDecl| crate::meta::StateDecl {
        ty: decl.ty.clone(),
        default: None,
        namespace: crate::meta::Namespace::User,
        owner: Some(lute_manifest::types::Owner::Engine),
    };
    let mut out = crate::meta::StateDecls::default();
    for (path, family) in decls.families() {
        // `<path>.<m>` is a `run.*` path exactly when `path` is `run` or a
        // `run.*` path.
        if path == "run" || path.starts_with("run.") {
            out.insert_family(
                format!("prev.{path}"),
                family.map_decl(mirror),
                crate::meta::Precedence::Replace,
            );
        }
    }
    for (path, decl) in decls.explicit() {
        if let Some(prev) = lute_manifest::semantics::cel_paths::prev_run_path(path) {
            out.insert(prev, mirror(decl));
        }
    }
    out
}

/// The def tables (dsl §8.1), each merged plugin < imported < inline: the
/// produced types (`E-REF-TYPE`), the ordered params (`@name(args)` arity),
/// and the raw CEL bodies (the D4 expander). A component's declared params
/// are 0-arity value refs of its own body (`params`).
type DefTables = (
    std::collections::BTreeMap<String, Type>,
    std::collections::BTreeMap<String, Vec<(String, Type)>>,
    std::collections::BTreeMap<String, String>,
);

fn def_tables<'a>(
    snapshot: &CapabilitySnapshot,
    defs: impl Iterator<Item = &'a std::collections::BTreeMap<String, yaml_serde::Value>> + Clone,
    params: &[lute_manifest::schema::DefParam],
) -> DefTables {
    let settled = |v: &yaml_serde::Value| {
        v.get("type")
            .filter(|_| !crate::def_decl::is_invalid(v))
            .cloned()
            .and_then(|tv| yaml_serde::from_value::<Type>(tv).ok())
    };
    // Plugin defs are already typed; imported and inline defs are untyped
    // YAML whose `type:` `settle_defs` settled — a def whose type could not
    // be settled has no entry (never a panic); `E-DEF-DECL` names it.
    let mut def_types: std::collections::BTreeMap<String, Type> = snapshot
        .defs
        .iter()
        .map(|(name, d)| (name.clone(), d.ty.clone()))
        .collect();
    let mut def_params: std::collections::BTreeMap<String, Vec<(String, Type)>> = snapshot
        .defs
        .iter()
        .map(|(name, d)| {
            let ps = d.params.iter().map(|p| (p.name.clone(), p.ty.clone()));
            (name.clone(), ps.collect())
        })
        .collect();
    let mut def_bodies: std::collections::BTreeMap<String, String> = snapshot
        .defs
        .iter()
        .map(|(name, d)| (name.clone(), d.cel.clone()))
        .collect();
    for table in defs {
        for (name, v) in table {
            if let Some(t) = settled(v) {
                def_types.insert(name.clone(), t);
            }
            def_params.insert(name.clone(), params_from_yaml(v));
            if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
                def_bodies.insert(name.clone(), c.to_string());
            }
        }
    }
    // A standalone component body's `@param` use in a typed attr slot is
    // type-checked, and a bare `@p` is well-formed while `@p(x)` is
    // `E-REF-ARITY` — the SAME entries `component_env` seeds for the
    // transitive `::use` path.
    for param in params {
        def_types.insert(param.name.clone(), param.ty.clone());
        def_params.insert(param.name.clone(), Vec::new());
    }
    (def_types, def_params, def_bodies)
}

/// 0.41.0 §3.5: the defs a document sees that declare `host: true` — a
/// schema file's or a plugin's.
fn host_defs(
    snapshot: &CapabilitySnapshot,
    imported: &std::collections::BTreeMap<String, yaml_serde::Value>,
) -> std::collections::BTreeSet<String> {
    snapshot
        .defs
        .values()
        .filter(|d| d.host)
        .map(|d| d.name.clone())
        .chain(
            imported
                .iter()
                .filter(|(_, v)| crate::def_decl::is_host(v))
                .map(|(n, _)| n.clone()),
        )
        .collect()
}

/// The `E-DEF-DECL` of an imported def, reported at the def's schema line
/// and folded across importers (Prerelease N5, dsl 0.26.0 §2.7).
fn imported_def_diag(imports: &SchemaImports, name: &str, message: String, at: Span) -> Diagnostic {
    crate::rel_schema::at_origin(
        Diagnostic {
            code: crate::def_decl::E_DEF_DECL.to_string(),
            severity: Severity::Error,
            message,
            evidence: None,
            span: at,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        },
        imports.rel.origins.defs.get(name),
    )
}

/// An `E-STATE-DECL` / `E-DOMAIN-UNKNOWN` at `span`.
fn state_decl_report(code: &str, message: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message: message.to_string(),
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The plugin directives storing results under `value` (dsl 0.28.0 T3-45).
fn results_under(snapshot: &CapabilitySnapshot, value: &str) -> Vec<String> {
    snapshot
        .directives
        .iter()
        .filter(|(_, d)| {
            d.state.iter().flat_map(|s| &s.declares).any(|slot| {
                let mut base = slot.scope.clone();
                for seg in &slot.path {
                    let lute_manifest::types::PathSegment::Literal(s) = seg else {
                        break;
                    };
                    base = format!("{base}.{s}");
                }
                base == value
                    || base.starts_with(&format!("{value}."))
                    || value.starts_with(&format!("{base}."))
            })
        })
        .map(|(tag, _)| tag.clone())
        .collect()
}

/// The project-wide fold stages over `imports`, run once per vocab (spec
/// 0.49.0 §9.2): what [`fold_env`] reuses for a document that changes none
/// of their inputs. The frontmatter-reading stages run against an empty
/// frontmatter spanning [`crate::vocab::KEY_PLACEHOLDER`]; a diagnostic that
/// carries it depends on the document, and the stage is then rerun per
/// document.
pub(crate) fn base_fold(
    base: &crate::vocab::ProjectBase,
    imports: &SchemaImports,
) -> crate::vocab::BaseFold {
    use crate::schema_import::AT_PLACEHOLDER;
    use crate::vocab::KEY_PLACEHOLDER;
    use std::sync::Arc;
    let snapshot = &*base.snapshot;
    let meta = lute_syntax::ast::Meta {
        raw_yaml: String::new(),
        span: KEY_PLACEHOLDER,
    };
    let typed = crate::meta::TypedMeta::default();
    let (domains_pre_clock, mut rel, rel_diags) = rel_stage(
        snapshot,
        imports,
        &base.plugin_origins,
        &typed,
        &meta,
        AT_PLACEHOLDER,
    );
    let mut state = imports.state.clone();
    let mirrors = prev_run_mirrors(&state.decls);
    state.decls.append(mirrors);
    // The clock the imports declare, settled over the base state: its enums
    // join the domains a document that changes nothing sees.
    let clocks = imported_clocks(imports);
    let view = crate::StateView::from(state);
    let (clock, _) = crate::clock::check_clock(
        &clocks,
        &view,
        &domains_pre_clock,
        &snapshot.occasions,
        AT_PLACEHOLDER,
    );
    let mut state = view.into_overlay();
    let mut domains = domains_pre_clock.clone();
    if let Some(clock) = &clock {
        for (name, domain) in crate::clock::clock_domains(clock) {
            domains.entry(name).or_insert(domain);
        }
    }
    state.resolve_domains(&domains);
    // By domain, the explicit paths and the families typed with it.
    let by_domain = |decls: &mut dyn Iterator<Item = (&String, &crate::meta::StateDecl)>| {
        let mut out: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for (path, decl) in decls {
            if let Type::Domain(name) | Type::Entity(name) = &decl.ty {
                out.entry(name.clone()).or_default().push(path.clone());
            }
        }
        out
    };
    let paths_by_domain = by_domain(&mut state.decls.explicit().iter());
    let families_by_domain = by_domain(
        &mut state
            .decls
            .families()
            .iter()
            .map(|(path, family)| (path, family.decl())),
    );
    let view = crate::StateView::over(Arc::new(state));
    // The imported defs, settled against the base state.
    let mut imported_defs = imports.defs.clone();
    let (imported_def_msgs, _) = crate::def_decl::settle_defs(
        &mut imported_defs,
        &mut std::collections::BTreeMap::new(),
        snapshot.defs.iter().map(|(n, d)| (n.clone(), d.ty.clone())),
        &view,
    );
    let def_diags = imported_def_msgs
        .into_iter()
        .map(|(name, msg)| imported_def_diag(imports, &name, msg, AT_PLACEHOLDER))
        .collect();
    let mut def_reads = std::collections::BTreeSet::new();
    for v in imports.defs.values() {
        let Some(cel) = v.get("cel").and_then(|c| c.as_str()) else {
            continue;
        };
        let mut arena = CelArena::default();
        if let Some(root) =
            lute_cel::parse_slot_marked_refs(&mut arena, cel).and_then(|h| arena.get(h))
        {
            crate::cel_paths::every_static_path(&root.expr, &mut def_reads);
        }
    }
    let (def_types, def_params, def_bodies) =
        def_tables(snapshot, std::iter::once(&imported_defs), &[]);
    let pre_guard_rules = rel.rules.clone();
    let guards_use_defs = rel.rules.iter().any(|r| {
        r.rule.body.iter().any(|lit| {
            matches!(lit, lute_syntax::datalog::BodyLiteral::Guard { cel, .. } if cel.contains('@'))
        })
    });
    let guard_diags = crate::cel_resolve::expand_rule_guards(
        &mut rel,
        &DefTable {
            bodies: &def_bodies,
            params: &def_params,
        },
    );
    let origins = &imports.rel.origins.state;
    let value_prefix = crate::state_decls::value_prefix_by_path(
        &view,
        &crate::state_decls::ValuePrefixSite {
            declared: &|path| origins.contains_key(path),
            results_under: &|value| results_under(snapshot, value),
            locate: &|path, message| {
                let origin = origins.get(path)?;
                let d = state_decl_report("E-STATE-DECL", message, AT_PLACEHOLDER);
                Some(crate::rel_schema::at_origin(d, Some(&origin)))
            },
        },
    );
    let domain_types = crate::state_decls::domain_types_by_at(
        &view,
        &domains,
        &rel.indexed_state,
        &|path, _, message, code| {
            // Its schema line, or the first member's for a `per:` family.
            let origin = origins.at_or_under(path)?;
            let d = state_decl_report(code, message, AT_PLACEHOLDER);
            Some(crate::rel_schema::at_origin(d, Some(&origin)))
        },
    );
    // Guard expansion reads no frontmatter: its diagnostics are the rules'.
    let rel_shared = !crate::vocab::carries_span(&rel_diags, KEY_PLACEHOLDER);
    let mut template_domains = snapshot.domains.clone();
    template_domains.extend(imports.domains.clone());
    crate::vocab::BaseFold {
        domains_pre_clock: Arc::new(domains_pre_clock),
        domains: Arc::new(domains),
        clock,
        rel: Arc::new(rel),
        pre_guard_rules,
        guards_use_defs,
        rel_diags,
        guard_diags,
        rel_shared,
        view,
        paths_by_domain,
        families_by_domain,
        imported_defs: Arc::new(imported_defs),
        def_diags,
        def_reads,
        def_types,
        def_params,
        def_bodies,
        value_prefix,
        domain_types,
        template_domains: Arc::new(template_domains),
    }
}

/// The clocks the imports declare, each with where it is declared.
fn imported_clocks(
    imports: &SchemaImports,
) -> Vec<(crate::clock::ClockSite, lute_manifest::clock::ClockDecl)> {
    imports
        .clock
        .iter()
        .map(|(path, c, at)| {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let at = Some((path.display().to_string(), *at));
            (crate::clock::ClockSite { name, at }, c.clone())
        })
        .collect()
}

/// Whether a path the document declares is one an imported def body names,
/// or lies above or below one — its type decision may differ from the
/// base's (spec 0.49.0 §9.2 R3).
fn touches_def_reads(state: &crate::StateView, reads: &std::collections::BTreeSet<String>) -> bool {
    state.overlay().decls.keys().any(|path| {
        if reads.contains(&*path) {
            return true;
        }
        let mut end = path.len();
        while let Some(dot) = path[..end].rfind('.') {
            end = dot;
            if reads.contains(&path[..end]) {
                return true;
            }
        }
        let below = format!("{path}.");
        reads
            .range::<str, _>((
                std::ops::Bound::Included(below.as_str()),
                std::ops::Bound::Unbounded,
            ))
            .next()
            .is_some_and(|r| r.starts_with(&below))
    })
}

/// Merges one inline state declaration into `schema` (dsl §9.2). Precedence
/// depends on where the imported declaration came from
/// ([`SchemaImports::is_overridable`]): a `uses`-peer path may not be
/// redeclared (`E-STATE-REDECLARE`, the imported one stays), an
/// `extends`-base path is refined by the inline declaration (a type change
/// is `E-EXTENDS-STATE-TYPE`: persisted state keeps a stable type), and a
/// path no import declares joins the document's state.
fn merge_inline_decl(
    schema: &mut crate::StateView,
    imports: &SchemaImports,
    path: &str,
    decl: &crate::meta::StateDecl,
    at: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let report = |code: &str, message: String| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span: at,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    match imports.state.decls.get(path) {
        Some(imported) if imports.is_overridable(path) => {
            if decl.ty != imported.ty {
                diags.push(report(
                    "E-EXTENDS-STATE-TYPE",
                    format!(
                        "state path `{path}` overrides base declared type {:?} with {:?}; persisted state must keep a stable type",
                        imported.ty, decl.ty
                    ),
                ));
            }
            schema.insert(path.to_string(), decl.clone());
        }
        Some(_) => diags.push(report(
            "E-STATE-REDECLARE",
            format!(
                "state path `{path}` is declared by an imported schema (§9.2); a scene must not redeclare or override it"
            ),
        )),
        None => schema.insert(path.to_string(), decl.clone()),
    }
}

fn fold_env_in(
    doc: &Document,
    input: &CheckInput,
    shared: Option<&crate::vocab::BaseFold>,
) -> (FoldedEnv, Vec<Diagnostic>, Vec<Diagnostic>) {
    use std::sync::Arc;
    let snapshot = input.snapshot();
    let imports = input.imports();
    let at = doc.meta.span;
    // 3. Resolve the root document kind (dsl 0.2.0 §3.1) FIRST: it gates which
    //    per-kind frontmatter keys the meta parse below allows. Defaults to
    //    `Scene` — the degrade-safe path — when unresolved (missing/unknown
    //    `kind:`), so a mis-kinded doc still gets the scene-triad required-key
    //    treatment it had pre-0.2.0.
    let (doc_kind, meta_kind, kind_diags) = resolve_kinds(doc, input.defaults());

    // 3b. Typed frontmatter + inline state schema, dispatched by the resolved
    //     kind (dsl 0.2.0 §3.1, §6.1): a Quest doc carries none of the scene
    //     triad and rejects it as an unknown key.
    let (mut typed, mut fold_diags) = crate::meta::parse_meta_kind_with_defaults(
        &doc.meta,
        snapshot,
        meta_kind,
        input.defaults(),
    );
    fold_diags.splice(0..0, kind_diags);
    // dsl 0.21.0 §3.1: a scene beat's `when` slot is lifted from the raw
    // frontmatter with a byte-only span; give it its line/column here, where
    // the document text is at hand, so a fact provenance citing the guard
    // (`crate::fact_must`) names the right line. dsl 0.27.0 §5: `spentBy`
    // likewise.
    if let Some(b) = typed.beat.as_mut() {
        let idx = TextIndex::new(&input.text);
        for slot in [b.when.as_mut(), b.spent_by.as_mut()].into_iter().flatten() {
            let end = slot.span.byte_end.min(input.text.len());
            let start = slot.span.byte_start.min(end);
            if input.text.is_char_boundary(start) && input.text.is_char_boundary(end) {
                slot.span = Span::from_bytes(&idx, start, end);
            }
        }
    }

    // 3c. The merged domain vocabulary and relational vocabulary
    //     ([`rel_stage`]), computed ONCE here so `lute-compile` (which calls
    //     `fold_env` directly) sees the SAME vocabulary, never double-emitting
    //     `E-DOMAIN-DUP`. The vocab's own result when this document declares
    //     nothing they read; its diagnostics too, unless one of them
    //     depends on the frontmatter text (then the stage reruns for them).
    //     `rel_from_base`: the vocab's rule guards are already expanded.
    let rel_from_base = shared.filter(|_| !changes_rel(&typed, imports));
    let (domains_pre, mut vocab) = match rel_from_base {
        Some(b) => {
            if b.rel_shared {
                fold_diags.extend(anchored(&b.rel_diags, at));
            } else {
                let (_, _, diags) = rel_stage(
                    snapshot,
                    imports,
                    input.plugin_origins(),
                    &typed,
                    &doc.meta,
                    at,
                );
                fold_diags.extend(diags);
            }
            (Arc::clone(&b.domains_pre_clock), Arc::clone(&b.rel))
        }
        None => {
            let (domains, vocab, diags) = rel_stage(
                snapshot,
                imports,
                input.plugin_origins(),
                &typed,
                &doc.meta,
                at,
            );
            fold_diags.extend(diags);
            (Arc::new(domains), Arc::new(vocab))
        }
    };

    // 4. Merge imported schema (dsl §9.2) first, then the scene's inline
    //    `state:`. Precedence depends on WHERE the imported decl came from
    //    (see `SchemaImports::state_overridable`): a `uses`-peer path may NOT
    //    be redeclared (`E-STATE-REDECLARE`, imported wins), but an
    //    `extends`-base path MAY be refined by the scene's inline decl (the
    //    inline wins; a TYPE change is `E-EXTENDS-STATE-TYPE`, the persisted
    //    type must stay stable). Shared: the imports are the vocab's base
    //    state and every decl below goes to this document's overlay.
    let mut schema = match shared {
        Some(b) => b.view.clone(),
        None => crate::StateView::from(imports.state.clone()),
    };
    let mut state_merge_diags: Vec<Diagnostic> = Vec::new();
    // An inline `per:` family no imported declaration names a member of
    // joins whole (spec 0.49.0 §9.3); otherwise every path is merged one by
    // one, in path order.
    let whole = typed
        .state
        .decls
        .families()
        .iter()
        .all(|(path, family)| !imports.state.decls.declares_member_of(path, family));
    let inline: Box<dyn Iterator<Item = (std::borrow::Cow<'_, str>, &crate::meta::StateDecl)>> =
        if whole {
            for (path, family) in typed.state.decls.families() {
                schema.insert_family(path.clone(), family.clone());
            }
            Box::new(
                typed
                    .state
                    .decls
                    .explicit()
                    .iter()
                    .map(|(k, d)| (std::borrow::Cow::Borrowed(k.as_str()), d)),
            )
        } else {
            Box::new(typed.state.decls.iter())
        };
    for (path, decl) in inline {
        merge_inline_decl(
            &mut schema,
            imports,
            &path,
            decl,
            at,
            &mut state_merge_diags,
        );
    }
    {
        let overlay = schema.overlay_mut();
        overlay.faulty.extend(typed.state.faulty.iter().cloned());
        overlay.clock_rejected |= typed.state.clock_rejected;
    }
    {
        let mut own = crate::meta::StateSchema {
            invalid: typed.state.invalid.clone(),
            ..Default::default()
        };
        let file = std::path::Path::new(input.uri.trim_start_matches("file://"))
            .file_name()
            .map_or_else(|| input.uri.clone(), |n| n.to_string_lossy().into_owned());
        own.place_invalid(&file);
        for (path, decl) in own.invalid {
            if schema.invalid(&path).is_none() {
                schema.overlay_mut().invalid.insert(path, decl);
            }
        }
    }
    // dsl 0.28.0: an inline `per:` over a kind an imported schema declares.
    let (per_families, per_index, per_faults) = crate::meta::expand_per_pending(
        &typed.per_pending,
        &vocab.kinds,
        &mut std::collections::BTreeMap::new(),
    );
    for (path, family) in per_families {
        if !schema.declares_member_of(&path, &family) {
            schema.insert_family(path, family);
            continue;
        }
        // #190: a member this document declares itself keeps that
        // declaration (merged above); every other member is merged against
        // the imports as an inline path is — a `uses`-peer redeclaration is
        // refused, an `extends` refinement applies with its type guarded.
        for m in family.members() {
            let key = format!("{path}.{m}");
            if typed.state.decls.contains_key(&key) {
                continue;
            }
            let decl = family.member(m).unwrap_or(family.decl());
            merge_inline_decl(&mut schema, imports, &key, decl, at, &mut state_merge_diags);
        }
    }
    if !per_index.is_empty() {
        Arc::make_mut(&mut vocab).indexed_state.extend(per_index);
    }
    fold_diags.extend(per_faults);
    // Fold every `<branch>`/`<hub>`'s implicit recording decls
    // (`scene.choices.<id>` + a hub's per-choice `scene.visited.<id>.*`) into
    // the schema BEFORE the checks that resolve against them. This pre-pass
    // owns the episode-wide `E-DUP-BRANCH` detection (hub + branch ids share
    // one domain) so the main walk never double-counts ids.
    let mut seen_branches = std::collections::BTreeSet::new();
    fold_branches(
        doc,
        schema.overlay_mut(),
        &mut seen_branches,
        &mut fold_diags,
    );

    // 4a. Fold every `<quest>`'s implicit reserved `quest.<id>.*` decls (dsl
    //     0.2.0 §5.2), threading a per-document `seen` id set SEEDED from
    //     the import-reachable quest ids (dsl 0.2.0 §6.3: quest ids are
    //     unique PROJECT-WIDE), so `E-QUEST-ID-DUP` fires exactly once per
    //     duplicate. `quest.<id>.state` / `quest.<id>.objectives.<oid>.done`
    //     are RESERVED (dsl 0.2.0 §5.2/§9.3): a reserved path colliding with
    //     a path that existed BEFORE this loop (inline or imported) is
    //     `E-QUEST-RESERVED-DECL`; a same-id repeat re-inserts the identical
    //     decl unflagged, since the set is frozen before the loop.
    let pre_existing_overlay = schema.overlay().decls.clone();
    let pre_existing = |path: &str, schema: &crate::StateView| {
        pre_existing_overlay.contains_key(path) || schema.base().decls.contains_key(path)
    };
    let mut seen_quests: std::collections::BTreeSet<String> =
        imports.imported_quest_ids.keys().cloned().collect();
    for quest in &doc.quests {
        let record = check_quest(quest, &mut seen_quests);
        for (path, decl) in record.decls {
            if pre_existing(&path, &schema) {
                fold_diags.push(Diagnostic {
                    code: "E-QUEST-RESERVED-DECL".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "state path `{path}` collides with an implicitly-declared reserved \
                         quest field (dsl 0.2.0 §5.2); it must not be author-declared in \
                         `state:`"
                    ),
                    evidence: None,
                    span: at,
                    layer: Layer::Content,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                });
            } else {
                schema.insert(path, decl);
            }
        }
        fold_diags.extend(record.diags);
        // dsl 0.16.0 §2/§4/§6: reward shape/closure/vocabulary, alongside
        // the rest of the quest fold.
        fold_diags.extend(check_quest_rewards(
            quest,
            snapshot,
            input.providers(),
            &vocab.kinds,
        ));
    }
    // dsl 0.23.0 §6: a subquest's `tier` equals its parent's — the
    // same-document half of `E-QUEST-TIER-MIX`.
    fold_diags.extend(crate::project_check::check_doc_quest_tiers(doc));

    // 4a'. Every `<entry>`'s implicit reserved `entry.<id>.read` decl (dsl
    //      0.19.0 §5) and its attribute / identity diagnostics — the lore
    //      mirror of the quest fold above, at the RESOLVED positions (a lore
    //      document's `series:` orders its entries by place in the file).
    let mut seen_entries: std::collections::BTreeSet<String> =
        imports.imported_entry_ids.keys().cloned().collect();
    let entry_record = crate::lore::check_entries(
        typed.series.as_deref(),
        &doc.entries,
        &mut seen_entries,
        snapshot,
    );
    for (path, decl) in entry_record.decls {
        schema.insert(path, decl);
    }
    fold_diags.extend(entry_record.diags);
    // 4a''. dsl 0.23.0 §6: `prev.run.<path>` mirrors every declared
    //       `run.<path>`. The base state holds the mirrors of the imported
    //       paths; this document's own `run.*` decls mirror here.
    let mirrors = prev_run_mirrors(&schema.overlay().decls);
    schema.overlay_mut().decls.append(mirrors);
    // 4a'''. dsl 0.24.0 §1: the project's clock — its imports' `clock:` and,
    //        for a schema document, its own — checked against the folded
    //        schema; its reserved read-only `clock.*` paths join the schema.
    //        A handful of lookups, so it runs for every document.
    let clocks: Vec<(crate::clock::ClockSite, lute_manifest::clock::ClockDecl)> =
        imported_clocks(imports)
            .into_iter()
            .chain(typed.clock.clone().map(|c| {
                (
                    crate::clock::ClockSite {
                        name: "this schema".to_string(),
                        at: None,
                    },
                    c,
                )
            }))
            .collect();
    // A schema document's own clock is reported at its `clock:` key.
    let clock_span = if typed.clock.is_some() {
        crate::meta::meta_key_span(&doc.meta, "clock")
    } else {
        at
    };
    let (clock, clock_diags) = crate::clock::check_clock(
        &clocks,
        &schema,
        &domains_pre,
        &snapshot.occasions,
        clock_span,
    );
    fold_diags.extend(clock_diags);
    // `{ domain: clock.slot }` / `{ domain: clock.weekdayLabel }` name the
    // clock's own enums: the vocab's domains when the clock is the vocab's.
    let domains = match shared
        .filter(|b| Arc::ptr_eq(&domains_pre, &b.domains_pre_clock) && clock == b.clock)
    {
        Some(b) => Arc::clone(&b.domains),
        None => match &clock {
            Some(c) => {
                let mut d = (*domains_pre).clone();
                for (name, domain) in crate::clock::clock_domains(c) {
                    d.entry(name).or_insert(domain);
                }
                Arc::new(d)
            }
            None => Arc::clone(&domains_pre),
        },
    };
    if let Some(clock) = &clock {
        let reserved = crate::clock::reserved_decls(clock, &schema);
        // dsl 0.24.0 §1 / 0.27.0 §4: `clock.weekday`, and a finite clock's
        // `clock.index` and day path, range over known whole numbers.
        let ranges = crate::clock::int_ranges(clock, &schema);
        // dsl 0.28.0 (T3-40): a clock that ends on its first day narrows
        // its slot path to the slots it reaches.
        let members = crate::clock::slot_members(clock, &schema);
        let overlay = schema.overlay_mut();
        overlay.decls.extend(reserved);
        overlay.int_ranges.extend(ranges);
        overlay.clock_members.extend(members);
        overlay.clock = Some(clock.clone());
    }
    // T3-1: a written `clock:` that was rejected is the cause; what needs a
    // clock is not judged against its absence.
    if clock.is_some() {
        schema.overlay_mut().clock_rejected = false;
    } else if !clocks.is_empty() {
        // 0.41.0 §3.2: a clock over an invalid state row is not checked.
        schema.overlay_mut().clock_rejected = true;
    }
    if !schema.clock_rejected() {
        fold_diags.extend(crate::clock::check_once_needs_clock(
            doc,
            typed.beat.as_ref(),
            clock.as_ref(),
        ));
    }
    // dsl 0.27.0 §5: the project's seasons — its imports' and, for a schema
    // document, its own — and every use of one.
    let season_span = if typed.seasons.is_empty() {
        at
    } else {
        crate::meta::meta_key_span(&doc.meta, "seasons")
    };
    let season_names: Vec<String> = imports
        .seasons
        .iter()
        .map(|(path, _, _)| {
            path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        })
        .collect();
    let (seasons, season_diags) = crate::season::fold_seasons(
        imports
            .seasons
            .iter()
            .zip(&season_names)
            .map(|((path, s, at), name)| {
                let origin = crate::rel_schema::DeclOrigin {
                    file: path.clone(),
                    span: *at,
                };
                (name.as_str(), s, Some(origin))
            })
            .chain(std::iter::once(("this schema", &typed.seasons, None))),
        season_span,
    );
    fold_diags.extend(season_diags);
    let (mirrors, season_diags) = crate::season::check_state(
        &schema,
        &seasons,
        crate::meta::meta_key_span(&doc.meta, "state"),
        &vocab.origins.state,
    );
    schema.overlay_mut().decls.append(mirrors);
    fold_diags.extend(season_diags);
    fold_diags.extend(crate::season::check_uses(
        doc,
        typed.beat.as_ref(),
        &seasons,
        input.defaults(),
    ));
    fold_diags.extend(crate::season::check_relation_tiers(
        &vocab,
        &seasons,
        crate::meta::meta_key_span(&doc.meta, "relations"),
    ));
    // dsl 0.21.0 §2: every entry beat's occasion against the vocabulary.
    fold_diags.extend(crate::beats::check_entry_occasions(
        &doc.entries,
        &snapshot.occasions,
    ));
    // dsl 0.23.0 §4: every bundle `<beat>`'s shape, id, and occasion.
    fold_diags.extend(crate::bundles::check_bundle_beats(
        typed.id.as_deref(),
        lute_manifest::yaml_text::key_span(&doc.meta.raw_yaml, &["id"]).is_some(),
        &doc.beats,
        &snapshot.occasions,
    ));
    // A bundle beat id repeated, or shared with an entry of the document.
    fold_diags.extend(crate::bundles::check_beat_ids(
        typed.id.as_deref(),
        &doc.entries,
        &doc.beats,
    ));
    // dsl 0.27.0 §6: a template component's header values every use derives
    // unchanged, judged once here at the header key.
    if let Some(template) = &typed.beat_template {
        fold_diags.extend(crate::templates::check_template_header(
            template,
            &snapshot.occasions,
        ));
    }
    // dsl 0.21.0 §7a.2: every objective's `on` occasion, checked like a beat's.
    fold_diags.extend(crate::beats::check_objective_occasions(
        &doc.quests,
        &snapshot.occasions,
    ));
    // dsl 0.22.0 §8: every beat target against its occasion's target domain.
    fold_diags.extend(crate::beats::check_beat_target_domains(
        doc,
        typed.beat.as_ref(),
        &snapshot.occasions,
        &vocab.kinds,
    ));
    // dsl 0.27.0 §3 (T2-10): every `for="kind:<kind>"` beat (a scene's `for:`).
    fold_diags.extend(crate::occasion_bind::check_for_kinds(
        doc,
        typed.beat.as_ref(),
        &snapshot.occasions,
        &vocab.kinds,
    ));
    // dsl 0.27.0 §3: a beat on an occasion declaring a typed `payload:`
    // reads each field as `occasion.payload.<field>` (engine-owned, bound
    // by the raise); a read in a beat of another occasion is `E-UNDECLARED`.
    let (payload_decls, payload_diags) =
        crate::occasion_bind::payload_decls(doc, typed.beat.as_ref(), &snapshot.occasions);
    for (path, ty) in payload_decls {
        schema.insert(
            path,
            crate::meta::StateDecl {
                ty,
                default: None,
                namespace: crate::meta::Namespace::Scene,
                owner: Some(lute_manifest::types::Owner::Engine),
            },
        );
    }
    fold_diags.extend(payload_diags);
    // dsl 0.26.0 §5 / 0.27.0 §3: a kind beat reads the member it was raised
    // for as `occasion.target`, typed by every kind its kind beats answer;
    // each beat's own slots are checked with its own kind's members
    // (`FoldedEnv::env_at`, built below).
    let occasion_scopes = crate::occasion_bind::occasion_scopes(
        doc,
        typed.beat.as_ref(),
        &snapshot.occasions,
        &vocab.kinds,
    );
    let occasion_members = occasion_scopes.members();
    let target_scoped = !occasion_members.is_empty();
    if target_scoped {
        schema.insert(
            lute_manifest::semantics::beats::OCCASION_TARGET.to_string(),
            crate::meta::StateDecl {
                ty: lute_manifest::types::Type::Enum(occasion_members),
                default: None,
                namespace: crate::meta::Namespace::Scene,
                owner: Some(lute_manifest::types::Owner::Engine),
            },
        );
    }
    // dsl 0.24.0 §2: every `<on event target>`, checked like an objective's.
    fold_diags.extend(crate::on::check_on_targets(
        &doc.quests,
        &snapshot.occasions,
        &vocab.kinds,
    ));
    // dsl 0.25.0 §6: every relation's `changedOn:` occasion, checked like one.
    fold_diags.extend(crate::rel_schema::check_changed_on(
        &vocab,
        &snapshot.occasions,
        &doc.meta,
    ));
    // 0.27 prerelease G-6: a `{ domain: K }` payload field names a declared K.
    fold_diags.extend(crate::occasion_bind::check_payload_domains(
        doc,
        typed.beat.as_ref(),
        &snapshot.occasions,
        &domains,
        input.plugin_origins(),
    ));
    // 0.49.0 §4.3: reference-typed plugin frontmatter values against the
    // merged vocabulary (entity kinds, domains, providers, asset kinds).
    fold_diags.extend(crate::meta::check_frontmatter_refs(
        &typed,
        &doc.meta,
        snapshot,
        input.providers(),
        &domains,
    ));

    // 4b. Expand every active directive's `state.declares[]` into concrete
    //     state slots at each use site (plugin §8/§9), before the walk +
    //     defassign so plugin-declared state resolves.
    fold_directive_slots(doc, snapshot, &input.components, schema.overlay_mut());
    // dsl 0.27.0 §2 (T1-2): a `{ domain: K }` / `{ entity: K }` path is
    // member-checked like an inline enum — its members, from the merged
    // domains, ride on the schema every pass reads. The base state's are
    // resolved against the vocab's domains; a base path whose domain this
    // document changed is resolved again (spec 0.49.0 §9.2 R1).
    schema.overlay_mut().resolve_domains(&domains);
    let changed_domains = match shared {
        Some(b) if !Arc::ptr_eq(&domains, &b.domains) => {
            crate::vocab::changed_domains(&b.domains, &domains)
        }
        _ => std::collections::BTreeSet::new(),
    };
    if let Some(b) = shared {
        for name in &changed_domains {
            for path in b.paths_typed_by(name) {
                if schema.overlay().decls.contains_key(&path) {
                    continue;
                }
                match domains.get(name).filter(|d| !d.open) {
                    Some(d) => {
                        let members = (name.clone(), d.members.clone());
                        schema
                            .overlay_mut()
                            .domain_members
                            .insert(path.into_owned(), members);
                    }
                    None => schema.mask_domain(path.into_owned()),
                }
            }
        }
    }

    // The def names the `@ref` resolver validates against (dsl §8.1): inline
    // frontmatter defs plus plugin-exported and imported defs.
    let mut defs: std::collections::BTreeSet<String> = typed.defs.keys().cloned().collect();
    defs.extend(snapshot.defs.keys().cloned());
    defs.extend(imports.defs.keys().cloned());
    // A component's declared `params:` ARE the `@param` ref namespace for its
    // OWN presentational body (dsl §13/§8.1), so a STANDALONE `lute check` of
    // the component file resolves its own `@param` refs. Guarded to
    // `MetaKind::Component`; the `def_types`/`def_params` tables are seeded
    // under the SAME guard below — keep all three in sync.
    let component_params: &[lute_manifest::schema::DefParam] =
        if meta_kind == crate::meta::MetaKind::Component {
            &typed.params
        } else {
            &[]
        };
    if meta_kind == crate::meta::MetaKind::Component {
        defs.extend(typed.params.iter().map(|p| p.name.clone()));
        // Round-5 T3-23: a literal param `default:` is judged once, here, at
        // the component's own `params:` entry — not at every `::use`.
        fold_diags.extend(check_param_literal_defaults(&typed, &doc.meta));
        // dsl 0.27.0 §6: a template condition's `@name` resolves here, once,
        // against the params and every def this component sees.
        if let Some(template) = &typed.beat_template {
            fold_diags.extend(crate::templates::check_template_refs(template, &defs));
        }
    }

    // dsl 0.21.0 §7b: settle every imported and inline def's produced type
    // against the FOLDED schema, in dependency order (Round-5 T3-14): an
    // absent `type:` is inferred from the body and written back, an
    // explicit one must agree with it. The vocab's settled imported defs
    // stand while this document declares no def and no path one of them
    // reads (spec 0.49.0 §9.2 R3); otherwise all of them settle again.
    let base_defs =
        shared.filter(|b| typed.defs.is_empty() && !touches_def_reads(&schema, &b.def_reads));
    let (imported_defs, def_tables) = match base_defs {
        Some(b) => {
            fold_diags.extend(anchored(&b.def_diags, at));
            let tables = if component_params.is_empty() {
                (
                    b.def_types.clone(),
                    b.def_params.clone(),
                    b.def_bodies.clone(),
                )
            } else {
                def_tables(
                    snapshot,
                    std::iter::once(&*b.imported_defs),
                    component_params,
                )
            };
            (Arc::clone(&b.imported_defs), tables)
        }
        None => {
            let mut imported_defs = imports.defs.clone();
            let (imported_def_msgs, inline_def_msgs) = crate::def_decl::settle_defs(
                &mut imported_defs,
                &mut typed.defs,
                snapshot.defs.iter().map(|(n, d)| (n.clone(), d.ty.clone())),
                &schema,
            );
            for (name, msg) in imported_def_msgs {
                fold_diags.push(imported_def_diag(imports, &name, msg, at));
            }
            for (name, msg) in inline_def_msgs {
                fold_diags.push(Diagnostic {
                    code: crate::def_decl::E_DEF_DECL.to_string(),
                    severity: Severity::Error,
                    message: msg,
                    evidence: None,
                    span: crate::meta::meta_key_span(&doc.meta, &name),
                    layer: Layer::Content,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                });
            }
            let tables = def_tables(
                snapshot,
                [&imported_defs, &typed.defs].into_iter(),
                component_params,
            );
            (Arc::new(imported_defs), tables)
        }
    };
    let (def_types, def_params, def_bodies) = def_tables;
    // dsl 0.26.0 §5, G-7: a read of `occasion.target` — direct or through a
    // def — outside the document's kind beats. A component without one is
    // judged where a use expands its beats.
    if target_scoped || meta_kind != crate::meta::MetaKind::Component {
        fold_diags.extend(crate::beats::check_occasion_target_scope(
            doc,
            &def_bodies,
            target_scoped,
        ));
    }
    // dsl 0.24 T1-1: expand `@def`s in rule guards against the merged def
    // table, so the guard checks, the compiled IR and trace's evaluator all
    // read the expanded body. The vocab's guards are expanded with the base
    // def table: kept while this document's table is the same.
    let deftable = crate::cel_expand::DefTable {
        bodies: &def_bodies,
        params: &def_params,
    };
    match rel_from_base {
        Some(b) if !b.guards_use_defs || (typed.defs.is_empty() && component_params.is_empty()) => {
            fold_diags.extend(anchored(&b.guard_diags, at));
        }
        Some(b) => {
            let own = Arc::make_mut(&mut vocab);
            own.rules.clone_from(&b.pre_guard_rules);
            fold_diags.extend(crate::cel_resolve::expand_rule_guards(own, &deftable));
        }
        None => fold_diags.extend(crate::cel_resolve::expand_rule_guards(
            Arc::make_mut(&mut vocab),
            &deftable,
        )),
    }

    // dsl 0.27.0 §4: the project's `terminal:` — its imports' and, for a
    // schema document, its own; the game is over when any of them holds.
    // It persists when every declaration says its ending outlives runs.
    let terminal_decls: Vec<(&str, bool)> = imports
        .terminal
        .iter()
        .map(|t| (t.when.as_str(), t.persists.is_yes()))
        .chain(
            typed
                .terminal
                .as_ref()
                .map(|t| (t.when.raw.as_str(), t.persists.is_yes())),
        )
        .collect();
    let terminal = crate::gates::combine_terminal(terminal_decls.iter().map(|(w, _)| *w));
    let terminal_persists = terminal.is_some() && terminal_decls.iter().all(|(_, p)| *p);
    // dsl 0.28.0 (T3-45): a declared path that is also another's prefix — at
    // this document's own `state:` key, else at the schema that declares it.
    // The vocab's reports stand for every value path neither declared by
    // this document nor above one of its paths.
    let origins = &imports.rel.origins.state;
    let declared = |path: &str| typed.state.decls.contains_key(path) || origins.contains_key(path);
    let locate_value = |path: &str, message: &str| {
        if typed.state.decls.contains_key(path) {
            let span = crate::meta::meta_path_span(&doc.meta, &["state", path]);
            Some(state_decl_report("E-STATE-DECL", message, span))
        } else {
            let origin = origins.get(path)?;
            let d = state_decl_report("E-STATE-DECL", message, at);
            Some(crate::rel_schema::at_origin(d, Some(&origin)))
        }
    };
    let site = crate::state_decls::ValuePrefixSite {
        declared: &declared,
        results_under: &|value| results_under(snapshot, value),
        locate: &locate_value,
    };
    // A path the document writes in its `state:` is placed at its own key
    // even when the merge rejected it (a `uses:`-peer redeclaration), so the
    // reports touching it are rebuilt whether or not it is in the overlay.
    let authored = || {
        schema
            .overlay()
            .decls
            .keys()
            .chain(typed.state.decls.keys())
    };
    match shared {
        Some(b) => {
            let mut affected: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            for path in authored() {
                let mut end = path.len();
                while let Some(dot) = path[..end].rfind('.') {
                    end = dot;
                    if schema.contains(&path[..end]) {
                        affected.insert(path[..end].to_string());
                    }
                }
                affected.insert(path.into_owned());
            }
            let mut out: std::collections::BTreeMap<&str, Diagnostic> = b
                .value_prefix
                .iter()
                .filter(|(v, _)| !affected.contains(v.as_str()))
                .map(|(v, d)| (v.as_str(), d.clone()))
                .collect();
            for value in &affected {
                if let Some(d) = crate::state_decls::value_prefix_at(&schema, value, &site) {
                    out.insert(value, d);
                }
            }
            fold_diags.extend(out.into_values());
        }
        None => fold_diags.extend(crate::state_decls::check_value_prefix(&schema, &site)),
    }
    // A `{ domain: K }` / `{ entity: K }` path naming no declared K, or with a
    // `default:` outside K — at this document's key, else at the schema's.
    let locate_domain = |path: &str, key: &str, message: &str, code: &str| {
        let own_family = typed.state_index.contains_key(path)
            || typed.per_pending.iter().any(|p| p.path == path);
        if typed.state.decls.contains_key(path) || own_family {
            let span = crate::meta::meta_path_span(&doc.meta, &["state", path, key]);
            Some(state_decl_report(code, message, span))
        } else {
            let origin = origins.at_or_under(path)?;
            let d = state_decl_report(code, message, at);
            Some(crate::rel_schema::at_origin(d, Some(&origin)))
        }
    };
    let families_shared = typed.state_index.is_empty() && typed.per_pending.is_empty();
    match shared.filter(|b| families_shared && vocab.indexed_state == b.rel.indexed_state) {
        Some(b) => {
            let families = &vocab.indexed_state;
            let at_of = |p: &str| crate::state_decls::domain_type_at(p, families).to_string();
            let mut affected: std::collections::BTreeSet<String> =
                authored().map(|p| at_of(&p)).collect();
            for name in &changed_domains {
                affected.extend(b.paths_typed_by(name).map(|p| at_of(&p)));
            }
            if !changed_domains.is_empty() {
                // A report names the declared domains in its hint.
                affected.extend(b.domain_types.keys().cloned());
            }
            let mut out: std::collections::BTreeMap<String, Diagnostic> = b
                .domain_types
                .iter()
                .filter(|(a, _)| !affected.contains(*a))
                .map(|(_, (first, d))| (first.clone(), d.clone()))
                .collect();
            for a in &affected {
                if let Some((first, d)) = crate::state_decls::domain_type_report_at(
                    &schema,
                    a,
                    &domains,
                    families,
                    &locate_domain,
                ) {
                    out.insert(first, d);
                }
            }
            fold_diags.extend(out.into_values());
        }
        None => fold_diags.extend(crate::state_decls::check_domain_types(
            &schema,
            &domains,
            &vocab.indexed_state,
            &locate_domain,
        )),
    }
    let env = Env {
        mode: input.mode,
        state: schema,
        defs,
        def_types,
        def_params,
        rel_vocab: vocab,
        // 0.3.0 T11 fix: the SAME merged domains value as
        // `FoldedEnv.domains` below, so `check_fact_queries` validates a
        // query pattern's domain-typed arg against what
        // `check_assert`/`check_retract`/`build_rel_vocab` receive.
        domains: Arc::clone(&domains),
        clock,
        terminal,
        terminal_persists,
        seasons,
        occasion_scopes,
    };
    let member_envs = member_envs(&env);
    let declared_cast = crate::cast::declared_cast(snapshot, imports, &typed.cast);
    let use_lines = use_speaker_lines(doc, &input.components);
    (
        FoldedEnv {
            typed,
            env,
            def_bodies,
            host_defs: host_defs(snapshot, &imported_defs),
            doc_kind,
            domains,
            occasions: snapshot.occasions.clone(),
            cast: declared_cast,
            use_lines,
            member_envs,
            snapshot: Arc::clone(&input.base.snapshot),
        },
        fold_diags,
        state_merge_diags,
    )
}

/// One environment per distinct member list the document's kind and `for=`
/// beats answer, each with `occasion.target` typed by that list alone —
/// empty when they all answer the same members, whose type the document's
/// own decl already is.
fn member_envs(env: &Env) -> Vec<(Vec<String>, Env)> {
    let mut lists: Vec<&Vec<String>> = Vec::new();
    for (_, _, ms) in &env.occasion_scopes.scopes {
        if !lists.contains(&ms) {
            lists.push(ms);
        }
    }
    if lists.len() < 2 {
        return Vec::new();
    }
    lists
        .into_iter()
        .map(|ms| {
            let mut scoped = env.clone();
            let mut sorted = ms.clone();
            sorted.sort();
            sorted.dedup();
            if let Some(decl) = scoped
                .state
                .overlay_mut()
                .decls
                .get_mut(lute_manifest::semantics::beats::OCCASION_TARGET)
            {
                decl.ty = lute_manifest::types::Type::Enum(sorted);
            }
            (ms.clone(), scoped)
        })
        .collect()
}

/// Pre-pass: fold every `<branch>`'s and `<hub>`'s implicit recording decls
/// (`scene.choices.<id>`, plus a hub's per-choice `scene.visited.<id>.<choiceId>`)
/// into `schema` in document order, threading the episode-wide `seen` id set
/// (branch + hub ids share it) so `E-DUP-BRANCH` fires exactly once per duplicate.
/// Recurses into nested bodies (a branch/hub may live inside a match arm or
/// another choice body). A `<branch>` is ALSO admitted directly inside a quest
/// body (dsl 0.2.0 §6.7), reached via `<quest>` top level or an `<on>`/
/// `<objective>` arm — `doc.quests` is folded here too so a quest `<branch>`
/// gets the SAME `E-CHOICE-DUP`/implicit-decl treatment a scene one gets
/// (`<hub>` is never legal in a quest doc, dsl 0.2.0 §6.7 — grammar admission
/// rejects it separately, so it is never reached from a quest walk in
/// practice, but the recursion below tolerates it structurally regardless).
/// Entry bodies (dsl 0.19.0 §4) are folded too: a `<branch>`/`<hub>` there
/// is an admission error, and folding its decls keeps that the only report.
fn fold_branches(
    doc: &Document,
    schema: &mut crate::meta::StateSchema,
    seen: &mut std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for shot in &doc.sections {
        fold_branches_nodes(&shot.body, schema, seen, diags);
    }
    for quest in &doc.quests {
        fold_branches_nodes(&quest.body, schema, seen, diags);
    }
    for entry in &doc.entries {
        fold_branches_nodes(&entry.body, schema, seen, diags);
    }
    // dsl 0.23.0 §4: a bundle beat's `<branch>`/`<hub>` records its choice
    // under `scene.choices.<id>` exactly as a scene's does.
    for beat in &doc.beats {
        fold_branches_nodes(&beat.body, schema, seen, diags);
    }
}

fn fold_branches_nodes(
    nodes: &[Node],
    schema: &mut crate::meta::StateSchema,
    seen: &mut std::collections::BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                let rec = check_branch(b, seen);
                schema.decls.insert(rec.path, rec.decl);
                diags.extend(rec.diags);
                for choice in &b.choices {
                    fold_branches_nodes(&choice.body, schema, seen, diags);
                }
            }
            Node::Hub(h) => {
                // Hub ids share the branch uniqueness domain (`seen`); fold the
                // implicit `scene.choices.<hubId>` + `scene.visited.<hubId>.*`
                // decls, then recurse into hub arms for any nested branch/hub/
                // match (dsl §7.3.2, §11.1.3).
                let rec = check_hub(h, seen);
                for (path, decl) in rec.decls {
                    schema.decls.insert(path, decl);
                }
                diags.extend(rec.diags);
                for b in h.bodies() {
                    fold_branches_nodes(b, schema, seen, diags);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            fold_branches_nodes(body, schema, seen, diags)
                        }
                    }
                }
            }
            // Quest-only arms (dsl 0.2.0 §4, §6.4): a `<branch>`/`<match>` may
            // live directly inside an `<on>` event arm or an `<objective>`
            // body (grammar admission's `Emittable` context), so the fold
            // recurses through them too.
            Node::On(o) => fold_branches_nodes(&o.body, schema, seen, diags),
            Node::Objective(o) => fold_branches_nodes(&o.body, schema, seen, diags),
            Node::Line(_) | Node::Directive(_) | Node::Set(_) | Node::Timeline(_) => {}
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// Pre-pass: expand every active directive's `state.declares[]` into concrete
/// state slots at each use site (plugin §8/§9). A `::minigame{resultKey="k"}`
/// whose declaration declares `scene.minigame.<resultKey>` with shape
/// `minigameResult` opens `scene.minigame.k.<field>` for each field of that
/// shape, feeding the SAME `schema` the walk + defassign consume. Walks every
/// directive location (top-level, branch choices, match arms, timeline clips),
/// mirroring the CEL/inject walkers' recursion.
///
/// dsl 0.26.0 §3.1: a `::use` declares, in its host, the slots of every
/// plugin directive its component's body holds (nested `::use`s included),
/// bound to this use's arguments — exactly the slots the expanded body would
/// declare written here. Every consumer of the folded schema (the checker,
/// the compiled `state` table, trace's mocks, `lute play`) sees them.
pub(super) fn fold_directive_slots(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    components: &ComponentSet,
    schema: &mut crate::meta::StateSchema,
) {
    let mut cx = SlotFold {
        snapshot,
        components,
        schema,
        stack: Vec::new(),
    };
    let bodies = doc
        .sections
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        // dsl 0.19.0 §4: a directive in an entry body is an admission error;
        // folding its declared slots keeps that the only report.
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body));
    for body in bodies {
        cx.nodes(body, None);
    }
}

/// The walk [`fold_directive_slots`] makes. `bind` is `Some` inside a
/// component body: each directive there is bound to the enclosing `::use`'s
/// arguments before its slots resolve. `stack` guards a `::use` cycle
/// (reported elsewhere as `E-COMPONENT-CYCLE`).
struct SlotFold<'a> {
    snapshot: &'a CapabilitySnapshot,
    components: &'a ComponentSet,
    schema: &'a mut crate::meta::StateSchema,
    stack: Vec<String>,
}

type SlotBinding<'b> = (
    &'b std::collections::BTreeMap<String, AttrValue>,
    &'b [(String, Type)],
);

impl SlotFold<'_> {
    fn nodes(&mut self, nodes: &[Node], bind: Option<SlotBinding<'_>>) {
        for node in nodes {
            match node {
                Node::Directive(d) => self.directive(d, bind),
                Node::Branch(b) => {
                    for c in &b.choices {
                        self.nodes(&c.body, bind);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.nodes(body, bind)
                            }
                        }
                    }
                }
                Node::Timeline(tl) => {
                    for track in &tl.tracks {
                        for clip in &track.clips {
                            if let ClipNode::Directive(d) = &clip.node {
                                self.directive(d, bind);
                            }
                        }
                    }
                }
                Node::Hub(h) => {
                    for b in h.bodies() {
                        self.nodes(b, bind);
                    }
                }
                // Quest-only arms (dsl 0.2.0 §4, §6.4): a directive-opening slot
                // (e.g. `::minigame{resultKey="k"}`) may be used directly inside
                // an `<on>` event arm or an `<objective>` body — recurse so its
                // declared state slots open for the quest walk + defassign, same
                // as a scene shot's directives do.
                Node::On(o) => self.nodes(&o.body, bind),
                Node::Objective(o) => self.nodes(&o.body, bind),
                Node::Line(_) | Node::Set(_) => {}
                Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }

    fn directive(&mut self, d: &Directive, bind: Option<SlotBinding<'_>>) {
        let bound;
        let d = match bind {
            Some((args, params)) => {
                let mut b = d.clone();
                crate::component_effects::bind_attrs(&mut b.attrs, args, params);
                bound = b;
                &bound
            }
            None => d,
        };
        if d.tag != "use" {
            expand_directive_slots(d, self.snapshot, self.schema);
            return;
        }
        let components = self.components;
        let Some((name, def)) = use_target(d).and_then(|n| components.table.get_key_value(n))
        else {
            return;
        };
        if self.stack.contains(name) {
            return;
        }
        let args = crate::component_effects::use_args_for(d, def);
        self.stack.push(name.clone());
        for shot in &def.body.sections {
            self.nodes(&shot.body, Some((&args, &def.params)));
        }
        self.stack.pop();
    }
}

/// Expand one directive USE: look up its declaration, and for each declared slot
/// resolve the concrete path and insert one `StateDecl` per field of the
/// referenced shape. A directive with no declaration / no `state` / an
/// unresolvable path / a missing shape / an untierable base is skipped.
fn expand_directive_slots(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
    schema: &mut crate::meta::StateSchema,
) {
    let Some(decl) = snapshot.directive(&dir.tag) else {
        return;
    };
    let Some(state) = &decl.state else {
        return;
    };
    for slot in &state.declares {
        let Some(base) = resolve_slot_path(slot, dir) else {
            continue;
        };
        let Some(shape) = snapshot.state_shapes.get(&slot.shape) else {
            continue;
        };
        let Some(ns) = crate::meta::namespace_of(&base) else {
            continue;
        };
        insert_shape_fields(
            schema,
            &base,
            ns,
            shape,
            snapshot,
            &mut std::collections::BTreeSet::new(),
        );
    }
}

/// Resolve a `SlotDecl`'s path (scope + segments) into a concrete dotted path at
/// a use site: literal segments verbatim; `fromAttr` segments -> that attr's
/// value. Returns `None` if any `fromAttr` attr is absent or not a plain string.
fn resolve_slot_path(slot: &SlotDecl, dir: &Directive) -> Option<String> {
    crate::permissions::resolve_path(&slot.scope, &slot.path, &dir.attrs)
}

/// The string value of a directive attribute — a plain string literal only; a
/// Ref/CEL-valued key cannot seed a static path, so it yields `None`.
pub(super) fn attr_str(dir: &Directive, key: &str) -> Option<String> {
    dir.attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        })
}

/// Insert one `StateDecl` per shape field at `<base>.<field>`; a field that
/// itself references a nested shape recurses into that shape. `visiting` tracks
/// the shapes on the current expansion path (by name) with stack semantics: a
/// shape already on the path is a cycle and is skipped, guaranteeing
/// termination on self- or mutually-referential shapes. Removing the name after
/// the field loop keeps legitimate diamonds (a shape reached via two disjoint
/// paths) from being flagged as false cycles.
fn insert_shape_fields(
    schema: &mut crate::meta::StateSchema,
    base: &str,
    ns: crate::meta::Namespace,
    shape: &StateShape,
    snapshot: &CapabilitySnapshot,
    visiting: &mut std::collections::BTreeSet<String>,
) {
    if !visiting.insert(shape.name.clone()) {
        return;
    }
    for f in &shape.fields {
        let path = format!("{base}.{}", f.name);
        if let Some(nested_name) = &f.shape {
            if let Some(nested) = snapshot.state_shapes.get(nested_name) {
                insert_shape_fields(schema, &path, ns, nested, snapshot, visiting);
                continue;
            }
        }
        schema.decls.insert(
            path,
            crate::meta::StateDecl {
                ty: f.ty.clone(),
                default: f.default.clone(),
                namespace: ns,
                owner: None,
            },
        );
    }
    visiting.remove(&shape.name);
}
