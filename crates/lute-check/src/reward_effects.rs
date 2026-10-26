//! 0.50.0 §5 (#169): reward effects — the facts a granted `<reward>`
//! retracts and asserts and the quests it accepts, declared by its kind
//! (`rewardKinds.<K>.effects`, 0.50.0 §5.1).
//!
//! Every analysis reads a reward effect as if it were the corresponding
//! `::retract` / `::assert` / `::accept` written first in the owner's grant
//! point (§5.4): a quest's completion (before its `questComplete`
//! handlers), its failure (`outcome="failed"`, before `questFailed`), or an
//! objective's completion. [`reward_grant_sites`] lists one quest's granted
//! rewards once each, at their grant points (0.51.0 §4.5: a raises-only
//! reward included); [`quest_grants`], built on it, resolves them into
//! [`GrantEffect`]s in grant order; [`GrantTable`] holds a whole root's,
//! built once per pass from the root's [`ProjectDoc`]s — each document's
//! rewards resolve against the reward vocabulary it was checked against
//! (the `rewardKinds` of [`ProjectDoc::snapshot`]). A reward's `when=` is carried as the
//! effect's guard, so no consumer re-finds the `<reward>`. No synthetic AST
//! node is created.
//!
//! An effect reading `@target` binds the reward's quoted `target=` (0.49.0
//! §3.3: the only form that reaches the IR); one reading another `@<attr>`
//! binds the reward's quoted value of the kind's attribute, else its
//! declared `default:` (0.52.0 §7.3, [`reward_attr_value`]). A reward
//! leaving the attribute unbound does not apply that effect, the directive
//! rule for an unbound `@attr`.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use lute_core_span::{Diagnostic, Span};
use lute_manifest::fact::{FactArg, FactPattern, FactTerm};
use lute_manifest::schema::{
    AcceptEffect, FactEffect, FactEffectArg, RewardEffects, RewardKindDecl, REWARD_TARGET_ATTR,
};
use lute_manifest::snapshot::Domain;
use lute_manifest::types::Literal;
use lute_syntax::ast::{Attr, AttrValue, CelSlot, Node, Objective, Quest, Reward};

use crate::directive_facts::CallFacts;
use crate::{Ctx, ProjectDoc};

/// A reward vocabulary by kind id (a capability snapshot's `rewardKinds`).
pub type RewardKinds = BTreeMap<String, RewardKindDecl>;

/// Where a reward grants (0.50.0 §5.4 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GrantPoint {
    /// A quest-level reward without `outcome`: the quest's completion.
    QuestComplete,
    /// A quest-level `outcome="failed"` reward: the quest's failure.
    QuestFailed,
    /// An objective-level reward: the objective's completion.
    Objective,
}

impl GrantPoint {
    /// The lifecycle event whose handlers run after this point's grants:
    /// `questComplete` / `questFailed`, none for an objective.
    pub fn handler_event(self) -> Option<&'static str> {
        match self {
            GrantPoint::QuestComplete => Some("questComplete"),
            GrantPoint::QuestFailed => Some("questFailed"),
            GrantPoint::Objective => None,
        }
    }
}

/// One effect's place in the engine's grant order (0.50.0 §5.3): objective
/// grants (`rank` 0, by objective) before quest grants (`rank` 1); within an
/// owner by reward; within a reward retracts, then asserts, then accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GrantOrder {
    pub rank: u8,
    pub owner: usize,
    pub reward: usize,
    pub effect: usize,
}

/// What one reward effect does.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// A fact the grant asserts (`assert`) or retracts, ground once bound
    /// (a retract may carry `_`).
    Fact { pattern: FactPattern, assert: bool },
    /// A quest the grant accepts; `from_target` when the id is the reward's
    /// `target=` (`@target`), else a literal of the kind's `effects.accepts`.
    Accept { quest: String, from_target: bool },
}

/// One resolved reward effect (0.50.0 §5.4 "Mechanism").
#[derive(Clone, Debug)]
pub struct GrantEffect<'d> {
    pub quest: &'d Quest,
    pub objective: Option<&'d Objective>,
    pub point: GrantPoint,
    pub order: GrantOrder,
    pub reward: &'d Reward,
    pub effect: Effect,
}

