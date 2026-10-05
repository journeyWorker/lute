//! The [`crate::exec::Machine`]'s world (`docs/design/runtime-unification.md`
//! §3.4): live state, base facts, the Datalog closure over them, the visited
//! set — and the one write path every construct uses.
//!
//! - **Reads** go value (a declared default, a seed, a carried value or a
//!   write) → reserved default (`entry.<id>.read` / `.everRead` `false`,
//!   `quest.<id>.state` / `.failedBy` `unset`, an objective flag `false`) →
//!   unset: the order [`crate::EffectiveState`] implements, over an empty
//!   schema because every declared default is already a value.
//! - **Derivation is lazy.** An assert or retract, or a write while some rule
//!   reads state (`cel(…)`), marks the closure dirty; the next query
//!   recomputes it. A derived relation whose rule reads state is therefore
//!   never stale after a `::set` (D4).
//! - **Writes** refresh the reserved `clock.*` values when they move the
//!   clock's `day` / `slot` (D14). Exclusivity after a write is the
//!   Machine's (it records and refuses).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use cel_parser::ast::Expr;
use lute_cel::CelArena;
use lute_check::{RelVocab, StateSchema};
use serde_json::Value as Json;

use crate::datalog::{Fact, Program};
use crate::eval::{Read, ReservedReadKind};
use crate::{eval, EffectiveState, EvalEnv, FactStore, UnresolvedAtom, Value};

/// One member's declared label forms (the artifact's `labelForms` entry).
#[derive(Clone, Debug, Default)]
pub(crate) struct LabelForms {
    pub(crate) start: Option<String>,
    pub(crate) indefinite: Option<String>,
}

impl LabelForms {
    /// An artifact `labelForms` object: member → `{ start?, indefinite? }`.
    fn map_of(forms: &Json) -> BTreeMap<String, LabelForms> {
        let form = |f: &Json, key: &str| f.get(key).and_then(Json::as_str).map(str::to_string);
        forms
            .as_object()
            .into_iter()
            .flatten()
            .map(|(m, f)| {
                (
                    m.clone(),
                    LabelForms {
                        start: form(f, "start"),
                        indefinite: form(f, "indefinite"),
                    },
                )
            })
            .collect()
    }
}

pub(crate) struct Store {
    /// Live scalar state (path → value).
    pub(crate) values: BTreeMap<String, Value>,
    /// Declared value-type per state path (the artifact `state[]` table).
    pub(crate) types: BTreeMap<String, String>,
    /// dsl 0.24.0 §1: per state path, the member → display-label map its
    /// artifact `state[].labels` declares.
    pub(crate) labels: BTreeMap<String, BTreeMap<String, String>>,
    /// dsl 0.27.0 §7: per entity kind, the member → display-label map its
    /// artifact `entities[].labels` declares — what an `occasionTarget`
    /// placeholder of that kind renders.
    pub(crate) kind_labels: BTreeMap<String, BTreeMap<String, String>>,
    /// Per state path, the member → declared label forms its artifact
    /// `state[].labelForms` carries (a `{{path:start}}` renders them).
    pub(crate) label_forms: BTreeMap<String, BTreeMap<String, LabelForms>>,
    /// Per entity kind, the member → declared label forms its artifact
    /// `entities[].labelForms` carries.
    pub(crate) kind_label_forms: BTreeMap<String, BTreeMap<String, LabelForms>>,
    /// Always empty: every declared default is already in `values`, so a
    /// schema tier would only shadow reserved defaults.
    schema: StateSchema,
    /// Empty under `derive: true` (every relation is materialized, so a
    /// query is definite); under `derive: false` (dsl 0.22.0 §6) it marks
    /// the derived relations, so an unmatched query of one is unknown.
    vocab: RelVocab,
    program: Program,
    derive: bool,
    /// Some rule body reads state: a write can change the closure.
    rules_read_state: bool,
    /// Whether a rule guard reads the ephemeral occasion target.
    rules_read_occasion_target: bool,
    /// Seeds ∪ asserted − retracted.
    base: BTreeSet<Fact>,
    /// `base` ∪ the derived least fixpoint (valid unless `dirty`).
    all: BTreeSet<Fact>,
    /// Derived relations whose last fixpoint read an undecided rule guard.
    undecided: BTreeMap<String, Vec<UnresolvedAtom>>,
    dirty: bool,
    /// dsl 0.21.0 §7a.1: the scene ids `visited('<id>')` reads.
    pub(crate) visited: BTreeSet<String>,
    /// dsl 0.25.0 §1: relation pairs declared exclusive (`a < b`).
    excludes: Vec<(String, String)>,
    clock: Option<lute_manifest::clock::ClockDecl>,
    /// dsl 0.5.1 §1.3: every reserved quest path an evaluation read, and how
    /// it resolved (what `lute trace`'s foreign-quest notes name).
    reserved_reads: BTreeMap<String, ReservedReadKind>,
    /// dsl 0.22.0 §6: derived relations a query read under `derive: false`
    derived_reads: BTreeSet<String>,
    /// Whether the caller installed a CEL evaluation observer that needs the
    /// paths and values read by the most recent expression.
    capture_reads: bool,
    pub(crate) last_reads: Vec<(String, Read)>,
}

