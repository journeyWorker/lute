//! `<reward/>` checks (dsl 0.16.0, 0.23.0 §8, 0.26.0 §2.5): shape, kind
//! vocabulary, target contract, and double credit.

use super::*;

/// `E-REWARD-ATTR` (dsl 0.16.0 §2/§6): a `<reward>` element's shape is
/// malformed — missing/empty `kind`, an `amount=` value that is not a
/// signed integer or an inclusive `N..M` range with `N <= M`, an `outcome=`
/// value other than `"failed"`, or `outcome=` authored on an objective-level
/// reward (only quest-level rewards fire on failure). Anchored at the
/// offending attribute value (or the reward element for missing `kind`).
/// Unknown attribute keys are `E-UNKNOWN-ATTR` via the D-J closure.
pub const E_REWARD_ATTR: &str = "E-REWARD-ATTR";

/// `E-REWARD-KIND` (dsl 0.16.0 §4/§6): a `<reward kind="…">` value is not
/// a declared reward kind in the resolved capability snapshot's
/// `rewardKinds` vocabulary. Silently accepted when the snapshot carries
/// NO reward kinds (shape-only mode — every scenario compiles clean until
/// a plugin publishes a vocabulary).
pub const E_REWARD_KIND: &str = "E-REWARD-KIND";

/// Reward shape (`E-REWARD-ATTR`) + vocabulary (`E-REWARD-KIND`) + target
/// contract (`E-REWARD-TARGET`) + D-J attribute closure (`E-UNKNOWN-ATTR`)
/// checks for every `<reward/>` on this quest AND its objectives (dsl 0.16.0
/// §2, §4, §6; 0.26.0 §2.5). The vocabulary gate stays silent when
/// `snapshot.reward_kinds` is empty. `providers` resolves a `{ provider: … }`
/// target contract, `kinds` (the document's merged entity kinds) an
/// `{ entity: … }` one. Anchored at the offending attribute so the author's
/// editor lands on the fault, not the enclosing element.
pub fn check_quest_rewards(
    quest: &Quest,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let targets = RewardTargetEnv { providers, kinds };
    for r in &quest.rewards {
        check_one_reward(r, snapshot, &targets, RewardPos::Quest, &mut diags);
    }
    for node in &quest.body {
        if let Node::Objective(o) = node {
            for r in &o.rewards {
                check_one_reward(r, snapshot, &targets, RewardPos::Objective, &mut diags);
            }
        }
    }
    check_reward_double_credit(quest, snapshot, &mut diags);
    diags
}

/// What a reward kind's `target:` contract resolves against.
struct RewardTargetEnv<'a> {
    providers: &'a ProviderSet,
    kinds: &'a BTreeMap<String, EntityKindDecl>,
}

/// `E-REWARD-TARGET` (dsl 0.26.0 §2.5).
pub const E_REWARD_TARGET: &str = "E-REWARD-TARGET";

/// dsl 0.26.0 §2.5: `r.target` against its kind's `target:` contract. A
/// missing target fails only `required: true`; a present one must be a
/// member of the `entity:` kind (did-you-mean) or an id of the `provider:`
/// catalog (a stale snapshot only warns, as for a `providerRef` attr).
fn check_reward_target(
    r: &Reward,
    contract: &RewardTarget,
    env: &RewardTargetEnv<'_>,
) -> Option<Diagnostic> {
    let kind = r.kind.trim();
    let Some(target) = r.target.as_deref() else {
        return contract.required.then(|| {
            diag(
                E_REWARD_TARGET,
                Severity::Error,
                format!(
                    "`<reward kind=\"{kind}\">` needs a `target=`: the reward kind declares \
                     `target: {{ required: true }}` (dsl 0.26.0 §2.5)"
                ),
                r.span,
            )
        });
    };
    let at = r.target_span.unwrap_or(r.span);
    if let Some(entity) = contract.entity.as_deref() {
        let message = match env.kinds.get(entity).map(|d| &d.shape) {
            None => format!(
                "`<reward kind=\"{kind}\">`'s target contract names entity kind `{entity}`, which \
                 this document does not declare — import the schema that declares it through \
                 `uses:` (dsl 0.26.0 §2.5)"
            ),
            Some(KindShape::Members(ms)) if !ms.iter().any(|m| m == target) => format!(
                "`{target}` is not a member of entity kind `{entity}`, which `<reward \
                 kind=\"{kind}\">` targets (dsl 0.26.0 §2.5){}",
                crate::rel_schema::member_hint(target, ms)
            ),
            Some(_) => return None,
        };
        return Some(diag(E_REWARD_TARGET, Severity::Error, message, at));
    }
    let provider = contract.provider.as_deref()?;
    match env.providers.contains(provider, target) {
        IdStatus::Fresh => None,
        IdStatus::Stale => Some(diag(
            "W-CATALOG-STALE",
            Severity::Warning,
            format!("`{target}` not found in `{provider}` catalog (snapshot is stale/offline)"),
            at,
        )),
        IdStatus::Absent => Some(diag(
            E_REWARD_TARGET,
            Severity::Error,
            format!(
                "`{target}` is not a known `{provider}` id, which `<reward kind=\"{kind}\">` \
                 targets (dsl 0.26.0 §2.5)"
            ),
            at,
        )),
    }
}

