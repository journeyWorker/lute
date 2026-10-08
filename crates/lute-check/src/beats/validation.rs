use super::*;
use lute_manifest::semantics::beats::OCCASION_TARGET;

/// dsl 0.21.0 §7a.2 (D-I): every `<objective on="<occasion>">` of `quests`
/// — the occasion at which the objective's `done` is judged — checked
/// exactly as a beat's `on`: [`E_BEAT_ATTR`] when the value is not a quoted
/// name, [`E_OCCASION_UNKNOWN`] against the resolved vocabulary
/// (shape-only while `occasions` is empty). dsl 0.23.0 §2: its `target=`
/// follows the beat target rule — a dotted id, only beside `on`, only on an
/// occasion raised for a target (the domain is
/// [`check_beat_target_domains`]'s).
pub(crate) fn check_objective_occasions(
    quests: &[Quest],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for node in quests.iter().flat_map(|q| &q.body) {
        let Node::Objective(o) = node else { continue };
        for attr in o
            .attrs
            .iter()
            .filter(|a| matches!(a.key.as_str(), "on" | "target"))
        {
            // A non-string value stays residual (the parser extracts only
            // quoted strings).
            diags.push(beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                format!(
                    "`<objective>` attribute `{}` must be a quoted string (dsl 0.21.0 §7a.2, \
                     0.23.0 §2)",
                    attr.key
                ),
                attr.span,
                Layer::Logic,
            ));
        }
        let target_span = match &o.target {
            Some((t, span)) if !is_entry_target(t) => {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    malformed_target("`<objective>`", t, false),
                    *span,
                    Layer::Logic,
                ));
                None
            }
            Some((_, span)) if o.on.is_none() => {
                if !o.attrs.iter().any(|a| a.key == "on") {
                    diags.push(beat_diag(
                        E_BEAT_ATTR,
                        Severity::Error,
                        "`<objective>` `target` requires `on`; the objective is judged when the \
                         occasion is raised for that target (dsl 0.23.0 §2)"
                            .to_string(),
                        *span,
                        Layer::Logic,
                    ));
                }
                None
            }
            Some((t, span)) => Some((t.as_str(), *span)),
            None => None,
        };
        // dsl 0.24.0 §2.1: `until` is the place-bound deadline — judged only
        // when the objective's occasion is raised, so it needs one.
        if let (Some(until), None) = (&o.until, &o.on) {
            if !o.attrs.iter().any(|a| a.key == "on") {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    "`<objective>` `until` requires `on`; it is judged only when that occasion \
                     is raised — a deadline that holds everywhere is `by=` (dsl 0.24.0 §2.1)"
                        .to_string(),
                    until.span,
                    Layer::Logic,
                ));
            }
        }
        let Some((on, span)) = &o.on else { continue };
        if !is_name(on) {
            diags.push(beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                occasion_malformed("`<objective>`", on),
                *span,
                Layer::Logic,
            ));
            continue;
        }
        check_occasion(
            on,
            *span,
            target_span,
            false,
            occasions,
            Layer::Logic,
            &mut diags,
        );
    }
    diags
}

/// dsl 0.23.0 §3: a side remark rides along a single winner — on a
/// `select: all` / `sequence` occasion every eligible beat is already offered
/// or presented, so `also` means nothing there. The one rule a scene's
/// `also: true`, a bundle `<beat also>`, and a template header's `also: true`
/// share; `yaml` spells the key as frontmatter writes it. `None` for a
/// `select: first` or undeclared occasion.
pub(crate) fn also_fault(
    on: &str,
    span: Span,
    yaml: bool,
    occasions: &BTreeMap<String, OccasionDecl>,
    layer: Layer,
) -> Option<Diagnostic> {
    let decl = occasions
        .get(on)
        .filter(|d| d.select != OccasionSelect::First)?;
    let (written, remove) = if yaml {
        ("also: true", "also:")
    } else {
        ("also", "also")
    };
    Some(beat_diag(
        E_BEAT_ATTR,
        Severity::Error,
        format!(
            "`{written}` applies only to a `select: first` occasion; `{on}` is `select: {}`, \
             which already presents or offers every eligible beat — remove `{remove}` \
             (dsl 0.23.0 §3)",
            decl.select.as_str()
        ),
        span,
        layer,
    ))
}

