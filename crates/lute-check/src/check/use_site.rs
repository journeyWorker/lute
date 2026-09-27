//! Reusable content components (`::use`, dsl §13): the invocation-site
//! checks — component/argument resolution, typed and speaker arguments, def
//! enum arguments, and the `speaker`-param line binding.

use super::*;

/// Component-invocation / body diagnostic codes (dsl §13).
const E_COMPONENT_UNDECLARED: &str = "E-COMPONENT-UNDECLARED";
pub(super) const E_COMPONENT_ARG: &str = "E-COMPONENT-ARG";
pub(super) const E_COMPONENT_CYCLE: &str = "E-COMPONENT-CYCLE";
pub(super) const E_COMPONENT_BODY: &str = "E-COMPONENT-BODY";

/// dsl 0.4.0 §6.1/§6.2: a component-body position depends on or affects
/// ambient state — a CEL reference to a state path, a fact query
/// (`holds`/`count`/`validAt`), `now()`, or a directive whose resolved
/// manifest/lowering declares state or bridge-result writes. Distinct from
/// `E_COMPONENT_BODY` (a construct that is never admitted, regardless of what
/// it reads/writes): this code names the purity defect itself, so a plain
/// `E-UNDECLARED`/`E-RELATION-UNKNOWN` the empty component env would
/// otherwise misreport for the SAME site never surfaces (D6).
pub(super) const E_COMPONENT_STATE: &str = "E-COMPONENT-STATE";