impl Store {
    /// The artifact's declared state table, defaults, seed facts, rules and
    /// exclusive pairs. `derive: false` leaves the rules unapplied.
    pub(crate) fn of_artifact(art: &Json, derive: bool) -> Self {
        Self::of_artifact_with(art, derive, None, None)
    }

    /// Build a store from one document's commands while taking the
    /// project-wide `rules` and `state` tables from the caller. This is the
    /// play path's equivalent of widening the artifact JSON, without cloning
    /// the document's command tree for every occasion.
    pub(crate) fn of_artifact_with_project(
        art: &Json,
        derive: bool,
        rules: &Json,
        state: &BTreeMap<String, Json>,
    ) -> Self {
        Self::of_artifact_with(art, derive, Some(rules), Some(state))
    }

    fn of_artifact_with(
        art: &Json,
        derive: bool,
        rules_override: Option<&Json>,
        state_override: Option<&BTreeMap<String, Json>>,
    ) -> Self {
        let state_entries: Vec<&Json> = match state_override {
            Some(state) => state.values().collect(),
            None => art
                .get("state")
                .and_then(Json::as_array)
                .map_or_else(Vec::new, |entries| entries.iter().collect()),
        };
        let mut types = BTreeMap::new();
        let mut labels = BTreeMap::new();
        let mut label_forms = BTreeMap::new();
        let mut values = BTreeMap::new();
        for e in state_entries.iter().copied() {
            let path = e.get("path").and_then(Json::as_str).unwrap_or("");
            if path.is_empty() {
                continue;
            }
            let ty = e.get("type").and_then(Json::as_str).unwrap_or("string");
            types.insert(path.to_string(), ty.to_string());
            if let Some(map) = e.get("labels").and_then(Json::as_object) {
                let map: BTreeMap<String, String> = map
                    .iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect();
                labels.insert(path.to_string(), map);
            }
            if let Some(forms) = e.get("labelForms") {
                label_forms.insert(path.to_string(), LabelForms::map_of(forms));
            }
            if let Some(v) = e.get("default").and_then(|j| typed_json_to_value(j, ty)) {
                values.insert(path.to_string(), v);
            }
        }
        let mut base = BTreeSet::new();
        // dsl 0.22.0 §6: under `derive: false` the world is exactly the
        // supplied facts — the project's seed `facts:` are not loaded.
        let seeds = art
            .get("seedFacts")
            .and_then(Json::as_array)
            .filter(|_| derive);
        for s in seeds.into_iter().flatten() {
            let rel = s.get("relation").and_then(Json::as_str).unwrap_or("");
            let args: Vec<String> = s
                .get("args")
                .and_then(Json::as_array)
                .map(|a| a.iter().map(json_arg_to_string).collect())
                .unwrap_or_default();
            if !rel.is_empty() {
                base.insert((rel.to_string(), args));
            }
        }
        let rule_json = rules_override.or_else(|| art.get("rules"));
        let program =
            Program::from_ir(rule_json).with_kinds(crate::datalog::ir_kinds(art.get("entities")));
        let mut vocab = RelVocab::default();
        if !derive {
            let declared = art
                .get("relations")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter(|r| r.get("derive").and_then(Json::as_bool) == Some(true))
                .filter_map(|r| r.get("name").and_then(Json::as_str));
            let heads = rule_json
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|r| r.pointer("/head/relation").and_then(Json::as_str));
            for rel in declared.chain(heads) {
                vocab.relations.insert(
                    rel.to_string(),
                    lute_manifest::relations::RelationDecl {
                        derive: true,
                        ..Default::default()
                    },
                );
            }
        }
        let excludes = art
            .get("relations")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .flat_map(|r| {
                let name = r
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                r.get("excludes")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Json::as_str)
                    .filter(|o| name.as_str() < *o)
                    .map(|o| (name.clone(), o.to_string()))
                    .collect::<Vec<_>>()
            })
            .collect();
        Store {
            values,
            types,
            labels,
            label_forms,
            kind_label_forms: art
                .get("entities")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|k| {
                    let name = k.get("name")?.as_str()?;
                    Some((name.to_string(), LabelForms::map_of(k.get("labelForms")?)))
                })
                .collect(),
            kind_labels: art
                .get("entities")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|k| {
                    let name = k.get("name")?.as_str()?;
                    let map = k.get("labels")?.as_object()?;
                    let map = map
                        .iter()
                        .filter_map(|(m, l)| Some((m.clone(), l.as_str()?.to_string())))
                        .collect();
                    Some((name.to_string(), map))
                })
                .collect(),
            schema: StateSchema::default(),
            vocab,
            rules_read_state: derive && program.reads_state(),
            rules_read_occasion_target: derive && program.reads_state_path("occasion.target"),
            program,
            derive,
            base,
            all: BTreeSet::new(),
            undecided: BTreeMap::new(),
            dirty: true,
            visited: BTreeSet::new(),
            excludes,
            clock: art
                .get("clock")
                .and_then(|c| serde_json::from_value(c.clone()).ok()),
            reserved_reads: BTreeMap::new(),
            derived_reads: BTreeSet::new(),
            capture_reads: false,
            last_reads: Vec::new(),
        }
    }

    pub(crate) fn enable_read_capture(&mut self) {
        self.capture_reads = true;
    }

    /// Replace the live world with a carried one (`lute play`'s resume).
    pub(crate) fn restore(&mut self, values: BTreeMap<String, Value>, base: BTreeSet<Fact>) {
        self.values = values;
        self.base = base;
        self.dirty = true;
    }

    /// Coerce a raw seed literal against the declared value-type. `prev.*`
    /// is the previous-run view of the corresponding `run.*` declaration.
    pub(crate) fn coerce_literal(&self, path: &str, lit: &str) -> Value {
        let type_path = path.strip_prefix("prev.").unwrap_or(path);
        match self.types.get(type_path).map(String::as_str) {
            Some("bool") => match lit {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::Str(lit.to_string()),
            },
            Some("int") => lit.parse::<i64>().map(Value::Int).unwrap_or_else(|_| Value::Str(lit.to_string())),
            Some("double") => lit.parse::<f64>().map(Value::Double).unwrap_or_else(|_| Value::Str(lit.to_string())),
            _ => match lit {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::Str(lit.to_string()),
            },
        }
    }

    /// A value in the reserved tiers' read order (see the module doc).
    pub(crate) fn read(&self, path: &str) -> Read {
        if let Some(v) = self.values.get(path) {
            return Read::Value(v.clone());
        }
        if lute_check::is_reserved_entry_read(path) || is_entry_ever_read(path) {
            return Read::Value(Value::Bool(false));
        }
        if crate::eval::is_reserved_quest_path(path) {
            return Read::Value(crate::eval::reserved_quest_default(path));
        }
        Read::Unset
    }

    /// Set a value with no consequence (a seed, a carried value).
    pub(crate) fn put(&mut self, path: String, v: Value) {
        let is_occasion_target = path == lute_check::beats::OCCASION_TARGET;
        self.values.insert(path, v);
        self.dirty |= self.rules_read_state
            && (!is_occasion_target || self.rules_read_occasion_target);
    }

    /// Forget a value (an unbound `occasion.target`).
    pub(crate) fn remove(&mut self, path: &str) {
        if self.values.remove(path).is_some() {
            self.dirty |= self.rules_read_state
                && (path != lute_check::beats::OCCASION_TARGET || self.rules_read_occasion_target);
        }
    }

    pub(crate) fn visit(&mut self, id: &str) {
        self.visited.insert(id.to_string());
    }

    pub(crate) fn visit_all<'a>(&mut self, ids: impl IntoIterator<Item = &'a String>) {
        for id in ids {
            self.visit(id);
        }
    }

    /// The one write: the value, then the reserved `clock.*` values when it
    /// moved the clock (dsl 0.24.0 §1).
    pub(crate) fn write(&mut self, path: &str, v: Value) {
        self.values.insert(path.to_string(), v);
        self.dirty |= self.rules_read_state
            && (path != lute_check::beats::OCCASION_TARGET || self.rules_read_occasion_target);
        if self
            .clock
            .as_ref()
            .is_some_and(|c| c.day == path || c.slot.as_deref() == Some(path))
        {
            self.refresh_clock();
        }
    }

    /// Re-derive the reserved `clock.*` values from the live `day` / `slot`.
    pub(crate) fn refresh_clock(&mut self) {
        if let Some(clock) = &self.clock {
            crate::clock::refresh(clock, &mut self.values);
            self.dirty |= self.rules_read_state;
        }
    }

    pub(crate) fn assert(&mut self, fact: Fact) {
        self.base.insert(fact);
        self.dirty = true;
    }

    /// Retract every base fact of `rel` matching `args` (`_` is a wildcard).
    pub(crate) fn retract(&mut self, rel: &str, args: &[String]) {
        self.base.retain(|(r, a)| {
            !(r == rel
                && a.len() == args.len()
                && args.iter().zip(a).all(|(p, v)| p == "_" || p == v))
        });
        self.dirty = true;
    }

    /// Recompute the closure when a change since the last one could move it.
    pub(crate) fn derive(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.derive {
            let eff = EffectiveState::new(&self.schema, self.values.clone());
            let closure = self.program.fixpoint(&self.base, &eff);
            self.all = closure.facts;
            self.undecided = closure.undecided;
        } else {
            self.all = self.base.clone();
            self.undecided.clear();
        }
    }

    /// Evaluate a `raw` CEL fragment over live state and the closure. Empty
    /// or unparsable text is unknown, with no atom.
    pub(crate) fn eval(&mut self, raw: &str) -> (Value, Vec<UnresolvedAtom>) {
        match parse(raw) {
            Some(expr) => self.eval_expr(&expr),
            None => {
                if self.capture_reads {
                    self.last_reads.clear();
                }
                (Value::Unknown, Vec::new())
            }
        }
    }

    /// Evaluate the canonical dotted `path` (`run.visits.lab-b2`), a path
    /// field of the artifact, through its CEL spelling
    /// (`run.visits["lab-b2"]`): a segment need not be an identifier.
    pub(crate) fn eval_path(&mut self, path: &str) -> (Value, Vec<UnresolvedAtom>) {
        self.eval(&lute_cel::path::bracket_spelling_of(path))
    }

    pub(crate) fn eval_expr(&mut self, expr: &Expr) -> (Value, Vec<UnresolvedAtom>) {
        self.derive();
        let eff = if self.capture_reads {
            EffectiveState::new(&self.schema, self.values.clone()).with_read_log()
        } else {
            EffectiveState::new(&self.schema, self.values.clone())
        };
        let mut fs = FactStore::new(&self.vocab)
            .with_facts(&self.all)
            .with_undecided(self.undecided.clone());
        for id in &self.visited {
            fs.visit(id);
        }
        let env = EvalEnv {
            state: &eff,
            facts: &fs,
        };
        let mut atoms = Vec::new();
        let v = eval(expr, &env, &mut atoms);
        if self.capture_reads {
            self.last_reads = eff.reads();
        }
        for (path, kind) in eff.reserved_reads() {
            self.reserved_reads.entry(path).or_insert(kind);
        }
        self.derived_reads.extend(fs.derived_reads());
        (v, atoms)
    }

    /// HW27-10: why the derived `fact` does not hold — every rule that could
    /// conclude it, with the premises it misses ([`Program::explain`] over
    /// the live closure). `None` for a relation no rule concludes (or under
    /// `derive: false`); empty for a fact that holds.
    pub(crate) fn why_not(&mut self, fact: &Fact) -> Option<Vec<crate::datalog::Attempt>> {
        self.derive();
        if !self.derive || !self.program.derives(&fact.0) {
            return None;
        }
        if self.all.contains(fact) {
            return Some(Vec::new());
        }
        let eff = EffectiveState::new(&self.schema, self.values.clone());
        let closure = crate::datalog::Closure::of_facts(self.all.clone());
        Some(match self.program.explain(&closure, fact, &eff) {
            crate::datalog::Explanation::Fails { attempts, .. } => attempts,
            crate::datalog::Explanation::Holds(_) => Vec::new(),
        })
    }

    /// dsl 0.25.0 §1: every pair of facts of exclusive relations holding now
    /// (derived ones included), rendered `a(x) and b(x) both hold`.
    pub(crate) fn exclusive_now(&mut self) -> Vec<String> {
        if self.excludes.is_empty() {
            return Vec::new();
        }
        self.derive();
        let mut out = Vec::new();
        for (a, b) in &self.excludes {
            for (_, args) in self.all.iter().filter(|(r, _)| r == a) {
                if self.all.contains(&(b.clone(), args.clone())) {
                    out.push(format!(
                        "{} and {} both hold",
                        render_fact(a, args),
                        render_fact(b, args)
                    ));
                }
            }
        }
        out
    }

    pub(crate) fn has_excludes(&self) -> bool {
        !self.excludes.is_empty()
    }

    /// The closure as of the last [`Store::derive`] — the Machine derives
    /// before handing control back, so a caller never sees it stale.
    pub(crate) fn all_facts(&self) -> &BTreeSet<Fact> {
        &self.all
    }

    pub(crate) fn base_facts(&self) -> &BTreeSet<Fact> {
        &self.base
    }

    /// Derived relations whose closure read an undecided guard.
    pub(crate) fn undecided(&self) -> &BTreeMap<String, Vec<UnresolvedAtom>> {
        &self.undecided
    }

    pub(crate) fn reserved_reads(&self) -> &BTreeMap<String, ReservedReadKind> {
        &self.reserved_reads
    }

    pub(crate) fn derived_reads(&self) -> &BTreeSet<String> {
        &self.derived_reads
    }

    pub(crate) fn into_parts(self) -> (BTreeMap<String, Value>, BTreeSet<Fact>) {
        (self.values, self.base)
    }
}

