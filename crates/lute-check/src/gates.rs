//! dsl 0.27.0 §4 (T2-3, T2-4): the engine seam's declared conditions.
//!
//! - **Gates.** An occasion MAY declare `raisedWhen: "<condition>"`: the
//!   engine raises it only while the condition holds (it may read
//!   `occasion.target`, the member the occasion is raised for). A beat
//!   answering the occasion can only be presented while the gate holds, so
//!   the checker judges the beat's `when` together with it.
//! - **Terminal state.** A schema MAY declare `terminal: "<condition>"`:
//!   once it holds the engine raises no occasion, so every beat is judged
//!   under `!terminal`.
//!
//! This module turns a beat (its occasion, target and `when`) into the
//! conditions the reachability passes decide — one per member when
//! `occasion.target` has to be ground ([`crate::occasion_bind`]) — and
//! words their verdicts. The runtime twin is `lute-trace`'s session, which
//! refuses a `lute play` step raising a gated occasion while the gate is
//! false, or any raise once the terminal condition holds
//! ([`E_OCCASION_GATE`]).

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::{OccasionDecl, OccasionTarget};

use crate::occasion_bind::{instantiate, mentions_target};

/// `E-OCCASION-GATE` (dsl 0.27.0 §4): a `lute play` step raises an occasion
/// the engine would not raise — its `raisedWhen` gate is false, or the
/// project's `terminal:` condition holds.
pub const E_OCCASION_GATE: &str = "E-OCCASION-GATE";

/// The project's `terminal:` from every declaration (in order, duplicates
/// once): one condition as written, several joined by `||` — the game is
/// over when any of them holds.
pub fn combine_terminal<'a>(parts: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut seen: Vec<&str> = Vec::new();
    for p in parts {
        let p = p.trim();
        if !p.is_empty() && !seen.contains(&p) {
            seen.push(p);
        }
    }
    match seen.as_slice() {
        [] => None,
        [one] => Some(one.to_string()),
        many => Some(
            many.iter()
                .map(|p| format!("({p})"))
                .collect::<Vec<_>>()
                .join(" || "),
        ),
    }
}

/// Occasion `on`'s declared gate, trimmed; `None` without one.
pub fn gate_of<'o>(occasions: &'o BTreeMap<String, OccasionDecl>, on: &str) -> Option<&'o str> {
    occasions
        .get(on)?
        .raised_when
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
}

/// The member a concrete `target` (`room.office`) names on a domain-target
/// occasion (`office`); `None` for a shape-only occasion or another prefix.
pub fn target_member(decl: &OccasionDecl, target: &str) -> Option<String> {
    let OccasionTarget::Domain { prefix, .. } = &decl.target else {
        return None;
    };
    target
        .strip_prefix(prefix.as_str())?
        .strip_prefix('.')
        .filter(|m| !m.is_empty())
        .map(str::to_string)
}

