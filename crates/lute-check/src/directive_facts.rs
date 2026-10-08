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
        return crate::component_effects::fact_arg_constant(&a.value).unwrap_or_else(|_| {
            // `::gift{from=@who}` in a component body: the fact's argument
            // is the component's param `who`, which each `::use` binds — not
            // the directive's own attr name.
            let param = match &a.value {
                lute_syntax::ast::AttrValue::Ref(slot) => slot
                    .raw
                    .trim()
                    .strip_prefix('@')
                    .filter(|p| lute_manifest::ident::is_ident(p))
                    .map(str::to_string),
                _ => None,
            };
            FactTerm::Param(param.unwrap_or_else(|| name.to_string()))
        });
    }
    match decl
        .attrs
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| a.default.as_ref())
    {
        Some(Literal::Str(s)) if lute_manifest::ident::is_name(s) => const_term(s),
        Some(Literal::Bool(b)) => FactTerm::Bool(*b),
        _ => FactTerm::Param(name.to_string()),
    }
}

/// `holding(brassKey)` / `holding(_)` / `at("lab-b2")`: a resolved pattern
/// as the transcript and the artifact write it (`@attr` for one left
/// unbound; a name that is not an identifier quoted, so it reads back).
pub fn pattern_text(p: &FactPattern) -> String {
    let args: Vec<String> = p
        .args
        .iter()
        .map(|a| match &a.term {
            FactTerm::Ident(s) if lute_manifest::ident::is_ident(s) => s.clone(),
            FactTerm::Ident(s) => format!("\"{s}\""),
            FactTerm::Bool(b) => b.to_string(),
            FactTerm::Wildcard => "_".to_string(),
            FactTerm::Param(n) => format!("@{n}"),
            FactTerm::Target => lute_manifest::semantics::beats::OCCASION_TARGET.to_string(),
        })
        .collect();
    format!("{}({})", p.relation, args.join(", "))
}

/// Check the facts the call `dir` writes, at the call: the relation is
/// declared, the arity and every bound argument fit it — the checks an
/// `::assert` / `::retract` of the same fact gets. A `reserved: true`
/// relation MAY be written this way (it is the engine's own write: the
/// engine seam); a derived or `app`-tier one may not. A fact whose `@attr`
/// the call leaves unbound is not written, and not judged. A fault in a
/// fact that reads no `@attr` is the declaration's, not the call's: it is
/// reported at the plugin file's line when the CLI placed it
/// ([`RelVocab::effect_origins`]; `check-project` folds the copies).
pub fn check_call(
    dir: &Directive,
    decl: &DirectiveDecl,
    domains: &BTreeMap<String, Domain>,
    ctx: &Ctx<'_>,
) -> Vec<Diagnostic> {
    let Some(effects) = &decl.effects else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (fact, assert) in effects
        .retracts
        .iter()
        .map(|f| (f, false))
        .chain(effects.asserts.iter().map(|f| (f, true)))
    {
        let pattern = bind(fact, decl, &dir.attrs);
        if pattern
            .args
            .iter()
            .any(|a| matches!(a.term, FactTerm::Param(_)))
        {
            continue;
        }
        let origin = if fact.attrs().next().is_none() {
            ctx.env
                .rel_vocab
                .effect_origins
                .get(&crate::rel_schema::effect_origin_key(
                    &decl.name,
                    &fact.to_string(),
                ))
        } else {
            None
        };
        let list = if assert { "asserts" } else { "retracts" };
        for mut d in
            crate::fact_write::check_effect_write(&pattern, dir.span, !assert, domains, ctx)
        {
            d.message = format!(
                "`::{}` {list} `{}` (its declared `effects.{list}`): {}",
                dir.tag,
                pattern_text(&pattern),
                d.message
            );
            out.push(crate::rel_schema::at_plugin_origin(d, origin));
        }
    }
    out
}