impl GrantEffect<'_> {
    /// The reward's kind id.
    pub fn kind(&self) -> &str {
        self.reward.kind.trim()
    }

    /// The reward's `when=`: the effect applies only when it holds.
    pub fn guard(&self) -> Option<&CelSlot> {
        self.reward.when.as_ref()
    }

    /// The fact the effect writes, with `true` for an assert.
    pub fn fact(&self) -> Option<(&FactPattern, bool)> {
        match &self.effect {
            Effect::Fact { pattern, assert } => Some((pattern, *assert)),
            Effect::Accept { .. } => None,
        }
    }

    /// The quest the effect accepts.
    pub fn accepted(&self) -> Option<&str> {
        match &self.effect {
            Effect::Accept { quest, .. } => Some(quest),
            Effect::Fact { .. } => None,
        }
    }

    /// Where a diagnostic about this effect anchors: the reward's `target=`
    /// value when the effect reads it, else its `kind=`.
    pub fn anchor(&self) -> Span {
        let reads_target = match &self.effect {
            Effect::Accept { from_target, .. } => *from_target,
            Effect::Fact { .. } => false,
        };
        match self.reward.target_span {
            Some(span) if reads_target => span,
            _ => self.reward.kind_span,
        }
    }

    /// `` `<reward kind="BADGE">` `` — how a message names the reward.
    pub fn reward_text(&self) -> String {
        reward_text(self.reward)
    }
}

/// `` `<reward kind="BADGE">` `` (with `target="…"` when written).
pub fn reward_text(reward: &Reward) -> String {
    match &reward.target {
        Some(target) => format!(
            "`<reward kind=\"{}\" target=\"{target}\">`",
            reward.kind.trim()
        ),
        None => format!("`<reward kind=\"{}\">`", reward.kind.trim()),
    }
}

/// `reward`'s kind in `kinds`, when it declares `effects`, with them.
pub fn kind_effects<'k>(
    kinds: &'k RewardKinds,
    reward: &Reward,
) -> Option<(&'k RewardKindDecl, &'k RewardEffects)> {
    let kind = kinds.get(reward.kind.trim())?;
    Some((kind, kind.effects.as_ref()?))
}

/// What a reward binds an effect's `@<name>` to (0.52.0 §7.3).
#[derive(Clone, Copy, Debug)]
pub enum RewardAttrValue<'a> {
    /// `@target`: the reward's quoted `target=`.
    Target(&'a str),
    /// A kind attribute the reward writes: a quoted value or a bare flag.
    Authored(&'a Attr),
    /// The declared `default:` of a kind attribute the reward omits.
    Default(&'a Literal),
}

impl RewardAttrValue<'_> {
    /// The value as written: `gold`, `true` for a bare flag, a default as its
    /// YAML literal reads.
    pub fn text(&self) -> Cow<'_, str> {
        match self {
            RewardAttrValue::Target(t) => Cow::Borrowed(t),
            RewardAttrValue::Authored(a) => match &a.value {
                AttrValue::Str(s) => Cow::Borrowed(s),
                AttrValue::BoolTrue => Cow::Borrowed("true"),
                // Never bound ([`reward_attr_value`]); the reference as
                // written.
                AttrValue::Ref(slot) => Cow::Borrowed(slot.raw.trim()),
            },
            RewardAttrValue::Default(Literal::Str(s)) => Cow::Borrowed(s),
            RewardAttrValue::Default(l) => Cow::Owned(lute_manifest::types::lit_str(l)),
        }
    }

    /// The value as a fact argument: `@target` names a member as written
    /// (0.50.0 §5.1); any other value reads as a fact constant does
    /// (`true`/`false` a bool, else a member).
    fn term(&self) -> FactTerm {
        match self {
            RewardAttrValue::Target(t) => FactTerm::Ident((*t).to_string()),
            _ => crate::directive_facts::const_term(&self.text()),
        }
    }
}

/// The one binding rule of a reward's attributes (0.52.0 §7.3), shared by
/// the checker's facts ([`reward_facts`]), the raise table
/// ([`crate::raises::RaiseTable`], [`crate::raises::reward_raises`]) and
/// lowering (`RewardEntry.attrs`): `@target` is the reward's quoted
/// `target=`; a kind attribute is the reward's quoted value (a bare flag
/// `true`), else the attribute's `default:`. `None` — unbound — without
/// either, for an `@ref` value (`E-REWARD-ATTR`: a reward's attributes are
/// static) and for a name the kind does not declare.
pub fn reward_attr_value<'a>(
    kind: &'a RewardKindDecl,
    reward: &'a Reward,
    name: &str,
) -> Option<RewardAttrValue<'a>> {
    if name == REWARD_TARGET_ATTR {
        return reward.target.as_deref().map(RewardAttrValue::Target);
    }
    let decl = kind.attrs.iter().find(|a| a.name == name)?;
    match reward.attrs.iter().find(|a| a.key == name) {
        Some(a) if matches!(a.value, AttrValue::Ref(_)) => None,
        Some(a) => Some(RewardAttrValue::Authored(a)),
        None => decl.default.as_ref().map(RewardAttrValue::Default),
    }
}