/// Every member a domain-target occasion is raised for (its `members`
/// subset, else its closed kind's members); `None` otherwise.
pub fn domain_members(
    decl: &OccasionDecl,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Option<Vec<String>> {
    let OccasionTarget::Domain { entity, .. } = &decl.target else {
        return None;
    };
    let kind_members = match kinds.get(entity).map(|k| &k.shape) {
        Some(KindShape::Members(ms)) => Some(ms.as_slice()),
        _ => None,
    };
    decl.target
        .domain_members(kind_members)
        .map(<[String]>::to_vec)
}

/// The members `occasion.target` ranges over for a beat answering `decl`
/// with `target`: the one member a concrete target names, a kind target's
/// members, or — untargeted — every member the occasion is raised for.
pub fn beat_members(
    decl: &OccasionDecl,
    target: Option<&str>,
    kinds: &BTreeMap<String, EntityKindDecl>,
) -> Option<Vec<String>> {
    match target {
        Some(t) => match crate::lore::kind_target(t) {
            Some(kind) => crate::beats::kind_target_members(decl, kind, kinds)
                .ok()
                .map(|(_, ms)| ms),
            None => target_member(decl, t).map(|m| vec![m]),
        },
        None => domain_members(decl, kinds),
    }
}

/// What a beat's eligibility is judged under (dsl 0.27.0 §4): its
/// occasion's gate and the project's terminal condition, conjoined with
/// its own `when`. Each list holds one condition per member `occasion.target`
/// ranges over (one when nothing reads it); the beat is dead when every one
/// of them is provably false.
#[derive(Clone, Debug)]
pub struct BeatSeam {
    /// The occasion's declared gate, as written.
    pub gate: Option<String>,
    /// The project's terminal condition, as combined.
    pub terminal: Option<String>,
    /// The gate alone, per member (empty without a gate).
    pub gate_conds: Vec<String>,
    /// `gate && !terminal && when`, per member.
    pub conds: Vec<String>,
}

/// The [`BeatSeam`] of a beat answering `on` (for `target`) with `when`,
/// `None` when neither a gate nor a terminal condition applies, or when
/// `occasion.target` must be ground and its members are not known.
pub fn beat_seam(
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
    terminal: Option<&str>,
    on: &str,
    target: Option<&str>,
    when: Option<&str>,
) -> Option<BeatSeam> {
    seam_with(
        gate_of(occasions, on),
        occasions.get(on),
        terminal,
        occasions
            .get(on)
            .and_then(|d| beat_members(d, target, kinds)),
        when,
    )
}

/// [`beat_seam`] in `folded`'s project, leaving out a gate or terminal
/// text that compares a subject with a string outside its domain: that
/// text's own `E-WHEN-LITERAL-DOMAIN` ([`check_seam_texts`]) owns the
/// fault, and every beat judged under it would only repeat it as a dead
/// beat (HW27-02).
pub(crate) fn folded_seam(
    folded: &crate::check::FoldedEnv,
    on: &str,
    target: Option<&str>,
    when: Option<&str>,
) -> Option<BeatSeam> {
    let members = folded
        .occasions
        .get(on)
        .and_then(|d| beat_members(d, target, &folded.env.rel_vocab.kinds));
    folded_seam_over(folded, on, members, when)
}

/// [`folded_seam`] with `occasion.target` ranging over `members` — a kind
/// or `for=` beat's own, when known.
fn folded_seam_over(
    folded: &crate::check::FoldedEnv,
    on: &str,
    members: Option<Vec<String>>,
    when: Option<&str>,
) -> Option<BeatSeam> {
    let gate = gate_of(&folded.occasions, on).filter(|g| gate_hits(folded, on, g).is_empty());
    let terminal = folded
        .env
        .terminal
        .as_deref()
        .filter(|t| literal_hits(t, &folded_defs(folded), &folded.env.state).is_empty());
    seam_with(gate, folded.occasions.get(on), terminal, members, when)
}

/// `members`: what `occasion.target` ranges over for the beat, when known.
fn seam_with(
    gate: Option<&str>,
    decl: Option<&OccasionDecl>,
    terminal: Option<&str>,
    members: Option<Vec<String>>,
    when: Option<&str>,
) -> Option<BeatSeam> {
    // dsl 0.28.0 (T2-9): an `outsideRun` occasion is raised after the game
    // is over too — its beats are not judged under `!terminal`.
    let terminal = terminal
        .map(str::trim)
        .filter(|t| !t.is_empty() && !decl.is_some_and(|d| d.outside_run));
    if gate.is_none() && terminal.is_none() {
        return None;
    }
    let when = when.map(str::trim).filter(|w| !w.is_empty());
    let reads_target = gate.is_some_and(mentions_target) || when.is_some_and(mentions_target);
    let members: Vec<Option<String>> = if reads_target {
        let ms = members?;
        if ms.is_empty() {
            return None;
        }
        ms.into_iter().map(Some).collect()
    } else {
        vec![None]
    };
    let ground = |cel: &str, m: &Option<String>| match m {
        Some(m) => instantiate(cel, m),
        None => cel.to_string(),
    };
    let mut gate_conds = Vec::new();
    let mut conds = Vec::new();
    for m in &members {
        let mut parts = Vec::new();
        if let Some(g) = gate {
            let g = ground(g, m);
            gate_conds.push(g.clone());
            parts.push(format!("({g})"));
        }
        if let Some(t) = terminal {
            parts.push(format!("!({t})"));
        }
        if let Some(w) = when {
            parts.push(format!("({})", ground(w, m)));
        }
        conds.push(parts.join(" && "));
    }
    Some(BeatSeam {
        gate: gate.map(str::to_string),
        terminal: terminal.map(str::to_string),
        gate_conds,
        conds,
    })
}

/// How a [`BeatSeam`] decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamVerdict {
    /// The gate alone is false for every member: the occasion is never
    /// raised for this beat.
    GateDead,
    /// `gate && !terminal && when` is false for every member.
    Dead,
    /// Possibly eligible.
    Live,
}

