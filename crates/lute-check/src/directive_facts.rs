//! dsl 0.27.0 §4 (T2-11): directive facts — the facts a plugin directive
//! declares it retracts / asserts (`effects: { asserts: ["holding(@item)"] }`),
//! resolved at one call. The engine applies them after the call exactly as
//! it applies an `::assert` / `::retract`, so every analysis that reads what
//! `::assert{F}` writes reads what `::give{item="brassKey"}` writes through
//! [`CallFacts`]: one resolution, shared by the checker's fact passes, the
//! scenario tools and the compiler.
//!
//! The table of effect directives rides the merged relational vocabulary
//! ([`RelVocab::effect_directives`], folded from the capability snapshot in
//! `fold_env`), so a project-wide pass resolves a call without the snapshot.

use std::collections::BTreeMap;

use lute_core_span::Diagnostic;
use lute_manifest::schema::{DirectiveDecl, FactEffect, FactEffectArg};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::Literal;
use lute_syntax::ast::{Attr, Directive, Node};
use lute_syntax::datalog::{FactArg, FactPattern, FactTerm};

use crate::rel_schema::RelVocab;
use crate::Ctx;

/// The snapshot's directives that declare a fact effect, by tag — what
/// [`RelVocab::effect_directives`] carries.
pub fn effect_directives(snapshot: &CapabilitySnapshot) -> BTreeMap<String, DirectiveDecl> {
    snapshot
        .directives
        .iter()
        .filter(|(_, d)| d.effects.as_ref().is_some_and(|e| e.has_facts()))
        .map(|(tag, d)| (tag.clone(), d.clone()))
        .collect()
}

/// The facts one call writes, in the order the engine applies them:
/// `retracts` first, then `asserts` (a directive that moves a fact drops the
/// old one before it adds the new). An `@attr` the call leaves unbound (no
/// value, no declared `default:`, or a runtime `@ref`) stays a
/// [`FactTerm::Param`]: the pattern is not ground, so a ground consumer
/// (`GroundFact::from_pattern`) skips it while a relation-level one still
/// counts the relation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallFacts {
    pub retracts: Vec<FactPattern>,
    pub asserts: Vec<FactPattern>,
}

impl CallFacts {
    /// Every pattern with `true` for an assert, retracts first.
    pub fn writes(&self) -> impl Iterator<Item = (&FactPattern, bool)> {
        self.retracts
            .iter()
            .map(|p| (p, false))
            .chain(self.asserts.iter().map(|p| (p, true)))
    }
}

/// Resolve `decl`'s declared fact effects at the call `dir`.
pub fn call_facts(decl: &DirectiveDecl, dir: &Directive) -> CallFacts {
    let Some(effects) = &decl.effects else {
        return CallFacts::default();
    };
    let resolve = |facts: &[FactEffect]| facts.iter().map(|f| bind(f, decl, &dir.attrs)).collect();
    CallFacts {
        retracts: resolve(&effects.retracts),
        asserts: resolve(&effects.asserts),
    }
}

/// The directives declaring a fact effect, by tag.
pub type EffectDirectives = BTreeMap<String, DirectiveDecl>;

/// The facts the call `dir` writes, when its directive (in `table`)
/// declares any.
pub fn lookup(table: &EffectDirectives, dir: &Directive) -> Option<CallFacts> {
    table.get(&dir.tag).map(|decl| call_facts(decl, dir))
}

/// The effect directives of a whole root: every document's
/// [`RelVocab::effect_directives`], unioned (a scene's own `plugins:` may
/// add some).
pub fn root_table<'a>(
    foldeds: impl IntoIterator<Item = &'a crate::check::FoldedEnv>,
) -> EffectDirectives {
    let mut out = EffectDirectives::new();
    for f in foldeds {
        for (tag, decl) in &f.env.rel_vocab.effect_directives {
            out.entry(tag.clone()).or_insert_with(|| decl.clone());
        }
    }
    out
}

impl RelVocab {
    /// The facts the call `dir` writes, when its directive declares any.
    pub fn call_facts(&self, dir: &Directive) -> Option<CallFacts> {
        lookup(&self.effect_directives, dir)
    }
}

/// One declared fact at a call: each `@attr` bound to the call's constant
/// value, else the attr's declared default.
fn bind(fact: &FactEffect, decl: &DirectiveDecl, attrs: &[Attr]) -> FactPattern {
    let args = fact
        .args
        .iter()
        .map(|a| FactArg {
            term: match a {
                FactEffectArg::Const(c) => const_term(c),
                FactEffectArg::Wildcard => FactTerm::Wildcard,
                FactEffectArg::Attr(name) => attr_term(name, decl, attrs),
            },
            span: (0, 0),
        })
        .collect();
    FactPattern {
        relation: fact.relation.clone(),
        relation_span: (0, 0),
        args,
        span: (0, 0),
    }
}