/// The facts one reward of `kind` writes, retracts first — `call_facts`'s
/// twin ([`crate::directive_facts::call_facts`]). A pattern reading an
/// attribute the reward leaves unbound ([`reward_attr_value`]) is not
/// applied, so not listed.
pub fn reward_facts(kind: &RewardKindDecl, reward: &Reward) -> CallFacts {
    let Some(effects) = &kind.effects else {
        return CallFacts::default();
    };
    let resolve =
        |facts: &[FactEffect]| facts.iter().filter_map(|f| bind(f, kind, reward)).collect();
    CallFacts {
        retracts: resolve(&effects.retracts),
        asserts: resolve(&effects.asserts),
    }
}

/// The quests one reward accepts, each with whether it is the reward's
/// `target=`; an `@target` accept of a reward without one is not applied.
pub fn reward_accepts(effects: &RewardEffects, reward: &Reward) -> Vec<(String, bool)> {
    effects
        .accepts
        .iter()
        .filter_map(|a| match a {
            AcceptEffect::Target => reward.target.as_ref().map(|t| (t.clone(), true)),
            AcceptEffect::Quest(id) => Some((id.clone(), false)),
        })
        .collect()
}

/// One declared pattern with each `@attr` bound; `None` when one is
/// unbound.
fn bind(fact: &FactEffect, kind: &RewardKindDecl, reward: &Reward) -> Option<FactPattern> {
    let args = fact
        .args
        .iter()
        .map(|a| {
            let term = match a {
                FactEffectArg::Const(c) => crate::directive_facts::const_term(c),
                FactEffectArg::Wildcard => FactTerm::Wildcard,
                FactEffectArg::Attr(name) => reward_attr_value(kind, reward, name)?.term(),
            };
            Some(FactArg { term, span: (0, 0) })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(FactPattern {
        relation: fact.relation.clone(),
        relation_span: (0, 0),
        args,
        span: (0, 0),
    })
}

/// One granted `<reward>` whose kind declares `effects` (0.51.0 §4.5,
/// Review051b F2): where it grants and its place in grant order
/// (`order.effect` is 0 — the reward's own effects are numbered by the
/// consumer). A reward is one site however many effects its kind declares,
/// `raises` included.
#[derive(Clone, Copy, Debug)]
pub struct RewardGrantSite<'d, 'k> {
    pub quest: &'d Quest,
    pub objective: Option<&'d Objective>,
    pub point: GrantPoint,
    pub order: GrantOrder,
    pub reward: &'d Reward,
    pub kind: &'k RewardKindDecl,
    pub effects: &'k RewardEffects,
}

/// Each `<reward>` of `quest` whose kind in `kinds` (its document's
/// vocabulary, the `rewardKinds` of [`ProjectDoc::snapshot`]) declares `effects`, once, in
/// grant order: its top-level objectives' rewards (granted at the
/// objective's completion; one with any `outcome=` grants nothing), then
/// its own — without `outcome` at its completion, `outcome="failed"` at its
/// failure, any other `outcome=` (`E-REWARD-ATTR`) never.
pub fn reward_grant_sites<'d, 'k>(
    quest: &'d Quest,
    kinds: &'k RewardKinds,
) -> Vec<RewardGrantSite<'d, 'k>> {
    let mut out = Vec::new();
    let mut push = |objective: Option<&'d Objective>,
                    point: GrantPoint,
                    (rank, owner, reward_ix): (u8, usize, usize),
                    reward: &'d Reward| {
        if let Some((kind, effects)) = kind_effects(kinds, reward) {
            out.push(RewardGrantSite {
                quest,
                objective,
                point,
                order: GrantOrder {
                    rank,
                    owner,
                    reward: reward_ix,
                    effect: 0,
                },
                reward,
                kind,
                effects,
            });
        }
    };
    let objectives = quest.body.iter().filter_map(|n| match n {
        Node::Objective(o) => Some(o),
        _ => None,
    });
    for (owner, o) in objectives.enumerate() {
        for (index, r) in o.rewards.iter().enumerate() {
            if r.outcome.is_none() {
                push(Some(o), GrantPoint::Objective, (0, owner, index), r);
            }
        }
    }
    for (index, r) in quest.rewards.iter().enumerate() {
        let point = match r.outcome.as_deref() {
            None => GrantPoint::QuestComplete,
            Some("failed") => GrantPoint::QuestFailed,
            Some(_) => continue,
        };
        push(None, point, (1, 0, index), r);
    }
    out
}

/// Every effect `quest`'s rewards apply — its objectives' (top-level
/// `<objective>` children, the rewards the checker judges) and its own — in
/// grant order: per [`reward_grant_sites`] site, its retracts, asserts and
/// accepts. A reward whose `outcome=` is illegal (`E-REWARD-ATTR`) applies
/// nothing; a reward kind's `raises` is no fact or accept effect
/// ([`crate::raises::RaiseTable`] reads the sites).
pub fn quest_grants<'d>(quest: &'d Quest, kinds: &RewardKinds) -> Vec<GrantEffect<'d>> {
    let mut out = Vec::new();
    for site in reward_grant_sites(quest, kinds) {
        let facts = reward_facts(site.kind, site.reward);
        let accepts = reward_accepts(site.effects, site.reward);
        let effects = facts
            .writes()
            .map(|(p, assert)| Effect::Fact {
                pattern: p.clone(),
                assert,
            })
            .chain(
                accepts
                    .into_iter()
                    .map(|(quest, from_target)| Effect::Accept { quest, from_target }),
            );
        for (effect_ix, effect) in effects.enumerate() {
            out.push(GrantEffect {
                quest,
                objective: site.objective,
                point: site.point,
                order: GrantOrder {
                    effect: effect_ix,
                    ..site.order
                },
                reward: site.reward,
                effect,
            });
        }
    }
    out
}