impl BeatSeam {
    /// Judge the seam, `dead(cond)` answering whether one condition is
    /// provably false.
    pub fn judge(&self, mut dead: impl FnMut(&str) -> bool) -> SeamVerdict {
        if !self.gate_conds.is_empty() && self.gate_conds.iter().all(|c| dead(c)) {
            SeamVerdict::GateDead
        } else if self.conds.iter().all(|c| dead(c)) {
            SeamVerdict::Dead
        } else {
            SeamVerdict::Live
        }
    }

    /// The message of a dead seam for `beat` (`scene `x``, `entry `y``)
    /// answering `on` with `when`; `reasons` are the fact envelope's.
    pub fn message(
        &self,
        beat: &str,
        on: &str,
        when: Option<&str>,
        verdict: SeamVerdict,
        reasons: Option<&str>,
    ) -> String {
        let because = reasons
            .filter(|r| !r.is_empty())
            .map_or_else(String::new, |r| format!(" — {r}"));
        match (verdict, &self.gate) {
            (SeamVerdict::GateDead, Some(gate)) => format!(
                "{beat} is never eligible: occasion `{on}` is raised only when `{gate}` (its \
                 `raisedWhen`), which is provably false for this beat{because} (dsl 0.27.0 §4)"
            ),
            _ => {
                let mut under = Vec::new();
                if let Some(gate) = &self.gate {
                    under.push(format!("occasion `{on}`'s gate `raisedWhen: {gate}` holds"));
                }
                if let Some(t) = &self.terminal {
                    under.push(format!("the project's `terminal: {t}` does not hold yet"));
                }
                let what = match when {
                    Some(w) => format!("its `when` `{}` is provably false", w.trim()),
                    None => "it can never be presented".to_string(),
                };
                format!(
                    "{beat} is never eligible: {what} whenever {} — the engine raises no \
                     occasion otherwise{because} (dsl 0.27.0 §4)",
                    under.join(" and ")
                )
            }
        }
    }
}

/// Whether `cond` is provably `false` in `folded`'s document — its `@def`s
/// expanded, its state schema's domains (a finite clock's range included),
/// and, with `facts`, the root's fact envelope at that path and slot.
pub fn provably_false(
    cond: &str,
    folded: &crate::check::FoldedEnv,
    facts: Option<(&crate::FactEnv, &std::path::Path, Span)>,
) -> bool {
    use crate::decide::{decide_slot, DecideCtx, Decided};
    let params = BTreeMap::new();
    let defs = crate::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let ctx = DecideCtx {
        schema: &folded.env.state,
        dollar: None,
        params: &params,
        facts: facts.map(|(env, path, span)| crate::fact_env::FactScope {
            env,
            vocab: &folded.env.rel_vocab,
            path,
            span,
            wip: false,
        }),
    };
    matches!(decide_slot(cond, &defs, &ctx), Some(Decided::Bool(false)))
}

/// dsl 0.27.0 §4: whether a beat answering `on` (for `target`) with `when`
/// (as written) can never be presented — its `when` alone, or judged under
/// the occasion's gate and the project's `!terminal`, is provably false —
/// `dead` answering whether one condition is.
pub fn beat_never_eligible(
    folded: &crate::check::FoldedEnv,
    on: &str,
    target: Option<&str>,
    when: Option<&str>,
    mut dead: impl FnMut(&str) -> bool,
) -> bool {
    if when.is_some_and(|w| !w.trim().is_empty() && dead(w)) {
        return true;
    }
    folded_seam(folded, on, target, when).is_some_and(|s| s.judge(dead) != SeamVerdict::Live)
}

/// dsl 0.27.0 §4 (`lute beats`): whether occasion `on`'s `raisedWhen` gate
/// can never hold when it is raised for `target` (`None`: without one — then
/// for none of the members it is raised for) — `dead` answering whether one
/// condition is provably false. `false` without a gate.
pub fn gate_never_holds(
    occasions: &BTreeMap<String, OccasionDecl>,
    kinds: &BTreeMap<String, EntityKindDecl>,
    on: &str,
    target: Option<&str>,
    dead: impl FnMut(&str) -> bool,
) -> bool {
    beat_seam(occasions, kinds, None, on, target, None)
        .is_some_and(|s| s.judge(dead) == SeamVerdict::GateDead)
}

