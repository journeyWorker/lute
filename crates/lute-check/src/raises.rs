//! 0.51.0 §4.3–§4.6 (#170): declared occasion raises — the project raise
//! table, the per-call target check, the graph's raise anchors, the
//! never-raised warning and the artifact's resolved raises.
//!
//! A directive's or reward kind's `effects.raises` (0.51.0 §4.1) promises
//! that the host raises an occasion after the call's presentation ends, or
//! after a grant settles. [`RaiseTable::build`] resolves every declared raise
//! of one project root once per pass (D14), like 0.50's
//! [`crate::reward_effects::GrantTable`]: each document's calls against its
//! own capability snapshot and each reward against its own
//! `rewardKinds` of [`crate::ProjectDoc::snapshot`]. The sources are `accept_sources`'
//! kinds — a scene's shots, a lore bundle beat, a quest's body and each of
//! its rewards once ([`crate::reward_effects::reward_grant_sites`]) — plus
//! a lore entry's body (a raise that anchors nothing); a call inside a
//! component body counts at each `::use`, in the body the expansion builds
//! (arguments bound, param-scoped `<match>`es folded), stopping at a `::use`
//! cycle; a component file itself is no host. No synthetic node is created.
//!
//! A target resolves by one rule (§4.3): the call's authored attribute value
//! (an `@def` folds to its constant), else the attribute's declared
//! `default:`, else it is unbound and the raise does not happen. A value
//! reading `occasion.target` is the one dynamic target; a reward binds its
//! quoted `target=`. The compiler records the same resolution
//! ([`call_raises`], [`reward_raises`]). [`RaiseTable::reaches`] is the one
//! answerer rule every consumer shares: [`check_project_raise_targets`]
//! ([`E_RAISES_TARGET`]), the graph's raise anchors ([`RaiseTable::anchors`]),
//! [`check_project_never_raised`] ([`W_OCCASION_NEVER_RAISED`]) and the
//! clock analyses, for which a reached answerer may be raised at any
//! position (§4.4). A declared raise is a may-edge: it adds no fact, write
//! or Must claim anywhere (§4.5 "Soundness").

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::KindShape;
use lute_manifest::schema::{
    DirectiveDecl, OccasionDecl, OccasionTarget, RaiseDecl, RaiseTargetDecl, RaisedBy,
    RewardEffects,
};
use lute_manifest::semantics::beats::OCCASION_TARGET;
use lute_manifest::types::Literal;
use lute_syntax::ast::{Arm, Attr, AttrValue, ClipNode, Directive, Node, Quest, Reward};