/// Build a `Layer::Staging` diagnostic for a `::use` invocation / component body
/// (dsl §13).
pub(super) fn use_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Validate a `::use{ component="name" <arg>=<value> … }` invocation (dsl §13)
/// against the resolved component table. `use` is reserved (recognized before the
/// unknown-directive check), so it is never `E-UNKNOWN-DIRECTIVE`.
///
/// * the `component=` attr must be a plain string naming a declared component
///   (`E-COMPONENT-UNDECLARED` when the name is absent from the table);
/// * every remaining attr is a NAMED arg bound to a param by name — an unknown
///   arg, a missing required param, or a value incompatible with its param's
///   declared type is `E-COMPONENT-ARG`.
///
/// `enclosing` is the params of the component whose body holds the `::use`
/// (empty outside one): a bare `@p` naming one passes that argument through.
pub(super) fn check_use(
    dir: &Directive,
    components: &ComponentSet,
    ctx: &Ctx<'_>,
    enclosing: &std::collections::BTreeMap<String, DomainInfo>,
    diags: &mut Vec<Diagnostic>,
) {
    // E-AT-CONTEXT (dsl §7.5): a reserved `at` on a `::use` OUTSIDE a <track>
    // (track clips strip `at` via the parser). `::use` is dispatched here before
    // `check_directive`, so this shared check keeps `at` from being misread as a
    // component arg. Emitted regardless of the component's validity below.
    if let Some(d) = at_context(dir) {
        diags.push(d);
    }
    let Some(name_attr) = dir.attrs.iter().find(|a| a.key == "component") else {
        diags.push(use_diag(
            E_COMPONENT_ARG,
            "`::use` requires a `component` attribute naming the component (dsl §13)".to_string(),
            dir.span,
        ));
        return;
    };
    let AttrValue::Str(name) = &name_attr.value else {
        diags.push(use_diag(
            E_COMPONENT_ARG,
            "`::use` `component` must be a plain string naming the component (dsl §13)".to_string(),
            name_attr.value_span,
        ));
        return;
    };
    let Some(def) = components.table.get(name) else {
        diags.push(use_diag(
            E_COMPONENT_UNDECLARED,
            format!(
                "unknown component `{name}`: not declared in `components:`{} (dsl §13)",
                lute_manifest::suggest::nearest(
                    name,
                    components.table.keys().map(String::as_str),
                    2
                )
                .map(|n| format!(" — did you mean `{n}`?"))
                .unwrap_or_default()
            ),
            name_attr.value_span,
        ));
        return;
    };
    // Named-arg validation: each supplied arg binds to a param by name.
    // `at` is reserved (E-AT-CONTEXT above), never a component arg.
    let supplied = |p: &str| dir.attrs.iter().any(|a| a.key == p);
    // A missing param an unknown argument is the typo of: that one error
    // explains both, so the missing param is not reported again (ML-F6).
    let mut misspelt: Vec<&str> = Vec::new();
    for attr in dir
        .attrs
        .iter()
        .filter(|a| a.key != "component" && a.key != "at")
    {
        match def.params.iter().find(|(p, _)| p == &attr.key) {
            None => {
                let near = lute_manifest::suggest::nearest(
                    &attr.key,
                    def.params
                        .iter()
                        .map(|(p, _)| p.as_str())
                        .filter(|p| !supplied(p)),
                    2,
                );
                misspelt.extend(near);
                // A `<beat use=…>` also writes the `<beat>`'s own keys
                // (EMB-07: `priorty="5"` is the header key `priority`).
                let hint = match near {
                    Some(n) => format!(" — did you mean `{n}`?"),
                    None if crate::templates::is_template_use(dir) => {
                        lute_manifest::suggest::nearest(
                            &attr.key,
                            crate::templates::TEMPLATE_KEYS.iter().copied(),
                            2,
                        )
                        .map(|k| format!(" — did you mean the `<beat>` key `{k}`?"))
                        .unwrap_or_default()
                    }
                    None => String::new(),
                };
                diags.push(use_diag(
                    E_COMPONENT_ARG,
                    format!(
                        "component `{name}` has no parameter `{}`{hint} (dsl §13)",
                        attr.key
                    ),
                    attr.span,
                ))
            }
            // dsl 0.24.0 §4: a `speaker` arg is judged against the host's
            // cast by `check_speaker_args`, never as a plain string.
            Some((_, _)) if def.speakers.contains(&attr.key) => {}
            Some((_, pty)) => {
                if !use_arg_ok(pty, &attr.value, ctx) {
                    let shown = match &attr.value {
                        AttrValue::Ref(slot) => format!("`{}={}`", attr.key, slot.raw.trim()),
                        AttrValue::Str(s) => format!("`{}=\"{s}\"`", attr.key),
                        AttrValue::BoolTrue => format!("a bare `{}`", attr.key),
                    };
                    let hint = match (pty, &attr.value) {
                        (Type::Enum(members), AttrValue::Str(s)) => {
                            lute_manifest::suggest::nearest(
                                s,
                                members.iter().map(String::as_str),
                                2,
                            )
                            .map(|m| format!(" — did you mean `{m}`?"))
                            .unwrap_or_default()
                        }
                        _ => String::new(),
                    };
                    // Name what a `@def` argument produces: the declared
                    // type alone does not say why it does not fit.
                    let produced = match &attr.value {
                        AttrValue::Ref(slot) => ref_produced_type(&slot.raw, ctx)
                            .map(|t| format!(" is `{}`, which", param_ty_label(t)))
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    diags.push(use_diag(
                        E_COMPONENT_ARG,
                        format!(
                            "argument {shown} to component `{name}`{produced} does not fit `{}`, \
                             the type its param `{}` declares{hint} (dsl §13)",
                            param_ty_label(pty),
                            attr.key
                        ),
                        attr.value_span,
                    ));
                }
            }
        }
    }
    // Every param must be supplied — dsl 0.26.0 §3.3: or declare a
    // `default:`, which an omitted param takes. A `@def` default resolves in
    // this host and is judged here, at the `::use`, like the argument it
    // stands for; a literal one is the component's own, judged once at its
    // `params:` entry ([`check_param_literal_defaults`], round-5 T3-23).
    let mut args = crate::component_effects::use_args_for(dir, def);
    let mut defaulted: Vec<Attr> = Vec::new();
    for (p, pty) in &def.params {
        // A param named like a key of the use itself (`when`, a header key
        // of a beat template) can never be passed: refused at its
        // declaration (`templates::check_param_names`), never again here.
        if dir.attrs.iter().any(|a| &a.key == p)
            || misspelt.contains(&p.as_str())
            || crate::templates::reserved_param(p, def.beat.is_some())
        {
            continue;
        }
        let Some(value) = args.remove(p) else {
            diags.push(use_diag(
                E_COMPONENT_ARG,
                format!("component `{name}` requires argument `{p}` (dsl §13)"),
                dir.span,
            ));
            continue;
        };
        let shown = match &value {
            AttrValue::Ref(slot) => slot.raw.clone(),
            AttrValue::Str(s) => format!("\"{s}\""),
            AttrValue::BoolTrue => "true".to_string(),
        };
        let unresolved = match &value {
            AttrValue::Ref(slot) => scan_refs(&slot.raw)
                .iter()
                .find(|r| !r.is_dollar && !ctx.env.defs.contains(&r.name))
                .map(|r| r.name.clone()),
            _ => None,
        };
        if let Some(r) = unresolved {
            diags.push(use_diag(
                E_COMPONENT_ARG,
                format!(
                    "component `{name}` defaults `{p}` to `{shown}`, but `@{r}` is not a def \
                     here — declare it, or pass `{p}=…` (dsl 0.26.0 §3.3)"
                ),
                dir.span,
            ));
        } else if let (AttrValue::Ref(slot), false) = (&value, def.speakers.contains(p)) {
            if let Some(produced) = ref_produced_type(&slot.raw, ctx)
                .filter(|t| !compatible(t, &ExpectedType::Ty(pty.clone())))
            {
                diags.push(use_diag(
                    E_COMPONENT_ARG,
                    format!(
                        "component `{name}` param `{p}` is {} but its default `{shown}` is {} \
                         here — pass `{p}=…` at this `::use`, or default it to a def of the \
                         param's type (dsl 0.26.0 §3.3)",
                        param_ty_label(pty),
                        param_ty_label(produced),
                    ),
                    dir.span,
                ));
            }
        }
        defaulted.push(Attr {
            key: p.clone(),
            value,
            value_span: dir.span,
            span: dir.span,
        });
    }
    // dsl 0.24.0 §4: a param an `::assert` / `::retract` passes as a fact
    // argument, or a `::set` uses as a `per:` member index
    // (`run.approval[@who]`), is bound to its `::use` argument, which must be
    // a constant — and, for an index, a member of the family's kind. A bare
    // `@name` that is an enclosing component's param (or no def at all, as
    // the host sees a nested body) passes through — judged where the outer
    // `::use` binds it.
    let (fact_params, index_params) = component_bound_params(&def.body, components);
    for attr in dir
        .attrs
        .iter()
        .chain(&defaulted)
        .filter(|a| fact_params.contains_key(&a.key) || index_params.contains_key(&a.key))
    {
        let pass_through = matches!(&attr.value, AttrValue::Ref(s)
            if bare_param_ref(&s.raw)
                .is_some_and(|p| enclosing.contains_key(&p) || !ctx.env.defs.contains(&p)));
        if pass_through {
            continue;
        }
        let constant = crate::component_effects::fact_arg_constant(&attr.value);
        if let (Err(why), Some(atom)) = (&constant, fact_params.get(&attr.key)) {
            diags.push(use_diag(
                E_COMPONENT_ARG,
                format!(
                    "argument `{}` to component `{name}` binds `@{}` in the fact atom `{atom}`, so \
                     it must be a constant — an entity or enum member id, `true`, or `false` — \
                     but {why}; a fact's arguments are ground (dsl 0.24.0 §4)",
                    attr.key, attr.key
                ),
                attr.value_span,
            ));
        }
        let Some(family) = index_params.get(&attr.key) else {
            continue;
        };
        let vocab = &ctx.env.rel_vocab;
        let Some(kind) = vocab.indexed_state.get(family) else {
            diags.push(use_diag(
                E_COMPONENT_ARG,
                format!(
                    "component `{name}` writes `{family}[@{}]`, but `{family}` is not a `per:` \
                     state family here — declare `{family}: {{ …, per: <kind> }}` (dsl 0.24.0 §3)",
                    attr.key
                ),
                dir.span,
            ));
            continue;
        };
        let members: &[String] = match vocab.kinds.get(kind).map(|k| &k.shape) {
            Some(lute_manifest::relations::KindShape::Members(ms)) => ms,
            _ => &[],
        };
        let problem = match &constant {
            Ok(lute_syntax::datalog::FactTerm::Ident(m)) if members.contains(m) => continue,
            Ok(lute_syntax::datalog::FactTerm::Ident(m)) => {
                let hint =
                    lute_manifest::suggest::nearest(m, members.iter().map(String::as_str), 2)
                        .map(|n| format!(" — did you mean `{n}`?"))
                        .unwrap_or_default();
                format!(
                    "`{m}` is not a member of entity kind `{kind}` [{}]{hint}",
                    members.join(", ")
                )
            }
            Ok(_) => "a boolean is no member id".to_string(),
            Err(why) => why.clone(),
        };
        diags.push(use_diag(
            E_COMPONENT_ARG,
            format!(
                "argument `{}` to component `{name}` picks the member of `{family}[@{}]` \
                 (`per: {kind}`), so it must name a member of `{kind}`, but {problem} \
                 (dsl 0.24.0 §3/§4)",
                attr.key, attr.key
            ),
            attr.value_span,
        ));
    }
}

/// The params a component body binds at each `::use` (at any depth): those
/// an `::assert` / `::retract` passes as a fact argument (with the first such
/// atom's text), and those a `::set` uses as a `per:` member index (with the
/// family path) — through a nested `::use` too, when it passes `@p` whole to
/// a param the inner component binds so.
fn component_bound_params(
    body: &Document,
    components: &ComponentSet,
) -> (
    std::collections::BTreeMap<String, String>,
    std::collections::BTreeMap<String, String>,
) {
    type Out = std::collections::BTreeMap<String, String>;
    fn walk(
        nodes: &[Node],
        components: &ComponentSet,
        stack: &mut Vec<String>,
        facts: &mut Out,
        index: &mut Out,
    ) {
        for node in nodes {
            match node {
                Node::Assert(lute_syntax::ast::Assert { pattern, raw, .. })
                | Node::Retract(lute_syntax::ast::Retract { pattern, raw, .. }) => {
                    for a in &pattern.args {
                        if let lute_syntax::datalog::FactTerm::Param(p) = &a.term {
                            facts.entry(p.clone()).or_insert_with(|| raw.clone());
                        }
                    }
                }
                Node::Set(s) => {
                    if let Some((family, p)) = crate::component_effects::set_path_index(&s.path) {
                        index
                            .entry(p.to_string())
                            .or_insert_with(|| family.to_string());
                    }
                }
                Node::Directive(d) if d.tag == "use" => {
                    let Some((name, def)) =
                        use_target(d).and_then(|n| components.table.get(n).map(|def| (n, def)))
                    else {
                        continue;
                    };
                    // A cycle is `E-COMPONENT-CYCLE`'s.
                    if stack.iter().any(|s| s == name) {
                        continue;
                    }
                    stack.push(name.to_string());
                    let (mut inner_facts, mut inner_index) = (Out::new(), Out::new());
                    for shot in &def.body.shots {
                        walk(
                            &shot.body,
                            components,
                            stack,
                            &mut inner_facts,
                            &mut inner_index,
                        );
                    }
                    stack.pop();
                    for a in &d.attrs {
                        let AttrValue::Ref(slot) = &a.value else {
                            continue;
                        };
                        let Some(p) = bare_param_ref(&slot.raw) else {
                            continue;
                        };
                        if let Some(atom) = inner_facts.get(&a.key) {
                            facts.entry(p.clone()).or_insert_with(|| atom.clone());
                        }
                        if let Some(family) = inner_index.get(&a.key) {
                            index.entry(p).or_insert_with(|| family.clone());
                        }
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                        walk(body, components, stack, facts, index);
                    }
                }
                _ => {}
            }
        }
    }
    let (mut facts, mut index) = (Out::new(), Out::new());
    for shot in &body.shots {
        walk(
            &shot.body,
            components,
            &mut Vec::new(),
            &mut facts,
            &mut index,
        );
    }
    (facts, index)
}

/// dsl 0.24.0 §4: every `speaker` argument of every `::use` in `doc`
/// ([`check_speaker_args`]); `enclosing` is the document's own `speaker`
/// params when it is a component (a `who=@who` passes its id through).
pub(super) fn check_use_speaker_args(
    doc: &Document,
    components: &ComponentSet,
    cast: &std::collections::BTreeMap<String, lute_manifest::schema::CastMember>,
    enclosing: &[String],
) -> Vec<Diagnostic> {
    let mut dirs = Vec::new();
    for body in doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
    {
        collect_use_directives(body, &mut dirs);
    }
    let mut diags = Vec::new();
    for dir in dirs {
        check_speaker_args(dir, components, cast, enclosing, &mut diags);
    }
    diags
}

/// dsl 0.24.0 §4: a `speaker` param takes a LITERAL cast id — the name it
/// renders is looked up at expansion, so a def (whose value is only known at
/// runtime) cannot stand in for one. With a cast declared the id must be a
/// member (or `narrator`): `E-CAST-UNKNOWN` with a did-you-mean, exactly as
/// for a line's speaker; with none, any identifier is accepted. Inside a
/// component body a bare `@p` naming one of the ENCLOSING component's own
/// `speaker` params (`enclosing`) passes that id through.
pub(super) fn check_speaker_args(
    dir: &Directive,
    components: &ComponentSet,
    cast: &std::collections::BTreeMap<String, lute_manifest::schema::CastMember>,
    enclosing: &[String],
    diags: &mut Vec<Diagnostic>,
) {
    let Some((name, def)) = use_target(dir).and_then(|n| components.table.get(n).map(|d| (n, d)))
    else {
        return;
    };
    // dsl 0.26.0 §3.3: an omitted speaker param's `default:` is judged like
    // the argument it stands for.
    let defaulted: Vec<Attr> = def
        .defaults
        .iter()
        .filter(|(p, _)| def.speakers.contains(p) && !dir.attrs.iter().any(|a| &a.key == *p))
        .map(|(p, v)| Attr {
            key: p.clone(),
            value: v.clone(),
            value_span: dir.span,
            span: dir.span,
        })
        .collect();
    for attr in dir
        .attrs
        .iter()
        .chain(&defaulted)
        .filter(|a| def.speakers.contains(&a.key))
    {
        let literal = match &attr.value {
            // dsl 0.28.0 §3: the member the enclosing kind or `for=` beat
            // runs for — the walk judges it per member (`Walker::check_use_node`).
            v if crate::target_writes::is_target_value(v) => continue,
            AttrValue::Str(id) => Some(id.as_str()),
            AttrValue::Ref(slot)
                if bare_param_ref(&slot.raw).is_some_and(|p| enclosing.contains(&p)) =>
            {
                continue;
            }
            AttrValue::Ref(_) | AttrValue::BoolTrue => None,
        };
        let is_ident = literal.is_some_and(|id| {
            let mut chars = id.chars();
            chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        });
        match literal {
            Some(id) if is_ident => {
                if !cast.is_empty() && id != "narrator" && !cast.contains_key(id) {
                    diags.push(crate::cast::unknown(
                        format!("argument `{}` to component `{name}`: speaker `{id}`", attr.key),
                        id,
                        attr.value_span,
                        cast,
                    ));
                }
            }
            _ => diags.push(use_diag(
                E_COMPONENT_ARG,
                format!(
                    "argument `{}` to component `{name}` must be a literal cast id, e.g. `{}=\"isolde\"` — \
                     a `speaker` parameter renders that member's name (dsl 0.24.0 §4)",
                    attr.key, attr.key
                ),
                attr.value_span,
            )),
        }
    }
}

/// A `::use` arg value is compatible with its param type (dsl §13) when:
///
/// * it is a `@ref` (CEL, bound at expansion and typed in the ENCLOSING scene
///   scope) whose produced type — resolved via [`Env::def_types`] — is either
///   UNRESOLVABLE (not a known def: skipped, no false positive) or COMPATIBLE
///   with the param type (reusing the `E-REF-TYPE` [`compatible`] relation). A
///   ref whose def type is DEFINITELY incompatible flags `E-COMPONENT-ARG`;
/// * a value-level string for a `providerRef` param (id existence is out of the
///   v1 presentational scope); or
/// * a literal that coerces into the param type's domain and `type_accepts` it
///   (reusing the persist value-coercion helper).
fn use_arg_ok(ty: &Type, value: &AttrValue, ctx: &Ctx<'_>) -> bool {
    match value {
        AttrValue::Ref(slot) => match ref_produced_type(&slot.raw, ctx) {
            Some(produced) => compatible(produced, &ExpectedType::Ty(ty.clone())),
            None => true, // unresolvable ref — conservative, never flag
        },
        _ => literal_arg_ok(ty, value),
    }
}

/// Round-5 T3-23: every literal `default:` in a component's `params:` that
/// its declared type rejects is `E-COMPONENT-ARG` at that param's entry, with
/// the nearest enum member as a did-you-mean. A `@def` default resolves in
/// each host, so it stays with each `::use` ([`check_use`]); a `speaker`
/// default is judged against each host's cast.
pub(super) fn check_param_literal_defaults(
    typed: &crate::meta::TypedMeta,
    meta: &lute_syntax::ast::Meta,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for param in &typed.params {
        let p = &param.name;
        let Some(value @ (AttrValue::Str(_) | AttrValue::BoolTrue)) = typed.param_defaults.get(p)
        else {
            continue;
        };
        if typed.speaker_params.contains(p) || literal_arg_ok(&param.ty, value) {
            continue;
        }
        let shown = match value {
            AttrValue::Str(s) => s.as_str(),
            _ => "true",
        };
        let hint = match &param.ty {
            Type::Enum(members) => {
                lute_manifest::suggest::nearest(shown, members.iter().map(String::as_str), 2)
                    .map(|m| format!(" — did you mean `{m}`?"))
                    .unwrap_or_default()
            }
            _ => String::new(),
        };
        out.push(use_diag(
            E_COMPONENT_ARG,
            format!(
                "param `{p}` defaults to `{shown}`, which {}{hint} (dsl 0.26.0 §3.3)",
                match &param.ty {
                    Type::Enum(_) => format!(
                        "is not a member of its declared `{}`",
                        param_ty_label(&param.ty)
                    ),
                    t => format!("its declared type `{}` does not accept", param_ty_label(t)),
                },
            ),
            crate::meta::meta_key_span(meta, p),
        ));
    }
    out
}

/// A literal `::use` argument (or `default:`) the param type accepts.
pub(crate) fn literal_arg_ok(ty: &Type, value: &AttrValue) -> bool {
    match ty {
        Type::ProviderRef(_) => matches!(value, AttrValue::Str(_)),
        _ => into_literal(ty, value).is_some_and(|lit| type_accepts(ty, &lit)),
    }
}

/// A param's declared type as a message names it: `enum[a, b]`, `bool`, …
fn param_ty_label(ty: &Type) -> String {
    match ty {
        Type::Enum(members) => format!("enum[{}]", members.join(", ")),
        t => {
            let d = crate::cel_resolve::ty_desc(t);
            d.split_once(' ')
                .map_or(d.clone(), |(_, rest)| rest.to_string())
        }
    }
}

/// The produced [`Type`] of a whole-slot `@ref` arg (a `::use` value is only ever
/// a single `@name` or `@name(args)` — the attr parser captures nothing else),
/// resolved against the enclosing scope's [`Env::def_types`]. `None` when the
/// slot is not a resolvable def ref (e.g. the `$` subject, or a name with no
/// known produced type): conservatively skipped so no false positive fires.
fn ref_produced_type<'a>(raw: &str, ctx: &'a Ctx<'_>) -> Option<&'a Type> {
    let r = scan_refs(raw).into_iter().find(|r| !r.is_dollar)?;
    ctx.env.def_types.get(&r.name)
}

/// dsl 0.23.0 §5, the `::use` half of a component body's `{{@p}}` over a
/// `string` param: expansion splices a LITERAL arg into the line text, but a
/// `@ref` arg is rebound as `{{@ref}}` in the caller's scope — which renders
/// only when the ref's type does. So a `@ref` arg whose def produces a
/// non-renderable type (a `string` def), bound to a param the body
/// interpolates (directly or through a nested `::use` passing it on), is
/// `E-REF-TYPE` at the arg. Scene-scope `::use` only: inside a component body a
/// `@ref` arg names an enclosing param, bound (to this site's literal) before
/// the nested expansion — the enclosing site is the one checked.
pub(super) fn check_use_interp_args(
    dir: &Directive,
    components: &ComponentSet,
    ctx: &Ctx<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(name) = dir.attrs.iter().find_map(|a| match &a.value {
        AttrValue::Str(s) if a.key == "component" => Some(s.as_str()),
        _ => None,
    }) else {
        return;
    };
    for attr in &dir.attrs {
        let AttrValue::Ref(slot) = &attr.value else {
            continue;
        };
        let Some(ty) = ref_produced_type(&slot.raw, ctx) else {
            continue;
        };
        if is_renderable(ty)
            || !component_interpolates(components, name, &attr.key, &mut Vec::new())
        {
            continue;
        }
        diags.push(use_diag(
            "E-REF-TYPE",
            format!(
                "argument `{key}` to component `{name}`: component `{name}` interpolates \
                 `{{{{@{key}}}}}`, so the argument must be a literal string — `{raw}` produces \
                 a non-renderable type, and a `{{{{…}}}}` interpolation renders only \
                 number/bool/enum (dsl §7.6, 0.23.0 §5)",
                key = attr.key,
                raw = slot.raw.trim(),
            ),
            attr.value_span,
        ));
    }
}

/// dsl 0.26.0 §2.5 (prerelease N1): a literal `::use` argument whose param
/// the component types `{ entity: K }` / `{ domain: K }`, or passes whole
/// (`attr=@param`, at any depth, through nested `::use`s) to a directive
/// attribute typed that way, is judged HERE — at the argument, with the
/// did-you-mean — exactly as the attribute judges a literal written in place.
/// Inside the body the value is a `@param` ref, which the attribute check
/// cannot see through. The first failing judgement per argument is reported.
pub(super) fn check_use_typed_args(
    dir: &Directive,
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    domains: &std::collections::BTreeMap<String, Domain>,
    diags: &mut Vec<Diagnostic>,
) {
    let Some((name, def)) = use_target(dir).and_then(|n| components.table.get(n).map(|d| (n, d)))
    else {
        return;
    };
    let args = crate::component_effects::use_args_for(dir, def);
    for (param, pty) in &def.params {
        if def.speakers.contains(param) {
            continue; // judged against the cast (`check_speaker_args`)
        }
        let Some(value @ AttrValue::Str(_)) = args.get(param) else {
            continue; // a `@ref` is judged where it is bound; `true` is `E-COMPONENT-ARG`'s
        };
        let value_span = dir
            .attrs
            .iter()
            .find(|a| &a.key == param)
            .map_or(dir.span, |a| a.value_span);
        let mut sinks: Vec<(String, lute_manifest::schema::AttrDecl)> = Vec::new();
        if is_member_typed(pty) {
            sinks.push((
                crate::templates::use_label(dir, name),
                lute_manifest::schema::AttrDecl {
                    name: param.clone(),
                    required: false,
                    ty: pty.clone(),
                    default: None,
                },
            ));
        }
        typed_param_sinks(
            components,
            snapshot,
            name,
            param,
            param,
            &mut Vec::new(),
            &mut sinks,
        );
        let mut judged: Vec<Type> = Vec::new();
        for (owner, decl) in sinks {
            if judged.contains(&decl.ty) {
                continue;
            }
            judged.push(decl.ty.clone());
            let attr = Attr {
                key: decl.name.clone(),
                value: value.clone(),
                value_span,
                span: value_span,
            };
            let before = diags.len();
            crate::directives::check_attr_value(
                &owner, &decl, &attr, snapshot, providers, domains, diags,
            );
            if diags.len() > before {
                break;
            }
        }
    }
}

/// A type whose values are members of a named kind or domain (dsl 0.26.0
/// §2.5) — what [`check_use_typed_args`] judges through a component.
fn is_member_typed(ty: &Type) -> bool {
    matches!(ty, Type::Entity(_) | Type::Domain(_))
}

/// Every member-typed directive attribute (and nested component param)
/// component `comp`'s body passes its param `param` to whole, with the owner
/// text [`crate::directives::check_attr_value`] names it by. `arg` is the
/// argument of the outermost `::use` being judged. `seen` guards a `::use`
/// cycle (reported elsewhere as `E-COMPONENT-CYCLE`).
fn typed_param_sinks(
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
    comp: &str,
    param: &str,
    arg: &str,
    seen: &mut Vec<(String, String)>,
    out: &mut Vec<(String, lute_manifest::schema::AttrDecl)>,
) {
    if seen.iter().any(|(c, p)| c == comp && p == param) {
        return;
    }
    seen.push((comp.to_string(), param.to_string()));
    let Some(def) = components.table.get(comp) else {
        return;
    };
    let owner = |tag: &str, key: &str| {
        if key == arg {
            format!("{tag}` in component `{comp}")
        } else {
            format!("{tag}` in component `{comp}`, passed as argument `{arg}")
        }
    };
    let passes = |a: &Attr| matches!(&a.value, AttrValue::Ref(slot) if bare_param_ref(&slot.raw).as_deref() == Some(param));
    let mut directive =
        |d: &Directive, out: &mut Vec<(String, lute_manifest::schema::AttrDecl)>| {
            if d.tag == "use" {
                let Some((inner, inner_def)) =
                    use_target(d).and_then(|n| components.table.get(n).map(|c| (n, c)))
                else {
                    return;
                };
                for a in d.attrs.iter().filter(|a| passes(a)) {
                    if let Some((p, ty)) = inner_def.params.iter().find(|(p, _)| p == &a.key) {
                        if is_member_typed(ty) {
                            out.push((
                                owner(&format!("::use{{component=\"{inner}\"}}"), p),
                                lute_manifest::schema::AttrDecl {
                                    name: p.clone(),
                                    required: false,
                                    ty: ty.clone(),
                                    default: None,
                                },
                            ));
                        }
                    }
                    typed_param_sinks(components, snapshot, inner, &a.key, arg, seen, out);
                }
                return;
            }
            let Some(decl) = snapshot.directive(&d.tag) else {
                return;
            };
            for a in d.attrs.iter().filter(|a| passes(a)) {
                let adecl = decl
                    .attrs
                    .iter()
                    .find(|x| x.name == a.key)
                    .or_else(|| snapshot.stamp_attrs.get(&a.key));
                if let Some(adecl) = adecl.filter(|x| is_member_typed(&x.ty)) {
                    out.push((owner(&format!("::{}", d.tag), &a.key), adecl.clone()));
                }
            }
        };
    fn walk(
        nodes: &[Node],
        f: &mut dyn FnMut(&Directive, &mut Vec<(String, lute_manifest::schema::AttrDecl)>),
        out: &mut Vec<(String, lute_manifest::schema::AttrDecl)>,
    ) {
        for node in nodes {
            match node {
                Node::Directive(d) => f(d, out),
                Node::Timeline(t) => {
                    for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                        if let ClipNode::Directive(d) = &clip.node {
                            f(d, out);
                        }
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                        walk(body, f, out);
                    }
                }
                Node::Branch(b) => b.choices.iter().for_each(|c| walk(&c.body, f, out)),
                Node::Hub(h) => h.bodies().for_each(|b| walk(b, f, out)),
                Node::On(o) => walk(&o.body, f, out),
                Node::Objective(o) => walk(&o.body, f, out),
                Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }
    for shot in &def.body.shots {
        walk(&shot.body, &mut directive, out);
    }
}

/// `true` when component `comp`'s body interpolates its param `param` as a
/// bare `{{@param}}` — directly, or by passing it whole-slot to a nested
/// `::use` whose component interpolates the receiving param. `seen` guards a
/// `::use` cycle (reported elsewhere as `E-COMPONENT-CYCLE`).
fn component_interpolates(
    components: &ComponentSet,
    comp: &str,
    param: &str,
    seen: &mut Vec<(String, String)>,
) -> bool {
    if seen.iter().any(|(c, p)| c == comp && p == param) {
        return false;
    }
    seen.push((comp.to_string(), param.to_string()));
    let Some(def) = components.table.get(comp) else {
        return false;
    };
    def.body
        .shots
        .iter()
        .any(|s| body_interpolates(&s.body, components, param, seen))
}

fn body_interpolates(
    nodes: &[Node],
    components: &ComponentSet,
    param: &str,
    seen: &mut Vec<(String, String)>,
) -> bool {
    nodes.iter().any(|node| match node {
        Node::Line(l) => l
            .interps
            .iter()
            .any(|i| i.kind == InterpKind::Ref && bare_param_ref(&i.raw).as_deref() == Some(param)),
        Node::Match(m) => m.arms.iter().any(|arm| {
            let body = match arm {
                Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
            };
            body_interpolates(body, components, param, seen)
        }),
        Node::Directive(d) if d.tag == "use" => {
            let Some(inner) = d.attrs.iter().find_map(|a| match &a.value {
                AttrValue::Str(s) if a.key == "component" => Some(s.as_str()),
                _ => None,
            }) else {
                return false;
            };
            d.attrs.iter().any(|a| {
                matches!(&a.value, AttrValue::Ref(slot)
                    if bare_param_ref(&slot.raw).as_deref() == Some(param))
                    && component_interpolates(components, inner, &a.key, seen)
            })
        }
        _ => false,
    })
}

/// dsl 0.26.0 §4: drop a report identical (code, severity, span, message) to
/// an earlier one inside a guarded `::use`. The guard rides every write the
/// `::use` splices into the host ([`crate::component_effects`]), each
/// anchored at the `::use`, so the host passes judge the one guard once per
/// write. dsl 0.27.0 §6: likewise at a `<beat use=…>` — its derived header
/// and its body's `::use` all anchor at the `use=` value, so one argument
/// judged by both (a header `when` and a body `::assert`) is one report.
pub(super) fn dedup_guarded_use_reports(doc: &Document, diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut dirs = Vec::new();
    for body in doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
    {
        collect_use_directives(body, &mut dirs);
    }
    let guarded: Vec<Span> = dirs
        .into_iter()
        .filter(|d| d.when.is_some())
        .map(|d| d.span)
        .chain(
            doc.beats
                .iter()
                .filter_map(|b| Some(b.template.as_ref()?.span)),
        )
        .collect();
    if guarded.is_empty() {
        return diags;
    }
    let mut seen = std::collections::BTreeSet::new();
    diags
        .into_iter()
        .filter(|d| {
            !guarded
                .iter()
                .any(|g| g.byte_start <= d.span.byte_start && d.span.byte_end <= g.byte_end)
                || seen.insert((
                    d.code.clone(),
                    d.span.byte_start,
                    d.span.byte_end,
                    d.severity as u8,
                    d.message.clone(),
                ))
        })
        .collect()
}

/// dsl 0.26.0 §3.2: [`FoldedEnv::use_lines`] — every `::use` of `doc` that
/// speaks through a `speaker` param, with its bound `@@p:` lines.
pub fn use_speaker_lines(
    doc: &Document,
    components: &ComponentSet,
) -> std::collections::BTreeMap<usize, Vec<lute_syntax::ast::Line>> {
    let speaks = |def: &crate::component_import::ComponentDef| {
        def.body.shots.iter().any(|s| body_speaks_as_param(&s.body))
    };
    let mut out = std::collections::BTreeMap::new();
    if !components.table.values().any(speaks) {
        return out;
    }
    let mut dirs = Vec::new();
    for body in doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
    {
        collect_use_directives(body, &mut dirs);
    }
    for d in dirs {
        let lines = crate::component_effects::use_speaker_lines(d, components);
        if !lines.is_empty() {
            out.insert(d.span.byte_start, lines);
        }
    }
    out
}

/// A `@@p:` line anywhere in a component body (param-scoped `<match>` arms
/// included).
fn body_speaks_as_param(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Line(l) => l.speaker.starts_with('@'),
        Node::Match(m) => m.arms.iter().any(|arm| {
            let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
            body_speaks_as_param(body)
        }),
        _ => false,
    })
}

