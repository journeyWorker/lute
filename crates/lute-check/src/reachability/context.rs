use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::Span;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::{CelSlot, Node};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{decide_slot, DecideCtx, Decided};
use crate::match_check::{CoverItem, DomainValue};
use crate::solution::{disjoint, SolutionSet};

/// What [`check_reachability_in`] needs beyond the decide context (dsl
/// 0.24.0): the def result types a `<match on="@def">` subject takes its
/// domain from, the scene beat's `when` (a frontmatter slot, outside the
/// document tree), and the snapshot a body's directives are looked up in —
/// `None` (a component body) makes no [`Assumption`]. dsl 0.28.0: and the
/// document's per-beat environments ([`FoldedEnv::env_at`]) — `None` for a
/// component body, which has no kind beat.
pub(crate) struct ReachEnv<'a> {
    pub(crate) def_types: &'a BTreeMap<String, Type>,
    pub(crate) beat_when: Option<&'a CelSlot>,
    pub(crate) snapshot: Option<&'a CapabilitySnapshot>,
    pub(crate) folded: Option<&'a FoldedEnv>,
}

impl<'a> ReachEnv<'a> {
    /// `base` over the environment of the entry or beat written at `span`.
    pub(super) fn ctx_at<'c>(&self, base: &DecideCtx<'c>, span: Span) -> DecideCtx<'c>
    where
        'a: 'c,
    {
        DecideCtx {
            schema: self.folded.map_or(base.schema, |f| &f.env_at(span).state),
            dollar: None,
            params: base.params,
            facts: base.facts,
        }
    }

    /// The members of the kind or `for=` beat written at `span`.
    pub(super) fn members_at(&self, span: Span) -> Option<&'a [String]> {
        self.folded?
            .env
            .occasion_scopes
            .members_at(span.byte_start, span.byte_end)
    }
}

/// One body's walk context: [`ReachEnv::def_types`], the body's own
/// [`Assumption`], the document's `::next` targets (dsl 0.27.0), and the
/// pick records the enclosing options hold (dsl 0.28.0, [`Pick`]).
pub(super) struct Reach<'a> {
    pub(super) def_types: &'a BTreeMap<String, Type>,
    pub(super) assume: Option<&'a Assumption>,
    pub(super) targets: &'a BTreeSet<String>,
    pub(super) picks: &'a [super::picks::Pick],
    /// The body runs at most once per scene play: a scene's shots, outside
    /// any hub or `<on>` handler ([`Pick::on_return`]).
    pub(super) once: bool,
}

/// A beat's / entry's `when` as an assumption over the body it guards (dsl
/// 0.24.0): the body runs only once the guard held, so a `<match>` literal
/// the guard rules out can never be the subject's value there. Kept to what
/// the body cannot change: the guard's top-level conjuncts over `run.*` /
/// `user.*` / `prev.*` paths no `::set` / `<choice into>` in the body writes
/// — none at all once the body runs a `::use` or a state-writing directive —
/// plus the paths its presence guards prove (nothing unsets a path). dsl
/// 0.28.0: in a kind or `for=` beat, also the members `occasion.target` can
/// be there — those the guard holds for, judged once per member before the
/// body runs, so nothing the body writes changes them.
pub(crate) struct Assumption {
    pub(super) raw: String,
    pub(super) conjuncts: Vec<(String, SolutionSet)>,
    pub(super) present: crate::defassign::Assigned,
}

