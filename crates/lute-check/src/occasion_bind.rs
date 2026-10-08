//! dsl 0.27.0 §3 (T2-1, T2-10): `occasion.target` as a typed ground term.
//!
//! Inside a beat that targets a kind (`target="kind:K"`) or presents once
//! per member (`for="kind:K"`), `occasion.target` is the member the beat
//! runs for. It may appear where a compile-time-ground member is expected:
//! a list-form fact-query pattern argument (`holds('owned', [occasion.target])`)
//! and an entity-indexed family index (`user.bond[occasion.target]`). The
//! checker judges such a slot once per member ([`instantiate`] over the
//! scope's members, [`OccasionScopes`]); the runtime substitutes the bound
//! member when it evaluates the slot. Any other read (`occasion.target == 'x'`,
//! `{{occasion.target}}`) is an ordinary read of the bound value.

use lute_manifest::semantics::occasion_bind::{occurrences, Position, OCCASION_PAYLOAD};

/// Whether `cel` reads `occasion.target` where only a ground member is
/// legal — a fact-query pattern argument or a family index — so it must be
/// judged per member.
pub fn binds_target(cel: &str) -> bool {
    occurrences(cel)
        .iter()
        .any(|o| o.position != Position::Value)
}

/// `cel` with every `occasion.target` replaced by `member`: a pattern
/// argument by the member as a name (`holds('owned', [aria])`,
/// `holds('at', ['lab-b2'])`), a family index by a member path
/// (`user.bond[occasion.target]` → `user.bond.aria` / `run.visits["lab-b2"]`),
/// any other read by the string literal `'aria'`.
pub fn instantiate(cel: &str, member: &str) -> String {
    rewrite(cel, member, true)
}

/// Like [`instantiate`], but only the positions that need a ground member
/// (pattern arguments, family indexes) are replaced; an ordinary read keeps
/// reading the (declared) `occasion.target` path.
pub fn instantiate_bound(cel: &str, member: &str) -> String {
    rewrite(cel, member, false)
}

fn rewrite(cel: &str, member: &str, values: bool) -> String {
    let mut out = String::with_capacity(cel.len());
    let mut at = 0;
    for o in occurrences(cel) {
        let list_context = cel[..o.range.0]
            .rmatch_indices('[')
            .next()
            .map_or(false, |(open, _)| {
                cel[..o.range.0]
                    .rmatch_indices(']')
                    .next()
                    .is_none_or(|(close, _)| open > close)
            });
        let replacement = match o.position {
            Position::PatternArg if !list_context && lute_manifest::ident::is_ident(member) => {
                member.to_string()
            }
            Position::PatternArg => {
                format!("'{}'", member.replace('\\', "\\\\").replace('\'', "\\'"))
            }
            Position::Index => lute_cel::path::bracket_spelling(&["", member]),
            Position::Value if values => format!("'{member}'"),
            Position::Value => continue,
        };
        out.push_str(&cel[at..o.range.0]);
        out.push_str(&replacement);
        at = o.range.1;
    }
    out.push_str(&cel[at..]);
    out
}

/// The members `occasion.target` ranges over in each part of one document:
/// a byte range (a kind-targeting or `for=` entry / bundle beat, or the
/// whole scene document) and its members, in member order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OccasionScopes {
    pub scopes: Vec<(usize, usize, Vec<String>)>,
}

impl OccasionScopes {
    /// The members of the innermost scope enclosing `[start, end)`.
    pub fn members_at(&self, start: usize, end: usize) -> Option<&[String]> {
        self.scopes
            .iter()
            .filter(|(s, e, _)| *s <= start && end <= *e)
            .min_by_key(|(s, e, _)| e - s)
            .map(|(_, _, m)| m.as_slice())
    }

    /// Every member of every scope, sorted: the domain `occasion.target` is
    /// declared with.
    pub fn members(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .scopes
            .iter()
            .flat_map(|(_, _, m)| m.iter().cloned())
            .collect();
        out.sort();
        out.dedup();
        out
    }
}