/// One quest's [`quest_grants`] grouped once by owner (0.50.0 §5.4 grant
/// points), each group in grant order: per objective (in declaration
/// order), the quest's completion, its failure. A consumer reads an
/// owner's slice directly, never rescanning the quest's effects.
#[derive(Clone, Debug, Default)]
pub struct QuestGrants<'d> {
    objectives: Vec<(&'d Objective, Vec<GrantEffect<'d>>)>,
    by_objective: HashMap<*const Objective, usize>,
    complete: Vec<GrantEffect<'d>>,
    failed: Vec<GrantEffect<'d>>,
}

impl<'d> QuestGrants<'d> {
    pub fn new(quest: &'d Quest, kinds: &RewardKinds) -> Self {
        let mut out = QuestGrants::default();
        for g in quest_grants(quest, kinds) {
            match (g.point, g.objective) {
                (GrantPoint::Objective, Some(o)) => {
                    let ix = *out
                        .by_objective
                        .entry(o as *const Objective)
                        .or_insert_with(|| {
                            out.objectives.push((o, Vec::new()));
                            out.objectives.len() - 1
                        });
                    out.objectives[ix].1.push(g);
                }
                (GrantPoint::QuestComplete, _) => out.complete.push(g),
                (GrantPoint::QuestFailed, _) => out.failed.push(g),
                (GrantPoint::Objective, None) => {}
            }
        }
        out
    }

    /// The effects `objective`'s rewards apply at its `done`.
    pub fn objective(&self, objective: &Objective) -> &[GrantEffect<'d>] {
        self.by_objective
            .get(&(objective as *const Objective))
            .map_or(&[], |&ix| &self.objectives[ix].1)
    }

    /// Every objective with effects, in declaration order, with them.
    pub fn objectives(&self) -> impl Iterator<Item = (&'d Objective, &[GrantEffect<'d>])> {
        self.objectives.iter().map(|(o, g)| (*o, g.as_slice()))
    }

    /// The quest-level effects of `point` (`QuestComplete` / `QuestFailed`);
    /// none for `Objective`.
    pub fn at(&self, point: GrantPoint) -> &[GrantEffect<'d>] {
        match point {
            GrantPoint::QuestComplete => &self.complete,
            GrantPoint::QuestFailed => &self.failed,
            GrantPoint::Objective => &[],
        }
    }
}

/// Every reward effect of one resolved project root, with its document, in
/// document then grant order. Built once per pass, as
/// [`crate::directive_facts::root_table`] is.
#[derive(Clone, Debug, Default)]
pub struct GrantTable<'d> {
    grants: Vec<(&'d Path, GrantEffect<'d>)>,
}