use crate::beats::{project_beats, ProjectBeat, ProjectBeatKind};
use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::clock_positions::RaiseRule;
use crate::component_effects::{bind_component_body, fold_component_matches, use_args_for};
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
                .snapshot
                .reward_kinds
                .values()
                .any(|k| k.effects.as_ref().is_some_and(|e| !e.raises.is_empty()));
            // A component file is no host: its calls count at each `::use`
            // (as its writes do, `component_effects`).
            let component = crate::meta::infer_meta_kind_from_shape(&item.doc.meta, true)
                == Some(crate::meta::MetaKind::Component);
            if component || (!directive_raises && !reward_raises) {
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

    /// Whether some declared raise reaches `beat` ([`Self::reaches`]) — the
    /// clock analyses' "raised at every position" (§4.4).
    pub fn reaches_any(&self, beat: &ProjectBeat<'_>) -> bool {
        self.sites.iter().any(|s| Self::reaches(s, beat))
    }

    /// The raise anchors of the root this table was built from (`docs`
    /// parallel to `foldeds`; 0.51.0 §4.5 "Anchors"): for each scene or
    /// bundle beat answering an occasion only declared raises raise
    /// ([`declared_only`]), the source node of every raise reaching it, in
    /// table order. A lore entry's raise has no source and anchors nothing;
    /// [`crate::connectivity::assemble_graph`] keeps the sources that are
    /// graph nodes and leaves a node that declares `after:` as it is.
    pub fn anchors(&self, docs: &[ProjectDoc<'_>], foldeds: &[&FoldedEnv]) -> RaiseAnchors {
        let mut by_node: BTreeMap<NodeId, Vec<RaiseFrom>> = BTreeMap::new();
        if self.sites.is_empty() {
            return RaiseAnchors { by_node };
        }
        for beat in &project_beats(docs, foldeds) {
            let node = match beat.kind {
                ProjectBeatKind::Scene => NodeId::Scene(beat.id.clone()),
                ProjectBeatKind::Bundle => NodeId::Beat(beat.id.clone()),
                ProjectBeatKind::Entry => continue,
            };
            if !declared_only(beat.folded, beat.on) {
                continue;
            }
            for site in self.sites.iter().filter(|s| Self::reaches(s, beat)) {
                let Some(source) = &site.source else {
                    continue;
                };
                let from = by_node.entry(node.clone()).or_default();
                if !from
                    .iter()
                    .any(|f| &f.source == source && f.shown == site.shown)
                {
                    from.push(RaiseFrom {
                        source: source.clone(),
                        shown: site.shown.clone(),
                    });
                }
            }
        }
        RaiseAnchors { by_node }
    }
}

/// Whether only declared raises raise `occasion` (0.51.0 §4.4): it is
/// declared `raisedBy: declared` and the project clock's `raise:` map does
/// not raise it ([`RaiseRule::of`]) — a clock-raised occasion is raised
/// outside any declared raise, so it draws no anchor and no
/// [`W_OCCASION_NEVER_RAISED`].
fn declared_only(folded: &FoldedEnv, occasion: &str) -> bool {
    folded
        .occasions
        .get(occasion)
        .is_some_and(|o| o.raised_by == RaisedBy::Declared)
        && folded
            .env
            .state
            .clock()
            .is_none_or(|clock| RaiseRule::of(clock, occasion).is_none())
}

/// One declared raise that reaches an answerer node: the node whose run
/// makes it and the call or reward as messages show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaiseFrom {
    pub source: NodeId,
    pub shown: String,
}

/// Every raise anchor of one project root ([`RaiseTable::anchors`]), by
/// answerer node. Empty for a root without declared raises.
#[derive(Clone, Debug, Default)]
pub struct RaiseAnchors {
    by_node: BTreeMap<NodeId, Vec<RaiseFrom>>,
}

impl RaiseAnchors {
    /// The anchors of one root, its raise table built for them alone (a
    /// caller that runs no other raise pass).
    pub fn of(docs: &[ProjectDoc<'_>], foldeds: &[&FoldedEnv]) -> Self {
        RaiseTable::build(docs, foldeds).anchors(docs, foldeds)
    }

    /// Each anchored answerer with the raises that reach it.
    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &[RaiseFrom])> {
        self.by_node.iter().map(|(n, from)| (n, from.as_slice()))
    }

    /// The raises that reach `node`, empty when none does.
    pub fn of_node(&self, node: &NodeId) -> &[RaiseFrom] {
        self.by_node.get(node).map_or(&[], Vec::as_slice)
    }
}

/// A document answers an occasion only declared raises raise, and no
/// declared raise reaches it (0.51.0 §4.5, whole-project `check`).
pub const W_OCCASION_NEVER_RAISED: &str = "W-OCCASION-NEVER-RAISED";