type Occasions = std::collections::BTreeMap<String, lute_manifest::schema::OccasionDecl>;
type Kinds = std::collections::BTreeMap<String, lute_manifest::relations::EntityKindDecl>;

/// dsl 0.27.0 §3 (T2-10): the members a `for="kind:<kind>"` beat on `on` is
/// presented for — the kind's members, in member order (a sub-kind's
/// already its parent's). The one rule the checker (`E-BEAT-ATTR`), the
/// compiler (the beat's `forKind`) and `lute play` share. `Err` is the
/// whole reason: not a `kind:` value, a `target` beside it, an occasion
/// that is raised for a target or is not `select: sequence`, or a kind that
/// is unknown (with a did-you-mean) or `open:`. An undeclared occasion is
/// `E-OCCASION-UNKNOWN`'s, so it is `Ok` with no members here.
pub fn for_kind_members(
    on: &str,
    raw: &str,
    has_target: bool,
    occasions: &Occasions,
    kinds: &Kinds,
) -> Result<(String, Vec<String>), String> {
    let Some(kind) = crate::kind_target(raw) else {
        // `for="villager"`: the kind named without its `kind:` prefix.
        let bare = raw.trim();
        let near = kinds
            .contains_key(bare)
            .then_some(bare)
            .or_else(|| lute_manifest::suggest::nearest(bare, kinds.keys().map(String::as_str), 2));
        let hint = near.map_or_else(String::new, |k| {
            format!(" — did you mean `for=\"kind:{k}\"`?")
        });
        return Err(format!(
            "`for=\"{raw}\"` must name a kind, `for=\"kind:<kind>\"`{hint}"
        ));
    };
    if has_target {
        return Err(format!(
            "`for=\"kind:{kind}\"` presents the beat once per member of an occasion raised for no \
             target; a beat with `target` answers one raise — remove one of the two (dsl 0.27.0 §3)"
        ));
    }
    if let Some(decl) = occasions.get(on) {
        if decl.target.takes_target() {
            return Err(format!(
                "`for=\"kind:{kind}\"` needs an occasion raised for no target, but `{on}` is \
                 raised for one — use `target=\"kind:{kind}\"` instead (dsl 0.27.0 §3)"
            ));
        }
        if decl.select != lute_manifest::schema::OccasionSelect::Sequence {
            return Err(format!(
                "`for=\"kind:{kind}\"` presents the beat once per member, one after another, \
                 which needs a `select: sequence` occasion; `{on}` is `select: {}` (dsl 0.27.0 §3)",
                decl.select.as_str()
            ));
        }
    }
    let Some(decl) = kinds.get(kind) else {
        let hint = lute_manifest::suggest::nearest(kind, kinds.keys().map(String::as_str), 2)
            .map_or_else(String::new, |near| {
                format!(" — did you mean `kind:{near}`?")
            });
        return Err(format!(
            "`for=\"kind:{kind}\"`: `{kind}` is not a declared entity kind{hint} (dsl 0.27.0 §3)"
        ));
    };
    match &decl.shape {
        lute_manifest::relations::KindShape::Members(ms) => Ok((kind.to_string(), ms.clone())),
        _ => Err(format!(
            "`for=\"kind:{kind}\"`: entity kind `{kind}` is `open:`, so its members are not \
             known to check or play; name a kind that lists its `members:` (dsl 0.27.0 §3)"
        )),
    }
}