/// The seam's CEL checked like any condition slot (dsl 0.27.0 §4): the
/// gate of every occasion a beat of this document answers — ground with
/// its first member when it reads `occasion.target` — reported at the
/// plugin's `raisedWhen:` line when the CLI placed it (`check-project` folds
/// the importers' identical reports into one), else at that beat's `on`;
/// the document's own `terminal:` at its value; an imported schema's
/// `terminal:` at the schema's line. A string literal no member of its
/// subject's domain is `E-WHEN-LITERAL-DOMAIN`, as in a `when`.
pub(crate) fn check_seam_texts(
    doc: &lute_syntax::ast::Document,
    folded: &crate::check::FoldedEnv,
    imports: &crate::SchemaImports,
    ctx: &crate::ctx::Ctx<'_>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for b in seam_beats(doc, folded) {
        if seen.contains(&b.on) {
            continue;
        }
        seen.push(b.on.clone());
        let Some(gate) = gate_of(&folded.occasions, &b.on) else {
            continue;
        };
        let origin = imports.plugin_origins.gates.get(&b.on);
        if mentions_target(gate)
            && folded
                .occasions
                .get(&b.on)
                .is_some_and(|d| d.target == OccasionTarget::Shape(false))
        {
            out.push(crate::rel_schema::at_plugin_origin(
                untargeted_gate_diag(&b.on, gate, b.on_span),
                origin,
            ));
            continue;
        }
        let Some(ground) = ground_gate(folded, &b.on, gate) else {
            // A shape-only or open target: nothing ground to check by.
            continue;
        };
        let mut found = check_condition(&ground, b.on_span, ctx);
        found.extend(gate_literals(folded, &b.on, gate, b.on_span));
        out.extend(found.into_iter().map(|d| {
            crate::rel_schema::at_plugin_origin(gate_diag(&b.on, gate, d, b.on_span), origin)
        }));
    }
    if let Some(own) = &folded.typed.terminal {
        let found = outside_occasion(&own.raw, own.span).map_or_else(
            || {
                let mut found = check_condition(&own.raw, own.span, ctx);
                found.extend(foreign_literals(&own.raw, folded, Some(&own.raw), own.span));
                found
            },
            |d| vec![d],
        );
        out.extend(
            found
                .into_iter()
                .map(|d| terminal_diag(&own.raw, d, own.span)),
        );
        out.extend(terminal_persistent(&own.raw, doc, folded, own.span));
    }
    for (file, raw, span) in &imports.terminal {
        let origin = crate::rel_schema::DeclOrigin {
            file: file.clone(),
            span: *span,
        };
        let found = outside_occasion(raw, doc.meta.span).map_or_else(
            || {
                let mut found = check_condition(raw, doc.meta.span, ctx);
                found.extend(foreign_literals(raw, folded, None, doc.meta.span));
                found
            },
            |d| vec![d],
        );
        out.extend(found.into_iter().map(|d| {
            crate::rel_schema::at_origin(terminal_diag(raw, d, doc.meta.span), Some(&origin))
        }));
        out.extend(
            terminal_persistent(raw, doc, folded, doc.meta.span)
                .map(|d| crate::rel_schema::at_origin(d, Some(&origin))),
        );
    }
    out
}

/// dsl 0.28.0 (T3-19): a `terminal:` reading state a new run keeps — once
/// it holds, it holds in every later run, so no new run can play on.
pub const W_TERMINAL_PERSISTENT: &str = "W-TERMINAL-PERSISTENT";

/// [`W_TERMINAL_PERSISTENT`] for terminal condition `raw` (its `@def`s
/// expanded), at `at`: the quests this document declares and the relations
/// it sees decide which of their reads a new run keeps.
fn terminal_persistent(
    raw: &str,
    doc: &lute_syntax::ast::Document,
    folded: &crate::check::FoldedEnv,
    at: Span,
) -> Option<Diagnostic> {
    let defs = crate::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let expanded = crate::cel_expand::expand_cel(raw, &defs, None, &mut Vec::new())
        .unwrap_or_else(|_| raw.to_string());
    let quest_kept = |id: &str| {
        doc.quests.iter().find(|q| q.id == id).map(|q| {
            !q.tier
                .as_ref()
                .is_some_and(|(t, _)| t == "run" || t.starts_with("season:"))
        })
    };
    let relation_kept = |name: &str| {
        folded
            .env
            .rel_vocab
            .relations
            .get(name)
            .map(|r| matches!(r.tier.as_deref(), Some("user" | "app")))
    };
    let reads = persistent_reads(&expanded, &quest_kept, &relation_kept);
    let first = reads.first()?;
    Some(Diagnostic {
        code: W_TERMINAL_PERSISTENT.to_string(),
        severity: Severity::Warning,
        message: format!(
            "`terminal:` reads `{first}`, which a new run keeps — once it holds, no new run can \
             play on; end the game on run state (`run.*`, a run-tier quest or relation), or \
             declare what a new run should forget as run-tier"
        ),
        span: at,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    })
}

