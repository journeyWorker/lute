//! 0.51.0 §4.2 (#170): declared occasion raises at assembly.
//!
//! A directive's or reward kind's `effects.raises` may name another plugin's
//! occasion, so it is judged once every active plugin is merged
//! ([`crate::assemble::assemble_snapshot`]), the 0.50 `E-PLUGIN-STAMP-SURFACE`
//! precedent: the occasion is in the snapshot, the entry's `target` matches
//! whether the occasion is raised for one, an `@attr` of a domain occasion is
//! typed by its entity kind, and a literal target has the domain's shape. A
//! refused entry is dropped and its siblings land (`E-PLUGIN-RAISES`). A raise
//! on a directive whose `lower:` carries no effects is refused by the
//! directive's own validation (`E-PLUGIN-INVALID-DIRECTIVE`) and dropped here.

use std::collections::BTreeMap;

use crate::schema::{
    Lowering, OccasionDecl, OccasionTarget, RaiseDecl, RaiseTargetDecl, RewardTarget,
};
use crate::snapshot::CapabilitySnapshot;
use crate::types::Type;

/// What declares a raise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaiseOwner {
    /// A directive, by tag.
    Directive(String),
    /// A reward kind, by id.
    RewardKind(String),
}

impl std::fmt::Display for RaiseOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RaiseOwner::Directive(tag) => write!(f, "directive `::{tag}`"),
            RaiseOwner::RewardKind(kind) => write!(f, "reward kind `{kind}`"),
        }
    }
}

/// Why one `effects.raises` entry is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RaiseFault {
    /// The occasion is not in the snapshot. `near`: a declared occasion of a
    /// close name; `inactive_owner`: the installed, inactive plugin that
    /// declares it.
    UnknownOccasion {
        near: Option<String>,
        inactive_owner: Option<String>,
    },
    /// A `target` on an occasion raised for none.
    TargetOnUntargeted(RaiseTargetDecl),
    /// No `target` on an occasion raised for one (`of`).
    MissingTarget(OccasionTarget),
    /// An `@attr` target of a domain occasion whose type is not the
    /// domain's entity kind: `actual` is the attribute's declared type — on
    /// a reward kind its `target:` contract, `None` when it declares none.
    AttrType {
        attr: String,
        actual: Option<String>,
        entity: String,
    },
    /// A literal target outside the occasion's domain shape (`of`).
    OutsideDomain { target: String, of: OccasionTarget },
}

/// One refused `effects.raises` entry (`E-PLUGIN-RAISES`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RaiseError {
    /// The plugin declaring the raise.
    pub plugin: String,
    pub owner: RaiseOwner,
    pub occasion: String,
    pub fault: RaiseFault,
}

impl std::fmt::Display for RaiseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let RaiseError {
            plugin,
            owner,
            occasion,
            fault,
        } = self;
        write!(f, "{owner} (`{plugin}`) raises ")?;
        match fault {
            RaiseFault::UnknownOccasion {
                inactive_owner: Some(declarer),
                ..
            } => write!(
                f,
                "occasion `{occasion}`, which no active plugin declares: it is an occasion of \
                 `{declarer}`, installed but not active in this profile — declare `{declarer}` \
                 in the `depends:` of `{plugin}`, so that the occasion is in every profile that \
                 activates `{plugin}`"
            ),
            RaiseFault::UnknownOccasion {
                near,
                inactive_owner: None,
            } => {
                write!(
                    f,
                    "occasion `{occasion}`, which no active plugin declares (a plugin raising \
                     another plugin's occasion lists it in `depends:`)"
                )?;
                match near {
                    Some(near) => write!(f, " — did you mean `{near}`?"),
                    None => Ok(()),
                }
            }
            RaiseFault::TargetOnUntargeted(target) => write!(
                f,
                "`{occasion}` for `{target}`, but `{occasion}` is raised for no target — drop \
                 the raise's `target:`"
            ),
            RaiseFault::MissingTarget(of) => {
                let attr = match owner {
                    RaiseOwner::Directive(_) => "@<attr>",
                    RaiseOwner::RewardKind(_) => "@target",
                };
                write!(
                    f,
                    "`{occasion}` without a target, but `{occasion}` is raised for a target ({}) \
                     — add `target: \"{attr}\"`",
                    shape_text(of)
                )
            }
            RaiseFault::AttrType {
                attr,
                actual,
                entity,
            } => {
                write!(f, "`{occasion}` for `@{attr}`, ")?;
                match (owner, actual) {
                    (RaiseOwner::Directive(_), Some(ty)) => write!(f, "which is typed `{ty}`")?,
                    (RaiseOwner::Directive(_), None) => write!(f, "which is not declared")?,
                    (RaiseOwner::RewardKind(_), Some(c)) => {
                        write!(f, "whose kind's `target:` contract is `{c}`")?;
                    }
                    (RaiseOwner::RewardKind(_), None) => {
                        write!(f, "but the kind declares no `target:` contract")?;
                    }
                }
                let fix = match owner {
                    RaiseOwner::Directive(_) => {
                        format!("type the attribute `{{ entity: {entity} }}`")
                    }
                    RaiseOwner::RewardKind(_) => {
                        format!("declare the kind's `target: {{ entity: {entity} }}`")
                    }
                };
                write!(
                    f,
                    "; the target of `{occasion}` is a `{entity}` member, so {fix}"
                )
            }
            RaiseFault::OutsideDomain { target, of } => write!(
                f,
                "`{occasion}` for `{target}`, but its targets are {}",
                shape_text(of)
            ),
        }
    }
}

