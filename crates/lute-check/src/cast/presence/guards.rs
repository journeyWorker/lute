use super::*;

/// A `start` conjunct that stays true once it held — `entry.<id>.everRead`
/// or `visited('<id>')` — as its text; `None` for any other shape.
pub(super) fn stable_text(e: &Expr) -> Option<String> {
    match e {
        Expr::Select(_) => {
            let path = select_text(e)?;
            crate::cel_paths::is_entry_ever_read(&path).then_some(path)
        }
        Expr::Call(c) if c.target.is_none() && c.func_name == crate::cel_resolve::VISITED_FN => {
            match c.args.as_slice() {
                [a] => match &a.expr {
                    Expr::Literal(Val::String(s)) => Some(format!("visited('{s}')")),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

/// A static path's canonical text (`a.b.c`, `entry["a-b"].read`); `None`
/// for a `has()` test or any other shape.
fn select_text(e: &Expr) -> Option<String> {
    match e {
        Expr::Select(sel) if sel.test => None,
        _ => crate::cel_paths::select_path(e),
    }
}

/// Every state path `e` reads (the dotted prefix of an index read
/// included) onto `out`; `false` for a shape it cannot see into (a
/// comprehension, a map or struct literal).
pub(super) fn read_paths(e: &Expr, out: &mut Vec<String>) -> bool {
    match e {
        Expr::Select(sel) => match select_text(e) {
            Some(p) => {
                out.push(p);
                true
            }
            None => read_paths(&sel.operand.expr, out),
        },
        Expr::Call(_) if crate::cel_paths::is_path_index(e) => {
            out.extend(select_text(e));
            true
        }
        Expr::Call(c) => {
            let mut ok = c.target.as_ref().is_none_or(|t| read_paths(&t.expr, out));
            for a in &c.args {
                ok &= read_paths(&a.expr, out);
            }
            ok
        }
        Expr::List(l) => l
            .elements
            .iter()
            .fold(true, |ok, x| read_paths(&x.expr, out) && ok),
        Expr::Ident(_) | Expr::Literal(_) => true,
        _ => false,
    }
}

/// How a fact atom sits in a condition: a write that can make it true
/// falsifies a `Neg` occurrence, one that can make it false a `Pos` one;
/// `Both` (under `count`, a comparison, a ternary…) either way.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pol {
    Pos,
    Neg,
    Both,
}

impl Pol {
    pub(crate) fn flip(self) -> Self {
        match self {
            Pol::Pos => Pol::Neg,
            Pol::Neg => Pol::Pos,
            Pol::Both => Pol::Both,
        }
    }
}

/// A fact atom a guard queries: relation, arguments (`None` for `_` or
/// anything not a constant) and polarity.
pub(crate) struct Atom {
    pub(crate) rel: String,
    pub(crate) args: Vec<Option<String>>,
    pub(crate) pol: Pol,
}

/// What a write may do to the facts: atoms matching `rel(args)` (`None`
/// matches anything) may become true (`up`) or false.
#[derive(Clone, PartialEq)]
pub(crate) struct Effect {
    pub(crate) rel: String,
    pub(crate) args: Vec<Option<String>>,
    pub(crate) up: bool,
}

impl Effect {
    /// `self` already stands for every atom `other` names.
    pub(super) fn covers(&self, other: &Effect) -> bool {
        self.rel == other.rel
            && self.up == other.up
            && self.args.len() == other.args.len()
            && self
                .args
                .iter()
                .zip(&other.args)
                .all(|(a, b)| a.is_none() || a == b)
    }
}

/// Past this many effects a write is taken to move every derived relation.
const EFFECT_CAP: usize = 256;

/// How deep a derived atom is read through its rules.
pub(super) const EXPAND_DEPTH: u8 = 4;

/// A condition's disjunctive normal form past this many terms is not split.
pub(super) const DNF_CAP: usize = 256;

pub(super) fn query_term_text(s: &str) -> String {
    if s == "true" || s == "false" {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

/// Every `holds`/`count`/`countDistinct` atom of `e`, with its polarity.
pub(super) fn fact_atoms(e: &Expr, pol: Pol, out: &mut Vec<Atom>) {
    let Expr::Call(c) = e else { return };
    if c.target.is_none() {
        match (c.func_name.as_str(), c.args.as_slice()) {
            (n, [a]) if n == op::LOGICAL_NOT => return fact_atoms(&a.expr, pol.flip(), out),
            (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => {
                fact_atoms(&a.expr, pol, out);
                return fact_atoms(&b.expr, pol, out);
            }
            ("holds" | "count" | "countDistinct", _) if crate::cel_resolve::is_profile_fact_query(c) => {
                if let Some(query) = crate::fact_env::QueryPattern::from_call(c) {
                    out.push(Atom {
                        rel: query.relation,
                        args: query.args,
                        pol: if c.func_name == "holds" {
                            pol
                        } else {
                            Pol::Both
                        },
                    });
                }
                return;
            }
            _ => {}
        }
    }
    if let Some(t) = &c.target {
        fact_atoms(&t.expr, Pol::Both, out);
    }
    for a in &c.args {
        fact_atoms(&a.expr, Pol::Both, out);
    }
}

/// Bind rule `terms` against `args` (`None` matches anything) on top of `b`;
/// `None` when a constant or an already-bound variable disagrees.
pub(super) fn bind(
    terms: &[RuleTerm],
    args: &[Option<String>],
    mut b: BTreeMap<String, Option<String>>,
) -> Option<BTreeMap<String, Option<String>>> {
    for (t, a) in terms.iter().zip(args) {
        match t {
            RuleTerm::Const(c) => {
                if a.as_ref().is_some_and(|a| a != c) {
                    return None;
                }
            }
            RuleTerm::Bool(x) => {
                if a.as_ref().is_some_and(|a| *a != x.to_string()) {
                    return None;
                }
            }
            RuleTerm::Var(v) => match b.get(v) {
                Some(Some(prev)) => {
                    if a.as_ref().is_some_and(|a| a != prev) {
                        return None;
                    }
                }
                _ => {
                    b.insert(v.clone(), a.clone());
                }
            },
        }
    }
    Some(b)
}

/// Everything asserting (`up`) or retracting `rel(args)` may move: the atom
/// itself, then every rule head with a premise it unifies with — in the
/// write's direction through a positive premise, against it through a
/// negated one — to a fixpoint. A relation whose rules did not parse moves
/// either way on any write.
pub(crate) fn write_effects(
    vocab: &RelVocab,
    rel: &str,
    args: Vec<Option<String>>,
    up: bool,
) -> Vec<Effect> {
    let mut out = vec![Effect {
        rel: rel.to_string(),
        args,
        up,
    }];
    let everything = |out: &mut Vec<Effect>, names: &mut dyn Iterator<Item = &String>| {
        for name in names {
            let arity = vocab.relations.get(name).map_or(0, |d| d.args.len());
            for up in [true, false] {
                out.push(Effect {
                    rel: name.clone(),
                    args: vec![None; arity],
                    up,
                });
            }
        }
    };
    let mut i = 0;
    while i < out.len() {
        let e = out[i].clone();
        i += 1;
        for r in &vocab.rules {
            let rule = &r.rule;
            for lit in &rule.body {
                let (atom, positive) = match lit {
                    BodyLiteral::Pos(a) => (a, true),
                    BodyLiteral::Neg(a) => (a, false),
                    _ => continue,
                };
                if atom.relation != e.rel || atom.terms.len() != e.args.len() {
                    continue;
                }
                let Some(b) = bind(&atom.terms, &e.args, BTreeMap::new()) else {
                    continue;
                };
                let head = rule
                    .head
                    .terms
                    .iter()
                    .map(|t| match t {
                        RuleTerm::Var(v) => b.get(v).cloned().flatten(),
                        RuleTerm::Const(c) => Some(c.clone()),
                        RuleTerm::Bool(x) => Some(x.to_string()),
                    })
                    .collect();
                let next = Effect {
                    rel: rule.head.relation.clone(),
                    args: head,
                    up: e.up == positive,
                };
                if !out.iter().any(|o| o.covers(&next)) {
                    out.push(next);
                }
            }
        }
        if out.len() > EFFECT_CAP {
            let derived: Vec<String> = vocab
                .relations
                .iter()
                .filter(|(_, d)| d.derive)
                .map(|(n, _)| n.clone())
                .collect();
            everything(&mut out, &mut derived.iter());
            break;
        }
    }
    everything(&mut out, &mut vocab.unparsed_heads.iter());
    out
}

/// A guard one of `effects` may falsify: an atom it queries unifies with
/// the effect's and the effect moves it the wrong way. A relation named in
/// the guard's text but not found as an atom (an unusual shape) counts.
pub(super) fn affected(effects: &[Effect], g: &Guard) -> bool {
    effects.iter().any(|e| {
        let mut named = false;
        let hit = g.atoms.iter().any(|a| {
            if a.rel != e.rel {
                return false;
            }
            named = true;
            a.args.len() == e.args.len()
                && a.args
                    .iter()
                    .zip(&e.args)
                    .all(|(x, y)| x.is_none() || y.is_none() || x == y)
                && match a.pol {
                    Pol::Both => true,
                    Pol::Pos => !e.up,
                    Pol::Neg => e.up,
                }
        });
        hit || (!named && g.text.contains(&format!("{}(", e.rel)))
    })
}

/// A derived atom read through its rules: the disjuncts (one per unifying
/// rule, the conjunction of its body literals with the head bound), whether
/// that disjunction is EXACTLY the atom (every literal kept), and the rule
/// `cel()` texts it reads.
pub(super) struct Definition {
    pub(super) disjuncts: Vec<String>,
    pub(super) exact: bool,
    pub(super) cels: Vec<String>,
}

/// `rel(args)` through its rules — `None` unless `rel` is derived, not
/// engine-`reserved`, has parsed rules, no seed fact unifies with the atom,
/// and some rule's head does. A literal left non-ground by the binding is
/// dropped (inexact) — except a positive atom's variable used nowhere else
/// and an anonymous `_`, which read as `_`. An entity-kind premise and a
/// ground comparison decide statically; a `cel()` guard is kept only in a
/// rule without variables (it may read one).
pub(super) fn definition(vocab: &RelVocab, rel: &str, args: &[Option<String>]) -> Option<Definition> {
    let decl = vocab.relations.get(rel)?;
    if !decl.derive || decl.reserved || vocab.unparsed_heads.contains(rel) {
        return None;
    }
    let seeded = vocab.facts.iter().any(|f| {
        f.fact.relation == rel
            && f.fact.args.len() == args.len()
            && f.fact
                .args
                .iter()
                .zip(args)
                .all(|(a, k)| match (&a.term, k) {
                    (FactTerm::Ident(i), Some(k)) => i == k,
                    (FactTerm::Bool(b), Some(k)) => b.to_string() == *k,
                    _ => true,
                })
    });
    if seeded {
        return None;
    }
    let is_var = |t: &RuleTerm| matches!(t, RuleTerm::Var(_));
    let mut def = Definition {
        disjuncts: Vec::new(),
        exact: true,
        cels: Vec::new(),
    };
    let mut unified = false;
    for r in vocab.rules.iter() {
        let rule = &r.rule;
        if rule.head.relation != rel || rule.head.terms.len() != args.len() {
            continue;
        }
        let Some(b) = bind(&rule.head.terms, args, BTreeMap::new()) else {
            continue;
        };
        unified = true;
        let mut uses: BTreeMap<&str, usize> = BTreeMap::new();
        let mut has_vars = rule.head.terms.iter().any(is_var);
        for lit in &rule.body {
            let terms: Vec<&RuleTerm> = match lit {
                BodyLiteral::Pos(a) | BodyLiteral::Neg(a) | BodyLiteral::Count { atom: a, .. } => {
                    a.terms.iter().collect()
                }
                BodyLiteral::Cmp { lhs, rhs, .. } => vec![lhs, rhs],
                BodyLiteral::Guard { .. } => Vec::new(),
            };
            for t in terms {
                if let RuleTerm::Var(v) = t {
                    has_vars = true;
                    *uses.entry(v.as_str()).or_default() += 1;
                }
            }
        }
        let value = |t: &RuleTerm| -> Option<String> {
            match t {
                RuleTerm::Const(c) => Some(c.clone()),
                RuleTerm::Bool(x) => Some(x.to_string()),
                RuleTerm::Var(v) => b.get(v).cloned().flatten(),
            }
        };
        let mut conj = Vec::new();
        let mut dead = false;
        for lit in &rule.body {
            match lit {
                BodyLiteral::Pos(a) | BodyLiteral::Neg(a) => {
                    let neg = matches!(lit, BodyLiteral::Neg(_));
                    if !vocab.relations.contains_key(&a.relation) {
                        // An entity-kind premise: static membership.
                        let member = match (
                            a.terms.as_slice(),
                            vocab.kinds.get(&a.relation).map(|k| &k.shape),
                        ) {
                            ([t], Some(KindShape::Members(ms))) => {
                                value(t).map(|v| ms.contains(&v))
                            }
                            _ => None,
                        };
                        match member {
                            Some(m) if m != neg => {}
                            Some(_) => {
                                dead = true;
                                break;
                            }
                            None => def.exact = false,
                        }
                        continue;
                    }
                    let mut ground = true;
                    let mut texts = Vec::with_capacity(a.terms.len());
                    for t in &a.terms {
                        texts.push(value(t).unwrap_or_else(|| {
                            if let RuleTerm::Var(v) = t {
                                let lone = !neg && uses.get(v.as_str()) == Some(&1);
                                if !(is_anonymous_var(v) || lone) {
                                    ground = false;
                                }
                            }
                            "_".to_string()
                        }));
                    }
                    if !ground {
                        def.exact = false;
                        // `!holds('r', [_])` would claim more than the rule does.
                        if neg {
                            continue;
                        }
                    }
                    conj.push(format!(
                        "{}holds(\"{}\", [{}])",
                        if neg { "!" } else { "" },
                        a.relation,
                        texts.iter().map(|t| query_term_text(t)).collect::<Vec<_>>().join(", ")
                    ));
                }
                BodyLiteral::Guard { cel, .. } => {
                    def.cels.push(cel.clone());
                    if has_vars {
                        def.exact = false;
                    } else {
                        conj.push(format!("({cel})"));
                    }
                }
                BodyLiteral::Cmp {
                    lhs, rhs, negated, ..
                } => match (value(lhs), value(rhs)) {
                    (Some(l), Some(r)) => {
                        if (l == r) == *negated {
                            dead = true;
                            break;
                        }
                    }
                    _ => def.exact = false,
                },
                // dsl 0.26.0 §6: the same count as a condition, when every
                // argument is ground or the count's own (`_`, or the one
                // counted variable).
                BodyLiteral::Count {
                    atom,
                    distinct,
                    op,
                    n,
                    ..
                } => {
                    let mut texts = Vec::with_capacity(atom.terms.len());
                    let mut ground = distinct.len() <= 1;
                    for t in &atom.terms {
                        texts.push(match (value(t), t) {
                            (Some(v), _) => v,
                            (None, RuleTerm::Var(v)) if distinct.contains(v) => "_".to_string(),
                            (None, RuleTerm::Var(v))
                                if is_anonymous_var(v) || uses.get(v.as_str()) == Some(&1) =>
                            {
                                "_".to_string()
                            }
                            _ => {
                                ground = false;
                                "_".to_string()
                            }
                        });
                    }
                    if !ground {
                        def.exact = false;
                        continue;
                    }
                    let list_text = texts
                        .iter()
                        .map(|t| query_term_text(t))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let call = match distinct.first() {
                        Some(v) => {
                            let column = atom
                                .terms
                                .iter()
                                .position(|t| matches!(t, RuleTerm::Var(name) if name == v))
                                .unwrap_or(0);
                            format!(
                                "countDistinct(\"{}\", [{}], {column})",
                                atom.relation, list_text
                            )
                        }
                        None => format!("count(\"{}\", [{}])", atom.relation, list_text),
                    };
                    conj.push(format!("{call} {} {n}", op.as_str()));
                }
            }
        }
        if !dead {
            def.disjuncts.push(if conj.is_empty() {
                "true".to_string()
            } else {
                conj.join(" && ")
            });
        }
    }
    unified.then_some(def)
}