/// dsl 0.28.0 (T3-19): the reads of condition `raw` a new run keeps, each
/// once, in reading order, as a message shows them — `visited('h')`,
/// `user.*`, `app.*`, `entry.<id>.everRead`, and a quest's `completed(…)` /
/// `active(…)` / `quest.<id>.*` or a relation's `holds(…)` / `count(…)` when
/// `quest_kept` / `relation_kept` say it survives a new run (`None`:
/// unknown, not named). Shared by the checker ([`W_TERMINAL_PERSISTENT`])
/// and the runtime's refusal after the game is over.
pub fn persistent_reads(
    raw: &str,
    quest_kept: &dyn Fn(&str) -> Option<bool>,
    relation_kept: &dyn Fn(&str) -> Option<bool>,
) -> Vec<String> {
    use cel_parser::ast::Expr;
    fn walk(
        e: &Expr,
        quest_kept: &dyn Fn(&str) -> Option<bool>,
        relation_kept: &dyn Fn(&str) -> Option<bool>,
        out: &mut Vec<String>,
    ) {
        let mut keep = |text: String| {
            if !out.contains(&text) {
                out.push(text);
            }
        };
        match e {
            Expr::Ident(_) | Expr::Select(_) => {
                let Some(p) = crate::cel_paths::select_path(e) else {
                    return;
                };
                let kept = p.starts_with("user.")
                    || p.starts_with("app.")
                    || crate::cel_paths::is_entry_ever_read(&p)
                    || p.strip_prefix("quest.")
                        .and_then(|r| r.split('.').next())
                        .is_some_and(|id| quest_kept(id) == Some(true));
                if kept {
                    keep(p);
                }
            }
            Expr::Call(c) if c.target.is_none() => {
                let lit = || match c.args.first().map(|a| &a.expr) {
                    Some(Expr::Literal(cel_parser::reference::Val::String(s))) => {
                        Some(s.to_string())
                    }
                    _ => None,
                };
                let kept = match c.func_name.as_str() {
                    "visited" => lit().is_some(),
                    "completed" | "active" => lit().is_some_and(|q| quest_kept(&q) == Some(true)),
                    "holds" | "count" | "countDistinct" => match c.args.first().map(|a| &a.expr) {
                        Some(Expr::Call(atom)) => relation_kept(&atom.func_name) == Some(true),
                        _ => false,
                    },
                    _ => {
                        for a in &c.args {
                            walk(&a.expr, quest_kept, relation_kept, out);
                        }
                        return;
                    }
                };
                if kept {
                    keep(crate::cel_types::show(e));
                }
            }
            Expr::Call(c) => {
                for a in c.target.iter().map(|t| &**t).chain(&c.args) {
                    walk(&a.expr, quest_kept, relation_kept, out);
                }
            }
            Expr::List(l) => {
                for x in &l.elements {
                    walk(&x.expr, quest_kept, relation_kept, out);
                }
            }
            _ => {}
        }
    }
    let mut arena = lute_cel::CelArena::default();
    let Ok(handle) = lute_cel::parse_slot(&mut arena, raw, 0) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(root) = arena.get(handle) {
        walk(&root.expr, quest_kept, relation_kept, &mut out);
    }
    out
}

/// dsl 0.28.0 (T1-5): a condition judged outside any occasion — the
/// project's `terminal:`, a season's `live:` — reading `occasion.*`, which
/// has a value only while a beat answers its occasion. One report, at `at`,
/// the same in every document (a kind beat elsewhere in the importing
/// document gives it no value here).
pub(crate) fn outside_occasion(raw: &str, at: Span) -> Option<Diagnostic> {
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot(&mut arena, raw, 0).ok()?;
    let root = arena.get(handle)?;
    let path = crate::cel_paths::collect_path_uses(&root.expr)
        .into_iter()
        .map(|u| u.path)
        .find(|p| p.split('.').next() == Some("occasion"))?;
    Some(Diagnostic {
        code: "E-UNDECLARED".to_string(),
        severity: Severity::Error,
        message: format!(
            "it is judged between steps, outside any occasion, so it cannot read `{path}` — \
             `occasion.*` has a value only while a beat answers its occasion; judge the member \
             in that beat's `when` (dsl 0.28.0 §1)"
        ),
        span: at,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    })
}