/// [`W_OCCASION_NEVER_RAISED`] over one resolved project root (`docs`
/// parallel to `foldeds`, `table` built from them): every scene, entry or
/// bundle beat answering a `raisedBy: declared` occasion the clock does not
/// raise ([`declared_only`]) that no site of `table` reaches
/// ([`RaiseTable::reaches`]), at its `on`. A `lute play`/`lute test` step
/// is no raise: a test proves a test, not the game.
pub fn check_project_never_raised(
    docs: &[ProjectDoc<'_>],
    foldeds: &[&FoldedEnv],
    table: &RaiseTable<'_>,
) -> Vec<(PathBuf, Diagnostic)> {
    let declared = foldeds.iter().any(|f| {
        f.occasions
            .values()
            .any(|o| o.raised_by == RaisedBy::Declared)
    });
    if !declared {
        return Vec::new();
    }
    let mut out = Vec::new();
    for beat in project_beats(docs, foldeds) {
        if !declared_only(beat.folded, beat.on) || table.reaches_any(&beat) {
            continue;
        }
        let target = beat
            .target
            .map(|t| format!(" for `{t}`"))
            .unwrap_or_default();
        let declarer = beat
            .folded
            .snapshot
            .occasion_owners
            .get(beat.on)
            .map_or_else(|| "its plugin".to_string(), |p| format!("`{p}`"));
        out.push((
            beat.path.to_path_buf(),
            crate::reachability::diag(
                W_OCCASION_NEVER_RAISED,
                Severity::Warning,
                format!(
                    "{} answers `{}`{target}, which {declarer} declares `raisedBy: declared`, but \
                     no `effects.raises` in the project raises it — add a `raises` entry to the \
                     directive or reward kind that leads here, or drop `raisedBy: declared` if \
                     the host raises `{}` on its own",
                    beat.name(),
                    beat.on,
                    beat.on
                ),
                beat.anchor,
            ),
        ));
    }
    out
}

/// The walk of one document's raise sources.
struct Walk<'w, 'd> {
    path: &'d Path,
    doc: usize,
    folded: &'d FoldedEnv,
    sites: &'w mut Vec<RaiseSite<'d>>,
    /// `(document, span, occasion, target, via)` already listed: an effects
    /// component's
    /// state-writing call is spliced into its host at the `::use`
    /// (`component_effects`), and the `::use` walk finds it again.
    seen: &'w mut BTreeSet<(usize, usize, usize, String, RaiseTarget, RaiseVia)>,
}

/// A call inside a (bound, folded) component body: the host `::use` it
/// counts at and the component whose body holds it.
struct InComponent<'a> {
    /// The outermost host `::use`: where the call is reported.
    at: Span,
    /// The component whose body holds the call.
    name: &'a str,
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

    /// A `::use`: its component's body as the expansion builds it — bound
    /// to the use's arguments ([`bind_component_body`]) and with every
    /// param-scoped `<match>` the arguments decide folded
    /// ([`fold_component_matches`]), so a raise only an unselected arm makes
    /// is not counted; a nested `::use` is walked from that bound body. A
    /// `::use` cycle stops the walk (`E-COMPONENT-CYCLE`'s).
    fn use_site(
        &mut self,
        d: &Directive,
        source: Option<&NodeId>,
        within: Option<&InComponent<'_>>,
        stack: &mut Vec<String>,
    ) {
        let folded: &'d FoldedEnv = self.folded;
        let components = &folded.components;
        let Some((name, def)) = d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
            ("component", AttrValue::Str(s)) => components.table.get_key_value(s),
            _ => None,
        }) else {
            return;
        };
        if stack.contains(name) {
            return;
        }
        let args = use_args_for(d, def);
        let mut body: Vec<Node> = def
            .body
            .sections
            .iter()
            .flat_map(|s| s.body.iter().cloned())
            .collect();
        bind_component_body(&mut body, &args, &def.params, &mut |_| {});
        fold_component_matches(&mut body, &folded.env.state);
        let inner = InComponent {
            at: within.map_or(d.span, |w| w.at),
            name,
        };
        stack.push(name.clone());
        self.nodes(&body, source, Some(&inner), stack);
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
            let defs = DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            };
            let fold_ref =
                |raw: &str| crate::def_inline::constant_attr(raw, &defs, &folded.env.state);
            let (target, origin, value_span, written) =
                directive_target(raise, occasion, decl, &d.attrs, &fold_ref);
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

    /// Each reward of `quest` once whose kind declares raises, at the grant
    /// points 0.50.0 §5.4 admits ([`crate::reward_effects::reward_grant_sites`]).
    fn rewards(&mut self, quest: &Quest, source: Option<&NodeId>, item: &ProjectDoc<'d>) {
        for site in crate::reward_effects::reward_grant_sites(quest, &item.snapshot.reward_kinds) {
            self.reward(site.reward, site.effects, source);
        }
    }

    fn reward(&mut self, reward: &Reward, effects: &RewardEffects, source: Option<&NodeId>) {
        let folded: &'d FoldedEnv = self.folded;
        for raise in &effects.raises {
            let Some(occasion) = folded.occasions.get(&raise.occasion) else {
                continue;
            };
            let (target, origin) = reward_target(raise, occasion, reward.target.as_deref());
            let span = match origin {
                TargetOrigin::Authored => reward.target_span.unwrap_or(reward.kind_span),
                TargetOrigin::Literal | TargetOrigin::Default => reward.kind_span,
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
            site.doc,
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

/// A directive raise's target (§4.3), with where its value sits and the
/// attribute as the call writes it (`scene="chapter2"`): the call's
/// authored value, else the attribute's declared `default:`, else unbound.
/// `fold_ref` folds an `@def` value to its constant (the checker's
/// `def_inline::constant_attr`; the compiler's values arrive folded). A call
/// in a component body arrives already bound to its `::use`'s arguments.
fn directive_target(
    raise: &RaiseDecl,
    occasion: &OccasionDecl,
    decl: &DirectiveDecl,
    attrs: &[Attr],
    fold_ref: &dyn Fn(&str) -> Option<String>,
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
        // Review051b F6: any declared `default:` is the value, spelled as
        // an authored one would be (`7`, `true`, `chapter2`); only no
        // default leaves the target unbound.
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
            Some(other) => (
                static_target(occasion, &lute_manifest::types::lit_str(other)),
                TargetOrigin::Default,
                None,
                None,
            ),
            None => (RaiseTarget::Unbound, TargetOrigin::Default, None, None),
        };
    };
    let shown = Some(match &written.value {
        AttrValue::Str(v) => format!("{attr}=\"{v}\""),
        AttrValue::Ref(slot) => format!("{attr}={}", slot.raw.trim()),
        AttrValue::BoolTrue => attr.clone(),
    });
    let target = match &written.value {
        AttrValue::Str(v) if v == OCCASION_TARGET => RaiseTarget::Dynamic,
        AttrValue::Str(v) => static_target(occasion, v),
        AttrValue::Ref(slot) => {
            fold_ref(&slot.raw).map_or(RaiseTarget::Unbound, |v| static_target(occasion, &v))
        }
        AttrValue::BoolTrue => static_target(occasion, "true"),
    };
    (
        target,
        TargetOrigin::Authored,
        Some(written.value_span),
        shown,
    )
}

/// A reward raise's target: its literal, or the reward's quoted `target=`
/// for `@target` (unbound without one — the 0.49.0 §3.3 rule `accepts`
/// follows).
fn reward_target(
    raise: &RaiseDecl,
    occasion: &OccasionDecl,
    target: Option<&str>,
) -> (RaiseTarget, TargetOrigin) {
    match (&raise.target, target) {
        (None, _) => (RaiseTarget::Untargeted, TargetOrigin::Literal),
        (Some(RaiseTargetDecl::Literal(t)), _) => {
            (RaiseTarget::Static(t.clone()), TargetOrigin::Literal)
        }
        (Some(RaiseTargetDecl::Attr(_)), Some(t)) => {
            (static_target(occasion, t), TargetOrigin::Authored)
        }
        (Some(RaiseTargetDecl::Attr(_)), None) => (RaiseTarget::Unbound, TargetOrigin::Authored),
    }
}

/// One raise as the artifact records it (0.51.0 §4.6): the occasion and the
/// target it is raised for, `None` for an untargeted occasion. A dynamic
/// target keeps the `occasion.target` reference with the occasion's domain
/// prefix applied, as any value gets it (`scene.occasion.target`), and says
/// so (`dynamic`): the runtime binds only a dynamic target to the member the
/// beat runs for, never a static one that happens to read the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireRaise {
    pub occasion: String,
    pub target: Option<String>,
    pub dynamic: bool,
}