impl Assumption {
    pub(crate) fn new(
        when: &CelSlot,
        bodies: &[&[Node]],
        defs: &DefTable<'_>,
        def_types: &BTreeMap<String, Type>,
        schema: &crate::meta::StateSchema,
        snapshot: &CapabilitySnapshot,
    ) -> Option<Self> {
        let raw = when.raw.trim();
        if raw.is_empty() {
            return None;
        }
        let mut written = BTreeSet::new();
        let mut opaque = false;
        for body in bodies {
            opaque |= super::picks::scan_writes(body, snapshot, &mut written);
        }
        let mut conjuncts: Vec<(String, SolutionSet)> = if opaque {
            Vec::new()
        } else {
            let overlaps = |p: &str, w: &str| {
                p == w || p.starts_with(&format!("{w}.")) || w.starts_with(&format!("{p}."))
            };
            super::when_conjuncts(raw, defs, schema, None)
                .0
                .into_iter()
                .filter(|(p, _)| {
                    matches!(p.split('.').next(), Some("run" | "user" | "prev"))
                        && !written.iter().any(|w| overlaps(p, w))
                })
                .collect()
        };
        if let Some(members) = schema.string_members(crate::beats::OCCASION_TARGET) {
            let params = BTreeMap::new();
            let ctx = DecideCtx {
                schema,
                dollar: None,
                params: &params,
                facts: None,
            };
            let dead = super::dead_members(raw, members, defs, &ctx);
            // All of them: the beat's own unreachable verdict.
            if !dead.is_empty() && dead.len() < members.len() {
                conjuncts.push(member_conjunct(members, &dead));
            }
        }
        let scope = crate::defassign::Scope {
            schema,
            defs: DefTable {
                bodies: defs.bodies,
                params: defs.params,
            },
            def_types,
            preceded: false,
            components: None,
            parse_cache: std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::HashMap::new(),
            )),
        };
        let present = crate::defassign::assumed_present(Some(when), &scope);
        (!conjuncts.is_empty() || !present.is_empty()).then(|| Self {
            raw: raw.to_string(),
            conjuncts,
            present,
        })
    }

    /// Whether the guard rules out `item` as the value of `path`.
    pub(crate) fn rules_out(
        &self,
        path: &str,
        item: &CoverItem,
        schema: &crate::meta::StateSchema,
    ) -> bool {
        let mut on_path = self
            .conjuncts
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, set)| set);
        match item {
            // A comparison, an ordering, or a bare bool read is true only on
            // a value (`!=` is true on `unset`, dsl 0.23.0 §9).
            CoverItem::Unset => {
                crate::defassign::is_present(path, &self.present, schema)
                    || on_path.any(|set| !matches!(set, SolutionSet::Except(_)))
            }
            CoverItem::Value(v) => {
                let lit = SolutionSet::Values(std::iter::once(v.clone()).collect());
                on_path.any(|set| disjoint(set, &lit))
            }
            CoverItem::Num(iv) => {
                let lit = SolutionSet::Interval {
                    lo: iv.lo,
                    lo_inc: iv.lo.is_finite(),
                    hi: iv.hi,
                    hi_inc: iv.hi.is_finite(),
                };
                on_path.any(|set| disjoint(set, &lit))
            }
        }
    }
}

/// dsl 0.28.0: the members of `members` a kind or `for=` beat's `when`
/// (`raw`) can never hold for, each judged with `occasion.target` bound to
/// it. Empty when `raw` does not read `occasion.target`.
pub(crate) fn dead_members(
    raw: &str,
    members: &[String],
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<String> {
    if !crate::occasion_bind::mentions_target(raw) {
        return Vec::new();
    }
    members
        .iter()
        .filter(|m| {
            decide_slot(&crate::occasion_bind::instantiate(raw, m), defs, ctx)
                == Some(Decided::Bool(false))
        })
        .cloned()
        .collect()
}

/// `occasion.target` is one of `members` other than `dead`.
pub(super) fn member_conjunct(members: &[String], dead: &[String]) -> (String, SolutionSet) {
    let live = members
        .iter()
        .filter(|m| !dead.contains(m))
        .map(|m| DomainValue::Str(m.clone()))
        .collect();
    (
        crate::beats::OCCASION_TARGET.to_string(),
        SolutionSet::Values(live),
    )
}

/// Whether a beat's or entry's `when` (`raw`) never holds, and why: decided
/// false as written (naming a finite clock's end when that is the cause),
/// or, in a kind or `for=` beat answering `members`, false for each of them.
pub(super) fn never_holds(
    raw: &str,
    members: Option<&[String]>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Option<Option<String>> {
    if decide_slot(raw, defs, ctx) == Some(Decided::Bool(false)) {
        return Some(crate::clock::false_reason(raw, defs, ctx));
    }
    let members = members.filter(|ms| !ms.is_empty())?;
    (super::dead_members(raw, members, defs, ctx).len() == members.len())
        .then(|| Some(for_every_member(members)))
}

/// Why a `when` false for each member its beat runs for never holds.
pub(crate) fn for_every_member(members: &[String]) -> String {
    let named: Vec<String> = members.iter().map(|m| format!("`{m}`")).collect();
    format!("for every member it runs for ({})", named.join(", "))
}