impl<'d> GrantTable<'d> {
    pub fn build(docs: &[ProjectDoc<'d>]) -> Self {
        let mut grants = Vec::new();
        for item in docs {
            for quest in &item.doc.quests {
                grants.extend(
                    quest_grants(quest, &item.snapshot.reward_kinds)
                        .into_iter()
                        .map(|g| (item.path, g)),
                );
            }
        }
        GrantTable { grants }
    }

    /// Every effect, with its document.
    pub fn iter(&self) -> impl Iterator<Item = (&'d Path, &GrantEffect<'d>)> {
        self.grants.iter().map(|(p, g)| (*p, g))
    }

    /// The effects `quest`'s rewards apply (that very declaration), in
    /// grant order.
    pub fn of<'s>(&'s self, quest: &'s Quest) -> impl Iterator<Item = &'s GrantEffect<'d>> {
        self.grants
            .iter()
            .map(|(_, g)| g)
            .filter(move |g| std::ptr::eq(g.quest, quest))
    }

    /// Every accept effect, with its document and the accepted quest id.
    pub fn accepts(&self) -> impl Iterator<Item = (&'d Path, &GrantEffect<'d>, &str)> {
        self.iter()
            .filter_map(|(p, g)| g.accepted().map(|q| (p, g, q)))
    }
}

