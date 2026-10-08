//! The [`crate::Machine`]'s world (`docs/design/runtime-unification.md`
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
use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as Json;

use super::expr::{self, Expr};
use crate::datalog::{Fact, Program};
use crate::eval::{Read, ReservedReadKind};
use crate::schema::{RelVocab, StateSchema};
use crate::{EffectiveState, EvalEnv, FactStore, UnresolvedAtom, Value};

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

/// Immutable, project-invariant evaluator data decoded from an artifact's
/// state/rules/entity declarations.  Each machine owns only the mutable
/// values, facts and evaluation bookkeeping that is layered over this schema.
pub(crate) struct StoreSchema {
    pub(crate) types: std::sync::Arc<BTreeMap<String, String>>,
    pub(crate) labels: std::sync::Arc<BTreeMap<String, BTreeMap<String, String>>>,
    pub(crate) kind_labels: std::sync::Arc<BTreeMap<String, BTreeMap<String, String>>>,
    pub(crate) label_forms: std::sync::Arc<BTreeMap<String, BTreeMap<String, LabelForms>>>,
    pub(crate) kind_label_forms: std::sync::Arc<BTreeMap<String, BTreeMap<String, LabelForms>>>,
    pub(crate) state: std::sync::Arc<StateSchema>,
    pub(crate) vocab: std::sync::Arc<RelVocab>,
    pub(crate) program: std::sync::Arc<Program>,
    pub(crate) derive: bool,
    pub(crate) rules_read_state: bool,
    pub(crate) rules_read_occasion_target: bool,
    pub(crate) defaults: BTreeMap<String, Value>,
    pub(crate) excludes: Vec<(String, String)>,
    pub(crate) clock: Option<lute_manifest::clock::ClockDecl>,
}

pub(crate) struct Store {
    pub(crate) store_schema: std::sync::Arc<StoreSchema>,
    /// Live scalar state (path → value).
    pub(crate) values: BTreeMap<String, Value>,
    /// Declared value-type per state path (the artifact `state[]` table).
    pub(crate) types: std::sync::Arc<BTreeMap<String, String>>,
    /// dsl 0.24.0 §1: per state path, the member → display-label map its
    /// artifact `state[].labels` declares.
    pub(crate) labels: std::sync::Arc<BTreeMap<String, BTreeMap<String, String>>>,
    /// dsl 0.27.0 §7: per entity kind, the member → display-label map its
    /// artifact `entities[].labels` declares — what an `occasionTarget`
    /// placeholder of that kind renders.
    pub(crate) kind_labels: std::sync::Arc<BTreeMap<String, BTreeMap<String, String>>>,
    /// Per state path, the member → declared label forms its artifact
    /// `state[].labelForms` carries (a `{{path:start}}` renders them).
    pub(crate) label_forms: std::sync::Arc<BTreeMap<String, BTreeMap<String, LabelForms>>>,
    /// Per entity kind, the member → declared label forms its artifact
    /// `entities[].labelForms` carries.
    pub(crate) kind_label_forms: std::sync::Arc<BTreeMap<String, BTreeMap<String, LabelForms>>>,
    vocab: std::sync::Arc<RelVocab>,
    program: std::sync::Arc<Program>,
    derive: bool,
    rules_read_state: bool,
    rules_read_occasion_target: bool,
    /// Seeds ∪ asserted − retracted.
    base: BTreeSet<Fact>,
    /// `base` ∪ the derived least fixpoint (valid unless `dirty`).
    all: std::sync::Arc<BTreeSet<Fact>>,
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
    fn schema_for(
        art: &Json,
        derive: bool,
        rules_override: Option<&Json>,
        state_override: Option<&BTreeMap<String, Json>>,
    ) -> std::sync::Arc<StoreSchema> {
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
        let mut defaults = BTreeMap::new();
        for e in state_entries.iter().copied() {
            let path = e.get("path").and_then(Json::as_str).unwrap_or("");
            if path.is_empty() {
                continue;
            }
            let ty = e.get("type").and_then(Json::as_str).unwrap_or("string");
            types.insert(path.to_string(), ty.to_string());
            if let Some(map) = e.get("labels").and_then(Json::as_object) {
                labels.insert(
                    path.to_string(),
                    map.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .collect(),
                );
            }
            if let Some(forms) = e.get("labelForms") {
                label_forms.insert(path.to_string(), LabelForms::map_of(forms));
            }
            if let Some(v) = e.get("default").and_then(|j| typed_json_to_value(j, ty)) {
                defaults.insert(path.to_string(), v);
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
                let name = r.get("name").and_then(Json::as_str).unwrap_or("").to_string();
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
        std::sync::Arc::new(StoreSchema {
            kind_label_forms: std::sync::Arc::new(
                art
                    .get("entities")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|k| {
                        let name = k.get("name")?.as_str()?;
                        Some((name.to_string(), LabelForms::map_of(k.get("labelForms")?)))
                    })
                    .collect(),
            ),
            kind_labels: std::sync::Arc::new(
                art
                    .get("entities")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|k| {
                        let name = k.get("name")?.as_str()?;
                        let map = k.get("labels")?.as_object()?;
                        Some((
                            name.to_string(),
                            map.iter()
                                .filter_map(|(m, l)| Some((m.clone(), l.as_str()?.to_string())))
                                .collect(),
                        ))
                    })
                    .collect(),
            ),
            state: std::sync::Arc::new(StateSchema::default()),
            vocab: std::sync::Arc::new(vocab),
            rules_read_state: derive && program.reads_state(),
            rules_read_occasion_target: derive && program.reads_state_path("occasion.target"),
            types: std::sync::Arc::new(types),
            labels: std::sync::Arc::new(labels),
            label_forms: std::sync::Arc::new(label_forms),
            program: std::sync::Arc::new(program),
            derive,
            defaults,
            excludes,
            clock: art
                .get("clock")
                .and_then(|c| serde_json::from_value(c.clone()).ok()),
        })
    }

