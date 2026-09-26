//! The per-node validator walk ([`Walker`]) over scene / quest / entry bodies.

use super::*;

/// The per-node validator walk. Holds the read-only capability surface and the
/// mutable diagnostic/table/suppression accumulators; `Ctx` is passed per level
/// so `in_match`/`match_subject` can be toggled for `<match>` arms without
/// re-cloning the schema on every node.
pub(super) struct Walker<'a> {
    pub(super) snapshot: &'a CapabilitySnapshot,
    pub(super) providers: &'a ProviderSet,
    /// The FULL merged domain vocabulary (data-catalog foundation A4):
    /// `snapshot.domains` UNION project-authored schema-import domains
    /// (A3's `merge_domains`), computed ONCE in `check()` and threaded here
    /// so `Type::Domain(name)` attrs resolve without recomputing the union.
    pub(super) domains: &'a std::collections::BTreeMap<String, Domain>,
    pub(super) arena: &'a CelArena,
    pub(super) diags: Vec<Diagnostic>,
    pub(super) timeline_tables: Vec<ResolvedTimeline>,
    /// Resolved `components:` table (dsl §13): the target of `::use` invocations.
    pub(super) components: &'a ComponentSet,
    /// The raw document source: `persist_removed_diag` reads it to widen the
    /// `E-PERSIST-REMOVED` deletion fixit onto one adjacent whitespace
    /// separator (dsl §4.5), so no stray double space is left behind.
    pub(super) src: &'a str,
    /// dsl 0.4.0 §6.2/§6.3: this document's OWN declared `params:` domains,
    /// non-empty ONLY for a STANDALONE `MetaKind::Component` self-check (the
    /// `Node::Match` arm below dispatches a bare-`@param` subject through
    /// `check_param_match` — the SAME admission rule `walk_component_body`
    /// applies transitively — instead of the ordinary state-path
    /// exhaustiveness engine, which has no notion of a param's declared
    /// domain and would false-flag `E-NONEXHAUSTIVE` on every finite-domain
    /// param match, `docs/examples/components/reaction.component.lute`
    /// (0.4.0 T8) included). Always empty for a Scene/Quest walk.
    pub(super) param_domains: std::collections::BTreeMap<String, DomainInfo>,
    /// The document's definite-assignment scope (dsl 0.24.0): the def table a
    /// `<match on="@def">` subject resolves through.
    pub(super) scope: &'a crate::defassign::Scope<'a>,
    /// The body being walked's beat / entry `when` as an assumption (dsl
    /// 0.24.0): a `<match>` needs no arm for a value it rules out — `unset`
    /// included — exactly as reachability's `E-ARM-DEAD` reads it.
    pub(super) assume: Option<crate::reachability::Assumption>,
}