/// dsl 0.28.0 (T1-18): each state write the call `dir`'s directive declares
/// (`effects.writes`), its path resolved with the call's attrs, judged like
/// a `::set` target ([`crate::set_op::write_fault`]): a path no declaration
/// covers (with a did-you-mean, and the season a `season.<path>` left out)
/// or a read-only engine path (`prev.*`, `clock.*`, a quest's, an entry's,
/// a record's, `occasion.*`) is an error at the call — the engine would
/// drop the first and overwrite a derived value with the second. A path
/// declared `owner: engine` and `app.*` are the engine's own to write, as a
/// directive's effect is. A write whose path takes an attr the call leaves
/// out is not judged.
pub fn check_call_writes(dir: &Directive, decl: &DirectiveDecl, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let Some(effects) = &decl.effects else {
        return Vec::new();
    };
    let writer = format!("`::{}` (its declared `effects.writes`)", dir.tag);
    let mut out = Vec::new();
    for w in &effects.writes {
        let Some(path) = crate::permissions::resolve_path(&w.scope, &w.path, &dir.attrs) else {
            continue;
        };
        if let Some(message) = effect_literal_fault(w, &path, ctx) {
            out.push(Diagnostic {
                code: crate::set_type::E_SET_TYPE.to_string(),
                severity: lute_core_span::Severity::Error,
                message,
                evidence: None,
                span: dir.span,
                layer: lute_core_span::Layer::Staging,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
        if let Some((code, message)) = crate::set_op::write_fault(
            &path,
            &ctx.env.state,
            &writer,
        )
        .filter(|(code, _)| {
            *code != crate::set_op::E_ENGINE_OWNED_WRITE && *code != "E-APP-READONLY"
        }) {
            out.push(Diagnostic {
                code: code.to_string(),
                severity: lute_core_span::Severity::Error,
                message,
                evidence: None,
                span: dir.span,
                layer: lute_core_span::Layer::Staging,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    }
    out
}

fn effect_literal_fault(
    write: &lute_manifest::schema::WriteDecl,
    path: &str,
    ctx: &Ctx<'_>,
) -> Option<String> {
    use lute_manifest::schema::{OpBy, WriteValue};
    use lute_manifest::types::{Literal, Type};
    let ty = &ctx.env.state.decls.get(path)?.ty;
    let lit = match &write.value {
        WriteValue::Op { by: OpBy::Num(n), .. } => Literal::Double(*n),
        WriteValue::Literal(lit) => lit.clone(),
        _ => return None,
    };
    let bad = match (ty, &lit) {
        (Type::Int, Literal::Double(n)) => !n.is_finite() || n.fract() != 0.0,
        (Type::Double, Literal::Int(_)) => false,
        (Type::Int, Literal::Int(_)) | (Type::Double, Literal::Double(_)) => false,
        _ => false,
    };
    bad.then(|| {
        format!(
            "plugin effect writes `{path}` with {}, which is not a `{}`",
            lute_manifest::types::lit_str(&lit),
            lute_manifest::types::type_str(ty)
        )
    })
}

/// dsl 0.27.0 §4: a directive a lore `<entry>` body admits — a plugin
/// directive whose ONLY behaviour is its declared effects: at least one
/// `writes`/`asserts`/`retracts`, no bridge call or result, no declared
/// result slot, no staging layer and no declarative lowering. It is applied
/// like the entry's own `::set` (first read only).
pub fn is_effect_only(snapshot: &CapabilitySnapshot, tag: &str) -> bool {
    effect_only_blocker(snapshot, tag).is_none()
}

/// Why `tag` is not [`is_effect_only`] — a clause completing "an entry may
/// call a directive only when its whole behaviour is its declared `effects:`;
/// `::tag` …" — or `None` when it is.
pub fn effect_only_blocker(snapshot: &CapabilitySnapshot, tag: &str) -> Option<&'static str> {
    use lute_manifest::schema::{Lowering, WriteValue};
    if snapshot.directive_owner(tag) == Some("lute.core") {
        return Some("is a `lute.core` directive, which stages or directs play");
    }
    let Some(d) = snapshot.directive(tag) else {
        return Some("is not a declared directive");
    };
    let effects = d.effects.as_ref().filter(|e| !e.is_empty());
    Some(if effects.is_none() {
        "declares no `effects:`"
    } else if d.bridge.is_some()
        || d.semantics.iter().any(|s| s == "bridgeCall")
        || effects.is_some_and(|e| {
            e.writes
                .iter()
                .any(|w| matches!(w.value, WriteValue::FromBridgeResult { .. }))
        })
    {
        "calls a bridge"
    } else if d.layer.is_some() {
        "stages a layer"
    } else if d.state.as_ref().is_some_and(|s| !s.declares.is_empty()) {
        "declares result state"
    } else if !matches!(d.lower, Lowering::Passthrough) {
        "lowers to other content"
    } else {
        return None;
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
            Node::Hub(h) => h.bodies().for_each(|b| for_each_call(b, f)),
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