/// dsl 0.28.0 §1 (T1-5): the literal faults of occasion `on`'s `gate`, with
/// `occasion.target` typed as the members the occasion is raised for — a
/// typo (`'vilage'`) or a prefixed member (`'place.village'`) is a literal
/// no member equals, and the gate would hold (or fail) for every member.
fn gate_hits(
    folded: &crate::check::FoldedEnv,
    on: &str,
    gate: &str,
) -> Vec<crate::decide::LiteralCmpHit> {
    let members = folded
        .occasions
        .get(on)
        .filter(|_| mentions_target(gate))
        .and_then(|d| domain_members(d, &folded.env.rel_vocab.kinds));
    let mut schema;
    let schema = match members {
        Some(members) => {
            schema = folded.env.state.clone();
            schema.decls.insert(
                crate::beats::OCCASION_TARGET.to_string(),
                crate::meta::StateDecl {
                    ty: lute_manifest::types::Type::Enum(members),
                    default: None,
                    namespace: crate::meta::Namespace::Scene,
                    owner: Some(lute_manifest::types::Owner::Engine),
                },
            );
            &schema
        }
        None => &folded.env.state,
    };
    literal_hits(gate, &folded_defs(folded), schema)
}

/// [`gate_hits`] as diagnostics, at `at`.
fn gate_literals(
    folded: &crate::check::FoldedEnv,
    on: &str,
    gate: &str,
    at: Span,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    crate::reachability::push_literal_cmp_diags(&mut out, &gate_hits(folded, on, gate), None, at);
    out
}

/// Occasion `on`'s `gate` as the checker judges its text: ground with the
/// first member it is raised for when it reads `occasion.target`; `None`
/// when that member is not known (a shape-only or open target).
fn ground_gate(folded: &crate::check::FoldedEnv, on: &str, gate: &str) -> Option<String> {
    if !mentions_target(gate) {
        return Some(gate.to_string());
    }
    let decl = folded.occasions.get(on)?;
    let first = domain_members(decl, &folded.env.rel_vocab.kinds)?
        .into_iter()
        .next()?;
    Some(instantiate(gate, &first))
}

/// `E-WHEN-LITERAL-DOMAIN` / `E-UNSET-LITERAL` for each comparison of a
/// finite-domain subject with a string outside its domain in `raw`, at `at`
/// — the literal check every `when` gets. `text` is the slot's text when it
/// is written at `at`, so each diagnostic can point at its literal.
fn foreign_literals(
    raw: &str,
    folded: &crate::check::FoldedEnv,
    text: Option<&str>,
    at: Span,
) -> Vec<Diagnostic> {
    condition_literals(raw, &folded_defs(folded), &folded.env.state, text, at)
}

fn folded_defs(folded: &crate::check::FoldedEnv) -> crate::cel_expand::DefTable<'_> {
    crate::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    }
}

/// [`foreign_literals`] over any def table and schema — the literal check
/// every condition slot judged outside the document tree gets (a gate, the
/// terminal condition, a season's `live:`, a rule guard).
pub(crate) fn condition_literals(
    raw: &str,
    defs: &crate::cel_expand::DefTable<'_>,
    schema: &crate::meta::StateSchema,
    text: Option<&str>,
    at: Span,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    crate::reachability::push_literal_cmp_diags(
        &mut out,
        &literal_hits(raw, defs, schema),
        text,
        at,
    );
    out
}

fn literal_hits(
    raw: &str,
    defs: &crate::cel_expand::DefTable<'_>,
    schema: &crate::meta::StateSchema,
) -> Vec<crate::decide::LiteralCmpHit> {
    let params = BTreeMap::new();
    let ctx = crate::decide::DecideCtx {
        schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    crate::decide::analyze_literal_comparisons(raw, defs, &ctx).hits
}

/// `raw` checked as a `Bool` condition slot; every diagnostic anchored at
/// `at` (the text is not the document's own).
pub(crate) fn check_condition(raw: &str, at: Span, ctx: &crate::ctx::Ctx<'_>) -> Vec<Diagnostic> {
    let mut arena = lute_cel::CelArena::default();
    let mut slot =
        lute_syntax::ast::CelSlot::raw(lute_syntax::ast::CelKind::Condition, raw.to_string(), at);
    match lute_cel::parse_slot(&mut arena, raw, 0) {
        Ok(handle) => slot.ast = Some(handle),
        Err(err) => {
            return vec![Diagnostic {
                code: "E-CEL-PARSE".to_string(),
                severity: Severity::Error,
                message: err.message,
                span: at,
                layer: Layer::Cel,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }]
        }
    }
    let mut diags = crate::cel_resolve::check_cel_slot(
        &slot,
        &arena,
        ctx,
        Some(&crate::ctx::ExpectedType::Bool),
    );
    for d in &mut diags {
        d.span = at;
        d.fixits.clear();
    }
    diags
}

/// A problem found in the `terminal:` condition, named as its.
fn terminal_diag(raw: &str, mut d: Diagnostic, at: Span) -> Diagnostic {
    d.message = format!("`terminal: {raw}`: {} (dsl 0.27.0 §4)", d.message);
    d.span = at;
    d
}

/// One beat of a document, as the seam passes judge it.
pub(crate) struct SeamBeat<'d> {
    /// `beat `x`` / `entry `y`` — how the verdict names it.
    pub name: String,
    /// `E-BEAT-UNREACHABLE` (scene and bundle beats) or
    /// `E-ENTRY-UNREACHABLE` (entry beats, whose `when` is the entry's own).
    pub code: &'static str,
    pub on: String,
    pub target: Option<String>,
    pub when: Option<&'d lute_syntax::ast::CelSlot>,
    /// The `when` value, else the `on` value.
    pub span: Span,
    /// The `on` value.
    pub on_span: Span,
    /// The beat element (a scene beat: its `on` value) — where
    /// [`crate::check::FoldedEnv::env_at`] finds its environment.
    pub at: Span,
    /// dsl 0.28.0: the members a kind or `for=` beat runs for.
    pub members: Option<&'d [String]>,
}