/// Every `::use` directive in `nodes`, in document order. Recurses every node
/// kind that can hold one.
pub(super) fn collect_use_directives<'a>(nodes: &'a [Node], out: &mut Vec<&'a Directive>) {
    for node in nodes {
        match node {
            Node::Directive(d) if d.tag == "use" => out.push(d),
            Node::Directive(_) => {}
            Node::Branch(b) => b
                .choices
                .iter()
                .for_each(|c| collect_use_directives(&c.body, out)),
            Node::Hub(h) => h.bodies().for_each(|b| collect_use_directives(b, out)),
            Node::On(o) => collect_use_directives(&o.body, out),
            Node::Objective(o) => collect_use_directives(&o.body, out),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_use_directives(body, out)
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                for track in &t.tracks {
                    for clip in &track.clips {
                        if let lute_syntax::ast::ClipNode::Directive(d) = &clip.node {
                            if d.tag == "use" {
                                out.push(d);
                            }
                        }
                    }
                }
            }
            Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// 0.21.1 T1-5: a `@def` argument to an ENUM component param. `use_arg_ok`
/// only compares the def's produced TYPE, and a string def is compatible
/// with every enum — so `pick: "run.flag ? 'blazing' : 'brief'"` passed as
/// `depth=@pick` checked `ok` and matched no arm at runtime. Every value the
/// def body can produce must be a member of the param's enum: each result
/// position (through `?:`) must be a string literal that is a member, or a
/// state path whose enum members all are. Anything the checker cannot prove
/// is `E-COMPONENT-ARG` too — a wrong verdict is worse than a stop.
pub(super) fn check_use_def_enum_args(
    doc: &Document,
    components: &ComponentSet,
    def_bodies: &std::collections::BTreeMap<String, String>,
    def_types: &std::collections::BTreeMap<String, Type>,
    schema: &crate::meta::StateSchema,
    diags: &mut Vec<Diagnostic>,
) {
    let mut dirs = Vec::new();
    for body in doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
    {
        collect_use_directives(body, &mut dirs);
    }
    for dir in dirs {
        let Some((name, def)) = dir.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
            ("component", AttrValue::Str(s)) => components.table.get(s).map(|d| (s, d)),
            _ => None,
        }) else {
            continue; // unknown component: `check_use` owns it
        };
        for attr in &dir.attrs {
            let AttrValue::Ref(slot) = &attr.value else {
                continue;
            };
            let Some((_, Type::Enum(members))) = def.params.iter().find(|(p, _)| p == &attr.key)
            else {
                continue;
            };
            let Some(r) = scan_refs(&slot.raw).into_iter().find(|r| !r.is_dollar) else {
                continue;
            };
            let Some(body) = def_bodies.get(&r.name) else {
                continue; // undeclared ref: `E-UNDECLARED-REF` owns it
            };
            // A def whose type does not fit the enum at all (a `bool`) is
            // `check_use`'s one report; its values are only asked of a def
            // the type check lets through (lighthouse NEW-3).
            if def_types
                .get(&r.name)
                .is_some_and(|t| !compatible(t, &ExpectedType::Ty(Type::Enum(members.clone()))))
            {
                continue;
            }
            let mut arena = CelArena::default();
            let values = lute_cel::parse_slot_marked_refs(&mut arena, body)
                .and_then(|h| arena.get(h))
                .and_then(|root| def_result_values(&root.expr, schema));
            let problem = match values {
                None => Some(format!(
                    "the values `@{}` (`{body}`) can produce cannot be proven to be members",
                    r.name
                )),
                Some(vals) => vals.into_iter().find(|v| !members.contains(v)).map(|v| {
                    format!(
                        "`@{}` (`{body}`) can produce `{v}`, which is not a member",
                        r.name
                    )
                }),
            };
            if let Some(problem) = problem {
                diags.push(use_diag(
                    E_COMPONENT_ARG,
                    format!(
                        "argument `{}` to component `{}`: {problem} of the parameter's enum [{}] \
                         (dsl §13)",
                        attr.key,
                        name,
                        members.join(", ")
                    ),
                    attr.value_span,
                ));
            }
        }
    }
}

/// Every value a def body's result can take (0.21.1 T1-5): through `?:`
/// branches, a string literal is itself and a state path of enum type is its
/// members. `None` when any result position is something else.
fn def_result_values(
    expr: &cel_parser::ast::Expr,
    schema: &crate::meta::StateSchema,
) -> Option<Vec<String>> {
    use cel_parser::ast::Expr;
    match expr {
        Expr::Call(c)
            if c.func_name == cel_parser::ast::operators::CONDITIONAL && c.args.len() == 3 =>
        {
            let mut vals = def_result_values(&c.args[1].expr, schema)?;
            vals.extend(def_result_values(&c.args[2].expr, schema)?);
            Some(vals)
        }
        Expr::Literal(cel_parser::reference::Val::String(s)) => Some(vec![s.to_string()]),
        Expr::Select(_) | Expr::Ident(_) => {
            let path = crate::cel_paths::select_path(expr)?;
            match &schema.decls.get(&path)?.ty {
                Type::Enum(ms) => Some(ms.clone()),
                _ => None,
            }
        }
        _ => None,
    }
}
