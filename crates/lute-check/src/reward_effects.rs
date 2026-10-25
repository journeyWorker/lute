//! 0.50.0 §5 (#169): reward effects — the facts a granted `<reward>`
//! retracts and asserts and the quests it accepts, declared by its kind
//! (`rewardKinds.<K>.effects`, 0.50.0 §5.1).
//!
//! Every analysis reads a reward effect as if it were the corresponding
//! `::retract` / `::assert` / `::accept` written first in the owner's grant
//! point (§5.4): a quest's completion (before its `questComplete`
//! handlers), its failure (`outcome="failed"`, before `questFailed`), or an
//! objective's completion. [`quest_grants`] resolves one quest's rewards
//! into [`GrantEffect`]s in grant order; [`GrantTable`] holds a whole root's,
//! built once per pass from the root's [`ProjectDoc`]s — each document's
//! rewards resolve against the reward vocabulary it was checked against
//! ([`ProjectDoc::reward_kinds`]). A reward's `when=` is carried as the
//! effect's guard, so no consumer re-finds the `<reward>`. No synthetic AST
//! node is created.
//!
//! An effect reading `@target` binds the reward's quoted `target=` (0.49.0
//! §3.3: the only form that reaches the IR); a reward without one does not
//! apply that effect, the directive rule for an unbound `@attr`.

use std::collections::BTreeMap;
use std::path::Path;

use lute_core_span::{Diagnostic, Span};
use lute_manifest::fact::{FactArg, FactPattern, FactTerm};
use lute_manifest::schema::{
    AcceptEffect, FactEffect, FactEffectArg, RewardEffects, RewardKindDecl, REWARD_TARGET_ATTR,
};
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::{CelSlot, Node, Objective, Quest, Reward};

use crate::directive_facts::CallFacts;
use crate::{Ctx, ProjectDoc};

/// A reward vocabulary by kind id (a capability snapshot's `rewardKinds`).
pub type RewardKinds = BTreeMap<String, RewardKindDecl>;

/// An empty reward vocabulary: the [`ProjectDoc::reward_kinds`] of a caller
/// that resolves no reward (identity-only passes).
pub static NO_REWARD_KINDS: RewardKinds = BTreeMap::new();

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

/// The effects `reward`'s kind declares in `kinds`.
pub fn kind_effects<'k>(kinds: &'k RewardKinds, reward: &Reward) -> Option<&'k RewardEffects> {
    kinds.get(reward.kind.trim())?.effects.as_ref()
}

/// The facts one reward writes, retracts first — `call_facts`'s twin
/// ([`crate::directive_facts::call_facts`]). A pattern reading `@target`
/// on a reward without a quoted `target=` is not applied, so not listed.
pub fn reward_facts(effects: &RewardEffects, reward: &Reward) -> CallFacts {
    let resolve = |facts: &[FactEffect]| {
        facts
            .iter()
            .filter_map(|f| bind(f, reward.target.as_deref()))
            .collect()
    };
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
            AcceptEffect::Target => reward.target.clone().map(|t| (t, true)),
            AcceptEffect::Quest(id) => Some((id.clone(), false)),
        })
        .collect()
}

/// One declared pattern with `@target` bound; `None` when unbound.
fn bind(fact: &FactEffect, target: Option<&str>) -> Option<FactPattern> {
    let args = fact
        .args
        .iter()
        .map(|a| {
            let term = match a {
                FactEffectArg::Const(c) => crate::directive_facts::const_term(c),
                FactEffectArg::Wildcard => FactTerm::Wildcard,
                FactEffectArg::Attr(name) if name == REWARD_TARGET_ATTR => {
                    FactTerm::Ident(target?.to_string())
                }
                // The loader refuses any other `@attr` (0.50.0 §5.1).
                FactEffectArg::Attr(_) => return None,
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

/// Every effect `quest`'s rewards apply — its objectives' (top-level
/// `<objective>` children, the rewards the checker judges) and its own — in
/// grant order. A reward whose `outcome=` is illegal (`E-REWARD-ATTR`)
/// applies nothing.
pub fn quest_grants<'d>(quest: &'d Quest, kinds: &RewardKinds) -> Vec<GrantEffect<'d>> {
    let mut out = Vec::new();
    let mut push = |objective: Option<&'d Objective>,
                    point: GrantPoint,
                    (rank, owner, index): (u8, usize, usize),
                    reward: &'d Reward| {
        let Some(effects) = kind_effects(kinds, reward) else {
            return;
        };
        let facts = reward_facts(effects, reward);
        let accepts = reward_accepts(effects, reward);
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
                objective,
                point,
                order: GrantOrder {
                    rank,
                    owner,
                    reward: index,
                    effect: effect_ix,
                },
                reward,
                effect,
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
                    quest_grants(quest, item.reward_kinds)
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
        let Some(effects) = kind_effects(kinds, reward) else {
            continue;
        };
        for (pattern, assert) in reward_facts(effects, reward).writes() {
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
