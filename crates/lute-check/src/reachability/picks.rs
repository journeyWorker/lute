use std::collections::{BTreeMap, BTreeSet};

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_core_span::Diagnostic;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, AttrValue, Choice, ClipNode, Document, Node};

use crate::cel_expand::DefTable;
use crate::decide::{DecideCtx, Decided};
use crate::match_check::{CoverItem, DomainValue};

use super::context::{Assumption, Reach};
use super::{E_ARM_DEAD, W_OTHERWISE_DEAD};

pub(crate) fn member_arm_verdicts(
    doc: &Document,
    body: &[Node],
    raw: &str,
    members: &[String],
    dead: &[String],
    defs: &DefTable<'_>,
    def_types: &BTreeMap<String, Type>,
    schema: &crate::meta::StateSchema,
) -> Vec<Diagnostic> {
    let assume = Assumption {
        raw: raw.trim().to_string(),
        conjuncts: vec![super::context::member_conjunct(members, dead)],
        present: Default::default(),
    };
    let targets = crate::next_labels::next_targets(doc);
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    let rx = Reach {
        def_types,
        assume: Some(&assume),
        targets: &targets,
        picks: &[],
        // A kind or `for=` beat's body: a scene body, as in the per-file walk.
        once: true,
    };
    let mut diags = Vec::new();
    super::walk::walk_reach(body, defs, &rx, &ctx, &mut diags);
    diags.retain(|d| d.code == E_ARM_DEAD || d.code == W_OTHERWISE_DEAD);
    diags
}

/// dsl 0.28.0 (T1-23): a menu's pick record inside the arm of the option it
/// records. The engine writes `scene.choices.<menu>` — and, for a hub,
/// `scene.visited.<hub>.<option>` — when the option is picked, before its
/// arm runs, and nothing unsets either there: inside that arm the record
/// holds exactly this value.
#[derive(Clone, Debug)]
pub(crate) struct Pick {
    path: String,
    value: DomainValue,
    /// Why the record holds `value` here, for the message.
    because: PickWhy,
}

/// Where a [`Pick`] holds and why.
#[derive(Clone, Debug)]
enum PickWhy {
    /// In its option's own arm: "the pick record" / "the visit record".
    Arm(&'static str),
    /// In the hub's `<return>`: the reason after the dash.
    Return(String),
}

impl Pick {
    /// What option `choice` of `menu` (`hub`: a `<hub>`) records in its own
    /// arm, added to the `outer` picks — just `outer` when a `::jump` in
    /// `targets` can jump into the arm, which reaches it without the pick.
    pub(crate) fn with(
        outer: &[Pick],
        menu: &str,
        choice: &Choice,
        hub: bool,
        targets: &BTreeSet<String>,
    ) -> Vec<Pick> {
        let mut out = outer.to_vec();
        let jumped_into = choice
            .body
            .iter()
            .any(|n| crate::next_labels::holds_label(n, targets));
        if menu.is_empty() || choice.id.is_empty() || jumped_into {
            return out;
        }
        out.push(Pick {
            path: format!("scene.choices.{menu}"),
            value: DomainValue::Str(choice.id.clone()),
            because: PickWhy::Arm("the pick record"),
        });
        if hub {
            out.push(Pick {
                path: format!("scene.visited.{menu}.{}", choice.id),
                value: DomainValue::Bool(true),
                because: PickWhy::Arm("the visit record"),
            });
        }
        out
    }