impl WireRaise {
    /// `None` for an unbound target: the raise does not happen.
    fn of(name: &str, occasion: &OccasionDecl, target: RaiseTarget) -> Option<Self> {
        let (target, dynamic) = match target {
            RaiseTarget::Untargeted => (None, false),
            RaiseTarget::Static(t) => (Some(t), false),
            RaiseTarget::Dynamic => (Some(prefixed(occasion, OCCASION_TARGET)), true),
            RaiseTarget::Unbound => return None,
        };
        Some(WireRaise {
            occasion: name.to_string(),
            target,
            dynamic,
        })
    }
}

/// The raises one directive call records, in list order — `decl`'s
/// `effects.raises` on an occasion of `occasions`, each target resolved as
/// [`RaiseTable::build`] resolves it (authored, then `default:`), an unbound
/// one left out. `attrs` are the call's, every `@def` already folded.
pub fn call_raises(
    decl: &DirectiveDecl,
    occasions: &BTreeMap<String, OccasionDecl>,
    attrs: &[Attr],
) -> Vec<WireRaise> {
    let Some(effects) = decl.effects.as_ref() else {
        return Vec::new();
    };
    effects
        .raises
        .iter()
        .filter_map(|raise| {
            let occasion = occasions.get(&raise.occasion)?;
            let (target, ..) = directive_target(raise, occasion, decl, attrs, &|_| None);
            WireRaise::of(&raise.occasion, occasion, target)
        })
        .collect()
}

