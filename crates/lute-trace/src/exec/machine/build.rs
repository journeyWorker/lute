//! Building a [`Machine`]: the constructors, the `with_*` configuration a
//! runtime layers on before [`Machine::run`], the seeds, and handing the
//! world on ([`Machine::into_carry`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::Value as Json;

use super::plugin::BridgeReads;
use super::{Carry, Machine, Seed};
use crate::exec::driver::Driver;
use crate::exec::session::parse_ground_fact;
use crate::exec::store::Store;
use crate::Value;

impl<D: Driver> Machine<D> {
    /// Build a Machine skeleton straight off an artifact: addr map, the
    /// [`Store`] (declared types, labels, defaults, seed facts, rules).
    /// [`Machine::resume`] (`lute play`, dsl 0.21.0 §6) then replaces the
    /// live world with the PRIOR scene's [`Carry`]. Both constructors layer
    /// the seed's own `state`/`facts` on top via [`Machine::apply_seeds`] —
    /// the one place that override rule lives.
    fn blank(art: &Json, seed: Seed, driver: D) -> Self {
        Self::blank_with_overrides(art, seed, driver, None, None)
    }

    fn blank_with_overrides(
        art: &Json,
        seed: Seed,
        driver: D,
        rules: Option<&Json>,
        state: Option<&BTreeMap<String, Json>>,
    ) -> Self {
        let kind = art
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("scene")
            .to_string();
        let commands = art
            .get("commands")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();

        let mut addr_index = BTreeMap::new();
        let mut addr_order = Vec::new();
        for (i, c) in commands.iter().enumerate() {
            if let Some(a) = c.get("addr").and_then(Json::as_str) {
                addr_index.insert(a.to_string(), i);
                addr_order.push((a.to_string(), i));
            }
        }
        addr_order.sort();

        let mut store = match (rules, state) {
            (Some(rules), Some(state)) => {
                Store::of_artifact_with_project(art, seed.derive, rules, state)
            }
            _ => Store::of_artifact(art, seed.derive),
        };
        // dsl 0.21.0 §7a.1: the seed's `visited` seeds the presented set.
        store.visit_all(&seed.visited);

        Machine {
            driver,
            kind,
            commands,
            addr_index,
            addr_order,
            display_names: BTreeMap::new(),
            occasion_target: None,
            store,
            seed,
            quest_status: BTreeMap::new(),
            quest_instances: BTreeMap::new(),
            incomplete: false,
            terminated: false,
            fatal: None,
            refused: false,
            unresolved: Vec::new(),
            eval_observer: None,
            quest_resume: false,
            entry: None,
            bundle_beat: None,
            apply_effects: true,
            observed_waiting: BTreeSet::new(),
            probe_arms: false,
            accepted: Vec::new(),
            accepted_next_run: Vec::new(),
            failed_objectives: BTreeSet::new(),
            objective_failed_by: BTreeMap::new(),
            defer_by: Vec::new(),
            deferred_handlers: None,
            bridge_reads: Arc::default(),
        }
    }

    /// A fresh walk (`lute run`, `lute trace`) of `art`: its declared
    /// defaults and seed facts, the `seed` layered over them. The artifact's
    /// own content is the reader of its bridge results (dsl 0.25.0 §7; `lute
    /// play` hands the project's in [`Machine::with_bridge_reads`]).
    pub fn new(art: &Json, seed: Seed, driver: D) -> Self {
        let mut m = Self::blank(art, seed, driver);
        m.bridge_reads = Arc::new(BridgeReads::of([art]));
        m.apply_seeds();
        m.store.derive();
        m
    }

    /// `lute play`'s chained-evaluation constructor (dsl 0.21.0 §6):
    /// seeds live state/facts/quest status from a PRIOR walk's [`Carry`]
    /// instead of this artifact's own declared defaults — `play.rs` already
    /// decided what carries forward across the scene boundary (its own
    /// state-tier filter: `run.*`/`user.*`/`app.*`/`quest.*` persist,
    /// `scene.*` resets); this constructor does not re-derive that policy,
    /// only accepts its result. `play.rs` never puts `state`/`facts` seeds
    /// in `seed` (it seeds the playthrough once, up front), so
    /// [`Machine::apply_seeds`] layers nothing over the carryover. Only the
    /// carry's `state`, `base_facts`, `quest_status` and quest instance
    /// counters are read.
    pub fn resume(art: &Json, seed: Seed, carry: Carry, driver: D) -> Self {
        Self::resume_with_overrides(art, seed, carry, driver, None, None)
    }

    /// Resume a play machine over one document's command tree while reading
    /// the project-wide rules and state declarations without widening/cloning
    /// the complete artifact JSON.
    pub fn resume_with_project(
        art: &Json,
        seed: Seed,
        carry: Carry,
        driver: D,
        rules: &Json,
        state: &BTreeMap<String, Json>,
    ) -> Self {
        Self::resume_with_overrides(art, seed, carry, driver, Some(rules), Some(state))
    }

    fn resume_with_overrides(
        art: &Json,
        seed: Seed,
        carry: Carry,
        driver: D,
        rules: Option<&Json>,
        state: Option<&BTreeMap<String, Json>>,
    ) -> Self {
        let mut m = Self::blank_with_overrides(art, seed, driver, rules, state);
        m.store.restore(carry.state, carry.base_facts);
        m.quest_status = carry.quest_status;
        m.quest_instances = carry.quest_instances;
        m.apply_seeds();
        m.store.derive();
        m
    }