/// `W-REWARD-DOUBLE-CREDIT` (dsl 0.23.0 §8): a reward kind that declares
/// `credits: <path>` already adds its amount to `<path>` when granted, so a
/// content `::set` of that same path in one of this quest's handlers — an
/// `<on>` body or an objective's completion body — pays twice.
pub const W_REWARD_DOUBLE_CREDIT: &str = "W-REWARD-DOUBLE-CREDIT";

fn check_reward_double_credit(
    quest: &Quest,
    snapshot: &CapabilitySnapshot,
    diags: &mut Vec<Diagnostic>,
) {
    let objective_rewards = quest.body.iter().flat_map(|n| match n {
        Node::Objective(o) => o.rewards.as_slice(),
        _ => &[],
    });
    // credited path -> the first reward kind crediting it.
    let mut credited: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for r in quest.rewards.iter().chain(objective_rewards) {
        let kind = r.kind.trim();
        if let Some(path) = snapshot
            .reward_kinds
            .get(kind)
            .and_then(|k| k.credits.as_deref())
        {
            credited.entry(path).or_insert(kind);
        }
    }
    if credited.is_empty() {
        return;
    }
    let mut sets: Vec<&lute_syntax::ast::Set> = Vec::new();
    for node in &quest.body {
        match node {
            Node::On(o) => collect_sets(&o.body, &mut sets),
            Node::Objective(o) => collect_sets(&o.body, &mut sets),
            _ => {}
        }
    }
    for s in sets {
        if let Some(kind) = credited.get(s.path.as_str()) {
            diags.push(diag(
                W_REWARD_DOUBLE_CREDIT,
                Severity::Warning,
                format!(
                    "`::set` of `{}` in a handler of quest `{}`: its `<reward kind=\"{kind}\">` \
                     already credits `{}` when granted, so the player is paid twice — drop \
                     the `::set` or the reward (dsl 0.23.0 §8)",
                    s.path, quest.id, s.path
                ),
                s.path_span,
            ));
        }
    }
}

/// Every `::set` in `nodes`, descending into choice and arm bodies.
fn collect_sets<'a>(nodes: &'a [Node], out: &mut Vec<&'a lute_syntax::ast::Set>) {
    for node in nodes {
        match node {
            Node::Set(s) => out.push(s),
            Node::Branch(b) => b.choices.iter().for_each(|c| collect_sets(&c.body, out)),
            Node::Hub(h) => h.bodies().for_each(|b| collect_sets(b, out)),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_sets(body, out)
                        }
                    }
                }
            }
            Node::Objective(o) => collect_sets(&o.body, out),
            Node::On(o) => collect_sets(&o.body, out),
            _ => {}
        }
    }
}

/// The two owner positions a `<reward/>` can occupy (dsl 0.16.0 §2 / D-D).
/// A quest-level reward MAY carry `outcome="failed"`; an objective-level reward
/// MAY NOT (an objective grants at first `done`, never at fail).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RewardPos {
    Quest,
    Objective,
}