/// dsl 0.27.0 §3: every `for=` of the document's entries and bundle beats,
/// and a scene's `for:`, against [`for_kind_members`]; a bad one is
/// `E-BEAT-ATTR` at its value. A `for=` without `on` is the missing-`on`
/// diagnostic's.
pub(crate) fn check_for_kinds(
    doc: &lute_syntax::ast::Document,
    beat: Option<&crate::beats::BeatMeta>,
    occasions: &Occasions,
    kinds: &Kinds,
) -> Vec<lute_core_span::Diagnostic> {
    let scene = beat.and_then(|b| Some((b.on.as_str(), b.for_kind.as_ref()?, b.target.is_some())));
    let elements = doc
        .entries
        .iter()
        .map(|e| (&e.on, &e.for_kind, e.target.is_some()))
        .chain(
            doc.beats
                .iter()
                .map(|b| (&b.on, &b.for_kind, b.target.is_some())),
        )
        .filter_map(|(on, for_kind, has_target)| {
            Some((on.as_ref()?.0.as_str(), for_kind.as_ref()?, has_target))
        });
    let mut out = Vec::new();
    let from_scene = usize::from(scene.is_some());
    for (i, (on, (raw, span), has_target)) in scene.into_iter().chain(elements).enumerate() {
        if let Err(why) = for_kind_members(on, raw, has_target, occasions, kinds) {
            // A scene writes these keys in its frontmatter: `for: "kind:K"`.
            let message = if i < from_scene {
                why.replace("`for=\"", "`for: \"")
                    .replace("`target=\"", "`target: \"")
                    .replace("a beat with `target`", "a scene with `target:`")
            } else {
                why
            };
            out.push(lute_core_span::Diagnostic {
                code: crate::beats::E_BEAT_ATTR.to_string(),
                severity: lute_core_span::Severity::Error,
                message,
                evidence: None,
                span: *span,
                layer: lute_core_span::Layer::Logic,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    }
    out
}

/// dsl 0.27.0 §3: the `occasion.payload.<field>` decls a document's beats
/// read — every field of every occasion one of its beats answers (a scene's
/// own beat, each entry / bundle beat with `on`), first declaration wins —
/// and an `E-UNDECLARED` for each condition reading a field the enclosing
/// beat's occasion does not declare (a read outside any beat included).
/// dsl 0.28.0 (T2-8): an `<objective on="O">` is judged at O's raise, and a
/// raise fires the same-named world event, so the objective (but not its
/// `by` / `visibleWhen`, judged between raises) and an `<on event="O">`
/// handler read O's payload too.
pub fn payload_decls(
    doc: &lute_syntax::ast::Document,
    beat: Option<&crate::beats::BeatMeta>,
    occasions: &Occasions,
) -> (
    std::collections::BTreeMap<String, lute_manifest::types::Type>,
    Vec<lute_core_span::Diagnostic>,
) {
    let mut regions: Vec<(usize, usize, &str)> = Vec::new();
    if let Some(b) = beat {
        regions.push((0, usize::MAX, b.on.as_str()));
    }
    for e in &doc.entries {
        if let Some(on) = meant_occasion(&e.on, &e.attrs) {
            regions.push((e.span.byte_start, e.span.byte_end, on));
        }
    }
    for b in &doc.beats {
        if let Some(on) = meant_occasion(&b.on, &b.attrs) {
            regions.push((b.span.byte_start, b.span.byte_end, on));
        }
    }
    // Slots inside such a region the raise does not judge: (span, what).
    let mut between: Vec<(lute_core_span::Span, &str)> = Vec::new();
    for q in &doc.quests {
        raised_regions(&q.body, occasions, &mut regions, &mut between);
    }
    for s in &doc.sections {
        raised_regions(&s.body, occasions, &mut regions, &mut between);
    }
    let mut decls = std::collections::BTreeMap::new();
    for (_, _, on) in &regions {
        for (field, ty) in occasions.get(*on).map(|d| &d.payload).into_iter().flatten() {
            decls
                .entry(format!("{OCCASION_PAYLOAD}.{field}"))
                .or_insert_with(|| ty.clone());
        }
    }
    let mut diags = Vec::new();
    if decls.is_empty() {
        return (decls, diags);
    }
    let prefix = format!("{OCCASION_PAYLOAD}.");
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        let mask = lute_manifest::text::cel_string_mask(&slot.raw);
        for (at, _) in slot.raw.match_indices(&prefix) {
            if mask.get(at).copied().unwrap_or(false) {
                continue;
            }
            let rest = &slot.raw[at + prefix.len()..];
            let field: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !decls.contains_key(&format!("{prefix}{field}")) {
                continue; // undeclared anywhere: the ordinary `E-UNDECLARED`
            }
            // dsl 0.28.0 (T2-8): an objective's `by` / `visibleWhen` is
            // judged between raises, when no payload is bound.
            let off = between.iter().find(|(s, _)| {
                s.byte_start <= slot.span.byte_start && slot.span.byte_end <= s.byte_end
            });
            let on = regions
                .iter()
                .filter(|(s, e, _)| *s <= slot.span.byte_start && slot.span.byte_end <= *e)
                .min_by_key(|(s, e, _)| e - s)
                .map(|(_, _, on)| *on);
            let declares = on
                .and_then(|on| occasions.get(on))
                .is_some_and(|d| d.payload.contains_key(&field));
            if declares && off.is_none() {
                continue;
            }
            let message = match (off, on) {
                (Some((_, what)), Some(on)) => format!(
                    "`{prefix}{field}` is bound only while its occasion is raised, and {what} is \
                     judged between raises of `{on}`, not at one — read the payload in the \
                     objective's `done` or `until`, which `{on}`'s raise judges"
                ),
                (_, Some(on)) => format!(
                    "`{prefix}{field}` is not part of occasion `{on}`'s payload{}; a beat, an \
                     `<objective on>` or an `<on event>` handler reads the payload of the occasion \
                     it answers (dsl 0.27.0 §3)",
                    match occasions.get(on).map(|d| &d.payload) {
                        Some(p) if !p.is_empty() => format!(
                            " (declared: {})",
                            p.keys()
                                .map(|k| format!("`{k}`"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        _ => " (it declares none)".to_string(),
                    }
                ),
                (_, None) => format!(
                    "`{prefix}{field}` is bound only while its occasion is raised: it is readable \
                     in a beat answering an occasion that declares it under `payload:`, in an \
                     `<objective on>` of that occasion (`done`, `until`) and in an `<on event>` \
                     handler of it (dsl 0.27.0 §3)"
                ),
            };
            diags.push(lute_core_span::Diagnostic {
                code: "E-UNDECLARED".to_string(),
                severity: lute_core_span::Severity::Error,
                message,
                evidence: None,
                span: slot.span,
                layer: lute_core_span::Layer::Cel,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    });
    (decls, diags)
}

/// The occasion an entry or bundle beat answers: its `on=`, else the value of
/// the `occasion=` / `event=` it wrote instead — that attribute's error is
/// the one report, so its payload reads are judged as the occasion's.
fn meant_occasion<'a>(
    on: &'a Option<(String, lute_core_span::Span)>,
    attrs: &'a [lute_syntax::ast::Attr],
) -> Option<&'a str> {
    on.as_ref().map(|(on, _)| on.as_str()).or_else(|| {
        attrs.iter().find_map(|a| match &a.value {
            lute_syntax::ast::AttrValue::Str(v) if a.key == "occasion" || a.key == "event" => {
                Some(v.as_str())
            }
            _ => None,
        })
    })
}

/// dsl 0.28.0 (T2-8): the parts of `nodes` a raise judges, with the
/// occasion whose payload is bound there — each `<objective on="O">` and
/// each `<on event="O">` handler of a declared occasion `O` — into
/// `regions`; the objective slots judged between raises (`by`,
/// `visibleWhen`) into `between`, named for the message.
fn raised_regions<'d>(
    nodes: &'d [lute_syntax::ast::Node],
    occasions: &Occasions,
    regions: &mut Vec<(usize, usize, &'d str)>,
    between: &mut Vec<(lute_core_span::Span, &'static str)>,
) {
    use lute_syntax::ast::{Arm, Node};
    for node in nodes {
        match node {
            Node::Objective(o) => {
                if let Some((on, _)) = &o.on {
                    regions.push((o.span.byte_start, o.span.byte_end, on));
                    if let Some(by) = &o.by {
                        between.push((by.span, "an objective's `by`"));
                    }
                    if let Some(when) = &o.visible_when {
                        between.push((when.span, "an objective's `visibleWhen`"));
                    }
                }
                raised_regions(&o.body, occasions, regions, between);
            }
            Node::On(h) => {
                if occasions.contains_key(&h.event) {
                    regions.push((h.span.byte_start, h.span.byte_end, h.event.as_str()));
                }
                raised_regions(&h.body, occasions, regions, between);
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    raised_regions(body, occasions, regions, between);
                }
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    raised_regions(&c.body, occasions, regions, between);
                }
            }
            Node::Hub(h) => {
                for body in h.bodies() {
                    raised_regions(body, occasions, regions, between);
                }
            }
            _ => {}
        }
    }
}

/// 0.27 prerelease G-6: a `payload:` field typed `{ domain: K }` /
/// `{ entity: K }` of an occasion this document's beats answer names a
/// declared enum or entity kind — else `E-DOMAIN-UNKNOWN` with a
/// did-you-mean, reported at the plugin's `payload:` line when the CLI
/// placed it (`check-project` folds the importers' copies into one), else
/// at the answering beat's `on`.
pub(crate) fn check_payload_domains(
    doc: &lute_syntax::ast::Document,
    beat: Option<&crate::beats::BeatMeta>,
    occasions: &Occasions,
    domains: &std::collections::BTreeMap<String, lute_manifest::snapshot::Domain>,
    origins: &crate::rel_schema::PluginOrigins,
) -> Vec<lute_core_span::Diagnostic> {
    use lute_manifest::types::Type;
    let mut ons: Vec<(&str, lute_core_span::Span)> = Vec::new();
    if let Some(b) = beat {
        ons.push((b.on.as_str(), doc.meta.span));
    }
    let entries = doc.entries.iter().filter_map(|e| e.on.as_ref());
    let beats = doc.beats.iter().filter_map(|b| b.on.as_ref());
    ons.extend(entries.chain(beats).map(|(on, span)| (on.as_str(), *span)));
    let names: Vec<String> = domains.keys().cloned().collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (on, span) in ons {
        if !seen.insert(on) {
            continue;
        }
        for (field, ty) in occasions.get(on).map(|d| &d.payload).into_iter().flatten() {
            let (Type::Domain(k) | Type::Entity(k)) = ty else {
                continue;
            };
            if domains.contains_key(k) {
                continue;
            }
            let form = if matches!(ty, Type::Entity(_)) {
                "entity"
            } else {
                "domain"
            };
            let d = lute_core_span::Diagnostic {
                code: "E-DOMAIN-UNKNOWN".to_string(),
                severity: lute_core_span::Severity::Error,
                message: format!(
                    "occasion `{on}`'s payload field `{field}` is typed `{{ {form}: {k} }}`, but \
                     `{k}` is not a declared enum or entity kind{}",
                    crate::rel_schema::member_hint(k, &names)
                ),
                evidence: None,
                span,
                layer: lute_core_span::Layer::Content,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            };
            out.push(crate::rel_schema::at_plugin_origin(
                d,
                origins
                    .payloads
                    .get(&crate::rel_schema::effect_origin_key(on, field)),
            ));
        }
    }
    out
}

/// The document's [`OccasionScopes`]: a scene whose own beat targets a kind
/// covers the whole document; each entry / bundle beat targeting a kind, or
/// presented once per member (`for="kind:<kind>"`), covers its element.
/// Members come from [`crate::beats::kind_target_members`] /
/// [`for_kind_members`]; an ill-formed one (already `E-BEAT-ATTR`) opens no
/// scope.
pub fn occasion_scopes(
    doc: &lute_syntax::ast::Document,
    beat: Option<&crate::beats::BeatMeta>,
    occasions: &Occasions,
    kinds: &Kinds,
) -> OccasionScopes {
    let members = |on: &str, target: Option<&str>, for_kind: Option<&str>| -> Option<Vec<String>> {
        if let Some(raw) = for_kind {
            return for_kind_members(on, raw, target.is_some(), occasions, kinds)
                .ok()
                .map(|(_, ms)| ms)
                .filter(|ms| !ms.is_empty());
        }
        let kind = crate::kind_target(target?)?;
        // A `target="kind:K"` its occasion refuses (not raised for a target,
        // or for no entity kind) is already `E-BEAT-ATTR`; its reads of
        // `occasion.target` still mean a member of K, so they are not
        // reported again as undeclared.
        crate::beats::kind_target_members(occasions.get(on)?, kind, kinds)
            .ok()
            .map(|(_, ms)| ms)
            .or_else(|| match &kinds.get(kind)?.shape {
                lute_manifest::relations::KindShape::Members(ms) => Some(ms.clone()),
                _ => None,
            })
    };
    fn text(v: &Option<(String, lute_core_span::Span)>) -> Option<&str> {
        v.as_ref().map(|(s, _)| s.as_str())
    }
    let mut scopes = Vec::new();
    if let Some(b) = beat {
        if let Some(ms) = members(&b.on, b.target.as_deref(), text(&b.for_kind)) {
            scopes.push((0, usize::MAX, ms));
        }
    }
    for e in &doc.entries {
        let Some((on, _)) = &e.on else { continue };
        if let Some(ms) = members(on, text(&e.target), text(&e.for_kind)) {
            scopes.push((e.span.byte_start, e.span.byte_end, ms));
        }
    }
    for b in &doc.beats {
        let Some((on, _)) = &b.on else { continue };
        if let Some(ms) = members(on, text(&b.target), text(&b.for_kind)) {
            scopes.push((b.span.byte_start, b.span.byte_end, ms));
        }
    }
    OccasionScopes { scopes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_positions() {
        let cel = "holds(owned(occasion.target)) && user.bond[ occasion.target ] >= 2 \
                   && occasion.target != 'x' && count(at(sol, occasion.target)) > 0";
        let pos: Vec<Position> = occurrences(cel).iter().map(|o| o.position).collect();
        assert_eq!(
            pos,
            [
                Position::PatternArg,
                Position::Index,
                Position::Value,
                Position::PatternArg
            ]
        );
        assert_eq!(
            instantiate(cel, "aria"),
            "holds(owned(aria)) && user.bond.aria >= 2 && 'aria' != 'x' && count(at(sol, aria)) > 0"
        );
        assert_eq!(
            instantiate_bound(cel, "aria"),
            "holds(owned(aria)) && user.bond.aria >= 2 && occasion.target != 'x' && count(at(sol, aria)) > 0"
        );
    }

    #[test]
    fn strings_def_args_and_lookalikes_are_values_or_untouched() {
        assert!(occurrences("'occasion.target' == run.x").is_empty());
        assert!(occurrences("occasion.targets == 1").is_empty());
        assert!(occurrences("run.occasion.target == 1").is_empty());
        let def = occurrences("@bondAt(occasion.target) > 1");
        assert_eq!(def[0].position, Position::Value);
        let isset = occurrences("holds(owned(_)) && isSet(occasion.target)");
        assert_eq!(isset[0].position, Position::Value);
        assert!(!binds_target("occasion.target in ['a', 'b']"));
        assert!(binds_target("!holds(owned(occasion.target))"));
    }

    /// FS-F1: a multi-byte char outside a literal (`≥`, a curly quote,
    /// Hangul) is walked past, not sliced into.
    #[test]
    fn multibyte_text_outside_literals() {
        assert!(occurrences("run.clues ≥ 2 && run.who == ‘ruben’").is_empty());
        assert!(occurrences("run.who == 루벤").is_empty());
        assert_eq!(
            instantiate("user.bond[occasion.target] ≥ 2 — ok", "aria"),
            "user.bond.aria ≥ 2 — ok"
        );
    }
}