/// How messages name the targets an occasion is raised for.
fn shape_text(of: &OccasionTarget) -> String {
    match of {
        OccasionTarget::Shape(_) => "any dotted id".to_string(),
        OccasionTarget::Domain {
            prefix,
            members: Some(members),
            ..
        } => members
            .iter()
            .map(|m| format!("`{prefix}.{m}`"))
            .collect::<Vec<_>>()
            .join(", "),
        OccasionTarget::Domain { prefix, entity, .. } => format!("`{prefix}.<{entity}>`"),
    }
}

/// A reward kind's `target:` contract as its YAML reads.
fn contract_text(t: &RewardTarget) -> Option<String> {
    match (&t.provider, &t.entity) {
        (Some(p), _) => Some(format!("{{ provider: {p} }}")),
        (None, Some(e)) => Some(format!("{{ entity: {e} }}")),
        (None, None) => None,
    }
}

/// What a raise's owner offers its `target` to bind.
enum Binder<'a> {
    /// A directive's attributes.
    Directive(&'a [crate::schema::AttrDecl]),
    /// A reward kind's `target:` contract.
    RewardKind(Option<&'a RewardTarget>),
}

/// The fault of one raise, `None` when it lands.
fn judge(
    raise: &RaiseDecl,
    binder: &Binder<'_>,
    occasions: &BTreeMap<String, OccasionDecl>,
    inactive: &BTreeMap<String, String>,
) -> Option<RaiseFault> {
    let Some(decl) = occasions.get(&raise.occasion) else {
        return Some(RaiseFault::UnknownOccasion {
            near: crate::suggest::nearest(&raise.occasion, occasions.keys().map(String::as_str), 2)
                .map(str::to_string),
            inactive_owner: inactive.get(&raise.occasion).cloned(),
        });
    };
    let target = match (&raise.target, decl.target.takes_target()) {
        (None, false) => return None,
        (Some(t), false) => return Some(RaiseFault::TargetOnUntargeted(t.clone())),
        (None, true) => return Some(RaiseFault::MissingTarget(decl.target.clone())),
        (Some(t), true) => t,
    };
    let OccasionTarget::Domain {
        prefix,
        entity,
        members,
    } = &decl.target
    else {
        return None;
    };
    match target {
        RaiseTargetDecl::Attr(attr) => {
            let (typed, actual) = match binder {
                Binder::Directive(attrs) => {
                    let ty = attrs.iter().find(|a| &a.name == attr).map(|a| &a.ty);
                    (
                        matches!(ty, Some(Type::Entity(e)) if e == entity),
                        ty.map(crate::types::type_str),
                    )
                }
                Binder::RewardKind(contract) => (
                    contract.is_some_and(|c| c.entity.as_ref() == Some(entity)),
                    contract.and_then(contract_text),
                ),
            };
            (!typed).then(|| RaiseFault::AttrType {
                attr: attr.clone(),
                actual,
                entity: entity.clone(),
            })
        }
        RaiseTargetDecl::Literal(t) => {
            let member = t
                .strip_prefix(prefix.as_str())
                .and_then(|rest| rest.strip_prefix('.'))
                .filter(|m| crate::ident::is_name(m));
            let inside =
                member.is_some_and(|m| members.as_ref().is_none_or(|ms| ms.iter().any(|x| x == m)));
            (!inside).then(|| RaiseFault::OutsideDomain {
                target: t.clone(),
                of: decl.target.clone(),
            })
        }
    }
}

/// Judge every declared raise of the merged `snap` (0.51.0 §4.2) and drop
/// each refused entry; a raise on a directive whose `lower:` carries no
/// effects is dropped without a second report. `dir_owner` / `rk_owner`:
/// the plugin declaring each directive / reward kind; `inactive`: each
/// occasion an installed but inactive plugin declares, with that plugin.
pub(crate) fn check_raises(
    snap: &mut CapabilitySnapshot,
    dir_owner: &BTreeMap<String, String>,
    rk_owner: &BTreeMap<String, String>,
    inactive: &BTreeMap<String, String>,
) -> Vec<RaiseError> {
    let mut errs = Vec::new();
    let occasions = &snap.occasions;
    for (tag, d) in &mut snap.directives {
        let Some(effects) = d.effects.as_mut().filter(|e| !e.raises.is_empty()) else {
            continue;
        };
        if !matches!(d.lower, Lowering::Passthrough) {
            effects.raises.clear();
            continue;
        }
        let binder = Binder::Directive(&d.attrs);
        effects.raises.retain(|r| {
            let Some(fault) = judge(r, &binder, occasions, inactive) else {
                return true;
            };
            errs.push(RaiseError {
                plugin: dir_owner.get(tag).cloned().unwrap_or_default(),
                owner: RaiseOwner::Directive(tag.clone()),
                occasion: r.occasion.clone(),
                fault,
            });
            false
        });
    }
    for (name, k) in &mut snap.reward_kinds {
        let contract = k.target.as_ref();
        let Some(effects) = k.effects.as_mut().filter(|e| !e.raises.is_empty()) else {
            continue;
        };
        let binder = Binder::RewardKind(contract);
        effects.raises.retain(|r| {
            let Some(fault) = judge(r, &binder, occasions, inactive) else {
                return true;
            };
            errs.push(RaiseError {
                plugin: rk_owner.get(name).cloned().unwrap_or_default(),
                owner: RaiseOwner::RewardKind(name.clone()),
                occasion: r.occasion.clone(),
                fault,
            });
            false
        });
    }
    errs
}