/// Every beat `doc` declares: its scene beat, its entry beats and its
/// bundle beats.
pub(crate) fn seam_beats<'d>(
    doc: &'d lute_syntax::ast::Document,
    folded: &'d crate::check::FoldedEnv,
) -> Vec<SeamBeat<'d>> {
    let members_at = |at: Span| {
        folded
            .env
            .occasion_scopes
            .members_at(at.byte_start, at.byte_end)
    };
    let mut out = Vec::new();
    if let Some(b) = &folded.typed.beat {
        let on_span = crate::beats::top_value_span(&doc.meta, "on");
        out.push(SeamBeat {
            name: format!("beat `{}`", crate::beats::scene_beat_name(folded)),
            code: crate::beats::E_BEAT_UNREACHABLE,
            on: b.on.clone(),
            target: b.target.clone(),
            when: b.when.as_ref(),
            span: b.when.as_ref().map_or(on_span, |w| w.span),
            on_span,
            at: on_span,
            members: members_at(on_span),
        });
    }
    for e in &doc.entries {
        let Some((on, on_span)) = &e.on else {
            continue;
        };
        out.push(SeamBeat {
            name: format!("entry `{}`", e.id),
            code: crate::reachability::E_ENTRY_UNREACHABLE,
            on: on.clone(),
            target: e.target.as_ref().map(|(t, _)| t.clone()),
            when: e.when.as_ref(),
            span: e.when.as_ref().map_or(*on_span, |w| w.span),
            on_span: *on_span,
            at: e.span,
            members: members_at(e.span),
        });
    }
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    for b in &doc.beats {
        let Some((on, on_span)) = &b.on else {
            continue;
        };
        out.push(SeamBeat {
            name: format!("beat `{}`", crate::bundles::bundle_beat_key(doc_id, &b.id)),
            code: crate::beats::E_BEAT_UNREACHABLE,
            on: on.clone(),
            target: b.target.as_ref().map(|(t, _)| t.clone()),
            when: b.when.as_ref(),
            span: b.when.as_ref().map_or(*on_span, |w| w.span),
            on_span: *on_span,
            at: b.span,
            members: members_at(b.span),
        });
    }
    out
}

impl SeamBeat<'_> {
    /// The beat's [`BeatSeam`] in `folded`'s project — over its own members
    /// when it is a kind or `for=` beat.
    pub(crate) fn seam(&self, folded: &crate::check::FoldedEnv) -> Option<BeatSeam> {
        let when = self.when.map(|w| w.raw.as_str());
        match self.members {
            Some(ms) => folded_seam_over(folded, &self.on, Some(ms.to_vec()), when),
            None => folded_seam(folded, &self.on, self.target.as_deref(), when),
        }
    }
}