fn const_term(c: &str) -> FactTerm {
    match c {
        "true" => FactTerm::Bool(true),
        "false" => FactTerm::Bool(false),
        _ => FactTerm::Ident(c.to_string()),
    }
}

fn attr_term(name: &str, decl: &DirectiveDecl, attrs: &[Attr]) -> FactTerm {
    if let Some(a) = attrs.iter().find(|a| a.key == name) {
        return crate::component_effects::fact_arg_constant(&a.value)
            .unwrap_or_else(|_| FactTerm::Param(name.to_string()));
    }
    match decl
        .attrs
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| a.default.as_ref())
    {
        Some(Literal::Str(s)) if is_ident(s) => const_term(s),
        Some(Literal::Bool(b)) => FactTerm::Bool(*b),
        _ => FactTerm::Param(name.to_string()),
    }
}

fn is_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `holding(brassKey)` / `holding(_)`: a resolved pattern as the transcript
/// and the artifact write it (`@attr` for one left unbound).
pub fn pattern_text(p: &FactPattern) -> String {
    let args: Vec<String> = p
        .args
        .iter()
        .map(|a| match &a.term {
            FactTerm::Ident(s) => s.clone(),
            FactTerm::Bool(b) => b.to_string(),
            FactTerm::Wildcard => "_".to_string(),
            FactTerm::Param(n) => format!("@{n}"),
        })
        .collect();
    format!("{}({})", p.relation, args.join(", "))
}

/// Check the facts the call `dir` writes, at the call: the relation is
/// declared, the arity and every bound argument fit it — the checks an
/// `::assert` / `::retract` of the same fact gets. A `reserved: true`
/// relation MAY be written this way (it is the engine's own write: the
/// engine seam); a derived or `app`-tier one may not. A fact whose `@attr`
/// the call leaves unbound is not written, and not judged.
pub fn check_call(
    dir: &Directive,
    decl: &DirectiveDecl,
    domains: &BTreeMap<String, Domain>,
    ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    let facts = call_facts(decl, dir);
    let mut out = Vec::new();
    for (pattern, assert) in facts.writes() {
        if pattern
            .args
            .iter()
            .any(|a| matches!(a.term, FactTerm::Param(_)))
        {
            continue;
        }
        let list = if assert { "asserts" } else { "retracts" };
        for mut d in crate::fact_write::check_effect_write(pattern, dir.span, !assert, domains, ctx)
        {
            d.message = format!(
                "`::{}` {list} `{}` (its declared `effects.{list}`): {}",
                dir.tag,
                pattern_text(pattern),
                d.message
            );
            out.push(d);
        }
    }
    out
}

/// dsl 0.27.0 §4: a directive a lore `<entry>` body admits — a plugin
/// directive whose ONLY behaviour is its declared effects: at least one
/// `writes`/`asserts`/`retracts`, no bridge call or result, no declared
/// result slot, no staging layer and no declarative lowering. It is applied
/// like the entry's own `::set` (first read only).
pub fn is_effect_only(snapshot: &CapabilitySnapshot, tag: &str) -> bool {
    use lute_manifest::schema::{Lowering, WriteValue};
    if snapshot.directive_owner(tag) == Some("lute.core") {
        return false;
    }
    snapshot.directive(tag).is_some_and(|d| {
        d.effects.as_ref().is_some_and(|e| {
            !e.is_empty()
                && !e
                    .writes
                    .iter()
                    .any(|w| matches!(w.value, WriteValue::FromBridgeResult { .. }))
        }) && d.bridge.is_none()
            && d.layer.is_none()
            && d.state.as_ref().is_none_or(|s| s.declares.is_empty())
            && !d.semantics.iter().any(|s| s == "bridgeCall")
            && matches!(d.lower, Lowering::Passthrough)
    })
}

/// Every directive call in a node stream, nested bodies included, with its
/// declared fact writes — the directive half of every "which asserts does
/// this body hold" scan ([`crate::connectivity::collect_asserts`]'s twin).
pub fn collect_call_facts<'d>(
    nodes: &'d [Node],
    table: &EffectDirectives,
    out: &mut Vec<(&'d Directive, CallFacts)>,
) {
    if table.is_empty() {
        return;
    }
    for_each_call(nodes, &mut |d| {
        if let Some(f) = lookup(table, d) {
            out.push((d, f));
        }
    });
}

/// Visit every directive of a node stream, nested bodies and `<timeline>`
/// clips included, in document order.
pub fn for_each_call<'d>(nodes: &'d [Node], f: &mut impl FnMut(&'d Directive)) {
    use lute_syntax::ast::{Arm, ClipNode};
    for node in nodes {
        match node {
            Node::Directive(d) => f(d),
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    for_each_call(body, f);
                }
            }
            Node::Branch(b) => b.choices.iter().for_each(|c| for_each_call(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| for_each_call(&c.body, f)),
            Node::On(o) => for_each_call(&o.body, f),
            Node::Objective(o) => for_each_call(&o.body, f),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|t| &t.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        f(d);
                    }
                }
            }
            Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}