    /// A Store over `schema`. Its live world is `world` (a resumed walk's
    /// carried state and base facts), or else the declared defaults and the
    /// artifact's seed facts.
    fn from_schema(art: &Json, schema: std::sync::Arc<StoreSchema>, world: Option<World>) -> Self {
        let (values, base) = match world {
            Some(world) => world,
            None => (schema.defaults.clone(), seed_facts(art, schema.derive)),
        };
        Self {
            store_schema: schema.clone(),
            types: schema.types.clone(),
            labels: schema.labels.clone(),
            kind_labels: schema.kind_labels.clone(),
            label_forms: schema.label_forms.clone(),
            kind_label_forms: schema.kind_label_forms.clone(),
            values,
            vocab: schema.vocab.clone(),
            program: schema.program.clone(),
            derive: schema.derive,
            rules_read_state: schema.rules_read_state,
            rules_read_occasion_target: schema.rules_read_occasion_target,
            base,
            all: std::sync::Arc::default(),
            undecided: BTreeMap::new(),
            dirty: true,
            visited: BTreeSet::new(),
            excludes: schema.excludes.clone(),
            clock: schema.clock.clone(),
            reserved_reads: BTreeMap::new(),
            derived_reads: BTreeSet::new(),
            capture_reads: false,
            last_reads: Vec::new(),
        }
    }

    pub(crate) fn schema_for_project(
        art: &Json,
        derive: bool,
        rules: &Json,
        state: &BTreeMap<String, Json>,
    ) -> std::sync::Arc<StoreSchema> {
        Self::schema_for(art, derive, Some(rules), Some(state))
    }

    pub(crate) fn from_project_schema(
        art: &Json,
        schema: std::sync::Arc<StoreSchema>,
        world: Option<World>,
    ) -> Self {
        Self::from_schema(art, schema, world)
    }

    /// The artifact's declared state table, defaults, seed facts, rules and
    /// exclusive pairs. `derive: false` leaves the rules unapplied.
    pub(crate) fn of_artifact(art: &Json, derive: bool, world: Option<World>) -> Self {
        let schema = Self::schema_for(art, derive, None, None);
        Self::from_schema(art, schema, world)
    }

    /// Compatibility constructor for callers that do not retain a project
    /// schema. Play sessions use `from_project_schema` instead.
    pub(crate) fn of_artifact_with_project(
        art: &Json,
        derive: bool,
        rules: &Json,
        state: &BTreeMap<String, Json>,
        world: Option<World>,
    ) -> Self {
        let schema = Self::schema_for_project(art, derive, rules, state);
        Self::from_project_schema(art, schema, world)
    }

    pub(crate) fn enable_read_capture(&mut self) {
        self.capture_reads = true;
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
        if lute_manifest::semantics::cel_paths::is_reserved_entry_read(path) || is_entry_ever_read(path) {
            return Read::Value(Value::Bool(false));
        }
        if crate::eval::is_reserved_quest_path(path) {
            return Read::Value(crate::eval::reserved_quest_default(path));
        }
        Read::Unset
    }