/// The per-file pass (no fact envelope): a beat whose `when`, judged under
/// its occasion's gate and `!terminal`, decides false. A `when` that is
/// already false alone is the ordinary unreachable verdict's, not this one.
pub(crate) fn seam_reachability(
    doc: &lute_syntax::ast::Document,
    folded: &crate::check::FoldedEnv,
    defs: &crate::cel_expand::DefTable<'_>,
    ctx: &crate::decide::DecideCtx<'_>,
) -> Vec<Diagnostic> {
    use crate::decide::{decide_slot, DecideCtx, Decided};
    let mut out = Vec::new();
    for b in seam_beats(doc, folded) {
        let Some(seam) = b.seam(folded) else {
            continue;
        };
        // dsl 0.28.0: over the beat's own environment.
        let own = DecideCtx {
            schema: &folded.env_at(b.at).state,
            dollar: None,
            params: ctx.params,
            facts: None,
        };
        let dead = |c: &str| matches!(decide_slot(c, defs, &own), Some(Decided::Bool(false)));
        if b.when.is_some_and(|w| dead(&w.raw)) {
            continue;
        }
        let verdict = seam.judge(dead);
        if verdict == SeamVerdict::Live {
            continue;
        }
        out.push(Diagnostic {
            code: b.code.to_string(),
            severity: Severity::Error,
            message: seam.message(
                &b.name,
                &b.on,
                b.when.map(|w| w.raw.as_str()),
                verdict,
                None,
            ),
            span: b.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }
    out
}

/// A problem found in a gate's CEL, re-anchored at the beat's `on` and
/// named as the gate's.
pub fn gate_diag(on: &str, gate: &str, mut d: Diagnostic, at: Span) -> Diagnostic {
    d.message = format!(
        "occasion `{on}`'s `raisedWhen: {gate}`: {} (dsl 0.27.0 §4)",
        d.message
    );
    d.span = at;
    d.fixits.clear();
    d
}

/// `occasion.target` in a gate of an occasion raised for no target.
pub fn untargeted_gate_diag(on: &str, gate: &str, at: Span) -> Diagnostic {
    Diagnostic {
        code: "E-BEAT-ATTR".to_string(),
        severity: Severity::Error,
        message: format!(
            "occasion `{on}`'s `raisedWhen: {gate}` reads `occasion.target`, but `{on}` is raised \
             for no target — declare its `target:` as `{{ prefix, entity }}` or drop the read \
             (dsl 0.27.0 §4)"
        ),
        span: at,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_manifest::relations::KindShape;

    fn occasions(gate: Option<&str>) -> BTreeMap<String, OccasionDecl> {
        let mut m = BTreeMap::new();
        m.insert(
            "enter".to_string(),
            OccasionDecl {
                name: "enter".into(),
                target: OccasionTarget::Domain {
                    prefix: "room".into(),
                    entity: "room".into(),
                    members: None,
                },
                raised_when: gate.map(str::to_string),
                ..Default::default()
            },
        );
        m
    }

    fn kinds() -> BTreeMap<String, EntityKindDecl> {
        let mut k = BTreeMap::new();
        k.insert(
            "room".to_string(),
            EntityKindDecl {
                shape: KindShape::Members(vec!["hall".into(), "office".into()]),
                subset_of: None,
                labels: Default::default(),
            },
        );
        k
    }

    #[test]
    fn terminal_declarations_join_by_or() {
        assert_eq!(combine_terminal([]), None);
        assert_eq!(
            combine_terminal(["@dead", " @dead "]).as_deref(),
            Some("@dead")
        );
        assert_eq!(combine_terminal(["a", "b"]).as_deref(), Some("(a) || (b)"));
    }

    #[test]
    fn a_concrete_target_grounds_the_gate() {
        let s = beat_seam(
            &occasions(Some("holds(canEnter(occasion.target))")),
            &kinds(),
            Some("@dead"),
            "enter",
            Some("room.office"),
            Some("run.n > 1"),
        )
        .unwrap();
        assert_eq!(s.gate_conds, vec!["holds(canEnter(office))"]);
        assert_eq!(
            s.conds,
            vec!["(holds(canEnter(office))) && !(@dead) && (run.n > 1)"]
        );
    }

    #[test]
    fn an_untargeted_beat_is_judged_per_member() {
        let s = beat_seam(
            &occasions(Some("holds(canEnter(occasion.target))")),
            &kinds(),
            None,
            "enter",
            None,
            None,
        )
        .unwrap();
        assert_eq!(s.conds.len(), 2);
        // Dead only when every member's gate is.
        let v = s.judge(|c| c.contains("office"));
        assert_eq!(v, SeamVerdict::Live);
        assert_eq!(s.judge(|_| true), SeamVerdict::GateDead);
    }

    #[test]
    fn nothing_to_judge_without_gate_or_terminal() {
        assert!(beat_seam(&occasions(None), &kinds(), None, "enter", None, Some("x")).is_none());
        let s = beat_seam(
            &occasions(None),
            &kinds(),
            Some("run.over"),
            "enter",
            None,
            None,
        )
        .unwrap();
        assert!(s.gate_conds.is_empty());
        assert_eq!(s.conds, vec!["!(run.over)"]);
    }
}