/// The raises one granted reward records, in list order: `effects.raises`
/// on an occasion of `occasions`, `@target` bound to the reward's quoted
/// `target=` (left out without one).
pub fn reward_raises(
    effects: &RewardEffects,
    occasions: &BTreeMap<String, OccasionDecl>,
    target: Option<&str>,
) -> Vec<WireRaise> {
    effects
        .raises
        .iter()
        .filter_map(|raise| {
            let occasion = occasions.get(&raise.occasion)?;
            WireRaise::of(
                &raise.occasion,
                occasion,
                reward_target(raise, occasion, target).0,
            )
        })
        .collect()
}

/// A resolved value as the target the occasion is raised for: a domain
/// occasion's prefix applied (`chapter2` → `scene.chapter2`), the value
/// itself for a shape-only target.
fn static_target(occasion: &OccasionDecl, value: &str) -> RaiseTarget {
    RaiseTarget::Static(prefixed(occasion, value))
}

/// `value` with a domain occasion's prefix applied.
fn prefixed(occasion: &OccasionDecl, value: &str) -> String {
    match &occasion.target {
        OccasionTarget::Domain { prefix, .. } => format!("{prefix}.{value}"),
        OccasionTarget::Shape(_) => value.to_string(),
    }
}

/// [`E_RAISES_TARGET`] over one resolved project root (`docs` parallel to
/// `foldeds`, `table` built from them; whole-project `check`): each static
/// target of a declared raise (a dynamic `occasion.target` is the
/// runtime's, an unbound one raises nothing) must lie in the occasion's
/// domain — its `members:` when listed, else its entity kind (a value from
/// an attribute or a reward's `target=` outside the kind is already refused
/// there) — and be answered by some scene or beat ([`RaiseTable::reaches`]).
/// One diagnostic per site.
pub fn check_project_raise_targets(
    docs: &[ProjectDoc<'_>],
    foldeds: &[&FoldedEnv],
    table: &RaiseTable<'_>,
) -> Vec<(PathBuf, Diagnostic)> {
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
