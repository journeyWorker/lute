//! 0.51.0 §4.3, §4.5 (#170): declared occasion raises — the project raise
//! table and the per-call target check.
//!
//! A directive's or reward kind's `effects.raises` (0.51.0 §4.1) promises
//! that the host raises an occasion after the call's presentation ends, or
//! after a grant settles. [`RaiseTable::build`] resolves every declared raise
//! of one project root once per pass (D14), like 0.50's
//! [`crate::reward_effects::GrantTable`]: each document's calls against its
//! own capability snapshot and each reward against its own
//! [`crate::ProjectDoc::reward_kinds`]. The sources are `accept_sources`'
//! kinds — a scene's shots, a lore bundle beat, a quest's body and each of
//! its rewards once — plus a lore entry's body (a raise that anchors
//! nothing); a call inside a component body counts at each `::use`, with the
//! use's arguments bound, stopping at a `::use` cycle. No synthetic node is
//! created.
//!
//! A target resolves by one rule (§4.3): the call's authored attribute value
//! (an `@def` folds to its constant), else the attribute's declared
//! `default:`, else it is unbound and the raise does not happen. A value
//! reading `occasion.target` is the one dynamic target; a reward binds its
//! quoted `target=`. [`check_project_raise_targets`] reports
//! [`E_RAISES_TARGET`] for a resolved target outside the occasion's domain or
//! answered by no scene or beat; [`RaiseTable::reaches`] is the one answerer
//! rule every consumer shares.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::KindShape;
use lute_manifest::schema::{
    DirectiveDecl, OccasionDecl, OccasionTarget, RaiseDecl, RaiseTargetDecl,
};
use lute_manifest::semantics::beats::OCCASION_TARGET;
use lute_manifest::types::Literal;
use lute_syntax::ast::{Arm, Attr, AttrValue, ClipNode, Directive, Node, Quest, Reward};

use crate::beats::{project_beats, ProjectBeat};
use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::component_effects::{bind_attrs, use_args_for};
use crate::connectivity::{bundle_id, scene_key, NodeId};
use crate::meta::{resolve_doc_kind, DocKind};
use crate::ProjectDoc;

/// A resolved raise target is outside the occasion's domain, or no scene or
/// beat answers it (0.51.0 §4.3, whole-project `check`).
pub const E_RAISES_TARGET: &str = "E-RAISES-TARGET";

/// What one declared raise is raised for (0.51.0 §4.3).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaiseTarget {
    /// The occasion is raised for no target.
    Untargeted,
    /// A target known statically, written whole (`scene.chapter2`): a
    /// literal, an authored or defaulted attribute value (an `@def` folded),
    /// or a reward's `target=`, with a domain occasion's prefix applied.
    Static(String),
    /// A value that reads `occasion.target` (a call in a beat): the runtime
    /// binds it; it stands for every answerer of the occasion.
    Dynamic,
    /// No value, no `default:`, or a value that does not fold: the raise
    /// does not happen.
    Unbound,
}

/// Where a static target's value came from — what judges it already.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TargetOrigin {
    /// The raise names it (`target: scene.chapter2`).
    Literal,
    /// The call's attribute value, or the reward's `target=`: an entity
    /// check at the value already refuses a non-member of the kind.
    Authored,
    /// The attribute's declared `default:`.
    Default,
}

/// What declares a raise.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RaiseVia {
    /// A directive call, by tag.
    Directive(String),
    /// A granted `<reward>`, by kind.
    Reward(String),
}

/// One declared raise at its call or grant (0.51.0 §4.5 "Raise sources").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaiseSite<'d> {
    /// The document holding the call or reward, and its index in the
    /// `docs` the table was built from.
    pub path: &'d Path,
    pub doc: usize,
    /// The graph node whose run makes the raise: the scene, bundle beat or
    /// quest; `None` for a lore entry's call (a raise that anchors nothing).
    /// The node may not be in the graph; consumers check.
    pub source: Option<NodeId>,
    /// Where [`E_RAISES_TARGET`] anchors: an authored attribute's value, a
    /// reward's `target=`, else the call (a component's call at its
    /// `::use`) or the reward's `kind=`.
    pub span: Span,
    pub occasion: String,
    pub target: RaiseTarget,
    /// Meaningful for a [`RaiseTarget::Static`] target.
    pub origin: TargetOrigin,
    pub via: RaiseVia,
    /// The call or reward as messages show it.
    pub shown: String,
}

/// Every declared raise of one project root, in document then source order
/// ([`RaiseTable::build`]).
#[derive(Clone, Debug, Default)]
pub struct RaiseTable<'d> {
    sites: Vec<RaiseSite<'d>>,
}