/// 0.50.0 §5.2: the facts each `<reward>` of `quest` (its own and its
/// objectives') writes through its kind's `effects`, judged exactly as an
/// `::assert` / `::retract` of them — relation declared, arity, argument
/// kinds and members — at the reward's `kind=`. A `reserved: true` relation
/// may be written: the engine applies a reward's effects itself.
pub fn check_reward_facts(
    quest: &Quest,
    kinds: &RewardKinds,
    domains: &BTreeMap<String, Domain>,
    ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    let objective_rewards = quest.body.iter().flat_map(|n| match n {
        Node::Objective(o) => o.rewards.as_slice(),
        _ => &[],
    });
    let mut out = Vec::new();
    for reward in objective_rewards.chain(&quest.rewards) {
        let Some((kind, _)) = kind_effects(kinds, reward) else {
            continue;
        };
        for (pattern, assert) in reward_facts(kind, reward).writes() {
            let verb = if assert { "asserts" } else { "retracts" };
            for mut d in crate::fact_write::check_effect_write(
                pattern,
                reward.kind_span,
                !assert,
                domains,
                ctx,
            ) {
                d.message = format!(
                    "`<reward kind=\"{}\">` {verb} `{}` (its kind's `effects`): {}",
                    reward.kind.trim(),
                    crate::directive_facts::pattern_text(pattern),
                    d.message
                );
                out.push(d);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S3 review R5: [`QuestGrants`] groups a quest's effects once by
    /// owner, each owner's slice in grant order, so no consumer rescans
    /// the quest's effects per objective.
    #[test]
    fn quest_grants_group_by_owner_in_grant_order() {
        let effects: RewardEffects =
            yaml_serde::from_str("{ retracts: [\"met(@target)\"], asserts: [\"met(@target)\"] }")
                .expect("effects");
        let kinds: RewardKinds = [(
            "SWAP".to_string(),
            RewardKindDecl {
                name: "SWAP".into(),
                target: None,
                attrs: Vec::new(),
                credits: None,
                effects: Some(effects),
            },
        )]
        .into_iter()
        .collect();
        let (doc, _) = lute_syntax::parse(
            "---\nkind: quest\nid: q\n---\n\n<quest id=\"q\" title=\"Q\" start=\"true\">\n  \
             <objective id=\"a\" title=\"A\" done=\"true\">\n    \
             <reward kind=\"SWAP\" target=\"mira\"/>\n  </objective>\n  \
             <objective id=\"b\" title=\"B\" done=\"true\"/>\n  \
             <objective id=\"c\" title=\"C\" done=\"true\">\n    \
             <reward kind=\"SWAP\" target=\"theo\"/>\n  </objective>\n  \
             <reward kind=\"SWAP\" target=\"ana\"/>\n  \
             <reward kind=\"SWAP\" target=\"bo\" outcome=\"failed\"/>\n</quest>\n",
        );
        let quest = &doc.quests[0];
        let grants = QuestGrants::new(quest, &kinds);
        let objective = |id: &str| {
            quest
                .body
                .iter()
                .find_map(|n| match n {
                    Node::Objective(o) if o.id == id => Some(o),
                    _ => None,
                })
                .expect("objective")
        };
        let facts = |gs: &[GrantEffect<'_>]| -> Vec<(String, bool)> {
            gs.iter()
                .filter_map(|g| g.fact())
                .map(|(p, assert)| match &p.args[0].term {
                    FactTerm::Ident(who) => (who.clone(), assert),
                    other => (format!("{other:?}"), assert),
                })
                .collect()
        };
        let pair = |who: &str| vec![(who.to_string(), false), (who.to_string(), true)];
        assert_eq!(facts(grants.objective(objective("a"))), pair("mira"));
        assert!(grants.objective(objective("b")).is_empty());
        assert_eq!(facts(grants.objective(objective("c"))), pair("theo"));
        let owners: Vec<&str> = grants.objectives().map(|(o, _)| o.id.as_str()).collect();
        assert_eq!(owners, ["a", "c"]);
        assert_eq!(facts(grants.at(GrantPoint::QuestComplete)), pair("ana"));
        assert_eq!(facts(grants.at(GrantPoint::QuestFailed)), pair("bo"));
        assert!(grants.at(GrantPoint::Objective).is_empty());
    }

    fn kind(name: &str, effects: &str) -> (String, RewardKindDecl) {
        (
            name.to_string(),
            RewardKindDecl {
                name: name.into(),
                target: None,
                attrs: Vec::new(),
                credits: None,
                effects: Some(yaml_serde::from_str(effects).expect("effects")),
            },
        )
    }

    /// 0.51.0 §4.5 (Review051b F2): one grant site per reward whose kind
    /// declares effects — a raises-only kind included, a kind with asserts,
    /// an accept and a raise once — at 0.50's grant points; `quest_grants`,
    /// built on the sites, keeps 0.50's effect order and lists no raise.
    #[test]
    fn reward_grant_sites_are_one_per_reward_at_its_grant_point() {
        let kinds: RewardKinds = [
            kind(
                "SHOW",
                "{ raises: [{ occasion: cutscene, target: \"@target\" }] }",
            ),
            kind(
                "MIX",
                "{ asserts: [\"met(@target)\"], accepts: [second], raises: [{ occasion: \
                 cutscene, target: \"@target\" }] }",
            ),
        ]
        .into_iter()
        .collect();
        let (doc, _) = lute_syntax::parse(
            "---\nkind: quest\nid: q\n---\n\n<quest id=\"q\" title=\"Q\" start=\"true\">\n  \
             <objective id=\"a\" title=\"A\" done=\"true\">\n    \
             <reward kind=\"SHOW\" target=\"mira\"/>\n    \
             <reward kind=\"SHOW\" target=\"never\" outcome=\"failed\"/>\n  </objective>\n  \
             <reward kind=\"MIX\" target=\"ana\"/>\n  \
             <reward kind=\"BADGE\"/>\n  \
             <reward kind=\"SHOW\" target=\"bo\" outcome=\"failed\"/>\n  \
             <reward kind=\"SHOW\" target=\"cy\" outcome=\"later\"/>\n</quest>\n",
        );
        let quest = &doc.quests[0];
        /// A site's grant point, kind, target and `(rank, owner, reward)`.
        type Site<'a> = (GrantPoint, &'a str, Option<&'a str>, (u8, usize, usize));
        let sites: Vec<Site<'_>> = reward_grant_sites(quest, &kinds)
            .iter()
            .map(|s| {
                (
                    s.point,
                    s.reward.kind.as_str(),
                    s.reward.target.as_deref(),
                    (s.order.rank, s.order.owner, s.order.reward),
                )
            })
            .collect();
        assert_eq!(
            sites,
            [
                (GrantPoint::Objective, "SHOW", Some("mira"), (0, 0, 0)),
                (GrantPoint::QuestComplete, "MIX", Some("ana"), (1, 0, 0)),
                (GrantPoint::QuestFailed, "SHOW", Some("bo"), (1, 0, 2)),
            ]
        );
        let effects: Vec<(GrantPoint, GrantOrder, Option<String>, bool)> =
            quest_grants(quest, &kinds)
                .iter()
                .map(|g| {
                    let accepted = g.accepted().map(str::to_string);
                    (g.point, g.order, accepted, g.fact().is_some())
                })
                .collect();
        let order = |effect| GrantOrder {
            rank: 1,
            owner: 0,
            reward: 0,
            effect,
        };
        assert_eq!(
            effects,
            [
                (GrantPoint::QuestComplete, order(0), None, true),
                (
                    GrantPoint::QuestComplete,
                    order(1),
                    Some("second".to_string()),
                    false,
                ),
            ]
        );
    }
}