    /// `lute play` (dsl 0.21.0 §6): present ONE `entry` record of a lore
    /// artifact — the same `--entry` surface `lute run` sets — so an entry
    /// beat follows the entry rules (first-read effects, `entry.<id>.read`).
    pub fn with_entry(mut self, id: &str) -> Self {
        self.entry = Some(id.to_string());
        self
    }

    /// dsl 0.23.0 §4: present ONE bundle `beat` record of a lore artifact
    /// (`lute play` for a `bundle` beat, `lute run --beat`).
    pub fn with_bundle_beat(mut self, id: &str) -> Self {
        self.bundle_beat = Some(id.to_string());
        self
    }

    /// Prerelease N8: the cast display names `{{occasion.target}}` renders a
    /// member by (`lute play`, `lute trace`).
    pub fn with_display_names(mut self, names: &BTreeMap<String, String>) -> Self {
        self.display_names = names.clone();
        self
    }

    /// The differential harness's IR oracle (design §4.4): every `match` arm
    /// judged by its structured `expr` is observed ([`Driver::observe`]) as
    /// `{"kind":"armExpr","addr","arm","expr","held","reads"}` — `held` the
    /// evaluator's verdict (`null`: undecided), `reads` each state path the
    /// `expr` names with its value (`null`: unset) — so the `expr` can be
    /// judged again by the IR's own evaluator.
    pub fn with_arm_probe(mut self) -> Self {
        self.probe_arms = true;
        self
    }

    /// dsl 0.26.0 §5: the member a `target="kind:<kind>"` beat was raised
    /// for, readable as `occasion.target` (cleared with `None`).
    pub fn bind_occasion_target(&mut self, member: Option<&str>) {
        let path = lute_check::beats::OCCASION_TARGET;
        self.occasion_target = member.map(str::to_string);
        match member {
            Some(m) => self.store.put(path.to_string(), Value::Str(m.to_string())),
            None => self.store.remove(path),
        }
    }

    /// `lute play` (dsl 0.21.0 §7a.1): the playthrough's presented scenes,
    /// joined to any seed `visited`, so `visited('<id>')` reads real
    /// presentation history.
    pub fn with_visited(mut self, visited: &BTreeSet<String>) -> Self {
        self.store.visit_all(visited);
        self
    }

    /// `lute play` (dsl 0.23.0 §2): the objectives earlier advances failed
    /// through their `by`, so a resumed walk neither completes nor fails them
    /// again.
    pub fn with_failed_objectives(mut self, failed: &BTreeSet<String>) -> Self {
        self.failed_objectives.extend(failed.iter().cloned());
        self
    }

    /// `lute play` (dsl 0.24.0 §2.1): the step raises `raise` (`name` or
    /// `name@target`) — the settles before it defer the `by` of the `on=`
    /// objectives it judges ([`Machine::defer_by`]).
    pub fn with_deferred_by(mut self, raise: Option<&str>) -> Self {
        self.defer_by.extend(raise.map(str::to_string));
        self
    }

    /// `lute play` (dsl 0.24.0 §2): answering a `judge: before` raise —
    /// collect the firing `<on>` handlers' bodies instead of running them
    /// ([`Machine::deferred_handlers`]).
    pub fn with_deferred_handlers(mut self, defer: bool) -> Self {
        self.deferred_handlers = defer.then(Vec::new);
        self
    }

    /// dsl 0.25.0 §7: what content reads of the bridge results — the
    /// project's, for `lute play` (whose documents read each other's).
    pub fn with_bridge_reads(mut self, reads: Arc<BridgeReads>) -> Self {
        self.bridge_reads = reads;
        self
    }

    /// Layer the seed's `state`/`facts` over whatever live state/facts the
    /// constructor already set (override, per path/fact — never a reset):
    /// shared by [`Machine::new`] (over the artifact's own defaults) and
    /// [`Machine::resume`] (over the prior scene's carryover), so the "seed
    /// wins on conflict" rule applies identically either way. The reserved
    /// `clock.*` values then follow the (seeded) `day` / `slot` — an
    /// explicitly seeded `clock.*` value is kept.
    fn apply_seeds(&mut self) {
        let seeds = std::mem::take(&mut self.seed.state);
        for (path, lit) in &seeds {
            let v = self.store.coerce_literal(path, lit);
            self.store.put(path.clone(), v);
        }
        self.store.refresh_clock();
        for (path, lit) in &seeds {
            if lute_manifest::clock::is_clock_path(path) {
                let v = self.store.coerce_literal(path, lit);
                self.store.put(path.clone(), v);
            }
        }
        self.seed.state = seeds;
        for f in &self.seed.facts {
            if let Some(fact) = parse_ground_fact(f) {
                self.store.assert(fact);
            }
        }
    }

    /// Consume a Machine after [`Machine::run`] into the carried world plus
    /// its driver (which holds what the driver collected: the transcript,
    /// the scripted-choice cursor, the unconsumed bridge answers). Only
    /// meaningful post-`run`; a pre-run carry would just echo the seeds back.
    pub fn into_carry(self) -> (Carry, D) {
        let quest_status = self.quest_status;
        let quest_instances = self.quest_instances;
        let (state, base_facts) = self.store.into_parts();
        let carry = Carry {
            state,
            base_facts,
            quest_status,
            quest_instances,
            incomplete: self.incomplete,
            unresolved: self.unresolved,
            accepted: self.accepted,
            accepted_next_run: self.accepted_next_run,
            refused: self.refused,
            failed_objectives: self.failed_objectives,
            deferred_handlers: self.deferred_handlers.unwrap_or_default(),
        };
        (carry, self.driver)
    }
}