fn check_one_reward(
    r: &Reward,
    snapshot: &CapabilitySnapshot,
    targets: &RewardTargetEnv<'_>,
    pos: RewardPos,
    diags: &mut Vec<Diagnostic>,
) {
    // Shape: `kind=` is required (dsl 0.16.0 §2). An empty value hits
    // `E-REWARD-ATTR` at the value span (or the open-tag span when the
    // attribute is missing — the parser stores the same span in both
    // cases, matching how `<quest follows=>` / `<objective quest=>` degrade
    // an absent attr).
    if r.kind.trim().is_empty() {
        diags.push(diag(
            E_REWARD_ATTR,
            Severity::Error,
            "`<reward>` requires a non-empty `kind` (dsl 0.16.0 §2)".to_string(),
            r.kind_span,
        ));
    }
    // A malformed `amount=` was preserved in `attrs` by the parser so the
    // checker can anchor `E-REWARD-ATTR` at the value span exactly. `N > M`
    // is one of the shapes `parse_reward_amount` rejects.
    if r.amount.is_none() {
        if let Some(a) = r.attrs.iter().find(|a| a.key == "amount") {
            let raw = match &a.value {
                AttrValue::Str(s) => s.as_str(),
                _ => "",
            };
            diags.push(diag(
                E_REWARD_ATTR,
                Severity::Error,
                format!(
                    "`<reward amount=\"{raw}\">` is not a valid literal; use a signed integer or \
                     an inclusive `N..M` range with `N <= M` (dsl 0.16.0 §2)"
                ),
                a.value_span,
            ));
        }
    }
    // `outcome=` enum: `"failed"` is the only legal value (dsl 0.16.0 §2),
    // and ONLY on a quest-level reward — an objective reward fires at first
    // `done`, never at fail. Both misuses share the E-REWARD-ATTR code
    // because they are the same rule ("the outcome= surface has no meaning
    // here"); the message names which half.
    if let (Some(outcome), Some(outcome_span)) = (r.outcome.as_deref(), r.outcome_span) {
        if matches!(pos, RewardPos::Objective) {
            diags.push(diag(
                E_REWARD_ATTR,
                Severity::Error,
                "`<reward outcome=…>` is legal only on a quest-level reward; an objective grants \
                 at first `done` and never at fail"
                    .to_string(),
                outcome_span,
            ));
        } else if outcome != "failed" {
            diags.push(diag(
                E_REWARD_ATTR,
                Severity::Error,
                format!(
                    "`<reward outcome=\"{outcome}\">` is not a legal outcome; `failed` is the only \
                     accepted value (a reward without `outcome=` grants on `complete`)"
                ),
                outcome_span,
            ));
        }
    }
    // Vocabulary (dsl 0.16.0 §4): only when a plugin has published one.
    // Skips a shape-invalid empty kind — the E-REWARD-ATTR above already
    // owns that (one code per fault, D-A).
    if !snapshot.reward_kinds.is_empty()
        && !r.kind.trim().is_empty()
        && !snapshot.reward_kinds.contains_key(r.kind.trim())
    {
        diags.push(diag(
            E_REWARD_KIND,
            Severity::Error,
            format!(
                "`<reward kind=\"{}\">` names no declared reward kind{}; add it to a plugin's \
                 `rewardKinds:` export",
                r.kind,
                lute_manifest::suggest::did_you_mean(
                    r.kind.trim(),
                    snapshot.reward_kinds.keys().map(String::as_str)
                )
            ),
            r.kind_span,
        ));
    }
    // dsl 0.26.0 §2.5: the kind's `target:` contract.
    if let Some(contract) = snapshot
        .reward_kinds
        .get(r.kind.trim())
        .and_then(|k| k.target.as_ref())
    {
        diags.extend(check_reward_target(r, contract, targets));
    }
    // D-J attribute closure (dsl 0.16.0 §2 tag row) — E-UNKNOWN-ATTR at
    // each residual key's own span. A malformed `amount=` also survives in
    // `attrs`, but `amount` is in REWARD_ATTRS so it is not reported here;
    // its E-REWARD-ATTR emission above owns the diagnostic.
    crate::logic_attrs::check_reward_attrs(r, diags);
}
