//! Environment folding: the frontmatter/schema/def/vocabulary fold
//! ([`fold_env`]) plus the implicit `<branch>`/`<hub>` recording decls and
//! plugin directive result slots it folds into the schema.

use super::*;

/// Extract ordered `(param name, Type)` pairs from an inline/imported def's raw
/// YAML value (dsl §8.1). Reads the def's `params:` sub-MAPPING in SOURCE order
/// (`serde_yaml::Mapping` is insertion-ordered), deserializing each value to a
/// `Type` via the same serde path `Type` uses. An absent/non-mapping `params:`
/// or a malformed entry yields no pair — never a panic.
pub(super) fn params_from_yaml(v: &serde_yaml::Value) -> Vec<(String, lute_manifest::types::Type)> {
    let Some(map) = v.get("params").and_then(|p| p.as_mapping()) else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(k, tv)| {
            let name = k.as_str()?.to_string();
            let ty = serde_yaml::from_value::<lute_manifest::types::Type>(tv.clone()).ok()?;
            Some((name, ty))
        })
        .collect()
}

/// Fold the analysis environment from an already-parsed document. Returns two
/// diagnostic streams kept SEPARATE so `check()` preserves its exact diagnostic
/// byte-order contract (a stable sort on `(byte_start, code)` makes same-span
/// ties order-sensitive): `.1` = the pre-import fold diags (meta + branch
/// dup/choice-dup) emitted just before import diags, and `.2` = the state-merge
/// diags (`E-EXTENDS-STATE-TYPE`/`E-STATE-REDECLARE`) emitted AFTER component
/// validation. Pure and total; never panics.
pub fn fold_env(
    doc: &Document,
    input: &CheckInput,
) -> (FoldedEnv, Vec<Diagnostic>, Vec<Diagnostic>) {
    // 3. Resolve the root document kind (dsl 0.2.0 §3.1) FIRST: it gates which
    //    per-kind frontmatter keys the meta parse below allows. Defaults to
    //    `Scene` — the degrade-safe path — when unresolved (missing/unknown
    //    `kind:`), so a mis-kinded doc still gets the scene-triad required-key
    //    treatment it had pre-0.2.0.
    let (resolved_kind, kind_diags) =
        crate::meta::resolve_doc_kind_with_defaults(&doc.meta, &input.defaults);
    let has_body = !doc.shots.is_empty()
        || !doc.quests.is_empty()
        || !doc.entries.is_empty()
        || !doc.beats.is_empty();
    let (doc_kind, meta_kind, kind_diags) = match resolved_kind {
        Some(crate::meta::DocKind::Scene) => (
            crate::meta::DocKind::Scene,
            crate::meta::MetaKind::Scene,
            kind_diags,
        ),
        Some(crate::meta::DocKind::Quest) => (
            crate::meta::DocKind::Quest,
            crate::meta::MetaKind::Quest,
            kind_diags,
        ),
        // dsl 0.19.0 §2: a lore document takes the quest-document frontmatter
        // keys (`MetaKind::Lore` mirrors `MetaKind::Quest`'s key set).
        Some(crate::meta::DocKind::Lore) => (
            crate::meta::DocKind::Lore,
            crate::meta::MetaKind::Lore,
            kind_diags,
        ),
        None => match crate::meta::infer_meta_kind_from_shape(&doc.meta, has_body) {
            // A fragment opened standalone: validate in its import role, drop the
            // root-only E-KIND-MISSING/E-META-MISSING false positives. Any body
            // (e.g. a component's `## Scene`) still walks as `DocKind::Scene` —
            // the same degrade-safe shape as the genuine-missing-kind default.
            Some(mk) => (crate::meta::DocKind::Scene, mk, Vec::new()),
            None => (
                crate::meta::DocKind::Scene,
                crate::meta::MetaKind::Scene,
                kind_diags,
            ), // genuine missing kind
        },
    };

    // 3b. Typed frontmatter + inline state schema, dispatched by the resolved
    //     kind (dsl 0.2.0 §3.1, §6.1): a Quest doc carries none of the scene
    //     triad and rejects it as an unknown key.
    let (mut typed, mut fold_diags) = crate::meta::parse_meta_kind_with_defaults(
        &doc.meta,
        &input.snapshot,
        meta_kind,
        &input.defaults,
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

    // 3c. The FULL merged domain vocabulary (data-catalog foundation A4):
    //     `snapshot.domains` (A2 — core baseline + active-plugin `enums`)
    //     UNION the PROJECT's domains — this scene's schema imports AND its own
    //     inline `enums:`/`entities:` projection (`typed.domains`), which is the
    //     same value `build_rel_vocab` gets one line below, so the domain map
    //     and `RelVocab` can never disagree about a name either of them
    //     declares (A3's `merge_domains`). Computed ONCE here (0.3.0 T7 moved
    //     this from `check()`) so `lute-compile` (which calls `fold_env`
    //     directly) sees the SAME vocabulary, never double-emitting
    //     `E-DOMAIN-DUP`. Then the merged, validated relational vocabulary
    //     (dsl 0.3.0 §3/§4): imports ∪ this document's inline
    //     `entities:`/`relations:`/`enums:`/`facts:`/`rules:`, every
    //     declaration checked (§3.1/§4) and every seed `facts:` entry
    //     validated via `check_atom` (D12 wildcard-in-seed included).
    let (mut domains, domain_diags) =
        merge_domains(&input.snapshot, &input.imports, &typed, doc.meta.span);
    let (mut vocab, rel_diags) =
        crate::rel_schema::build_rel_vocab(&input.imports, &typed, &domains, &doc.meta);
    vocab.effect_directives = crate::directive_facts::effect_directives(&input.snapshot);
    vocab.effect_origins = input.imports.plugin_origins.effect_facts.clone();
    // dsl 0.26.0 §2.3: a project kind's domain is the kind's final member
    // list — this document's `add:`s and sub-kinds included — and (dsl 0.27.0
    // §7) its final labels, so a `{ domain: K }` path renders them.
    for (name, decl) in &vocab.kinds {
        if let (lute_manifest::relations::KindShape::Members(ms), Some(d)) =
            (&decl.shape, domains.get_mut(name))
        {
            if !input.snapshot.domains.contains_key(name) {
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
    fold_diags.extend(domain_diags);
    fold_diags.extend(rel_diags);
    // Per-rule Datalog checks (dsl 0.3.0 §7.1/§7.2, 0.3.0 T8): heads, body
    // atoms, safety — over the MERGED rule set. Runs here (not in `check()`)
    // so `lute-compile`'s direct `fold_env` caller sees the same diagnostics;
    // the vocab is still a plain local here (not yet frozen into `Env`'s
    // `Arc`), which Task 9's stratification/guard-taint pass relies on too.
    fold_diags.extend(crate::datalog_check::check_rules(&vocab, &domains));
    // Whole-rule-set graph analyses (dsl 0.3.0 §7.2/§6, 0.3.0 T9): negation-
    // cycle stratification + the guard-taint closure. Mutates `vocab` in
    // place (fills `guard_tainted`) BEFORE it is frozen into `Env`'s `Arc`
    // below — the one place in this pipeline `vocab` is still a plain local.
    fold_diags.extend(crate::datalog_check::check_stratification(&mut vocab));

    // 4. Fold every `<branch>`/`<hub>`'s implicit recording decls
    //    (`scene.choices.<id>` + a hub's per-choice `scene.visited.<id>.*`) into
    //    the schema BEFORE the checks that resolve against them (match subjects,
    //    CEL state paths). This pre-pass owns the episode-wide `E-DUP-BRANCH`
    //    detection (hub + branch ids share one domain) so the main walk never
    //    double-counts ids.
    // Merge imported schema (dsl §9.2) first, then the scene's inline `state:`.
    // Precedence depends on WHERE the imported decl came from (see
    // `SchemaImports::state_overridable`): a `uses`-peer path may NOT be
    // redeclared (`E-STATE-REDECLARE`, imported wins), but an `extends`-base path
    // MAY be refined by the scene's inline decl (the inline wins; a TYPE change is
    // `E-EXTENDS-STATE-TYPE`, the persisted type must stay stable).
    let mut schema = input.imports.state.clone();
    let mut state_merge_diags: Vec<Diagnostic> = Vec::new();
    for (path, decl) in &typed.state.decls {
        match input.imports.state.decls.get(path) {
            Some(imported) if input.imports.state_overridable.contains(path) => {
                // Extends-base override: the inline decl wins; guard the type.
                if decl.ty != imported.ty {
                    state_merge_diags.push(Diagnostic {
                        code: "E-EXTENDS-STATE-TYPE".to_string(),
                        severity: Severity::Error,
                        message: format!(
                            "state path `{path}` overrides base declared type {:?} with {:?}; persisted state must keep a stable type",
                            imported.ty, decl.ty
                        ),
                        evidence: None,
                        span: doc.meta.span,
                        layer: Layer::Content,
                        fixits: Vec::new(),
                        provenance: None,
                        covered: Vec::new(),
                        related: Vec::new(),
                    });
                }
                schema.decls.insert(path.clone(), decl.clone());
            }
            Some(_) => {
                // Uses-peer path: a scene must not redeclare it (imported wins).
                state_merge_diags.push(Diagnostic {
                    code: "E-STATE-REDECLARE".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "state path `{path}` is declared by an imported schema (§9.2); a scene must not redeclare or override it"
                    ),
                    evidence: None,
                    span: doc.meta.span,
                    layer: Layer::Content,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                });
            }
            None => {
                schema.decls.insert(path.clone(), decl.clone());
            }
        }
    }
    schema.faulty.extend(typed.state.faulty.iter().cloned());
    schema.clock_rejected |= typed.state.clock_rejected;
    // dsl 0.28.0: an inline `per:` over a kind an imported schema declares.
    let (per_decls, per_index, per_faults) =
        crate::meta::expand_per_pending(&typed.per_pending, &vocab.kinds);
    for (path, decl) in per_decls {
        schema.decls.entry(path).or_insert(decl);
    }
    vocab.indexed_state.extend(per_index);
    fold_diags.extend(per_faults);
    let mut seen_branches = std::collections::BTreeSet::new();
    fold_branches(doc, &mut schema, &mut seen_branches, &mut fold_diags);

    // 4a. Fold every `<quest>`'s implicit reserved `quest.<id>.*` decls (dsl
    //     0.2.0 §5.2) into the schema, threading a SEPARATE per-document `seen`
    //     id set (quest ids key the `quest.<id>.*` tier, a namespace distinct
    //     from `scene.choices.*`) so `E-QUEST-ID-DUP` fires exactly once per
    //     duplicate. `seen_quests` is SEEDED from `input.imports.imported_quest_ids`
    //     (dsl 0.2.0 §6.3: quest ids are unique PROJECT-WIDE, across the import
    //     graph, not merely within this document) — redeclaring an
    //     import-reachable id then fails the same `seen_quests.insert` check as
    //     an in-document repeat, reusing `E-QUEST-ID-DUP` unchanged. A collision
    //     BETWEEN two import-reachable docs that this document itself never
    //     redeclares is instead caught in `resolve_imports` directly (this
    //     document's own `<quest>` fold never sees it).
    //
    //     `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` are RESERVED
    //     (dsl 0.2.0 §5.2/§9.3: "implicitly declared and MUST NOT be
    //     author-declared") — snapshot every state path that already exists
    //     BEFORE any reserved decl is folded (the author's inline `state:` and
    //     any imported schema, both merged above) so a reserved path that
    //     collides with one of THOSE is flagged (`E-QUEST-RESERVED-DECL`)
    //     instead of silently clobbered by `schema.decls.insert`. A collision
    //     with a path THIS loop itself already folded (the `E-QUEST-ID-DUP`
    //     repeat-id case, an identical decl either way) is NOT flagged — the
    //     snapshot is frozen before the loop starts, so a same-id repeat
    //     resolves against the pre-loop state and simply re-inserts the
    //     identical decl.
    let pre_existing_state: std::collections::BTreeSet<String> =
        schema.decls.keys().cloned().collect();
    let mut seen_quests: std::collections::BTreeSet<String> =
        input.imports.imported_quest_ids.keys().cloned().collect();
    for quest in &doc.quests {
        let record = check_quest(quest, &mut seen_quests);
        for (path, decl) in record.decls {
            if pre_existing_state.contains(&path) {
                fold_diags.push(Diagnostic {
                    code: "E-QUEST-RESERVED-DECL".to_string(),
                    severity: Severity::Error,
                    message: format!(
                        "state path `{path}` collides with an implicitly-declared reserved \
                         quest field (dsl 0.2.0 §5.2); it must not be author-declared in \
                         `state:`"
                    ),
                    evidence: None,
                    span: doc.meta.span,
                    layer: Layer::Content,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                });
            } else {
                schema.decls.insert(path, decl);
            }
        }
        fold_diags.extend(record.diags);
        // dsl 0.16.0 §2/§4/§6: reward shape/closure/vocabulary. Runs here
        // (with `fold_diags`) so a shape/vocab fault surfaces alongside the
        // rest of the quest fold — the Walker's `reward.when` Bool profile
        // gate below owns only the CEL-side check.
        fold_diags.extend(check_quest_rewards(
            quest,
            &input.snapshot,
            &input.providers,
            &vocab.kinds,
        ));
    }
    // dsl 0.23.0 §6: a subquest's `tier` equals its parent's — the
    // same-document half of `E-QUEST-TIER-MIX` (check-project owns the
    // cross-document edges).
    fold_diags.extend(crate::project_check::check_doc_quest_tiers(&doc));

    // 4a'. Fold every `<entry>`'s implicit reserved `entry.<id>.read` decl
    //      (dsl 0.19.0 §5: `bool`, default `false`) and its attribute /
    //      per-document identity diagnostics (`E-ENTRY-ATTR`,
    //      `E-ENTRY-ID-DUP`, `E-ENTRY-SERIES-ORDER`) — the lore mirror of the
    //      quest fold above, its id set seeded from the import-reachable
    //      entry ids the same way `seen_quests` is. No reserved-decl
    //      collision guard is needed: an `entry.*` path can never be
    //      author-declared (`entry` is not a `state:` tier,
    //      `E-STATE-NAMESPACE`).
    //      The positions are the RESOLVED ones (dsl 0.19.0 §2.1): a lore
    //      document's `series:` orders its entries by place in the file.
    let mut seen_entries: std::collections::BTreeSet<String> =
        input.imports.imported_entry_ids.keys().cloned().collect();
    let entry_record = crate::lore::check_entries(
        typed.series.as_deref(),
        &doc.entries,
        &mut seen_entries,
        &input.snapshot,
    );
    schema.decls.extend(entry_record.decls);
    fold_diags.extend(entry_record.diags);
    // 4a''. dsl 0.23.0 §6: `prev.run.<path>` is the reserved, read-only
    //       mirror of every declared `run.<path>` — the value it had when the
    //       previous run ended. Same type, no default: it is `unset` before
    //       the first run ends, so a read needs `isSet` or an `unset` arm.
    let prev_decls: Vec<(String, crate::meta::StateDecl)> = schema
        .decls
        .iter()
        .filter_map(|(path, decl)| {
            crate::cel_paths::prev_run_path(path).map(|prev| {
                let mirror = crate::meta::StateDecl {
                    ty: decl.ty.clone(),
                    default: None,
                    namespace: crate::meta::Namespace::User,
                    owner: Some(lute_manifest::types::Owner::Engine),
                };
                (prev, mirror)
            })
        })
        .collect();
    schema.decls.extend(prev_decls);
    // 4a'''. dsl 0.24.0 §1: the project's clock — its imports' `clock:` and,
    //        for a schema document, its own — checked against the folded
    //        schema; its reserved read-only `clock.*` paths join the schema.
    let clocks: Vec<(crate::clock::ClockSite, lute_manifest::clock::ClockDecl)> = input
        .imports
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
        doc.meta.span
    };
    let (clock, clock_diags) = crate::clock::check_clock(
        &clocks,
        &schema,
        &domains,
        &input.snapshot.occasions,
        clock_span,
    );
    fold_diags.extend(clock_diags);
    if let Some(clock) = &clock {
        schema
            .decls
            .extend(crate::clock::reserved_decls(clock, &schema));
        // dsl 0.24.0 §1 / 0.27.0 §4: `clock.weekday`, and a finite clock's
        // `clock.index` and day path, range over known whole numbers.
        let ranges = crate::clock::int_ranges(clock, &schema);
        schema.int_ranges.extend(ranges);
        // dsl 0.28.0 (T3-40): a clock that ends on its first day narrows
        // its slot path to the slots it reaches.
        let members = crate::clock::slot_members(clock, &schema);
        schema.clock_members.extend(members);
        // `{ domain: clock.slot }` / `{ domain: clock.weekdayLabel }` name
        // the clock's own enums.
        for (name, domain) in crate::clock::clock_domains(clock) {
            domains.entry(name).or_insert(domain);
        }
        schema.clock = Some(clock.clone());
    }
    // T3-1: a written `clock:` that was rejected is the cause; what needs a
    // clock is not judged against its absence.
    if clock.is_some() {
        schema.clock_rejected = false;
    }
    if !schema.clock_rejected {
        fold_diags.extend(crate::clock::check_once_needs_clock(
            doc,
            typed.beat.as_ref(),
            clock.as_ref(),
        ));
    }
    // dsl 0.27.0 §5: the project's seasons — its imports' and, for a schema
    // document, its own — and every use of one.
    let season_span = if typed.seasons.is_empty() {
        doc.meta.span
    } else {
        crate::meta::meta_key_span(&doc.meta, "seasons")
    };
    let season_names: Vec<String> = input
        .imports
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
        input
            .imports
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
    schema.decls.extend(mirrors);
    fold_diags.extend(season_diags);
    fold_diags.extend(crate::season::check_uses(
        doc,
        typed.beat.as_ref(),
        &seasons,
        &input.defaults,
    ));
    fold_diags.extend(crate::season::check_relation_tiers(
        &vocab,
        &seasons,
        crate::meta::meta_key_span(&doc.meta, "relations"),
    ));
    // dsl 0.21.0 §2: every entry beat's occasion against the vocabulary.
    fold_diags.extend(crate::beats::check_entry_occasions(
        &doc.entries,
        &input.snapshot.occasions,
    ));
    // dsl 0.23.0 §4: every bundle `<beat>`'s shape, id, and occasion.
    fold_diags.extend(crate::bundles::check_bundle_beats(
        typed.id.as_deref(),
        lute_manifest::yaml_text::key_span(&doc.meta.raw_yaml, &["id"]).is_some(),
        &doc.beats,
        &input.snapshot.occasions,
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
            &input.snapshot.occasions,
        ));
    }
    // dsl 0.21.0 §7a.2: every objective's `on` occasion, checked like a beat's.
    fold_diags.extend(crate::beats::check_objective_occasions(
        &doc.quests,
        &input.snapshot.occasions,
    ));
    // dsl 0.22.0 §8: every beat target against its occasion's target domain
    // (the entity kinds come from the merged vocabulary built above).
    fold_diags.extend(crate::beats::check_beat_target_domains(
        doc,
        typed.beat.as_ref(),
        &input.snapshot.occasions,
        &vocab.kinds,
    ));
    // dsl 0.27.0 §3 (T2-10): every `for="kind:<kind>"` beat (a scene's `for:`).
    fold_diags.extend(crate::occasion_bind::check_for_kinds(
        doc,
        typed.beat.as_ref(),
        &input.snapshot.occasions,
        &vocab.kinds,
    ));
    // dsl 0.27.0 §3: a beat on an occasion declaring a typed `payload:`
    // reads each field as `occasion.payload.<field>` (engine-owned, bound
    // by the raise); a read in a beat of another occasion is `E-UNDECLARED`.
    let (payload_decls, payload_diags) =
        crate::occasion_bind::payload_decls(doc, typed.beat.as_ref(), &input.snapshot.occasions);
    for (path, ty) in payload_decls {
        schema.decls.insert(
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
    // dsl 0.26.0 §5: a kind beat reads the member it was raised for as
    // `occasion.target` (engine-owned, always assigned while such a beat
    // runs). The document's decl is typed by every kind its kind beats
    // answer; each beat's own slots are checked with its own kind's members
    // (`FoldedEnv::env_at`, built below).
    // dsl 0.27.0 §3: and each kind beat's own members, the scope a slot
    // binding `occasion.target` as a fact argument / family index is judged
    // over, once per member.
    let occasion_scopes = crate::occasion_bind::occasion_scopes(
        doc,
        typed.beat.as_ref(),
        &input.snapshot.occasions,
        &vocab.kinds,
    );
    let occasion_members = occasion_scopes.members();
    let target_scoped = !occasion_members.is_empty();
    if target_scoped {
        schema.decls.insert(
            crate::beats::OCCASION_TARGET.to_string(),
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
        &input.snapshot.occasions,
        &vocab.kinds,
    ));
    // dsl 0.25.0 §6: every relation's `changedOn:` occasion, checked like one.
    fold_diags.extend(crate::rel_schema::check_changed_on(
        &vocab,
        &input.snapshot.occasions,
        &doc.meta,
    ));
    // 0.27 prerelease G-6: a `{ domain: K }` payload field names a declared K.
    fold_diags.extend(crate::occasion_bind::check_payload_domains(
        doc,
        typed.beat.as_ref(),
        &input.snapshot.occasions,
        &domains,
        &input.imports.plugin_origins,
    ));

    // 4b. Expand every active directive's `state.declares[]` into concrete state
    //     slots at each use site (plugin §8/§9): a `::minigame{resultKey="k"}`
    //     opens `scene.minigame.k.<field>` for each field of its shape. This runs
    //     before the walk + defassign so plugin-declared state resolves.
    fold_directive_slots(doc, &input.snapshot, &input.components, &mut schema);
    // dsl 0.27.0 §2 (T1-2): a `{ domain: K }` / `{ entity: K }` path is
    // member-checked like an inline enum — its members, from the merged
    // domains, ride on the schema every pass reads.
    schema.resolve_domains(&domains);

    // The def names the `@ref` resolver validates against (dsl §8.1): inline
    // frontmatter defs plus plugin-exported defs (both are declared refs).
    let mut defs: std::collections::BTreeSet<String> = typed.defs.keys().cloned().collect();
    defs.extend(input.snapshot.defs.keys().cloned());
    defs.extend(input.imports.defs.keys().cloned());
    // A component's declared `params:` ARE the `@param` ref namespace for its
    // OWN presentational body (dsl §13/§8.1) — already true when the
    // component is expanded transitively via `::use` (`component_env`
    // above). Seed the SAME names here so a STANDALONE `lute check` of the
    // component file itself (which walks as `DocKind::Scene` per the
    // fragment-shape inference above) resolves its own `@param` refs instead
    // of false-flagging `E-UNDECLARED-REF`. Guarded to `MetaKind::Component`
    // only — a normal scene/quest/schema doc's `defs` is untouched. The
    // parallel `def_types` (E-REF-TYPE) and `def_params` (empty → 0-arity, so
    // `@p(x)` is E-REF-ARITY, matching `component_env`) tables are seeded from
    // `params:` under the SAME guard below — keep all three in sync.
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
    // against the FOLDED schema. The meta parse already lifted each def to
    // its long form (the shorthand `name: "<CEL>"` included), so the body is
    // always at `cel:`; here an absent `type:` is inferred from that body and
    // written back, and an explicit one must agree with it. Every table below
    // then reads the settled values, so a shorthand def has a body, a type and
    // a (0-arity) params entry exactly like a long-form one.
    // Round-5 T3-14: in dependency order, so a def whose body calls another
    // def takes that def's type.
    let mut imported_defs = input.imports.defs.clone();
    let (imported_def_msgs, inline_def_msgs) = crate::def_decl::settle_defs(
        &mut imported_defs,
        &mut typed.defs,
        input
            .snapshot
            .defs
            .iter()
            .map(|(n, d)| (n.clone(), d.ty.clone())),
        &schema,
    );
    for (name, msg) in imported_def_msgs {
        // Prerelease N5: reported at the def's schema line and folded
        // across importers (dsl 0.26.0 §2.7), not at every importer's 1:1.
        fold_diags.push(crate::rel_schema::at_origin(
            Diagnostic {
                code: crate::def_decl::E_DEF_DECL.to_string(),
                severity: Severity::Error,
                message: msg,
                evidence: None,
                span: doc.meta.span,
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            },
            input.imports.rel.origins.defs.get(&name),
        ));
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

    // The def name -> produced `Type` table the `@ref` type-context check
    // (`E-REF-TYPE`, dsl §8) resolves against, merged from two sources.
    let mut def_types: std::collections::BTreeMap<String, lute_manifest::types::Type> =
        std::collections::BTreeMap::new();
    // Plugin defs are already typed.
    for (name, d) in &input.snapshot.defs {
        def_types.insert(name.clone(), d.ty.clone());
    }
    // Imported schema defs (untyped YAML, `type:` settled above). Imported
    // overrides plugin; inline (below) overrides imported.
    for (name, v) in &imported_defs {
        if let Some(t) = v
            .get("type")
            .cloned()
            .and_then(|tv| serde_yaml::from_value::<lute_manifest::types::Type>(tv).ok())
        {
            def_types.insert(name.clone(), t);
        }
    }
    // Inline frontmatter defs are stored untyped; extract the settled `type:`
    // and deserialize it via the same serde path `Type` uses. A def whose type
    // could not be settled has no entry (never a panic); `E-DEF-DECL` names it.
    // Inline overrides plugin (scene-local).
    for (name, v) in &typed.defs {
        if let Some(t) = v
            .get("type")
            .cloned()
            .and_then(|tv| serde_yaml::from_value::<lute_manifest::types::Type>(tv).ok())
        {
            def_types.insert(name.clone(), t);
        }
    }
    // Same seed for the type table (drives `E-REF-TYPE`, dsl §8): a
    // standalone component body's `@param` use in a typed attr slot must
    // still be type-checked, not merely resolved.
    if meta_kind == crate::meta::MetaKind::Component {
        for param in &typed.params {
            def_types.insert(param.name.clone(), param.ty.clone());
        }
    }

    // Parallel table of ORDERED params per def (dsl §8.1), for `@name(args)`
    // arity/arg-type checks. Same three sources & precedence as `def_types`
    // (plugin < imported < inline).
    let mut def_params: std::collections::BTreeMap<
        String,
        Vec<(String, lute_manifest::types::Type)>,
    > = std::collections::BTreeMap::new();
    // Plugin defs carry ordered `Vec<DefParam>` directly.
    for (name, d) in &input.snapshot.defs {
        def_params.insert(
            name.clone(),
            d.params
                .iter()
                .map(|p| (p.name.clone(), p.ty.clone()))
                .collect(),
        );
    }
    // Imported schema defs (untyped YAML): extract `params:` in order. Imported
    // overrides plugin; inline (below) overrides imported.
    for (name, v) in &imported_defs {
        def_params.insert(name.clone(), params_from_yaml(v));
    }
    // Inline frontmatter defs (untyped YAML): same extraction; scene-local override.
    for (name, v) in &typed.defs {
        def_params.insert(name.clone(), params_from_yaml(v));
    }
    // A component's declared params are 0-ARITY value refs in its own body:
    // a bare `@p` is well-formed, `@p(x)` is `E-REF-ARITY` — the SAME empty
    // entries `component_env` seeds for the transitive `::use` path, so a
    // STANDALONE check of the component file agrees with the transitive one on
    // arity (final-review parity fix). Guarded to `MetaKind::Component`.
    if meta_kind == crate::meta::MetaKind::Component {
        for param in &typed.params {
            def_params.insert(param.name.clone(), Vec::new());
        }
    }

    // def name -> raw CEL body for the D4 expander. Same three sources and the
    // same precedence as `def_types`: plugin < imported < inline.
    let mut def_bodies: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for (name, d) in &input.snapshot.defs {
        def_bodies.insert(name.clone(), d.cel.clone());
    }
    for (name, v) in &imported_defs {
        if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
            def_bodies.insert(name.clone(), c.to_string());
        }
    }
    for (name, v) in &typed.defs {
        if let Some(c) = v.get("cel").and_then(|c| c.as_str()) {
            def_bodies.insert(name.clone(), c.to_string());
        }
    }
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
    // table while `vocab` is still a local, so the guard checks, the compiled
    // IR and trace's evaluator all read the expanded body.
    fold_diags.extend(crate::cel_resolve::expand_rule_guards(
        &mut vocab,
        &crate::cel_expand::DefTable {
            bodies: &def_bodies,
            params: &def_params,
        },
    ));

    // dsl 0.27.0 §4: the project's `terminal:` — its imports' and, for a
    // schema document, its own; the game is over when any of them holds.
    // It persists when every declaration says its ending outlives runs.
    let terminal_decls: Vec<(&str, bool)> = input
        .imports
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
    let declared = |path: &str| {
        typed.state.decls.contains_key(path) || input.imports.rel.origins.state.contains_key(path)
    };
    let results_under = |value: &str| {
        input
            .snapshot
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
    };
    fold_diags.extend(crate::state_decls::check_value_prefix(
        &schema,
        &declared,
        &results_under,
        &|path, message| {
            let d = |span| Diagnostic {
                code: "E-STATE-DECL".to_string(),
                severity: Severity::Error,
                message: message.to_string(),
                evidence: None,
                span,
                layer: Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            };
            if typed.state.decls.contains_key(path) {
                Some(d(crate::meta::meta_path_span(&doc.meta, &["state", path])))
            } else {
                let origin = input.imports.rel.origins.state.get(path)?;
                Some(crate::rel_schema::at_origin(d(doc.meta.span), Some(origin)))
            }
        },
    ));
    // A `{ domain: K }` / `{ entity: K }` path naming no declared K, or with a
    // `default:` outside K — at this document's key, else at the schema's.
    fold_diags.extend(crate::state_decls::check_domain_types(
        &schema,
        &domains,
        &vocab.indexed_state,
        &|path, key, message, code| {
            let d = |span| Diagnostic {
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
            };
            let own_family = typed.state_index.contains_key(path)
                || typed.per_pending.iter().any(|p| p.path == path);
            if typed.state.decls.contains_key(path) || own_family {
                Some(d(crate::meta::meta_path_span(
                    &doc.meta,
                    &["state", path, key],
                )))
            } else {
                let origins = &input.imports.rel.origins.state;
                let member = format!("{path}.");
                let origin = origins.get(path).or_else(|| {
                    origins
                        .iter()
                        .find(|(p, _)| p.starts_with(&member))
                        .map(|(_, o)| o)
                })?;
                Some(crate::rel_schema::at_origin(d(doc.meta.span), Some(origin)))
            }
        },
    ));
    let env = Env {
        mode: input.mode,
        state: schema,
        defs,
        def_types,
        def_params,
        rel_vocab: std::sync::Arc::new(vocab),
        // 0.3.0 T11 fix: the SAME merged domains value moved into
        // `FoldedEnv.domains` below — cloned (not moved) here so
        // `check_fact_queries` (cel_resolve.rs) can validate a query
        // pattern's domain-typed arg via `ctx.env.domains`, matching what
        // `check_assert`/`check_retract`/`build_rel_vocab` already receive.
        domains: domains.clone(),
        clock,
        terminal,
        terminal_persists,
        seasons,
        occasion_scopes,
    };
    let member_envs = member_envs(&env);
    let declared_cast = crate::cast::declared_cast(&input.snapshot, &input.imports, &typed.cast);
    let use_lines = use_speaker_lines(doc, &input.components);
    (
        FoldedEnv {
            typed,
            env,
            def_bodies,
            doc_kind,
            domains,
            occasions: input.snapshot.occasions.clone(),
            cast: declared_cast,
            use_lines,
            member_envs,
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
            if let Some(decl) = scoped.state.decls.get_mut(crate::beats::OCCASION_TARGET) {
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
    for shot in &doc.shots {
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
        .shots
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
        for shot in &def.body.shots {
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