/// `entry.<id>.everRead` — engine-written, `false` until a read completes.
fn is_entry_ever_read(path: &str) -> bool {
    matches!(
        path.split('.').collect::<Vec<_>>().as_slice(),
        ["entry", _, "everRead"]
    )
}

/// Parse `raw` (`None` for blank or unparsable text), once per thread: a
/// play judges the same guards, `::set` values and rule-body `cel(…)`
/// fragments at every raise and settle, and parsing them afresh each time
/// was most of its run time. The memo holds at most [`PARSED_CAP`] texts.
pub(crate) fn parse(raw: &str) -> Option<Rc<Expr>> {
    thread_local! {
        static PARSED: RefCell<HashMap<String, Option<Rc<Expr>>>> = RefCell::default();
    }
    if raw.trim().is_empty() {
        return None;
    }
    if let Some(hit) = PARSED.with_borrow(|m| m.get(raw).cloned()) {
        return hit;
    }
    let mut arena = CelArena::default();
    let parsed = lute_cel::parse_slot(&mut arena, raw, 0)
        .ok()
        .and_then(|h| arena.get(h))
        .map(|e| Rc::new(e.expr.clone()));
    PARSED.with_borrow_mut(|m| {
        if m.len() >= PARSED_CAP {
            m.clear();
        }
        m.insert(raw.to_string(), parsed.clone());
    });
    parsed
}