impl<'d> RaiseTable<'d> {
    /// Resolve every declared raise of `docs` (parallel to `foldeds`).
    pub fn build(docs: &'d [ProjectDoc<'d>], foldeds: &[&'d FoldedEnv]) -> Self {
        let mut sites = Vec::new();
        let mut seen = BTreeSet::new();
        for (doc, (item, &folded)) in docs.iter().zip(foldeds).enumerate() {
            // A vocabulary that declares no raise has nothing to resolve: the
            // walk costs nothing for a project without the 0.51 keys.
            let directive_raises = folded
                .snapshot
                .directives
                .values()
                .any(|d| d.effects.as_ref().is_some_and(|e| !e.raises.is_empty()));
            let reward_raises = item
                .reward_kinds
                .values()
                .any(|k| k.effects.as_ref().is_some_and(|e| !e.raises.is_empty()));
            if !directive_raises && !reward_raises {
                continue;
            }
            let mut walk = Walk {
                path: item.path,
                doc,
                folded,
                sites: &mut sites,
                seen: &mut seen,
            };
            walk.document(item);
        }
        RaiseTable { sites }
    }

    /// Every site, unbound ones included.
    pub fn iter(&self) -> impl Iterator<Item = &RaiseSite<'d>> {
        self.sites.iter()
    }

    /// The one answerer rule (0.51.0 §4.4, §4.5): `site` reaches `beat`
    /// when it raises the beat's `on:` occasion and the occasion is
    /// untargeted, the beat answers every target (no `target:`), the target
    /// is `occasion.target`, or the beat answers the static target. An
    /// unbound raise reaches nothing.
    pub fn reaches(site: &RaiseSite<'_>, beat: &ProjectBeat<'_>) -> bool {
        site.occasion == beat.on
            && match &site.target {
                RaiseTarget::Untargeted | RaiseTarget::Dynamic => true,
                RaiseTarget::Static(t) => beat.cells().answers(t),
                RaiseTarget::Unbound => false,
            }
    }
}

/// The walk of one document's raise sources.
struct Walk<'w, 'd> {
    path: &'d Path,
    doc: usize,
    folded: &'d FoldedEnv,
    sites: &'w mut Vec<RaiseSite<'d>>,
    /// `(span, occasion, target, via)` already listed: an effects component's
    /// state-writing call is spliced into its host at the `::use`
    /// (`component_effects`), and the `::use` walk finds it again.
    seen: &'w mut BTreeSet<(usize, usize, String, RaiseTarget, RaiseVia)>,
}

/// A call inside a component body: the host `::use` it counts at, the
/// use's bound arguments and the component's params.
struct InComponent<'a> {
    /// The outermost host `::use`: where the call is reported.
    at: Span,
    /// The component whose body holds the call.
    name: &'a str,
    args: &'a BTreeMap<String, AttrValue>,
    params: &'a [(String, lute_manifest::types::Type)],
}

