//! The `check()` / `check_parsed` orchestration: parse, fill, fold, walk,
//! the document-level passes, and the final suppress/dedup/sort.

use super::*;

/// Translate every CEL parse failure `fill_document` reported into a
/// writer-voiced `E-CEL-PARSE` [`Diagnostic`] (dsl 0.4.0 §8.1, T13) —
/// `cel_message::translate_cel_parse` builds the message/fixits/span from the
/// FAILED slot's own `raw`/`span`, never `err.message`.
///
/// `fill_document(&mut arena, doc)` already consumed the mutable walk; this
/// re-walks `doc` IMMUTABLY in the exact same pre-order
/// (`lute_syntax::walk::for_each_cel_slot` mirrors `for_each_cel_slot_mut`
/// structurally) to recover each failed slot's full `raw`/`span` — a slot is
/// exactly one of `cel_errors` iff it is non-structural-gap (`raw` not
/// whitespace-only, the same filter `fill_document` itself applies) AND was
/// left `ast: None` (fill_document's ONLY paths are `ast = Some(handle)` on
/// success or push an error on failure; it never reorders/inserts/removes
/// slots), so zipping the two same-order sequences pairs each error with its
/// own slot 1:1.
pub(super) fn cel_parse_diagnostics(
    doc: &Document,
    cel_errors: Vec<CelParseError>,
) -> Vec<Diagnostic> {
    let mut failed: Vec<(&str, Span, CelKind)> = Vec::new();
    for_each_cel_slot(doc, &mut |slot| {
        if slot.raw.trim().is_empty() {
            return;
        }
        if slot.ast.is_none() {
            failed.push((slot.raw.as_str(), slot.span, slot.kind));
        }
    });
    debug_assert_eq!(
        failed.len(),
        cel_errors.len(),
        "fill_document's error count must match the (raw non-empty, ast=None) slots found by \
         re-walking the same document in the same pre-order"
    );
    failed
        .into_iter()
        .zip(cel_errors)
        .map(|((raw, slot_span, kind), err)| {
            let t = translate_cel_parse(raw, slot_span, &err, kind);
            Diagnostic {
                code: t.code.to_string(),
                severity: Severity::Error,
                message: t.message,
                evidence: None,
                span: t.span.unwrap_or(slot_span),
                layer: Layer::Cel,
                fixits: t.fixits,
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }
        })
        .collect()
}

/// 0.21.1 T1-6: every def BODY this document declares (`defs:`) or imports
/// (`uses:` schemas) — a `@name` use site is exempt from the CEL gates as a
/// compile-time macro, so without this pass nothing ever looked at the body
/// it expands to: `size()` or CEL that does not even parse all passed
/// `check`. A body that does not parse is `E-CEL-PARSE`; one that does gets
/// the inline slot's profile gate and integer-`%` typing
/// ([`crate::cel_resolve::check_def_body`]).
/// An inline def anchors at its own key; an imported one at the frontmatter,
/// naming its schema file — schema files are not checked on their own.
fn def_body_diagnostics(
    doc: &Document,
    inline: &std::collections::BTreeMap<String, serde_yaml::Value>,
    imports: &SchemaImports,
    schema: &crate::meta::StateSchema,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut check_one = |name: &str, def: &serde_yaml::Value, span: Span, prefix: &str| {
        let Some(cel) = def.get("cel").and_then(|c| c.as_str()) else {
            return; // no body: `E-DEF-DECL` already names it
        };
        let mut arena = CelArena::default();
        let mut diags = match parse_slot(&mut arena, cel, span.byte_start) {
            Ok(_) => {
                let params: Vec<String> =
                    params_from_yaml(def).into_iter().map(|(p, _)| p).collect();
                crate::cel_resolve::check_def_body(name, cel, &params, span, schema)
            }
            Err(err) => {
                let t = translate_cel_parse(cel, span, &err, CelKind::Condition);
                vec![Diagnostic {
                    code: t.code.to_string(),
                    severity: Severity::Error,
                    message: format!("def `{name}`: {}", t.message),
                    evidence: None,
                    span,
                    layer: Layer::Cel,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                }]
            }
        };
        for d in &mut diags {
            d.message.insert_str(0, prefix);
        }
        out.extend(diags);
    };
    for (name, def) in inline {
        check_one(name, def, crate::meta::meta_key_span(&doc.meta, name), "");
    }
    for (name, def) in &imports.defs {
        let origin = imports
            .def_origins
            .get(name)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        check_one(
            name,
            def,
            doc.meta.span,
            &format!("schema import `{origin}`: "),
        );
    }
    out
}

/// Parse errors that corrupt the node stream, making the `Resolved` view
/// misleading (see the Some-vs-None policy in the module docs).
const STRUCTURAL_CODES: &[&str] = &[
    "E-UNCLASSIFIED",
    "E-UNCLOSED-TAG",
    "E-COMMENT-UNTERMINATED",
    "E-META-PARSE",
    // dsl 0.5.0 §2.1: split off E-UNCLASSIFIED / E-UNCLOSED-TAG — each
    // corrupts the node stream the SAME way its parent code did (a dropped
    // line, or a tag whose attrs/close never resolve as intended).
    "E-CONTENT-OUTSIDE-SHOT",
    "E-CONTENT-LINE-BRACKET",
    "E-TAG-NOT-ONE-LINE",
    // dsl §2.3: an inline `<tag …>body</tag>` body is DROPPED from the node
    // stream (the parser consumes whole lines), the same corruption as the
    // wrapped-opener sibling above.
    "E-TAG-INLINE-BODY",
    // FL1 (dsl 0.5.0 §2.1/§2.2): split off E-UNCLASSIFIED — same drop-the-line
    // node-stream corruption as its former residual bucket.
    "E-LEGACY-CONTENT-SIGIL",
];

/// Statically validate a `.lute` document and return its structured result.
///
/// Never panics: every stage degrades to diagnostics + a best-effort view.
pub fn check(input: &CheckInput) -> CheckResult {
    check_parsed(input, parse(&input.text))
}