/// `on` against the resolved occasion vocabulary (dsl 0.21.0 §2): unknown is
/// [`E_OCCASION_UNKNOWN`]; a `target` on an occasion declared without
/// `target: true` is [`E_BEAT_ATTR`]. Silent when no occasion is declared.
/// `target` is the value with its span; `beat`: the target is a beat's
/// (not an objective's), so a `kind:<K>` value there points at `for`,
/// spelled for the layer (frontmatter in [`Layer::Content`]).
pub(crate) fn check_occasion(
    on: &str,
    on_span: Span,
    target: Option<(&str, Span)>,
    beat: bool,
    occasions: &BTreeMap<String, OccasionDecl>,
    layer: Layer,
    diags: &mut Vec<Diagnostic>,
) {
    if occasions.is_empty() {
        return;
    }
    let Some(decl) = occasions.get(on) else {
        let hint = lute_manifest::suggest::nearest(on, occasions.keys().map(String::as_str), 2)
            .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
        diags.push(beat_diag(
            E_OCCASION_UNKNOWN,
            Severity::Error,
            format!(
                "occasion `{on}` is not declared by any resolved plugin (declared: {}){hint} \
                 (dsl 0.21.0 §2)",
                occasions
                    .keys()
                    .map(|k| format!("`{k}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            on_span,
            layer,
        ));
        return;
    };
    if let (false, Some((value, span))) = (decl.target.takes_target(), target) {
        let sequence = decl.select == lute_manifest::schema::OccasionSelect::Sequence;
        let (key, spelled): (&str, fn(&str) -> String) = if layer == Layer::Content {
            ("target:", |k: &str| format!("for: \"kind:{k}\""))
        } else {
            ("target", |k: &str| format!("for=\"kind:{k}\""))
        };
        let fix = match kind_target(value).filter(|_| beat) {
            Some(kind) if sequence => format!(
                "; to present this beat once for each member of `{kind}`, write `{}` instead \
                 of `{key}`",
                spelled(kind)
            ),
            Some(kind) => format!(
                "; remove `{key}` (`{}` presents a beat once for each member, but only on a \
                 `select: sequence` occasion, and `{on}` is `select: {}`)",
                spelled(kind),
                decl.select.as_str()
            ),
            None => format!("; remove `{key}`"),
        };
        diags.push(beat_diag(
            E_BEAT_ATTR,
            Severity::Error,
            format!(
                "occasion `{on}` is not raised for a target (declared without `target: true`), \
                 so a beat on it cannot restrict itself to one{fix}"
            ),
            span,
            layer,
        ));
    }
}

/// Whether `target` lies in `decl`'s target domain (dsl 0.22.0 §8), the one
/// rule `lute check` (beat targets, [`E_BEAT_ATTR`]) and `lute play` (step
/// targets, a usage error) share. `kinds` is the project's `entities:`
/// vocabulary (`Env::rel_vocab.kinds`).
///
/// `Ok` for an occasion without a domain (`target: true` keeps the 0.21
/// shape-only meaning), for a member of the named entity kind under the
/// domain's prefix, and for any `<prefix>.<id>` of an `open:` kind (its
/// members are engine-populated). A domain with a `members:` subset admits
/// exactly `<prefix>.<member>` for a listed member — and every listed member
/// must belong to a closed kind (an `open:` kind's cannot be known). The
/// `Err` is the whole reason, with a did-you-mean over the legal
/// `<prefix>.<member>` targets (or, for a stray listed member, over the
/// kind's members) when one is close.
pub fn occasion_target_ok(
    decl: &OccasionDecl,
    target: &str,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Result<(), String> {
    let OccasionTarget::Domain {
        prefix,
        entity,
        members: subset,
    } = &decl.target
    else {
        return Ok(());
    };
    let on = &decl.name;
    let Some(kind) = kinds.get(entity) else {
        return Err(format!(
            "occasion `{on}` draws its targets from entity kind `{entity}`, which the project \
             does not declare under `entities:`"
        ));
    };
    // `open:` members are engine-populated: only the prefix is checked. An
    // invalid kind is `E-ENTITY-KIND-SHAPE`'s, never this rule's.
    let kind_members: Option<&[String]> = match &kind.shape {
        KindShape::Members(ms) => Some(ms),
        KindShape::Open | KindShape::Invalid => None,
    };
    if let (Some(subset), Some(ms)) = (subset, kind_members) {
        if let Some(stray) = subset.iter().find(|m| !ms.contains(m)) {
            let hint = lute_manifest::suggest::nearest(stray, ms.iter().map(String::as_str), 2)
                .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
            return Err(format!(
                "occasion `{on}` lists `{stray}` in its target `members:`, but `{stray}` is not \
                 a member of entity kind `{entity}`{hint}"
            ));
        }
    }
    let member = target
        .strip_prefix(prefix.as_str())
        .and_then(|rest| rest.strip_prefix('.'))
        .filter(|m| !m.is_empty());
    let Some(legal) = decl.target.domain_members(kind_members) else {
        return match member {
            Some(_) => Ok(()),
            None => Err(format!(
                "target `{target}` is outside occasion `{on}`'s domain: its targets are \
                 `{prefix}.<{entity}>`"
            )),
        };
    };
    if member.is_some_and(|m| legal.iter().any(|x| x == m)) {
        return Ok(());
    }
    let domain: Vec<String> = legal.iter().map(|m| format!("{prefix}.{m}")).collect();
    let hint = lute_manifest::suggest::nearest(target, domain.iter().map(String::as_str), 2)
        .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
    // A long domain is named by its first few members; the did-you-mean
    // carries the one that matters.
    pub(super) const SHOWN: usize = 8;
    let mut listed = domain
        .iter()
        .take(SHOWN)
        .map(|t| format!("`{t}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if domain.len() > SHOWN {
        listed.push_str(&format!(", … {} more", domain.len() - SHOWN));
    }
    Err(if subset.is_some() {
        format!(
            "target `{target}` is outside occasion `{on}`'s member list ({listed}), a subset of \
             entity kind `{entity}`{hint}"
        )
    } else {
        format!(
            "target `{target}` is outside occasion `{on}`'s domain `{prefix}.<{entity}>` \
             ({listed}){hint}"
        )
    })
}

/// dsl 0.26.0 §5: the targets a `target="kind:<kind>"` beat on `decl`
/// answers — the domain's prefix and the kind's members (a sub-kind's
/// members are already its parent's). The one rule `lute check` (beat
/// targets, [`E_BEAT_ATTR`]), the compiler (the beat's `targetKind`) and
/// `lute play` (candidates) share. `Err` when the occasion draws no targets
/// from an entity kind, the kind is unknown (with a did-you-mean) or `open:`,
/// or a member lies outside the occasion's domain.
pub fn kind_target_members(
    decl: &OccasionDecl,
    kind: &str,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Result<(String, Vec<String>), String> {
    let on = &decl.name;
    let OccasionTarget::Domain { prefix, entity, .. } = &decl.target else {
        return Err(format!(
            "`target=\"kind:{kind}\"` answers occasion `{on}` for every member of a kind, but \
             `{on}` does not draw its targets from an entity kind (declare its `target:` as \
             `{{ prefix, entity }}`) (dsl 0.26.0 §5)"
        ));
    };
    let Some(decl_kind) = kinds.get(kind) else {
        let hint = lute_manifest::suggest::nearest(kind, kinds.keys().map(String::as_str), 2)
            .map_or_else(String::new, |near| {
                format!(" — did you mean `kind:{near}`?")
            });
        return Err(format!(
            "`target=\"kind:{kind}\"`: `{kind}` is not a declared entity kind{hint} (dsl 0.26.0 §5)"
        ));
    };
    let KindShape::Members(members) = &decl_kind.shape else {
        return Err(format!(
            "`target=\"kind:{kind}\"`: entity kind `{kind}` is `open:`, so its members are not \
             known to check or play; target a kind that lists its `members:` (dsl 0.26.0 §5)"
        ));
    };
    let entity_members = match kinds.get(entity).map(|k| &k.shape) {
        Some(KindShape::Members(ms)) => Some(ms.as_slice()),
        _ => None,
    };
    let legal = decl.target.domain_members(entity_members);
    let outside: Vec<&str> = members
        .iter()
        .filter(|m| legal.is_none_or(|l| !l.contains(m)))
        .map(String::as_str)
        .collect();
    if !outside.is_empty() {
        return Err(format!(
            "`target=\"kind:{kind}\"`: {} outside occasion `{on}`'s domain `{prefix}.<{entity}>`; \
             a kind target names `{entity}` or one of its sub-kinds (dsl 0.26.0 §5)",
            outside
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ")
                + if outside.len() == 1 { " is" } else { " are" }
        ));
    }
    Ok((prefix.clone(), members.clone()))
}

/// dsl 0.22.0 §8: every beat target of one document — the scene's own
/// `target:` and each `<entry on= target=>` — against its occasion's target
/// domain ([`occasion_target_ok`], a `kind:` target [`kind_target_members`]);
/// outside it is [`E_BEAT_ATTR`] at the target value. Runs after the fold
/// (the entity vocabulary comes from the imported schema). An unknown
/// occasion, an untargeted one, or a malformed target is already someone
/// else's diagnostic.
pub(crate) fn check_beat_target_domains(
    doc: &Document,
    beat: Option<&BeatMeta>,
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    // `restricts`: whether a target on an occasion raised for none is already
    // reported (a scene's / bundle beat's, [`check_occasion`]); an entry's is
    // metadata there, which a `kind:` target can never be.
    let mut judge = |on: &str, target: &str, span: Span, layer: Layer, restricts: bool| {
        let Some(decl) = occasions.get(on) else {
            // An unknown occasion is `E-OCCASION-UNKNOWN`'s; without any
            // declared occasion a kind has no target prefix to answer.
            if occasions.is_empty() && kind_target(target).is_some() {
                diags.push(beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    format!(
                        "`target=\"{target}\"` answers occasion `{on}` for every member of a \
                         kind, which needs `{on}` declared by a plugin with a `target:` domain \
                         (dsl 0.26.0 §5)"
                    ),
                    span,
                    layer,
                ));
            }
            return;
        };
        let verdict = match kind_target(target) {
            Some(_) if restricts && !decl.target.takes_target() => return,
            // An entry's `kind:` target on a `select: sequence` occasion
            // raised for no target meant `for=`, as a scene's does
            // ([`check_occasion`]).
            Some(kind)
                if !decl.target.takes_target()
                    && decl.select == lute_manifest::schema::OccasionSelect::Sequence =>
            {
                Err(format!(
                    "occasion `{on}` is not raised for a target (declared without `target: \
                     true`), so `target=\"{target}\"` cannot answer it for each member; to \
                     present this entry once for each member of `{kind}`, write \
                     `for=\"kind:{kind}\"` instead of `target`"
                ))
            }
            Some(kind) => kind_target_members(decl, kind, kinds).map(drop),
            None => occasion_target_ok(decl, target, kinds),
        };
        if let Err(why) = verdict {
            diags.push(beat_diag(E_BEAT_ATTR, Severity::Error, why, span, layer));
        }
    };
    if let Some(BeatMeta {
        on,
        target: Some(target),
        ..
    }) = beat
    {
        judge(
            on,
            target,
            top_value_span(&doc.meta, "target"),
            Layer::Content,
            true,
        );
    }
    for entry in &doc.entries {
        let (Some((on, _)), Some((target, span))) = (&entry.on, &entry.target) else {
            continue;
        };
        if is_name(on) && is_beat_target(target) {
            judge(on, target, *span, Layer::Logic, false);
        }
    }
    // dsl 0.23.0 §4: a bundle beat's target, like an entry beat's.
    for beat in &doc.beats {
        let (Some((on, _)), Some((target, span))) = (&beat.on, &beat.target) else {
            continue;
        };
        if is_name(on) && is_beat_target(target) {
            judge(on, target, *span, Layer::Logic, true);
        }
    }
    // dsl 0.23.0 §2: an objective's target is checked like a beat's.
    for node in doc.quests.iter().flat_map(|q| &q.body) {
        let Node::Objective(o) = node else { continue };
        let (Some((on, _)), Some((target, span))) = (&o.on, &o.target) else {
            continue;
        };
        if is_name(on) && is_entry_target(target) {
            judge(on, target, *span, Layer::Logic, true);
        }
    }
    diags
}

/// Why a read of [`OCCASION_TARGET`] outside a kind beat has no value.
pub(crate) fn occasion_target_scope_message() -> String {
    format!(
        "`{OCCASION_TARGET}` is readable only in a beat or entry that targets a kind \
         (`target=\"kind:<kind>\"`) or runs once for each member of one \
         (`for=\"kind:<kind>\"`), where it is the member the beat answers (dsl 0.27.0 §3)"
    )
}

/// G-7: [`occasion_target_scope_message`] for a use of `@def`, whose body
/// (`body`, as declared) reads [`OCCASION_TARGET`].
pub(super) fn occasion_target_def_scope_message(def: &str, body: &str) -> String {
    format!(
        "`@{def}` reads `{OCCASION_TARGET}` (`{def}: {}`), which has a value only in a beat or \
         entry that targets a kind (`target=\"kind:<kind>\"`) or runs once for each member of \
         one (`for=\"kind:<kind>\"`) — use `@{def}` there, or read something this beat has \
         (dsl 0.27.0 §3)",
        body.trim()
    )
}

/// G-7: the defs of `bodies` whose body reads [`OCCASION_TARGET`], directly
/// or through another def.
pub(crate) fn defs_reading_target(
    bodies: &BTreeMap<String, String>,
) -> std::collections::BTreeSet<String> {
    let mut out: std::collections::BTreeSet<String> = bodies
        .iter()
        .filter(|(_, body)| lute_manifest::semantics::occasion_bind::mentions_target(body))
        .map(|(name, _)| name.clone())
        .collect();
    loop {
        let more: Vec<String> = bodies
            .iter()
            .filter(|(name, body)| {
                !out.contains(*name)
                    && lute_cel::scan_refs(body)
                        .iter()
                        .any(|r| !r.is_dollar && out.contains(&r.name))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if more.is_empty() {
            return out;
        }
        out.extend(more);
    }
}

/// dsl 0.26.0 §5: the members the document's kind beats answer — the domain
/// of [`OCCASION_TARGET`], sorted, empty without a (well-formed) kind beat
/// ([`crate::occasion_bind::occasion_scopes`], dsl 0.27.0 §3).
pub fn occasion_target_members(
    doc: &Document,
    beat: Option<&BeatMeta>,
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<String> {
    crate::occasion_bind::occasion_scopes(doc, beat, occasions, kinds).members()
}

/// dsl 0.26.0 §5: in a lore document, a read of [`OCCASION_TARGET`] in an
/// entry or bundle beat that does not target a kind is `E-UNDECLARED` (the
/// document declares it for its kind beats; this one is never raised for a
/// member). A scene has one beat, so its declaration is the whole scope.
/// `scoped`: the document has a kind beat — without one, every direct read
/// is already [`crate::cel_resolve::check_cel_slot`]'s (nothing declares
/// the path), so only a def's read is judged here. G-7: a use of a def of
/// `def_bodies` reading it ([`defs_reading_target`]) is judged the same.
pub(crate) fn check_occasion_target_scope(
    doc: &Document,
    def_bodies: &BTreeMap<String, String>,
    scoped: bool,
) -> Vec<Diagnostic> {
    let target_defs = defs_reading_target(def_bodies);
    if !scoped && target_defs.is_empty() {
        return Vec::new();
    }
    let is_kind =
        |t: &Option<(String, Span)>| t.as_ref().is_some_and(|(t, _)| kind_target(t).is_some());
    let outside: Vec<Span> = doc
        .entries
        .iter()
        .filter(|e| !is_kind(&e.target) && !is_kind(&e.for_kind))
        .map(|e| e.span)
        .chain(
            doc.beats
                .iter()
                .filter(|b| !is_kind(&b.target) && !is_kind(&b.for_kind))
                .map(|b| b.span),
        )
        .collect();
    if scoped && outside.len() == doc.entries.len() + doc.beats.len() {
        return Vec::new();
    }
    let within = |s: Span| {
        !scoped
            || outside
                .iter()
                .any(|o| o.byte_start <= s.byte_start && s.byte_end <= o.byte_end)
    };
    // What `raw` reads that has no value here: `occasion.target` itself, or
    // the first def reading it.
    let fault = |raw: &str| -> Option<String> {
        if raw.contains(OCCASION_TARGET) {
            return scoped.then(occasion_target_scope_message);
        }
        lute_cel::scan_refs(raw)
            .into_iter()
            .find(|r| !r.is_dollar && target_defs.contains(&r.name))
            .map(|r| occasion_target_def_scope_message(&r.name, &def_bodies[&r.name]))
    };
    let mut faults: Vec<(Span, String)> = Vec::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        if within(slot.span) {
            faults.extend(fault(&slot.raw).map(|m| (slot.span, m)));
        }
    });
    pub(super) fn lines<'a>(nodes: &'a [Node], f: &mut impl FnMut(&'a lute_syntax::ast::Line)) {
        for n in nodes {
            match n {
                Node::Line(l) => f(l),
                Node::Branch(b) => b.choices.iter().for_each(|c| lines(&c.body, f)),
                Node::Hub(h) => h.bodies().for_each(|b| lines(b, f)),
                Node::Match(m) => m.arms.iter().for_each(|arm| match arm {
                    lute_syntax::ast::Arm::When { body, .. }
                    | lute_syntax::ast::Arm::Otherwise { body, .. } => lines(body, f),
                }),
                Node::Objective(o) => lines(&o.body, f),
                Node::On(o) => lines(&o.body, f),
                _ => {}
            }
        }
    }
    let bodies = doc
        .entries
        .iter()
        .filter(|e| !scoped || (!is_kind(&e.target) && !is_kind(&e.for_kind)))
        .map(|e| &e.body)
        .chain(
            doc.beats
                .iter()
                .filter(|b| !scoped || (!is_kind(&b.target) && !is_kind(&b.for_kind)))
                .map(|b| &b.body),
        )
        .chain(doc.sections.iter().filter(|_| !scoped).map(|s| &s.body));
    for body in bodies {
        lines(body, &mut |l| {
            faults.extend(
                l.interps
                    .iter()
                    .filter_map(|i| fault(&i.raw).map(|m| (i.span, m))),
            );
        });
    }
    faults
        .into_iter()
        .map(|(span, message)| {
            beat_diag("E-UNDECLARED", Severity::Error, message, span, Layer::Cel)
        })
        .collect()
}

/// A scene `when` reads the scene's own `scene.*` state (dsl 0.21.0 §3.1):
/// the beat is chosen before the scene runs, so that state does not exist
/// yet. One [`E_BEAT_ATTR`] per distinct path, at the slot.
pub(crate) fn scene_when_scene_reads(paths: &[String], slot: &CelSlot) -> Vec<Diagnostic> {
    paths
        .iter()
        .map(|path| {
            beat_diag(
                E_BEAT_ATTR,
                Severity::Error,
                format!(
                    "`when:` reads `{path}`, but a beat's `when` is evaluated before the scene \
                     runs, when its own `scene.*` state does not exist yet — gate on `run.*` / \
                     `user.*` / `app.*` state, `quest.*`, `entry.<id>.read`, or a fact query \
                     (dsl 0.21.0 §3.1)"
                ),
                slot.span,
                Layer::Cel,
            )
        })
        .collect()
}

/// The per-file [`E_BEAT_UNREACHABLE`] message (the fact-envelope pass
/// appends its reasons).
pub(crate) fn beat_unreachable_message(scene: &str, when: &str, reasons: Option<&str>) -> String {
    match reasons {
        Some(reasons) => format!(
            "beat `{scene}` is never eligible: its `when` `{when}` is provably false — {reasons} \
             (dsl 0.21.0 §5)"
        ),
        None => format!(
            "beat `{scene}` is never eligible: its `when` `{when}` is provably false \
             (dsl 0.21.0 §5)"
        ),
    }
}

/// The name a beat scene goes by in messages: its canonical scene key, or
/// `this scene` when it has none (an `E-META-MISSING` document).
pub(crate) fn scene_beat_name(folded: &FoldedEnv) -> String {
    crate::meta::canonical_scene_key(&folded.typed).unwrap_or_else(|| "this scene".to_string())
}