impl Walker<'_> {
    /// `when` as an assumption over `bodies` (dsl 0.24.0).
    pub(super) fn assumption(
        &self,
        when: Option<&lute_syntax::ast::CelSlot>,
        bodies: &[&[Node]],
    ) -> Option<crate::reachability::Assumption> {
        crate::reachability::Assumption::new(
            when?,
            bodies,
            &self.scope.defs,
            self.scope.def_types,
            self.scope.schema,
            self.snapshot,
        )
    }

    pub(super) fn walk(&mut self, nodes: &[Node], ctx: &Ctx<'_>) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    self.check_attr_refs(&l.attrs, ctx, None);
                    crate::content_line::check_content_line_attrs(
                        l,
                        self.snapshot,
                        self.providers,
                        self.domains,
                        &mut self.diags,
                    );
                    check_interps(&l.interps, ctx, &mut self.diags);
                    self.diags.extend(text_looks_like_ref(l, ctx));
                    // dsl 0.26.0 §3.2: `@@p:` speaks as a component's
                    // `speaker` param — there is none outside a component.
                    if let Some(p) = l.speaker.strip_prefix('@') {
                        self.diags.push(use_diag(
                            E_COMPONENT_ARG,
                            format!(
                                "`@@{p}:` speaks as a component's `speaker` param `{p}`, and this \
                                 document is no component — write the cast id (`@{p}:`) (dsl \
                                 0.26.0 §3.2)"
                            ),
                            l.span,
                        ));
                    }
                    if let Some(when) = &l.when {
                        // D9 (dsl 0.4.0 §7.2): `$` is NOT in scope in a
                        // content-line `when=`, matching `<on when>` — force
                        // in_match=false/match_subject=None even when this
                        // line sits inside a `<match>` arm (the quest
                        // start/fail precedent above: check.rs:~608).
                        let ctx_no_dollar = Ctx {
                            env: ctx.env,
                            in_match: false,
                            match_subject: None,
                        };
                        self.diags.extend(check_cel_slot(
                            when,
                            self.arena,
                            &ctx_no_dollar,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                }
                Node::Directive(d) if d.tag == "use" => {
                    // `use` is a reserved directive (dsl §13): recognized BEFORE
                    // the unknown-directive check, so it is never
                    // `E-UNKNOWN-DIRECTIVE`. It is a component invocation, not a
                    // snapshot directive.
                    check_use(d, self.components, ctx, &mut self.diags);
                    check_use_typed_args(
                        d,
                        self.components,
                        self.snapshot,
                        self.providers,
                        self.domains,
                        &mut self.diags,
                    );
                    check_use_interp_args(d, self.components, ctx, &mut self.diags);
                    // `@ref`-valued args still resolve in the current scope; there
                    // is no directive decl to type them against.
                    self.check_attr_refs(&d.attrs, ctx, None);
                    self.diags
                        .extend(check_directive_when(d, self.snapshot, self.arena, ctx));
                }
                Node::Directive(d) if d.is_accept() => {
                    // dsl 0.21.0 §7a.3: `::accept` is a core directive of the
                    // language, recognized BEFORE the capability lookup (never
                    // `E-UNKNOWN-DIRECTIVE`). Its one attribute is a quest id,
                    // never a `@ref`, so there is no attr ref to resolve.
                    crate::accept::check_accept_directive(d, &mut self.diags);
                    self.diags
                        .extend(check_directive_when(d, self.snapshot, self.arena, ctx));
                }
                // dsl 0.27.0 §6: `::body` is a template marker, judged by
                // `templates::check_body_markers` — never a capability directive.
                Node::Directive(d) if d.tag == crate::templates::BODY_DIRECTIVE => {}
                Node::Directive(d) => {
                    self.diags.extend(check_directive(
                        d,
                        self.snapshot,
                        self.providers,
                        self.domains,
                        ctx,
                    ));
                    self.check_attr_refs(&d.attrs, ctx, Some(&d.tag));
                    self.diags
                        .extend(check_directive_when(d, self.snapshot, self.arena, ctx));
                }
                Node::Set(s) => {
                    self.diags.extend(check_set(s, &ctx.env.state, ctx));
                    // dsl 0.10.0 §3: the RHS half `set_op` explicitly does not
                    // do (`set_op.rs:27-29`).
                    self.diags.extend(crate::set_type::check_set_type(
                        s,
                        self.arena,
                        &ctx.env.state,
                    ));
                    let expected = resolve_type(&s.path, &ctx.env.state)
                        .cloned()
                        .map(ExpectedType::Ty);
                    self.diags
                        .extend(check_cel_slot(&s.expr, self.arena, ctx, expected.as_ref()));
                    if let Some(when) = &s.when {
                        // dsl 0.24.0 §1: `::set{… when=}` — a Bool condition
                        // with the SAME "$ not in scope" rule as a content-line
                        // `when=` (D9), even inside a `<match>` arm.
                        let ctx_no_dollar = Ctx {
                            env: ctx.env,
                            in_match: false,
                            match_subject: None,
                        };
                        self.diags.extend(check_cel_slot(
                            when,
                            self.arena,
                            &ctx_no_dollar,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                }
                Node::Branch(b) => {
                    // `E-DUP-BRANCH` + decl folding happened in the pre-pass; here
                    // we only validate the branch's own attrs and recurse.
                    self.check_attr_refs(&b.attrs, ctx, None);
                    crate::logic_attrs::check_branch_attrs(b, &mut self.diags);
                    for choice in &b.choices {
                        self.check_attr_refs(&choice.attrs, ctx, None);
                        crate::logic_attrs::check_choice_attrs(
                            choice,
                            crate::logic_attrs::ChoicePos::Branch,
                            &mut self.diags,
                        );
                        check_choice_record(choice, ctx, self.src, &mut self.diags);
                        // §7.6: a `<choice label>` string MAY embed `{{…}}`
                        // interpolations. Labels are String attrs, so their interps
                        // are not in the AST — scan and validate them via the same
                        // referent path as content lines (E-UNDECLARED /
                        // E-UNDECLARED-REF / E-REF-TYPE / §7.6 grammar).
                        check_interps(
                            &scan_label_interps(&choice.label, choice.span),
                            ctx,
                            &mut self.diags,
                        );
                        if let Some(when) = &choice.when {
                            self.diags.extend(check_cel_slot(
                                when,
                                self.arena,
                                ctx,
                                Some(&ExpectedType::Bool),
                            ));
                        }
                        self.walk(&choice.body, ctx);
                    }
                }
                Node::Match(m) => {
                    crate::logic_attrs::check_match_attrs(m, &mut self.diags);
                    for arm in &m.arms {
                        crate::logic_attrs::check_arm_attrs(arm, &mut self.diags);
                    }
                    // The subject expression is evaluated OUTSIDE match scope: `$`
                    // is only valid in a `<when test>` (dsl §8.2), never in `on=`.
                    // Force `in_match=false` so a nested `<match on="$">` (whose
                    // incoming ctx has in_match=true from the enclosing arm) is
                    // correctly flagged E-DOLLAR-OUTSIDE-MATCH.
                    let subject_ctx = Ctx {
                        env: ctx.env,
                        in_match: false,
                        match_subject: None,
                    };
                    let subject_expected = resolve_type(&m.subject.raw, &subject_ctx.env.state)
                        .cloned()
                        .map(ExpectedType::Ty);
                    self.diags.extend(check_cel_slot(
                        &m.subject,
                        self.arena,
                        &subject_ctx,
                        subject_expected.as_ref(),
                    ));
                    // dsl 0.27.0 §2 (T1-5a): the same firewall through `@def`s.
                    self.diags
                        .extend(crate::cel_resolve::check_match_subject_defs(
                            &m.subject,
                            &self.scope.defs,
                        ));
                    // dsl 0.4.0 §6.2/§6.3: a bare `@param` subject naming one of
                    // THIS document's own declared params (`self.param_domains`
                    // is non-empty ONLY for a standalone `MetaKind::Component`
                    // self-check) dispatches through the SAME admitted-form
                    // check the transitive `walk_component_body` walk applies —
                    // never the ordinary state-path exhaustiveness engine, which
                    // has no notion of a param's declared domain and would
                    // false-flag `E-NONEXHAUSTIVE` on every finite-domain param
                    // match.
                    match bare_param_ref(&m.subject.raw)
                        .and_then(|name| self.param_domains.get(&name).cloned())
                    {
                        Some(dom) => {
                            self.diags.extend(check_param_match(m, dom, ctx));
                        }
                        None => {
                            let (subject, info) = crate::match_check::resolve_subject(
                                m,
                                &self.scope.defs,
                                self.scope.def_types,
                                &ctx.env.state,
                            );
                            let assume = self.assume.as_ref();
                            let ruled_out = |item: &crate::match_check::CoverItem| {
                                subject
                                    .as_deref()
                                    .zip(assume)
                                    .is_some_and(|(p, a)| a.rules_out(p, item, &ctx.env.state))
                            };
                            self.diags
                                .extend(crate::match_check::check_match_with_domain(
                                    m,
                                    subject.as_deref(),
                                    info,
                                    ctx,
                                    &ruled_out,
                                ));
                        }
                    }
                    // Arms (tests + bodies) evaluate WITHIN match scope: `$` binds
                    // to the subject (carry-forward #2).
                    let arm_ctx = Ctx {
                        env: ctx.env,
                        in_match: true,
                        match_subject: Some(m.subject.raw.clone()),
                    };
                    for arm in &m.arms {
                        match arm {
                            Arm::When { test, body, .. } => {
                                self.diags.extend(check_cel_slot(
                                    test,
                                    self.arena,
                                    &arm_ctx,
                                    Some(&ExpectedType::Bool),
                                ));
                                self.walk(body, &arm_ctx);
                            }
                            Arm::Otherwise { body, .. } => self.walk(body, &arm_ctx),
                        }
                    }
                }
                Node::Timeline(tl) => {
                    let (table, tdiags) = resolve_timeline(tl, ctx, self.snapshot);
                    self.timeline_tables.push(table);
                    self.diags.extend(tdiags);
                    if let Some(dur) = &tl.duration {
                        self.diags
                            .extend(check_cel_slot(dur, self.arena, ctx, None));
                    }
                    for track in &tl.tracks {
                        for clip in &track.clips {
                            match &clip.node {
                                ClipNode::Directive(d) if d.tag == "use" => {
                                    check_use(d, self.components, ctx, &mut self.diags);
                                    check_use_typed_args(
                                        d,
                                        self.components,
                                        self.snapshot,
                                        self.providers,
                                        self.domains,
                                        &mut self.diags,
                                    );
                                    check_use_interp_args(d, self.components, ctx, &mut self.diags);
                                    self.check_attr_refs(&d.attrs, ctx, None);
                                }
                                // dsl 0.8.0: `::end` is a walk TERMINATOR, not
                                // a staging leaf — a `<track>` clip may hold
                                // only staging directives and `::set` (§7.4).
                                // The parser's own `E-TIMELINE-CONTENT` guard
                                // can't see this: a track clip is grammatically
                                // any `::name{…}`, and the D9 precedent
                                // (`::assert` in a `<track>` falls through to
                                // `E-UNKNOWN-DIRECTIVE`) only bites because
                                // `assert` is not a DECLARED directive. `::end`
                                // IS declared, so it needs this explicit arm —
                                // reusing §7.4's own code rather than minting a
                                // second "not admitted here" diagnostic.
                                ClipNode::Directive(d)
                                    if d.tag == lute_manifest::core::END_DIRECTIVE =>
                                {
                                    self.diags.push(timeline_content_diag(
                                        "`::end` terminates the walk and is not a staging leaf; \
                                         a <track> body may contain only staging directives and \
                                         ::set (dsl §7.4)"
                                            .to_string(),
                                        d.span,
                                    ));
                                }
                                // dsl 0.12.0: `::mark`/`::next` are control-flow
                                // constructs (a position label / a jump), not
                                // staging leaves — mirrors the `::end` arm above
                                // verbatim, reusing its exact diagnostic shape.
                                ClipNode::Directive(d)
                                    if d.tag == lute_manifest::core::MARK_DIRECTIVE =>
                                {
                                    self.diags.push(timeline_content_diag(
                                        "`::mark` is a control-flow label and is not a staging \
                                         leaf; a <track> body may contain only staging \
                                         directives and ::set (dsl §7.4)"
                                            .to_string(),
                                        d.span,
                                    ));
                                }
                                ClipNode::Directive(d)
                                    if d.tag == lute_manifest::core::NEXT_DIRECTIVE =>
                                {
                                    self.diags.push(timeline_content_diag(
                                        "`::next` is a control-flow jump and is not a staging \
                                         leaf; a <track> body may contain only staging \
                                         directives and ::set (dsl §7.4)"
                                            .to_string(),
                                        d.span,
                                    ));
                                }
                                ClipNode::Directive(d) => {
                                    self.diags.extend(check_directive(
                                        d,
                                        self.snapshot,
                                        self.providers,
                                        self.domains,
                                        ctx,
                                    ));
                                    self.check_attr_refs(&d.attrs, ctx, Some(&d.tag));
                                }
                                ClipNode::Set(s) => {
                                    self.diags.extend(check_set(s, &ctx.env.state, ctx));
                                    self.diags.extend(crate::set_type::check_set_type(
                                        s,
                                        self.arena,
                                        &ctx.env.state,
                                    ));
                                    let expected = resolve_type(&s.path, &ctx.env.state)
                                        .cloned()
                                        .map(ExpectedType::Ty);
                                    self.diags.extend(check_cel_slot(
                                        &s.expr,
                                        self.arena,
                                        ctx,
                                        expected.as_ref(),
                                    ));
                                }
                            }
                        }
                    }
                }
                Node::Hub(h) => {
                    // `E-HUB-NO-EXIT` / `E-DUP-BRANCH` / `E-CHOICE-DUP` and the
                    // implicit `scene.choices.*` + `scene.visited.*` decl folding
                    // happened in the pre-pass (`fold_branches`); here we only
                    // validate the hub's own attrs and recurse into each choice
                    // (attrs, record sugar, `when` guard, body) so the B1–B5
                    // node checks apply inside hub arms too (dsl §7.3.2).
                    self.check_attr_refs(&h.attrs, ctx, None);
                    crate::logic_attrs::check_hub_attrs(h, &mut self.diags);
                    for choice in &h.choices {
                        self.check_attr_refs(&choice.attrs, ctx, None);
                        crate::logic_attrs::check_choice_attrs(
                            choice,
                            crate::logic_attrs::ChoicePos::Hub,
                            &mut self.diags,
                        );
                        check_choice_record(choice, ctx, self.src, &mut self.diags);
                        // §7.6: hub choice labels carry `{{…}}` interpolations too
                        // (same as branch choices) — validate their referents.
                        check_interps(
                            &scan_label_interps(&choice.label, choice.span),
                            ctx,
                            &mut self.diags,
                        );
                        if let Some(when) = &choice.when {
                            self.diags.extend(check_cel_slot(
                                when,
                                self.arena,
                                ctx,
                                Some(&ExpectedType::Bool),
                            ));
                        }
                        self.walk(&choice.body, ctx);
                    }
                }
                Node::Objective(o) => {
                    // Completion predicate (dsl 0.2.0 §6.4): a value READ like a
                    // match subject — it doesn't gate the body (§6.3: `when`
                    // controls visibility, not the completion obligation), so it
                    // gets the SAME `Bool` `check_cel_slot` treatment a
                    // `<when test>` guard gets. `E-OBJECTIVE-MISSING-DONE` (an
                    // empty `done`) was already flagged by `check_quest`'s fold
                    // pass (Task 4); an empty raw's CEL `ast` stays `None`, so
                    // this pass is a no-op for a missing `done` beyond the
                    // `E-CEL-PARSE` `fill_document` already reported once.
                    self.diags.extend(check_cel_slot(
                        &o.done,
                        self.arena,
                        ctx,
                        Some(&ExpectedType::Bool),
                    ));
                    if let Some(when) = &o.when {
                        self.diags.extend(check_cel_slot(
                            when,
                            self.arena,
                            ctx,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                    // dsl 0.23.0 §2 / 0.24.0 §2.1: `by` and `until` —
                    // condition slots like `done`.
                    for deadline in o.by.iter().chain(&o.until) {
                        self.diags.extend(check_cel_slot(
                            deadline,
                            self.arena,
                            ctx,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                    // §7.6: an objective `title` MAY embed `{{…}}` interpolations,
                    // same as a choice label.
                    if let Some(title) = &o.title {
                        check_interps(&scan_label_interps(title, o.span), ctx, &mut self.diags);
                    }
                    self.check_attr_refs(&o.attrs, ctx, None);
                    crate::logic_attrs::check_objective_attrs(o, &mut self.diags);
                    self.walk(&o.body, ctx);
                    // dsl 0.16.0 §2: `reward.when` is a Bool CEL slot with
                    // the SAME profile treatment `<objective when>` gets.
                    // Runs AFTER body to match the walk-canonical order in
                    // `lute_syntax::walk`, so StableId assignment there and
                    // profile checking here agree.
                    for reward in &o.rewards {
                        if let Some(when) = &reward.when {
                            self.diags.extend(check_cel_slot(
                                when,
                                self.arena,
                                ctx,
                                Some(&ExpectedType::Bool),
                            ));
                        }
                    }
                }
                Node::On(o) => {
                    // ECA trigger (dsl 0.2.0 §4.1): the `event` name (a plain
                    // String, NOT CEL) resolves against the built-in lifecycle
                    // events + capability-declared world events; the `when`
                    // guard flows through the SAME `check_cel_slot` profile gate
                    // every other boolean guard gets.
                    self.diags
                        .extend(crate::on::check_on_event(o, self.snapshot));
                    if let Some(when) = &o.when {
                        self.diags.extend(check_cel_slot(
                            when,
                            self.arena,
                            ctx,
                            Some(&ExpectedType::Bool),
                        ));
                    }
                    self.check_attr_refs(&o.attrs, ctx, None);
                    crate::logic_attrs::check_on_attrs(o, &mut self.diags);
                    self.walk(&o.body, ctx);
                }
                Node::Assert(a) => {
                    self.diags
                        .extend(crate::fact_write::check_assert(a, self.domains, ctx));
                    self.diags
                        .extend(check_guard(a.when.as_ref(), self.arena, ctx));
                }
                Node::Retract(r) => {
                    self.diags
                        .extend(crate::fact_write::check_retract(r, self.domains, ctx));
                    self.diags
                        .extend(check_guard(r.when.as_ref(), self.arena, ctx));
                }
            }
        }
    }

    /// Validate every `@ref`-valued attribute's CEL slot in the current scope.
    /// `directive_tag` is `Some` only for directive attrs, letting a `@ref` attr
    /// value be typed against the attr's declared type (`E-REF-TYPE`, dsl §8).
    fn check_attr_refs(&mut self, attrs: &[Attr], ctx: &Ctx<'_>, directive_tag: Option<&str>) {
        for attr in attrs {
            if let AttrValue::Ref(slot) = &attr.value {
                let expected = directive_tag
                    .and_then(|tag| self.snapshot.directive(tag))
                    .and_then(|decl| decl.attrs.iter().find(|a| a.name == attr.key))
                    .map(|a| ExpectedType::Ty(a.ty.clone()));
                self.diags
                    .extend(check_cel_slot(slot, self.arena, ctx, expected.as_ref()));
            }
        }
    }
}

/// Build the §7.4 `E-TIMELINE-CONTENT` diagnostic for a `<track>` clip the
/// PARSER admitted but the checker rejects — same code, severity, and layer
/// `lute_syntax`'s own guard emits (`parser::emit_o`), so a track violation
/// reads identically whichever layer caught it.
fn timeline_content_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: lute_syntax::parser::E_TIMELINE_CONTENT.to_string(),
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