    /// What hub `hub` (id `id`) records inside its `<return>` block, added to
    /// `outer` (ledger LG28-17). `<return>` runs only after a non-`exit`
    /// option, so with one such option its pick and visit records hold it.
    /// `once`: this hub is entered at most once per scene play and no
    /// `::jump` can come back into it — then no `exit` option has been
    /// visited yet, as taking one leaves the hub for good. Nothing when a
    /// `::jump` can jump into a non-`exit` arm or the block itself.
    pub(crate) fn on_return(
        outer: &[Pick],
        id: &str,
        hub: &lute_syntax::ast::Hub,
        targets: &BTreeSet<String>,
        once: bool,
    ) -> Vec<Pick> {
        let mut out = outer.to_vec();
        // `None`: an `exit` value that is no flag (its own `E-FLAG-VALUE`).
        let exit = |c: &Choice| {
            c.attrs
                .iter()
                .find(|a| a.key == "exit")
                .map_or(Some(false), |a| a.value.flag())
        };
        let jumpable = |body: &[Node]| {
            body.iter()
                .any(|n| crate::next_labels::holds_label(n, targets))
        };
        let stays: Vec<&Choice> = hub
            .choices
            .iter()
            .filter(|c| exit(c) == Some(false))
            .collect();
        if id.is_empty()
            || hub.choices.iter().any(|c| exit(c).is_none())
            || hub.on_return.as_ref().is_some_and(|r| jumpable(&r.body))
            || stays.iter().any(|c| jumpable(&c.body))
        {
            return out;
        }
        if let [only] = stays[..] {
            if !only.id.is_empty() {
                let because = PickWhy::Return(format!(
                    "`{}` is its only option that is not `exit`, and `<return>` runs only after \
                     one of those",
                    only.id
                ));
                out.push(Pick {
                    path: format!("scene.choices.{id}"),
                    value: DomainValue::Str(only.id.clone()),
                    because: because.clone(),
                });
                out.push(Pick {
                    path: format!("scene.visited.{id}.{}", only.id),
                    value: DomainValue::Bool(true),
                    because,
                });
            }
        }
        if once && targets.is_empty() {
            for gone in hub
                .choices
                .iter()
                .filter(|c| exit(c) == Some(true) && !c.id.is_empty())
            {
                out.push(Pick {
                    path: format!("scene.visited.{id}.{}", gone.id),
                    value: DomainValue::Bool(false),
                    because: PickWhy::Return(format!(
                        "`{}` is an `exit` option, and `<return>` never runs after one",
                        gone.id
                    )),
                });
            }
        }
        out
    }

    /// The pick among `picks` that rules `item` out as the value of `path`.
    pub(crate) fn ruling_out<'p>(
        picks: &'p [Pick],
        path: &str,
        item: &CoverItem,
    ) -> Option<&'p Pick> {
        picks.iter().find(|p| {
            p.path == path
                && match item {
                    CoverItem::Value(v) => *v != p.value,
                    CoverItem::Unset | CoverItem::Num(_) => true,
                }
        })
    }

    /// `raw` decided with each pick's path read as its value: the pick that
    /// makes it provably false, when it is not false without them.
    pub(crate) fn deciding_false<'p>(
        picks: &'p [Pick],
        raw: &str,
        defs: &DefTable<'_>,
        ctx: &DecideCtx<'_>,
    ) -> Option<&'p Pick> {
        if picks.is_empty() || raw.trim().is_empty() {
            return None;
        }
        let mut stack = Vec::new();
        let expanded = crate::cel_expand::expand_cel(raw, defs, Some("$"), &mut stack)
            .unwrap_or_else(|_| raw.to_string());
        let mut arena = lute_cel::CelArena::default();
        let handle = lute_cel::parse_slot_marked_refs(&mut arena, &expanded)?;
        let mut tree = arena.get(handle)?.clone();
        let mut used = Vec::new();
        pin_picks(&mut tree, picks, &mut used);
        let first = *used.first()?;
        matches!(
            crate::decide::decide(&tree.expr, ctx),
            Some(Decided::Bool(false))
        )
        .then(|| &picks[first])
    }

    /// Why the pick decides: "`scene.visited.lamp.ledger` is `true` in its
    /// option's own arm — the visit record is set when the choice is
    /// picked, before its arm runs".
    pub(crate) fn why(&self) -> String {
        let value = match &self.value {
            DomainValue::Str(s) => format!("'{s}'"),
            DomainValue::Bool(b) => b.to_string(),
        };
        match &self.because {
            PickWhy::Arm(record) => format!(
                "`{}` is `{value}` in its option's own arm — {record} is set when the choice is \
                 picked, before its arm runs",
                self.path
            ),
            PickWhy::Return(why) => {
                format!(
                    "`{}` is `{value}` in the hub's `<return>` — {why}",
                    self.path
                )
            }
        }
    }
}