/// [`check`] over an already-parsed `input.text`: `parsed` MUST be exactly
/// `lute_syntax::parse(&input.text)`, so a batch caller that parsed the
/// document for its own needs does not parse it again.
pub fn check_parsed(input: &CheckInput, parsed: (Document, Vec<Diagnostic>)) -> CheckResult {
    let idx = TextIndex::new(&input.text);

    // 1. Parse the DSL structure (done by the caller).
    let (mut doc, mut parse_diags) = parsed;
    // `questTier` default, the keys `chapters:` derives and beat
    // templates (dsl 0.27.0 §6): ordinary beats before any pass reads them.
    parse_diags.extend(crate::desugar_document(&mut doc, input));
    // dsl 0.24.0 §4: each `::use` of an `effects: true` component performs
    // its body's writes HERE, in this document — splice them in (anchored at
    // the `::use`) so every pass below judges them against this document's
    // schema.
    crate::component_effects::splice_component_effects(
        &mut doc,
        &input.components,
        &input.snapshot,
    );

    // 2. Fill every CEL slot; a parse failure is reported ONCE here and never
    //    aborts the walk (CelSlot isolation). check_cel_slot skips the AST pass
    //    for a slot whose `ast` stayed `None`, so no duplicate CEL diagnostics.
    //    dsl 0.4.0 §8.1 (T13): the writer-voiced translation lives in
    //    `cel_message`, never the backend's own `err.message`.
    let mut arena = CelArena::default();
    let cel_errors = fill_document(&mut arena, &mut doc);
    let cel_diags: Vec<Diagnostic> = cel_parse_diagnostics(&doc, cel_errors);

    // 3–4b. Typed frontmatter + folded schema + merged def tables (one SoT:
    // the public fold_env accessor the compiler also consumes).
    let (folded, fold_diags, state_merge_diags) = fold_env(&doc, input);
    parse_diags.extend(crate::templates::check_body_markers(
        &doc,
        folded.typed.beat_template.is_some(),
    ));
    // dsl 0.28.0 §1: reserved ids are refused where they are declared.
    parse_diags.extend(crate::reserved_names::check_document_ids(
        &doc,
        &folded.typed,
        &input.text,
    ));
    // 0.21.1 T3-8 (seven F3): a frontmatter that does not parse leaves the
    // document with NO environment — no `uses:`/`defaults:` vocabulary, no
    // `state:`, no `defs:`, no id — so every semantic pass below would judge
    // the body against an empty world and report a dozen consequences of the
    // one real error (`E-DOMAIN-UNKNOWN`, `E-UNDECLARED`, …), each with advice
    // that sends the author to the wrong fix. Stop at the syntax layer: the
    // body's own parse errors, its CEL parse errors, and the `E-META-PARSE`.
    // A schema import whose YAML does not parse leaves the same empty world
    // (no cast, no clock, no kinds, no state from it): the import errors
    // are the cause, and the same stop applies.
    let meta_unparsed = fold_diags.iter().any(|d| d.code == "E-META-PARSE");
    let schema_unparsed = input.imports.diags.iter().any(|d| {
        d.code == "E-USES-PARSE"
            && d.related
                .iter()
                .any(|r| r.diagnostic.code == "E-META-PARSE")
    });
    if meta_unparsed || schema_unparsed {
        let import_diags = input
            .imports
            .diags
            .iter()
            .filter(|d| !meta_unparsed && d.code.starts_with("E-USES-"))
            .cloned();
        let mut diags: Vec<Diagnostic> = parse_diags
            .into_iter()
            .chain(cel_diags)
            .chain(fold_diags.into_iter().filter(|d| d.code == "E-META-PARSE"))
            .chain(import_diags)
            .collect();
        normalize_spans(&idx, &input.text, &mut diags);
        crate::evidence::annotate(&mut diags);
        super::postprocess::order_diagnostics(&mut diags);
        return CheckResult {
            ok: false,
            diagnostics: diags,
            resolved: None,
            domain_use: DomainUse {
                at: doc.meta.span,
                ..DomainUse::default()
            },
        };
    }
    // dsl 0.21.0 §3.1: a scene beat's `when` is a frontmatter CEL slot, so
    // `fill_document` (which walks the node tree) never saw it — parse it into
    // the same arena here, reporting a failure as the ordinary `E-CEL-PARSE`.
    // dsl 0.27.0 §5: `spentBy` is a frontmatter condition slot like `when`.
    let mut beat_when_parse_diags: Vec<Diagnostic> = Vec::new();
    let mut parse_beat_slot = |slot: Option<&CelSlot>| -> Option<CelSlot> {
        let mut slot = slot?.clone();
        match parse_slot(&mut arena, &slot.raw, slot.span.byte_start) {
            Ok(handle) => slot.ast = Some(handle),
            Err(err) => {
                let t = translate_cel_parse(&slot.raw, slot.span, &err, slot.kind);
                beat_when_parse_diags.push(Diagnostic {
                    code: t.code.to_string(),
                    severity: Severity::Error,
                    message: t.message,
                    evidence: None,
                    span: t.span.unwrap_or(slot.span),
                    layer: Layer::Cel,
                    fixits: t.fixits,
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                });
            }
        }
        Some(slot)
    };
    let scene_beat = folded.typed.beat.as_ref();
    let beat_when = parse_beat_slot(scene_beat.and_then(|b| b.when.as_ref()));
    let beat_spent_by = parse_beat_slot(scene_beat.and_then(|b| b.spent_by.as_ref()));
    let mut def_body_diags =
        def_body_diagnostics(&doc, &folded.typed.defs, &input.imports, &folded.env.state);
    check_use_def_enum_args(
        &doc,
        &input.components,
        &folded.def_bodies,
        &folded.env.def_types,
        &folded.env.state,
        &mut def_body_diags,
    );
    let permission_diags =
        crate::permissions::check_document_permissions(&doc, &folded.typed, input);
    let env = &folded.env;
    let base_ctx = Ctx {
        env,
        in_match: false,
        match_subject: None,
    };

    // 4c. The FULL merged domain vocabulary a `{domain: X}`-typed attr
    // resolves against (data-catalog foundation A4): `snapshot.domains`
    // (A2 — core baseline + active-plugin `enums`) UNION project-authored
    // domains lifted from this scene's schema imports (A3's
    // `merge_domains`) — computed ONCE in `fold_env` (0.3.0 T7 moved this
    // out of `check()` so a direct `fold_env` caller, like `lute-compile`,
    // sees the identical vocabulary) and threaded by reference to every
    // `check_directive` call site (the scene walk, timeline clips, and
    // component bodies) below — never recomputed per-attr.
    let domains = &folded.domains;

    // 5. Per-node validator walk (directives / cel-slots / set / match / timeline).
    // dsl 0.4.0 §6.2/§6.3: a STANDALONE component-file self-check's OWN
    // `params:` domain table (mirrors `validate_components`'s per-component
    // `param_domains` construction, `param_domain(ty)`) — empty for a
    // Scene/Quest walk, where `Node::Match` never takes the param-admission
    // branch below.
    // dsl 0.24.0 §4: a `speaker` param dispatches over the declared cast
    // (`component_effects::host_param_types`), the same table a `::use`
    // host builds in `validate_components`.
    let own_cast = if folded.typed.component.is_some() {
        crate::cast::declared_cast(&input.snapshot, &input.imports, &folded.typed.cast)
    } else {
        std::collections::BTreeMap::new()
    };
    let param_domains: std::collections::BTreeMap<String, DomainInfo> = if folded
        .typed
        .component
        .is_some()
    {
        let params: Vec<(String, Type)> = folded
            .typed
            .params
            .iter()
            .map(|p| (p.name.clone(), p.ty.clone()))
            .collect();
        crate::component_effects::host_param_types(&params, &folded.typed.speaker_params, &own_cast)
            .into_iter()
            .map(|(name, ty)| (name, param_domain(&ty, domains)))
            .collect()
    } else {
        std::collections::BTreeMap::new()
    };
    // dsl 0.24.0: every `@def` use is checked on its expansion, and a beat's
    // / entry's `when` is an assumption for its body.
    let mut scope = crate::defassign::Scope::of(&folded);
    // dsl 0.27.0 §2 (T1-10): a `::use` reads its omitted params' `@def` defaults.
    scope.components = Some(&input.components);
    let mut walker = Walker {
        snapshot: &input.snapshot,
        providers: &input.providers,
        domains,
        arena: &arena,
        diags: Vec::new(),
        timeline_tables: Vec::new(),
        components: &input.components,
        src: &input.text,
        param_domains,
        scope: &scope,
        assume: None,
        picks: Vec::new(),
        targets: crate::next_labels::next_targets(&doc),
    };
    // Kind-dispatched walk (dsl 0.2.0 §3.1): scene walks `doc.shots` (dsl
    // 0.1.0 grammar, unchanged); quest walks `doc.quests` — each quest's own
    // `start`/`fail` CEL guards (dsl 0.2.0 §6.3) are pure predicates the
    // engine derives `quest.<id>.state` from, so they get the SAME `Bool`
    // `check_cel_slot` treatment a `<when test>` guard gets — then the
    // quest's body, which recurses through the real `Node::On`/
    // `Node::Objective` walk arms below.
    match folded.doc_kind {
        // A COMPONENT file opened as its own root. `resolve_doc_kind` has no
        // Component variant (import-role docs carry no `kind:`), so such a file
        // degrades to `DocKind::Scene` above — and `Walker` admits every logic
        // block, because in a SCENE they are all legal. The presentational
        // contract (dsl 0.4.0 §6.2) therefore never applied on this leg:
        // `lute check c.component.lute` reported `ok` for a body that fails at
        // every `::use` site. This is the mirror of Task 7f (there the
        // standalone leg was too STRICT); the fix is the same shape — route the
        // body through the ONE implementation of the contract rather than
        // teaching `Walker` a second copy of it.
        //
        // `walk_component_body` anchors each diagnostic at the offending node's
        // OWN span, which is already what this leg wants (the `::use` leg
        // re-anchors in `validate_components`). Diags land in `walker.diags` so
        // the single `mem::take` below stays the one drain.
        crate::meta::DocKind::Scene if folded.typed.component.is_some() => {
            let body_scope = BodyScope {
                effects: folded.typed.effects,
                speakers: &folded.typed.speaker_params,
                cast: &own_cast,
                own: component_own_slots(&doc, &input.snapshot, &input.components),
            };
            for shot in &doc.shots {
                walk_component_body(
                    &shot.body,
                    &input.snapshot,
                    &input.providers,
                    domains,
                    &arena,
                    &base_ctx,
                    &input.components,
                    &walker.param_domains,
                    &body_scope,
                    &mut walker.diags,
                );
            }
            // A read of one of its defs is the body's one report
            // (`validate_components` gives each host's `::use` the same).
            walker
                .diags
                .extend(component_def_reads(&doc, &walker.param_domains, env, &[]));
        }
        crate::meta::DocKind::Scene => {
            // dsl 0.21.0 §3.1, §5: the beat's `when` joins the CEL-slot
            // registry like a `<quest start>` — evaluated when the occasion
            // is raised, before the scene runs. Canonical order: `when`, then
            // the shots.
            if let Some(when) = &beat_when {
                walker
                    .diags
                    .extend(check_beat_when(when, &arena, &base_ctx, &scope));
            }
            // dsl 0.27.0 §5: `spentBy` is judged at the same moment.
            if let Some(spent_by) = &beat_spent_by {
                walker
                    .diags
                    .extend(check_beat_when(spent_by, &arena, &base_ctx, &scope));
            }
            let shots: Vec<&[Node]> = doc.shots.iter().map(|s| s.body.as_slice()).collect();
            walker.assume = walker.assumption(beat_when.as_ref(), &shots, &env.state);
            for shot in &doc.shots {
                walker.walk(&shot.body, &base_ctx);
            }
        }
        crate::meta::DocKind::Quest => {
            for quest in &doc.quests {
                if let Some(start) = &quest.start {
                    walker.diags.extend(check_cel_slot(
                        start,
                        &arena,
                        &base_ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                if let Some(fail) = &quest.fail {
                    walker.diags.extend(check_cel_slot(
                        fail,
                        &arena,
                        &base_ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                // dsl 0.27.0 §5: `rearm` is a condition like `start`.
                if let Some(rearm) = &quest.rearm {
                    walker.diags.extend(check_cel_slot(
                        rearm,
                        &arena,
                        &base_ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                walker.walk(&quest.body, &base_ctx);
                // dsl 0.16.0 §2: quest-level `reward.when` slots share
                // `<quest start|fail>`'s Bool profile treatment and run
                // after the body walk to match the canonical walk order
                // (`lute_syntax::walk::quest`).
                for reward in &quest.rewards {
                    if let Some(when) = &reward.when {
                        walker.diags.extend(check_cel_slot(
                            when,
                            &arena,
                            &base_ctx,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                }
            }
        }
        // dsl 0.19.0 §3–§4: a lore document walks each entry's `when`
        // eligibility guard (the SAME `Bool` treatment a `<quest start>`
        // gets) and then its body through the SAME `Walker` a quest body
        // uses — lines, `<match>`, `::set`/`::assert`/`::retract` are checked
        // exactly as in scenes. Canonical order: `when`, then body
        // (`lute_syntax::walk::entry`).
        crate::meta::DocKind::Lore => {
            for entry in &doc.entries {
                // dsl 0.28.0: a kind or `for=` entry reads `occasion.target`
                // typed by its own members.
                let ctx = Ctx {
                    env: folded.env_at(entry.span),
                    in_match: false,
                    match_subject: None,
                };
                if let Some(when) = &entry.when {
                    walker.diags.extend(check_cel_slot(
                        when,
                        &arena,
                        &ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                // dsl 0.27.0 §5: `spentBy` is judged like `when`.
                if let Some(spent_by) = &entry.spent_by {
                    walker.diags.extend(check_cel_slot(
                        spent_by,
                        &arena,
                        &ctx,
                        Some(&ExpectedType::Bool),
                    ));
                }
                walker.assume =
                    walker.assumption(entry.when.as_ref(), &[&entry.body], &ctx.env.state);
                walker.walk(&entry.body, &ctx);
            }
            // dsl 0.23.0 §4: a bundle beat is a scene beat written in a lore
            // file — its `when` gets the scene beat's treatment (Bool slot,
            // fresh guard definite assignment, no `scene.*` reads) and its
            // body the scene shot walk.
            for beat in &doc.beats {
                let ctx = Ctx {
                    env: folded.env_at(beat.span),
                    in_match: false,
                    match_subject: None,
                };
                // dsl 0.28.0 §3: a condition a template header derived is
                // reported at the use with the header key it came from.
                if let Some(when) = &beat.when {
                    let found = check_beat_when(when, &arena, &ctx, &scope);
                    walker.diags.extend(crate::templates::attribute_header(
                        beat,
                        "when",
                        &input.components,
                        found,
                    ));
                }
                if let Some(spent_by) = &beat.spent_by {
                    let found = check_beat_when(spent_by, &arena, &ctx, &scope);
                    walker.diags.extend(crate::templates::attribute_header(
                        beat,
                        "spentBy",
                        &input.components,
                        found,
                    ));
                }
                walker.assume =
                    walker.assumption(beat.when.as_ref(), &[&beat.body], &ctx.env.state);
                walker.walk(&beat.body, &ctx);
            }
        }
    }

    // dsl 0.27.0 §4: the gates of the occasions this document's beats answer
    // and the project's `terminal:`, checked as condition slots.
    walker.diags.extend(crate::gates::check_seam_texts(
        &doc,
        &folded,
        &input.imports,
        &base_ctx,
    ));
    // dsl 0.27.0 §5: every season's `live:`, judged in this document's
    // environment like the seam's conditions.
    walker.diags.extend(crate::season::check_live_texts(
        &doc,
        &folded.typed.seasons,
        &input.imports.seasons,
        &input.imports.season_lives,
        &base_ctx,
        &scope,
    ));

    // 6. Definite assignment, kind-dispatched (dsl 0.2.0 §4.4): scene runs
    //    ONCE over the whole concatenated shot stream (carry-forward #1 —
    //    `scene.*`/`run.*` persist across shots within the episode); quest
    //    runs PER `quest.body` — quest instances share no dominance relation
    //    with one another, so each quest's def-assignment is its own scope,
    //    never folded across quests.
    // Captures the scene document's final `Assigned` set — defassign's own
    // must-write join (`intersect_all`) — for the envelope layer's
    // guaranteed-write set `G` (`crate::envelope::guaranteed`, connectivity
    // T8/§4.3). T9/T10 wire this into the per-node envelope; quest bodies are
    // out of envelope scope (no whole-document dominance relation, see above).
    let mut _scene_assigned: crate::defassign::Assigned = crate::defassign::Assigned::new();
    // Single source of truth for the T4.4/T4.6 "domain-exhaustive `<match>`
    // subject" exemption (dsl §7 soundness invariant): computed HERE, off
    // the SAME node lists `check_definite_assignment` walks, and consumed
    // BOTH by `suppress_exhaustive_subject_reads` below AND by connectivity
    // T11's project-envelope wiring (`lute-cli::run_check_project`,
    // `assemble_root_scenario`), which calls
    // `crate::defassign::exhaustive_match_subject_spans` directly on its own
    // re-derived node list — so a future change to exhaustiveness rules can
    // never drift between the standalone-check suppression and the
    // project-level reconciliation (see that function's own doc comment).
    let mut exhaustive_subject_spans: Vec<Span> = Vec::new();
    let defassign_diags: Vec<Diagnostic> = match folded.doc_kind {
        crate::meta::DocKind::Scene => {
            let all_nodes: Vec<Node> = doc
                .shots
                .iter()
                .flat_map(|s| s.body.iter().cloned())
                .collect();
            let (diags, assigned, _reads) =
                check_definite_assignment(&all_nodes, &scope, beat_when.as_ref());
            exhaustive_subject_spans =
                crate::defassign::exhaustive_match_subject_spans(&all_nodes, &scope);
            _scene_assigned = assigned;
            diags
        }
        crate::meta::DocKind::Quest => doc
            .quests
            .iter()
            .flat_map(|q| {
                // `start`/`fail` (dsl 0.2.0 §6.3) are evaluated at QUEST ENTRY —
                // nothing dominates them, so they get their own fresh
                // (empty-assigned-set) defassign check rather than folding into
                // `q.body`'s walk (which would wrongly let an in-body write
                // "prove" a guard evaluated strictly before the body runs).
                let mut ds = Vec::new();
                if let Some(start) = &q.start {
                    ds.extend(check_quest_guard_defassign(start, &scope));
                }
                if let Some(fail) = &q.fail {
                    ds.extend(check_quest_guard_defassign(fail, &scope));
                }
                // dsl 0.27.0 §5: `rearm` is watched from outside the quest.
                if let Some(rearm) = &q.rearm {
                    ds.extend(check_quest_guard_defassign(rearm, &scope));
                }
                // dsl 0.16.0 §2: a quest-level `reward.when` is a
                // quest-entry-shaped guard exactly like `start`/`fail` —
                // gets its own fresh defassign check (`isSet(p) && …`
                // intra-expression narrowing preserved, no dominance from
                // the body which runs strictly BEFORE the fresh transition
                // that grants the reward).
                for reward in &q.rewards {
                    if let Some(when) = &reward.when {
                        ds.extend(check_quest_guard_defassign(when, &scope));
                    }
                }
                let (diags, _, _reads) = check_definite_assignment(&q.body, &scope, None);
                exhaustive_subject_spans.extend(crate::defassign::exhaustive_match_subject_spans(
                    &q.body, &scope,
                ));
                ds.extend(diags);
                ds
            })
            .collect(),
        // dsl 0.19.0 §6: each entry is presented on its own against live
        // state — no entry dominates another — so, like a quest, each entry
        // is its own definite-assignment scope; its `when` is evaluated
        // before the body runs and gets the fresh entry-guard check
        // `<quest start>` gets, and is an assumption for the body (dsl
        // 0.24.0). dsl 0.23.0 §4: each bundle beat is presented on its own
        // too — one scope per beat body (its `when` got the fresh guard check
        // with the scene beat treatment in the walk above), assumed likewise.
        crate::meta::DocKind::Lore => {
            let mut ds: Vec<Diagnostic> = doc
                .entries
                .iter()
                .flat_map(|e| {
                    let mut ds = Vec::new();
                    if let Some(when) = &e.when {
                        ds.extend(check_quest_guard_defassign(when, &scope));
                    }
                    // dsl 0.27.0 §5: judged beside `when`, before the body.
                    if let Some(spent_by) = &e.spent_by {
                        ds.extend(check_quest_guard_defassign(spent_by, &scope));
                    }
                    // dsl 0.28.0: over the entry's own `occasion.target`.
                    let own = scope.with_schema(&folded.env_at(e.span).state);
                    let (diags, _, _reads) =
                        check_definite_assignment(&e.body, &own, e.when.as_ref());
                    exhaustive_subject_spans.extend(
                        crate::defassign::exhaustive_match_subject_spans(&e.body, &own),
                    );
                    ds.extend(diags);
                    ds
                })
                .collect();
            for beat in &doc.beats {
                let own = scope.with_schema(&folded.env_at(beat.span).state);
                let (diags, _, _reads) =
                    check_definite_assignment(&beat.body, &own, beat.when.as_ref());
                exhaustive_subject_spans.extend(crate::defassign::exhaustive_match_subject_spans(
                    &beat.body, &own,
                ));
                ds.extend(diags);
            }
            ds
        }
    };

    // 6b. Duplicate authored line codes (dsl §12): two `:line`s for the same
    //     speaker with the same trimmed `code` derive identical `lineId`/
    //     `voiceKey` join keys — a clean-check invariant the compile gate relies
    //     on. Whole-document, per-speaker; owns `E-DUP-LINE-CODE`. The ROOT
    //     document only: each imported component body gets its OWN isolated
    //     run of this same pass in `validate_components` (Task 7c).
    let line_code_diags =
        crate::match_check::check_line_codes_with_policy(&doc, input.snapshot.identity_require_stable);
    let mut instance_diags = Vec::new();
    for shot in &doc.shots {
        super::use_site::check_instance_scope(&shot.body, &mut instance_diags);
    }
    for quest in &doc.quests {
        super::use_site::check_instance_scope(&quest.body, &mut instance_diags);
    }
    for entry in &doc.entries {
        super::use_site::check_instance_scope(&entry.body, &mut instance_diags);
    }
    for beat in &doc.beats {
        super::use_site::check_instance_scope(&beat.body, &mut instance_diags);
    }
    // 6b'. dsl 0.23.0 §7: speakers against the declared cast (plugins ∪
    //      imported schemas ∪ this schema's own `cast:`), root document only.
    let cast = &folded.cast;
    let mut cast_diags = crate::cast::check_speakers(&doc, cast);
    // dsl 0.24.0 §4: `emotion=` against the speaker's `emotions:`, and
    // `W-CAST-ABSENT` (decided without facts here; `check-project`
    // re-decides it under the fact envelope).
    cast_diags.extend(crate::cast::check_emotions(
        &doc,
        cast,
        &folded.domains,
        &folded.use_lines,
    ));
    cast_diags.extend(crate::cast::check_presence(
        std::path::Path::new(&input.uri),
        &doc,
        &folded,
        None,
    ));

    // 7. Resolved view: injection fold + the timeline tables gathered in the walk.
    //    Unlike steps 6b/8, this pass is NOT root-only: `fold_injections`
    //    enters each `::use` in document position, folding the component body
    //    against the stage state inherited AT that site (Task 7g — see
    //    `fold_use`). That position dependence is why the body is folded here
    //    rather than isolated in `validate_components` like the line-code and
    //    reachability passes.
    let mut inject_state = StageState::default();
    let mut injections = Vec::new();
    let mut using = Vec::new();
    for shot in &doc.shots {
        fold_injections(
            &shot.body,
            &mut inject_state,
            &mut injections,
            domains,
            &input.components,
            &mut using,
        );
    }
    // dsl 0.23.0 §4: each bundle beat is staged on its own, from an empty
    // stage, exactly as a scene is.
    for beat in &doc.beats {
        let mut beat_state = StageState::default();
        fold_injections(
            &beat.body,
            &mut beat_state,
            &mut injections,
            domains,
            &input.components,
            &mut using,
        );
        inject_state.diags.append(&mut beat_state.diags);
    }
    let inject_diags = std::mem::take(&mut inject_state.diags);
    // `node_summary` already covers `Node::On`/`Node::Objective` (Plan A), so
    // the quest arm reuses it verbatim — no wildcard, both surfaces summarized
    // identically.
    let commands_preview: Vec<String> = match folded.doc_kind {
        crate::meta::DocKind::Scene => doc
            .shots
            .iter()
            .flat_map(|s| s.body.iter().map(node_summary))
            .collect(),
        crate::meta::DocKind::Quest => doc
            .quests
            .iter()
            .flat_map(|q| q.body.iter().map(node_summary))
            .collect(),
        crate::meta::DocKind::Lore => doc
            .entries
            .iter()
            .flat_map(|e| e.body.iter().map(node_summary))
            .chain(
                doc.beats
                    .iter()
                    .flat_map(|b| b.body.iter().map(node_summary)),
            )
            .collect(),
    };

    // 8. Collect every diagnostic, then apply the ordering contract.
    let mut diags = Vec::new();
    diags.extend(parse_diags);
    diags.extend(cel_diags);
    diags.extend(beat_when_parse_diags);
    diags.extend(def_body_diags);
    diags.extend(fold_diags);
    diags.extend(permission_diags);
    // Rule-guard CEL firewall (dsl 0.3.0 §7.2/§7.3, D7, 0.3.0 T8): holds()/
    // count()/validAt()/now() inside a rule-body guard, plus the ordinary
    // profile/path-declaredness checks — after `base_ctx` (needs `ctx.env`)
    // is constructed above.
    diags.extend(check_rule_guards(&env.rel_vocab, &base_ctx));
    // Round-5 T3-24: a quest left user-tier by default whose conditions read
    // only run state.
    diags.extend(crate::project_check::check_quest_tier_implicit(
        &doc, &folded,
    ));
    // A constant `rearm=`, and a subquest's `rearm=` (same-document parent).
    diags.extend(crate::project_check::check_doc_quest_rearm(&doc, &folded));
    diags.extend(input.imports.diags.clone());
    // Component-import resolution diagnostics (dsl §13) + the per-component
    // body validation and `::use` expansion-cycle diagnostics, reported at
    // the scene frontmatter span (a component file's own spans cannot be
    // represented in this document's diagnostic surface). dsl 0.5.1 §4: a
    // component body diagnostic (e.g. `E-COMPONENT-STATE`) whose file
    // already has its own `E-COMPONENT-PARSE` import failure ALSO gets
    // folded (as a copy) into that diagnostic's `related` list, with its
    // "(N issue(s))" count updated to match — additive, never a relocation:
    // every body diagnostic still lands in the flat top-level list exactly
    // as before, so an existing consumer scanning it for e.g.
    // `E-COMPONENT-STATE` keeps finding it there regardless of whether the
    // SAME component also failed to parse.
    let mut component_diags = input.components.diags.clone();
    // A component file that reaches itself through `defaults: components:`
    // re-imports its own text: an import failure whose every cause lies in
    // THIS file is the file's own parse/frontmatter diagnostics a second time
    // (round-5 T3-4) — they are already reported above, at their own lines.
    if let Ok(own) = std::fs::canonicalize(&input.uri) {
        let own = own.display().to_string();
        component_diags.retain(|d| {
            d.code != "E-COMPONENT-PARSE"
                || d.related.is_empty()
                || d.related.iter().any(|r| r.file != own)
        });
    }
    let component_body_diags = validate_components(
        &input.components,
        &input.snapshot,
        &input.providers,
        domains,
        doc.meta.span,
        &component_use_sites(&doc, &input.components),
        &cast,
        env,
        if folded.typed.component.is_some() {
            &folded.typed.params
        } else {
            &[]
        },
        input.snapshot.identity_require_stable,
    );
    let component_body_diags = crate::component_import::merge_component_body_diags(
        &mut component_diags,
        component_body_diags,
    );
    diags.extend(component_diags);
    diags.extend(component_body_diags);
    diags.extend(check_use_speaker_args(
        &doc,
        &input.components,
        &cast,
        &folded.typed.speaker_params,
    ));
    diags.extend(state_merge_diags);
    diags.extend(std::mem::take(&mut walker.diags));
    diags.extend(defassign_diags);
    diags.extend(line_code_diags);
    diags.extend(instance_diags);
    diags.extend(cast_diags);
    // 6c. Connectivity layer (T2, dsl connectivity spec §2.1/§5): a scene's
    // `after:` frontmatter and each quest's `after` attribute share the SAME
    // restricted `visited()`/`completed()`/`active()` formula grammar T1's
    // `prereq` module defines. `check()` (single-file) validates ONLY that
    // local grammar here — it has no project graph to resolve the atoms'
    // targets against, so node existence is deferred to `check-project`
    // (Task 3+).
    //
    // §2.1: frontmatter `after:` is a SCENE-ONLY prerequisite surface — a
    // quest pack declares its prerequisite via the per-`<quest>` `after`
    // attribute instead (handled separately below). `TypedMeta.after` is
    // still lifted uniformly for every `MetaKind` (meta.rs) so a non-scene
    // doc's `after:` key still reaches the ordinary unknown-meta-key check;
    // only the prereq-grammar validation is gated to `DocKind::Scene` here,
    // so it is never fed a surface the spec doesn't define as a prerequisite.
    if folded.doc_kind == crate::meta::DocKind::Scene {
        if let Some(after) = &folded.typed.after {
            if !after.is_empty() {
                let after_span = crate::meta::meta_key_span(&doc.meta, "after");
                let (_, after_diags) = crate::prereq::parse_prereq(after, after_span);
                diags.extend(after_diags);
            }
        }
    }
    for quest in &doc.quests {
        if let Some(after) = &quest.follows {
            if !after.is_empty() {
                let (_, after_diags) = crate::prereq::parse_prereq(after, quest.follows_span);
                diags.extend(after_diags);
            }
        }
    }
    // dsl 0.6.1 §3: frontmatter `luteVersion` freshness signal (a stale copied
    // stamp), independent of every other meta check — D13 keeps `luteVersion`
    // capability-unvalidated, so this warning is its ONLY treatment.
    diags.extend(check_lute_version_stale(&folded.typed, &doc.meta));
    // 0.4.0 T4 (§5.2 whole-document reachability pass): E-ARM-DEAD (dead
    // guard + subsumption) + W-OTHERWISE-DEAD.
    diags.extend(check_reachability(&doc, &folded, &input.snapshot));
    // 0.21.1 T1-3/T1-4: a def in an attribute must fold to a constant
    // (E-ATTR-DEF-DYNAMIC) and a `{{@def}}` must inline into one standalone
    // expression (E-INTERP-DEF) — the artifact has no defs table.
    diags.extend(crate::def_inline::check_def_inlining(&doc, &folded, input));
    // dsl 0.12.0: forward-jump labels — E-MARK-DUP / E-NEXT-UNDEFINED /
    // E-NEXT-BACKWARD, a whole-document pass (the label namespace spans
    // every shot/quest, unlike reachability's per-body scope above).
    diags.extend(crate::next_labels::check_next_labels(&doc));
    // dsl 0.18.0 §3: W-WHEN-TEST-LITERAL — a `<when test>` that only compares
    // `$` to literals, with its `is=` rewrite as a `migrate` fixit (the same
    // edit `lute fix` applies). Whole-document, every doc kind (components
    // included, whose bodies bypass `Walker`).
    diags.extend(crate::when_test_literal::check_when_test_literals(
        &doc,
        &input.text,
    ));
    diags.extend(inject_diags);
    // Table-driven grammar admission (dsl 0.2.0 §3.3, §6.7): per-kind,
    // per-context construct legality. `E-GRAMMAR-NOT-ADMITTED` is semantic, NOT
    // a `STRUCTURAL_CODE` — the resolved view stays `Some` even when a document
    // uses a construct its kind forbids.
    diags.extend(crate::admission::check_admission(
        &doc,
        folded.doc_kind,
        &input.snapshot,
    ));
    // An illegal `once` / `tier` value naming a declared season, and a
    // scene's legacy `season:` holding one.
    crate::season::hint_declared(
        &doc,
        folded.doc_kind == crate::meta::DocKind::Scene,
        &env.seasons,
        &env.rel_vocab,
        &mut diags,
    );

    // is_exhaustive suppression (carry-forward, T4.6 x T4.4): drop a maybe-unset
    // read whose span is a domain-exhaustive `<match>` subject.
    suppress_exhaustive_subject_reads(&mut diags, &exhaustive_subject_spans);
    // C4 (dsl 0.4.0 §8.2): a `<when>` arm already flagged E-ARM-DEAD must not
    // additionally produce the pre-existing W-OVERLAP-ARMS — the dead-arm
    // error is the root.
    suppress_dead_arm_overlaps(&mut diags);
    // dsl §2.3: a block whose CHILDREN did not parse gives the verdicts drawn
    // from that child list nothing to judge — drop them rather than claim the
    // author's logic is wrong on top of their parse error.
    suppress_unparsed_child_list_verdicts(&mut diags);

    // dsl 0.10.0 §9 rule 3: the STANDALONE leg reports its own malformed
    // `params:`. `component_import.rs` already does this for the IMPORTER and
    // `TypedMeta.params_malformed` is the very same fact — the standalone leg
    // simply never asked, so all it ever printed was the consequence. The
    // consequence is mechanical: the `defs`/`def_types`/`def_params` seeding
    // above registers `typed.params`, `get_params` drops every entry it cannot
    // deserialize, and so each `@param` in the body resolves against an empty
    // namespace. Reporting only that sends the author to `defs:` for a param
    // they declared four lines up, which is why the suppression is part of the
    // same rule and not a separate nicety.
    //
    // Scoped to THIS document's own `params:`. An IMPORTING document can carry
    // `E-COMPONENT-PARSE` for a component it imports while having a genuine
    // `E-UNDECLARED-REF` of its own — a file the malformed `params:` says
    // nothing about — so keying the `retain` on the code alone would lose a real
    // error.
    //
    // The message is `component_import.rs`'s, minus the file name: here the
    // offending file IS the document being checked. Same code, same
    // `Layer::Content`, so `--deny`/layer filters see one thing.
    if folded.typed.component.is_some() && folded.typed.params_malformed {
        diags.retain(|d| d.code != "E-UNDECLARED-REF");
        diags.push(Diagnostic {
            code: "E-COMPONENT-PARSE".to_string(),
            severity: Severity::Error,
            message: "this component has a malformed `params:` — each entry must be \
                      `name: <type>` or the long form `name: { type: <type>, default: <value> }` \
                      (dsl §13, 0.26.0 §3.3)"
                .to_string(),
            evidence: None,
            span: doc.meta.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }

    // dsl 0.26.0 §4: one of each identical report inside a guarded `::use`
    // (its guard rides every write it splices into the host).
    let diags = dedup_guarded_use_reports(&doc, diags);
    // Dedup overlapping `E-UNDECLARED` (carry-forward #4) BEFORE the sort.
    let mut diags = dedup_undeclared(dedup_rehomed(diags));

    // Normalize every span's line/column/utf16 from its bytes (some validators
    // leave them zeroed), then sort deterministically (carry-forward #3),
    // the cause first where several share a position.
    normalize_spans(&idx, &input.text, &mut diags);
    super::postprocess::order_diagnostics(&mut diags);

    // C3 (dsl 0.4.0 §8.2/D12): a failed uses/extends/components import
    // suppresses the absence diagnostics that depend on the merge it never
    // built. C1 (§8.2/D11): collapse the remaining same-root repeats to one
    // primary + `covered` occurrences. Both run AFTER the sort above, so
    // `diags` is already document order — C1 collapse never reorders it.
    let diags = suppress_unproven_absence(diags);
    let diags = collapse_same_root(diags);
    // A name the manifest's `defaults.uses` declares, missing because this
    // document's own `uses:` replaced that list: the error says so.
    let mut diags = crate::defaults_note::note_replaced_uses(&doc, input, diags);

    // Some-vs-None policy for the resolved view.
    let structural_break = diags
        .iter()
        .any(|d| STRUCTURAL_CODES.contains(&d.code.as_str()));
    let resolved = if structural_break {
        None
    } else {
        Some(Resolved {
            commands_preview,
            timeline_tables: walker.timeline_tables,
            injections,
        })
    };

    crate::evidence::annotate(&mut diags);
    let ok = !diags.iter().any(|d| d.severity == Severity::Error);
    let _ = &input.uri; // carried for the surfaces; check() itself is uri-agnostic.
    CheckResult {
        ok,
        diagnostics: diags,
        resolved,
        // dsl 0.10.0 §11.1 (**D-V**): both halves of the project-wide
        // domain-read question, computed from THIS document's merged vocabulary
        // and THIS document's resolved snapshot. `check()` draws no conclusion
        // from them — `check-project` unions them across the root.
        domain_use: DomainUse {
            // The clock's own enums (`clock.slot`, `clock.weekdayLabel`) are
            // nameable domains, not declarations of this project.
            declared: domains
                .keys()
                .filter(|k| !lute_manifest::clock::is_clock_path(k))
                .cloned()
                .collect(),
            read: {
                let mut read = crate::project_check::domain_reading_set(&input.snapshot);
                read.extend(crate::project_check::domain_reads_from_relations(
                    &env.rel_vocab,
                ));
                read.extend(crate::project_check::domain_reads_from_state(&env.state));
                read.extend(crate::project_check::domain_reads_from_kinds(
                    &env.rel_vocab,
                    std::iter::once(crate::usage::document_read_view(&input.text).as_str())
                        .chain(folded.def_bodies.values().map(String::as_str)),
                ));
                // A `target="kind:<kind>"` or `for="kind:<kind>"` beat (scene,
                // entry or bundle beat) reads its kind.
                let scene = folded.typed.beat.as_ref();
                let scene_kinds = scene.into_iter().flat_map(|b| {
                    b.target
                        .as_deref()
                        .into_iter()
                        .chain(b.for_kind.as_ref().map(|(f, _)| f.as_str()))
                });
                let entry_kinds = doc.entries.iter().flat_map(|e| {
                    e.target
                        .iter()
                        .chain(e.for_kind.iter())
                        .map(|(t, _)| t.as_str())
                });
                let beat_kinds = doc.beats.iter().flat_map(|b| {
                    b.target
                        .iter()
                        .chain(b.for_kind.iter())
                        .map(|(t, _)| t.as_str())
                });
                read.extend(
                    scene_kinds
                        .chain(entry_kinds)
                        .chain(beat_kinds)
                        .filter_map(crate::lore::kind_target)
                        .map(str::to_string),
                );
                read
            },
            at: doc.meta.span,
            homes: env
                .rel_vocab
                .origins
                .domains
                .iter()
                .map(|(n, o)| (n.clone(), DomainHome::Imported(o.clone())))
                .chain(folded.typed.domains.keys().map(|n| {
                    (
                        n.clone(),
                        DomainHome::Local(crate::meta::meta_key_span(&doc.meta, n)),
                    )
                }))
                .collect(),
        },
    }
}

/// A short, one-line desugared summary of a top-level node for the preview.
fn node_summary(node: &Node) -> String {
    match node {
        Node::Line(l) => format!(":{}", l.speaker),
        Node::Directive(d) => format!("::{}", d.tag),
        Node::Set(s) => format!("::set{{{} {} …}}", s.path, s.op),
        Node::Branch(b) => format!("<branch id=\"{}\"> ({} choices)", b.id, b.choices.len()),
        Node::Match(m) => format!("<match on=\"{}\"> ({} arms)", m.subject.raw, m.arms.len()),
        Node::Timeline(tl) => format!("<timeline> ({} tracks)", tl.tracks.len()),
        Node::Hub(h) => format!("<hub> ({} choices)", h.choices.len()),
        Node::On(o) => format!("<on event=\"{}\">", o.event),
        Node::Objective(o) => format!("<objective id=\"{}\">", o.id),
        Node::Assert(a) => format!("::assert{{{}(…)}}", a.pattern.relation),
        Node::Retract(r) => format!("::retract{{{}(…)}}", r.pattern.relation),
    }
}