/// How many parsed texts [`parse`] keeps per thread before starting over.
const PARSED_CAP: usize = 4096;

pub fn render_fact(rel: &str, args: &[String]) -> String {
    format!("{rel}({})", args.join(", "))
}

fn typed_json_to_value(j: &Json, ty: &str) -> Option<Value> {
    match ty {
        "int" => j.as_i64().map(Value::Int),
        "double" => j.as_f64().map(Value::Double),
        _ => json_to_value(j),
    }
}
/// A JSON artifact scalar → a [`Value`]; `None` for a non-scalar.
pub(crate) fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_i64().map(Value::Int).or_else(|| n.as_f64().map(Value::Double)),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

/// A fact-arg JSON scalar → its ground string (bools as `"true"`/`"false"`).
pub(crate) fn json_arg_to_string(j: &Json) -> String {
    match j {
        Json::String(s) => s.clone(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.to_string(),
        _ => j.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn visited_guard_cache_invalidates_when_visited_changes() {
        let artifact = json!({
            "state": [],
            "rules": [],
            "entities": []
        });
        let mut store = Store::of_artifact(&artifact, false);
        assert_eq!(store.eval("visited('scene-a')").0, Value::Bool(false));
        store.visit_all(&BTreeSet::from([String::from("scene-a")]));
        assert_eq!(store.eval("visited('scene-a')").0, Value::Bool(true));
    }
}