    /// Set a value with no consequence (a seed, a carried value).
    pub(crate) fn put(&mut self, path: String, v: Value) {
        let is_occasion_target = path == lute_manifest::semantics::beats::OCCASION_TARGET;
        self.values.insert(path, v);
        self.dirty |= self.rules_read_state
            && (!is_occasion_target || self.rules_read_occasion_target);
    }

    /// Forget a value (an unbound `occasion.target`).
    pub(crate) fn remove(&mut self, path: &str) {
        if self.values.remove(path).is_some() {
            self.dirty |= self.rules_read_state
                && (path != lute_manifest::semantics::beats::OCCASION_TARGET || self.rules_read_occasion_target);
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
            && (path != lute_manifest::semantics::beats::OCCASION_TARGET || self.rules_read_occasion_target);
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
            let (all, undecided) = LastClosure::get_or_derive(self);
            self.all = all;
            self.undecided = undecided;
        } else {
            self.all = std::sync::Arc::new(self.base.clone());
            self.undecided.clear();
        }
    }

    /// Evaluate `expr` over live state and the closure.
    pub(crate) fn eval(&mut self, expr: &Expr) -> (Value, Vec<UnresolvedAtom>) {
        self.derive();
        let eff = if self.capture_reads {
            EffectiveState::over(&self.store_schema.state, &self.values).with_read_log()
        } else {
            EffectiveState::over(&self.store_schema.state, &self.values)
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
        let v = expr::eval(expr, &env, None, &mut atoms);
        if self.capture_reads {
            self.last_reads = eff.reads();
        }
        for (path, kind) in eff.reserved_reads() {
            self.reserved_reads.entry(path).or_insert(kind);
        }
        self.derived_reads.extend(fs.derived_reads());
        (v, atoms)
    }

    /// Evaluate the canonical dotted `path` (`run.visits.lab-b2`), a path
    /// field of the artifact: a segment need not be an identifier.
    pub(crate) fn eval_path(&mut self, path: &str) -> (Value, Vec<UnresolvedAtom>) {
        self.eval(&Expr::Path(path.into()))
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
        let eff = EffectiveState::over(&self.store_schema.state, &self.values);
        let closure = crate::datalog::Closure::of_facts((*self.all).clone());
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

/// The last closure [`Store::derive`] computed on this thread, with the exact
/// inputs it was computed from.
///
/// A playthrough builds a fresh [`Store`] per walk, and most are built over
/// the same world as the walk before: on the largest dogfood project ~75% of
/// closures repeat the previous one's schema, base facts and state exactly.
/// The closure is a function of those three alone (the rules, the declared
/// state types, `base`, and the state values the rule guards read), so an
/// exact match returns the same closure the fixpoint would.
struct LastClosure {
    /// Held, not just compared by address, so a freed schema's address
    /// cannot be reused by another.
    schema: std::sync::Arc<StoreSchema>,
    base: BTreeSet<Fact>,
    values: BTreeMap<String, Value>,
    all: std::sync::Arc<BTreeSet<Fact>>,
    undecided: BTreeMap<String, Vec<UnresolvedAtom>>,
}

thread_local! {
    static LAST_CLOSURE: RefCell<Option<LastClosure>> = const { RefCell::new(None) };
}

impl LastClosure {
    fn matches(&self, store: &Store) -> bool {
        std::sync::Arc::ptr_eq(&self.schema, &store.store_schema)
            && self.base == store.base
            && self.values.len() == store.values.len()
            && self
                .values
                .iter()
                .zip(&store.values)
                .all(|((ka, va), (kb, vb))| ka == kb && same_value(va, vb))
    }

    /// `store`'s closure: the last one when its inputs are `store`'s, else a
    /// fresh fixpoint (which becomes the last one).
    fn get_or_derive(
        store: &Store,
    ) -> (std::sync::Arc<BTreeSet<Fact>>, BTreeMap<String, Vec<UnresolvedAtom>>) {
        LAST_CLOSURE.with_borrow_mut(|last| {
            if let Some(hit) = last.as_ref().filter(|l| l.matches(store)) {
                return (std::sync::Arc::clone(&hit.all), hit.undecided.clone());
            }
            let eff = EffectiveState::over(&store.store_schema.state, &store.values);
            let closure = store.program.fixpoint(&store.base, &eff);
            let all = std::sync::Arc::new(closure.facts);
            *last = Some(LastClosure {
                schema: std::sync::Arc::clone(&store.store_schema),
                base: store.base.clone(),
                values: store.values.clone(),
                all: std::sync::Arc::clone(&all),
                undecided: closure.undecided.clone(),
            });
            (all, closure.undecided)
        })
    }
}

/// Value identity for [`LastClosure`]: doubles by bit pattern, so `0.0` and
/// `-0.0` (which CEL `string()` spells differently) never share a closure.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Double(x), Value::Double(y)) => x.to_bits() == y.to_bits(),
        _ => a == b,
    }
}

/// A live world a Store starts from: state values and base facts.
pub(crate) type World = (BTreeMap<String, Value>, BTreeSet<Fact>);

/// The artifact's `seedFacts` as base facts (none when rules are unapplied).
fn seed_facts(art: &Json, derive: bool) -> BTreeSet<Fact> {
    let mut base = BTreeSet::new();
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
    base
}

/// A fact-arg JSON scalar → its ground string (bools as `"true"`/`"false"`).
pub fn json_arg_to_string(j: &Json) -> String {
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
    fn visited_reads_the_live_visited_set() {
        let artifact = json!({
            "state": [],
            "rules": [],
            "entities": []
        });
        let mut store = Store::of_artifact(&artifact, false, None);
        let visited = Expr::decode(&json!({"call": "visited", "args": [{"string": "scene-a"}]}));
        assert_eq!(store.eval(&visited).0, Value::Bool(false));
        store.visit_all(&BTreeSet::from([String::from("scene-a")]));
        assert_eq!(store.eval(&visited).0, Value::Bool(true));
    }

    /// The closure memo is exact: a Store over a different world (another
    /// base fact, or a state value a rule guard reads) never gets the
    /// previous Store's closure, and the same world gets the same one.
    #[test]
    fn closure_memo_follows_base_facts_and_guarded_state() {
        let artifact = json!({
            "state": [{"path": "run.power", "type": "bool", "default": false}],
            "rules": [
                {
                    "head": {"relation": "canEnter", "terms": [{"kind": "const", "value": "lift"}]},
                    "body": [{"kind": "guard", "cel": {"cel": "run.power", "expr": {"path": "run.power"}}}],
                    "raw": "canEnter(lift) :- cel(\"run.power\")"
                },
                {
                    "head": {"relation": "lit", "terms": [{"kind": "var", "name": "R"}]},
                    "body": [{"kind": "atom", "atom": {"relation": "lamp", "terms": [{"kind": "var", "name": "R"}]}, "negated": false}],
                    "raw": "lit(R) :- lamp(R)"
                }
            ],
            "entities": []
        });
        let schema = Store::schema_for(&artifact, true, None, None);
        let lamp = |room: &str| ("lamp".to_string(), vec![room.to_string()]);
        let world = |power: Value, lamps: &[&str]| -> World {
            (
                BTreeMap::from([("run.power".to_string(), power)]),
                lamps.iter().map(|r| lamp(r)).collect(),
            )
        };
        let closure = |w: World| {
            let mut store = Store::from_schema(&artifact, std::sync::Arc::clone(&schema), Some(w));
            store.derive();
            store.all_facts().clone()
        };
        let can_enter = ("canEnter".to_string(), vec!["lift".to_string()]);
        let lit = |room: &str| ("lit".to_string(), vec![room.to_string()]);

        let off = closure(world(Value::Bool(false), &["hall"]));
        assert!(off.contains(&lit("hall")) && !off.contains(&can_enter));
        // Same facts, the guarded state flipped: the guard's head appears.
        let on = closure(world(Value::Bool(true), &["hall"]));
        assert!(on.contains(&can_enter));
        // Same state, another base fact: its derived fact appears, the old one goes.
        let moved = closure(world(Value::Bool(true), &["cellar"]));
        assert!(moved.contains(&lit("cellar")) && !moved.contains(&lit("hall")));
        // The same world twice: the same closure.
        assert_eq!(moved, closure(world(Value::Bool(true), &["cellar"])));
        // Doubles key by bit pattern: 0.0 and -0.0 are different worlds.
        assert!(!same_value(&Value::Double(0.0), &Value::Double(-0.0)));
        assert!(same_value(&Value::Double(1.5), &Value::Double(1.5)));
    }
}