/// Replace every read of a pick's path in `e` by the pick's value, noting
/// which picks were read in `used` (a `has()` test is left alone).
fn pin_picks(e: &mut cel_parser::ast::IdedExpr, picks: &[Pick], used: &mut Vec<usize>) {
    use cel_parser::ast::EntryExpr;
    if let Expr::Select(sel) = &e.expr {
        if !sel.test {
            let at = crate::cel_paths::select_path(&e.expr)
                .and_then(|p| picks.iter().position(|pk| pk.path == p));
            if let Some(i) = at {
                e.expr = Expr::Literal(match &picks[i].value {
                    DomainValue::Bool(b) => Val::Boolean(*b),
                    DomainValue::Str(s) => Val::String(s.clone().into()),
                });
                used.push(i);
                return;
            }
        }
    }
    match &mut e.expr {
        Expr::Call(c) => {
            if let Some(t) = &mut c.target {
                pin_picks(t, picks, used);
            }
            for a in &mut c.args {
                pin_picks(a, picks, used);
            }
        }
        Expr::List(l) => l
            .elements
            .iter_mut()
            .for_each(|x| pin_picks(x, picks, used)),
        Expr::Map(m) => m.entries.iter_mut().for_each(|x| match &mut x.expr {
            EntryExpr::MapEntry(me) => {
                pin_picks(&mut me.key, picks, used);
                pin_picks(&mut me.value, picks, used);
            }
            EntryExpr::StructField(f) => pin_picks(&mut f.value, picks, used),
        }),
        Expr::Select(sel) => pin_picks(&mut sel.operand, picks, used),
        _ => {}
    }
}

/// Every state path a `::set` / `<choice into>` in `nodes` writes (at any
/// depth), into `out`; `true` when the body also runs something whose writes
/// are not spelled out here — a `::use` or a directive that writes state.
pub(super) fn scan_writes(nodes: &[Node], snapshot: &CapabilitySnapshot, out: &mut BTreeSet<String>) -> bool {
    let opaque_directive =
        |tag: &str| tag == "use" || crate::check::directive_writes_state(snapshot, tag);
    let mut opaque = false;
    for node in nodes {
        match node {
            Node::Set(s) => {
                out.insert(s.path.clone());
            }
            Node::Directive(d) => opaque |= opaque_directive(&d.tag),
            Node::Branch(b) => {
                for c in &b.choices {
                    opaque |= scan_choice_writes(c, snapshot, out);
                }
            }
            Node::Hub(h) => {
                for c in &h.choices {
                    opaque |= scan_choice_writes(c, snapshot, out);
                }
                if let Some(r) = &h.on_return {
                    opaque |= scan_writes(&r.body, snapshot, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    opaque |= scan_writes(body, snapshot, out);
                }
            }
            Node::On(o) => opaque |= scan_writes(&o.body, snapshot, out),
            Node::Objective(o) => opaque |= scan_writes(&o.body, snapshot, out),
            Node::Timeline(tl) => {
                for clip in tl.tracks.iter().flat_map(|t| &t.clips) {
                    match &clip.node {
                        ClipNode::Set(s) => {
                            out.insert(s.path.clone());
                        }
                        ClipNode::Directive(d) => opaque |= opaque_directive(&d.tag),
                    }
                }
            }
            Node::Line(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
    opaque
}

fn scan_choice_writes(
    choice: &Choice,
    snapshot: &CapabilitySnapshot,
    out: &mut BTreeSet<String>,
) -> bool {
    if let Some(AttrValue::Str(into)) = choice
        .attrs
        .iter()
        .find(|a| a.key == "into")
        .map(|a| &a.value)
    {
        out.insert(into.clone());
    }
    scan_writes(&choice.body, snapshot, out)
}