impl<'d> Walk<'_, 'd> {
    fn document(&mut self, item: &ProjectDoc<'d>) {
        let doc = item.doc;
        let scene = scene_key(item.meta).map(NodeId::Scene);
        for shot in &doc.sections {
            self.nodes(&shot.body, scene.as_ref(), None, &mut Vec::new());
        }
        let bundle = (resolve_doc_kind(&doc.meta).0 == Some(DocKind::Lore))
            .then(|| bundle_id(item.meta))
            .flatten();
        for beat in &doc.beats {
            let source = bundle
                .as_deref()
                .filter(|_| lute_manifest::ident::is_name(&beat.id))
                .map(|d| NodeId::Beat(crate::bundles::bundle_beat_key(d, &beat.id)));
            self.nodes(&beat.body, source.as_ref(), None, &mut Vec::new());
        }
        for entry in &doc.entries {
            self.nodes(&entry.body, None, None, &mut Vec::new());
        }
        for quest in &doc.quests {
            let source = (!quest.id.is_empty()).then(|| NodeId::Quest(quest.id.clone()));
            self.nodes(&quest.body, source.as_ref(), None, &mut Vec::new());
            self.rewards(quest, source.as_ref(), item);
        }
    }

    fn nodes(
        &mut self,
        nodes: &[Node],
        source: Option<&NodeId>,
        within: Option<&InComponent<'_>>,
        stack: &mut Vec<String>,
    ) {
        for node in nodes {
            match node {
                Node::Directive(d) if d.tag == "use" => self.use_site(d, source, within, stack),
                Node::Directive(d) => self.call(d, source, within),
                Node::Branch(b) => {
                    for c in &b.choices {
                        self.nodes(&c.body, source, within, stack);
                    }
                }
                Node::Hub(h) => {
                    for body in h.bodies() {
                        self.nodes(body, source, within, stack);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.nodes(body, source, within, stack);
                            }
                        }
                    }
                }
                Node::Timeline(t) => {
                    for clip in t.tracks.iter().flat_map(|track| &track.clips) {
                        if let ClipNode::Directive(d) = &clip.node {
                            self.call(d, source, within);
                        }
                    }
                }
                Node::Objective(o) => self.nodes(&o.body, source, within, stack),
                Node::On(o) => self.nodes(&o.body, source, within, stack),
                Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
            }
        }
    }

    /// A `::use`: its component's body, with the arguments bound (a nested
    /// `::use`'s arguments are bound by the outer use's first); a `::use`
    /// cycle stops the walk (`E-COMPONENT-CYCLE`'s).
    fn use_site(
        &mut self,
        d: &Directive,
        source: Option<&NodeId>,
        within: Option<&InComponent<'_>>,
        stack: &mut Vec<String>,
    ) {
        let bound = match within {
            Some(outer) => {
                let mut bound = d.clone();
                bind_attrs(&mut bound.attrs, outer.args, outer.params);
                std::borrow::Cow::Owned(bound)
            }
            None => std::borrow::Cow::Borrowed(d),
        };
        let folded: &'d FoldedEnv = self.folded;
        let components = &folded.components;
        let Some((name, def)) = bound.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
            ("component", AttrValue::Str(s)) => components.table.get_key_value(s),
            _ => None,
        }) else {
            return;
        };
        if stack.contains(name) {
            return;
        }
        let args = use_args_for(&bound, def);
        let inner = InComponent {
            at: within.map_or(d.span, |w| w.at),
            name,
            args: &args,
            params: &def.params,
        };
        stack.push(name.clone());
        for shot in &def.body.sections {
            self.nodes(&shot.body, source, Some(&inner), stack);
        }
        stack.pop();
    }

    /// One directive call's declared raises.
    fn call(&mut self, d: &Directive, source: Option<&NodeId>, within: Option<&InComponent<'_>>) {
        let folded = self.folded;
        let Some(decl) = folded.snapshot.directive(&d.tag) else {
            return;
        };
        let Some(effects) = decl.effects.as_ref().filter(|e| !e.raises.is_empty()) else {
            return;
        };
        for raise in &effects.raises {
            let Some(occasion) = folded.occasions.get(&raise.occasion) else {
                continue;
            };
            let (target, origin, value_span, written) =
                self.directive_target(raise, occasion, decl, &d.attrs, within);
            let shown = match (&written, within) {
                (Some(attr), None) => format!("`::{}{{{attr}}}`", d.tag),
                (Some(attr), Some(w)) => format!(
                    "`::{}{{{attr}}}` (in component `{}`, at this `::use`)",
                    d.tag, w.name
                ),
                (None, None) => format!("`::{}`", d.tag),
                (None, Some(w)) => {
                    format!("`::{}` (in component `{}`, at this `::use`)", d.tag, w.name)
                }
            };
            let span = match within {
                Some(w) => w.at,
                None => value_span.unwrap_or(d.span),
            };
            self.push(RaiseSite {
                path: self.path,
                doc: self.doc,
                source: source.cloned(),
                span,
                occasion: raise.occasion.clone(),
                target,
                origin,
                via: RaiseVia::Directive(d.tag.clone()),
                shown,
            });
        }
    }

    /// A directive raise's target (§4.3), with where its value sits and the
    /// attribute as the call writes it (`scene="chapter2"`).
    fn directive_target(
        &self,
        raise: &RaiseDecl,
        occasion: &OccasionDecl,
        decl: &DirectiveDecl,
        attrs: &[Attr],
        within: Option<&InComponent<'_>>,
    ) -> (RaiseTarget, TargetOrigin, Option<Span>, Option<String>) {
        let attr = match &raise.target {
            None => return (RaiseTarget::Untargeted, TargetOrigin::Literal, None, None),
            Some(RaiseTargetDecl::Literal(t)) => {
                return (
                    RaiseTarget::Static(t.clone()),
                    TargetOrigin::Literal,
                    None,
                    None,
                );
            }
            Some(RaiseTargetDecl::Attr(attr)) => attr,
        };
        let Some(written) = attrs.iter().find(|a| &a.key == attr) else {
            let default = decl
                .attrs
                .iter()
                .find(|a| &a.name == attr)
                .and_then(|a| a.default.as_ref());
            return match default {
                Some(Literal::Str(v)) => (
                    static_target(occasion, v),
                    TargetOrigin::Default,
                    None,
                    None,
                ),
                _ => (RaiseTarget::Unbound, TargetOrigin::Default, None, None),
            };
        };
        let shown = Some(match &written.value {
            AttrValue::Str(v) => format!("{attr}=\"{v}\""),
            AttrValue::Ref(slot) => format!("{attr}={}", slot.raw.trim()),
            AttrValue::BoolTrue => attr.clone(),
        });
        let mut value = written.value.clone();
        if let Some(w) = within {
            let mut one = [Attr {
                value,
                ..written.clone()
            }];
            bind_attrs(&mut one, w.args, w.params);
            let [bound] = one;
            value = bound.value;
        }
        let target = match &value {
            AttrValue::Str(v) if v == OCCASION_TARGET => RaiseTarget::Dynamic,
            AttrValue::Str(v) => static_target(occasion, v),
            AttrValue::Ref(slot) => {
                let folded = self.folded;
                let defs = DefTable {
                    bodies: &folded.def_bodies,
                    params: &folded.env.def_params,
                };
                crate::def_inline::constant_attr(&slot.raw, &defs, &folded.env.state)
                    .map_or(RaiseTarget::Unbound, |v| static_target(occasion, &v))
            }
            AttrValue::BoolTrue => RaiseTarget::Unbound,
        };
        (
            target,
            TargetOrigin::Authored,
            Some(written.value_span),
            shown,
        )
    }

    /// Each reward of `quest` (its own and its top-level objectives'), once,
    /// whose kind declares raises — the grant points 0.50.0 §5.4 admits (an
    /// `outcome=` other than `failed`, or any on an objective reward, grants
    /// nothing).
    fn rewards(&mut self, quest: &Quest, source: Option<&NodeId>, item: &ProjectDoc<'d>) {
        let objective_rewards = quest
            .body
            .iter()
            .filter_map(|n| match n {
                Node::Objective(o) => Some(o),
                _ => None,
            })
            .flat_map(|o| o.rewards.iter().filter(|r| r.outcome.is_none()));
        let quest_rewards = quest
            .rewards
            .iter()
            .filter(|r| matches!(r.outcome.as_deref(), None | Some("failed")));
        for reward in objective_rewards.chain(quest_rewards) {
            self.reward(reward, source, item);
        }
    }

    fn reward(&mut self, reward: &Reward, source: Option<&NodeId>, item: &ProjectDoc<'d>) {
        let Some(effects) = crate::reward_effects::kind_effects(item.reward_kinds, reward) else {
            return;
        };
        let folded: &'d FoldedEnv = self.folded;
        for raise in &effects.raises {
            let Some(occasion) = folded.occasions.get(&raise.occasion) else {
                continue;
            };
            let (target, origin, span) = match &raise.target {
                None => (
                    RaiseTarget::Untargeted,
                    TargetOrigin::Literal,
                    reward.kind_span,
                ),
                Some(RaiseTargetDecl::Literal(t)) => (
                    RaiseTarget::Static(t.clone()),
                    TargetOrigin::Literal,
                    reward.kind_span,
                ),
                Some(RaiseTargetDecl::Attr(_)) => match &reward.target {
                    Some(t) => (
                        static_target(occasion, t),
                        TargetOrigin::Authored,
                        reward.target_span.unwrap_or(reward.kind_span),
                    ),
                    None => (
                        RaiseTarget::Unbound,
                        TargetOrigin::Authored,
                        reward.kind_span,
                    ),
                },
            };
            self.push(RaiseSite {
                path: self.path,
                doc: self.doc,
                source: source.cloned(),
                span,
                occasion: raise.occasion.clone(),
                target,
                origin,
                via: RaiseVia::Reward(reward.kind.trim().to_string()),
                shown: crate::reward_effects::reward_text(reward),
            });
        }
    }

    fn push(&mut self, site: RaiseSite<'d>) {
        let key = (
            site.span.byte_start,
            site.span.byte_end,
            site.occasion.clone(),
            site.target.clone(),
            site.via.clone(),
        );
        if self.seen.insert(key) {
            self.sites.push(site);
        }
    }
}

/// A resolved value as the target the occasion is raised for: a domain
/// occasion's prefix applied (`chapter2` → `scene.chapter2`), the value
/// itself for a shape-only target.
fn static_target(occasion: &OccasionDecl, value: &str) -> RaiseTarget {
    RaiseTarget::Static(match &occasion.target {
        OccasionTarget::Domain { prefix, .. } => format!("{prefix}.{value}"),
        OccasionTarget::Shape(_) => value.to_string(),
    })
}

/// [`E_RAISES_TARGET`] over one resolved project root (`docs` parallel to
/// `foldeds`, whole-project `check`): each static target of a declared raise
/// (a dynamic `occasion.target` is the runtime's, an unbound one raises
/// nothing) must lie in the occasion's domain — its `members:` when listed,
/// else its entity kind (a value from an attribute or a reward's `target=`
/// outside the kind is already refused there) — and be answered by some
/// scene or beat ([`RaiseTable::reaches`]). One diagnostic per site.
pub fn check_project_raise_targets(
    docs: &[ProjectDoc<'_>],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let table = RaiseTable::build(docs, foldeds);
    if table.sites.is_empty() {
        return Vec::new();
    }
    let beats = project_beats(docs, foldeds);
    let mut out = Vec::new();
    for (site, folded) in table
        .sites
        .iter()
        .filter_map(|s| Some((s, *foldeds.get(s.doc)?)))
    {
        let RaiseTarget::Static(target) = &site.target else {
            continue;
        };
        let Some(occasion) = folded.occasions.get(&site.occasion) else {
            continue;
        };
        if let Some(message) = outside_domain(site, target, occasion, folded) {
            out.push((site.path.to_path_buf(), diag(message, site.span)));
            continue;
        }
        if beats.iter().any(|b| RaiseTable::reaches(site, b)) {
            continue;
        }
        let answered: BTreeSet<&str> = beats
            .iter()
            .filter(|b| b.on == site.occasion)
            .flat_map(|b| {
                b.kind_targets
                    .iter()
                    .flatten()
                    .map(String::as_str)
                    .chain(b.target.filter(|_| b.kind_targets.is_none()))
            })
            .collect();
        let hint = lute_manifest::suggest::nearest(target, answered.iter().copied(), 2)
            .map(|s| format!(" — did you mean `{s}`?"))
            .unwrap_or_default();
        out.push((
            site.path.to_path_buf(),
            diag(
                format!(
                    "{} raises `{}` for `{target}`{}, but no scene or beat answers it{hint}",
                    site.shown,
                    site.occasion,
                    default_note(site)
                ),
                site.span,
            ),
        ));
    }
    out
}

/// ` (its declared default)` for a target taken from the attribute's
/// `default:`.
fn default_note(site: &RaiseSite<'_>) -> &'static str {
    match site.origin {
        TargetOrigin::Default => " (the attribute's declared `default:`)",
        TargetOrigin::Literal | TargetOrigin::Authored => "",
    }
}

/// Why `target` lies outside `occasion`'s domain, `None` when it does not
/// (or the domain is an `open:` kind's, whose members the engine populates).
fn outside_domain(
    site: &RaiseSite<'_>,
    target: &str,
    occasion: &OccasionDecl,
    folded: &FoldedEnv,
) -> Option<String> {
    let OccasionTarget::Domain {
        prefix,
        entity,
        members,
    } = &occasion.target
    else {
        return None;
    };
    let member = target.strip_prefix(prefix.as_str())?.strip_prefix('.')?;
    let kind: Option<&[String]> = match folded.env.rel_vocab.kinds.get(entity).map(|k| &k.shape) {
        Some(KindShape::Members(ms)) => Some(ms),
        _ => None,
    };
    let in_kind = kind.is_none_or(|ms| ms.iter().any(|m| m == member));
    // A value outside the kind from an attribute or `target=` is the entity
    // check's (`E-ATTR-TYPE` / `E-REWARD-TARGET`) at that value.
    if !in_kind && site.origin == TargetOrigin::Authored {
        return None;
    }
    let legal = occasion.target.domain_members(kind)?;
    if legal.iter().any(|m| m == member) {
        return None;
    }
    let near = lute_manifest::suggest::nearest(member, legal.iter().map(String::as_str), 2)
        .map(|s| format!(" — did you mean `{prefix}.{s}`?"))
        .unwrap_or_default();
    let why = if members.is_some() && in_kind {
        format!(
            "`{}` is raised only for {} (its `members:`)",
            site.occasion,
            legal
                .iter()
                .map(|m| format!("`{prefix}.{m}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        format!("`{member}` is no `{entity}` member")
    };
    Some(format!(
        "{} raises `{}` for `{target}`{}, but {why}{near}",
        site.shown,
        site.occasion,
        default_note(site)
    ))
}

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_RAISES_TARGET.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
