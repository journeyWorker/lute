//! One world, one orchestration (`docs/design/runtime-unification.md`
//! §3.7, S5): the playthrough state every occasion-driven runtime carries
//! from one step to the next — the tiers, `once` spending, the clock, the
//! presented scenes, quest statuses, accepts, failed objectives, deferred
//! `by` deadlines and handlers — and every rule that moves it: seeding a
//! save, eligibility and selection, presenting a beat through the
//! [`Machine`], folding the walk back, the quest-lifecycle fixpoint, raising
//! occasions and events, `engine:` writes, a new run and clock movement.
//! `lute play`, `lute calendar` and the play files of `lute test` drive it
//! through [`Session`]; they keep only script parsing, planning, the step
//! loop's I/O and rendering.
//!
//! The session builds every walk's [`Machine`] itself, so the walk driver is
//! the session's: [`PlayDriver`] (scripted `choose:` over the world's
//! cursor, the world's bridge answers, refusal of a pick that is not
//! offered, a halt at every unknown site).
//!
//! wasm-clean: no filesystem, process or threads.
//!
//! [`Machine`]: crate::Machine

mod advance;
mod eligibility;
mod lifecycle;
mod present;
mod producers;
mod project;
mod resolve;
mod step;
mod walk;
mod world;

pub use advance::*;
pub use eligibility::*;
pub use lifecycle::*;
pub use present::*;
pub use producers::*;
pub use project::*;
pub use resolve::*;
pub use step::*;
pub use walk::*;
pub use world::*;

use std::collections::BTreeMap;

use crate::Value;

/// What one step operation did: its body record, the quest advances it
/// made after the body, and what ends the playthrough, if anything.
pub type StepOutcome = (StepBody, Vec<QuestAdvance>, Option<PlayHalt>);

/// One playthrough over one project (`docs/design/runtime-unification.md`
/// §3.7): the [`World`] and every operation that moves it. Every operation
/// that changes the world settles the quest lifecycle after it — a
/// presentation, an `engine:` write, a new run (dsl 0.22.0 §1.1) — and a
/// raised occasion or event is then answered by the quests. `n` is the
/// script step an operation runs for, the `N` of its "step N" messages.
#[derive(Clone)]
pub struct Session<'p> {
    project: &'p ExecProject,
    pub world: World,
}

impl<'p> Session<'p> {
    /// The playthrough's starting world ([`seed_world`]); `Err` is every seed
    /// the project cannot take.
    pub fn seed<B, C>(
        project: &'p ExecProject,
        seed: &WorldSeed<'_, B, C>,
    ) -> Result<Self, Vec<SeedError>> {
        Ok(Session {
            project,
            world: seed_world(project, seed)?,
        })
    }

    /// A session over `world` as it stands.
    pub fn resume(project: &'p ExecProject, world: World) -> Self {
        Session { project, world }
    }

    pub fn project(&self) -> &'p ExecProject {
        self.project
    }

    /// The start settle: every quest lifecycle advanced to a fixpoint.
    pub fn settle(&mut self) -> (Vec<QuestAdvance>, Option<PlayHalt>) {
        advance_quests(self.project, &mut self.world)
    }

    /// dsl 0.25.0 §1: every pair of exclusive facts that hold together now.
    pub fn exclusive(&self) -> Vec<String> {
        exclusive_violations(self.project, &self.world)
    }

    /// The world as expectations judge it ([`world_view`]).
    pub fn view(&self, with_facts: bool) -> WorldView {
        world_view(self.project, &self.world, with_facts)
    }

    /// The declared clock's position, `None` without one.
    pub fn clock_at(&self) -> Option<lute_manifest::clock::ClockAt> {
        clock_at(self.project, &self.world)
    }

    /// Re-derive `clock.*` after a write the walk did not make.
    pub fn refresh_clock(&mut self) {
        refresh_clock(self.project, &mut self.world)
    }

    /// Every candidate of `occasion` / `target` with its verdict, in
    /// selection order ([`eligible_at`]).
    pub fn candidates(&self, occasion: &str, target: Option<&str>) -> Vec<Candidate> {
        eligible_at(self.project, &self.world, occasion, target)
    }

    /// One beat's eligibility in this world — the one judgment `lute play`
    /// selects by, `lute calendar` reports and `lute test`'s `eligible:`
    /// asserts: `once` spending, `after:`, then `when` (reading `member` as
    /// `occasion.target` for a kind beat). `None` when no beat has that id.
    pub fn eligibility(&self, id: &str, member: Option<&str>) -> Option<Candidate> {
        let id = self.project.entry_id(id);
        let beat = self.project.index.beats.iter().find(|b| b.id == id)?;
        let mut eval = self
            .world
            .evaluator_with_schema(
                &self.project.eval_json,
                self.project.store_schemas[self.world.derive.unwrap_or(true) as usize].clone(),
            )
            .with_visited(&self.world.visited);
        Some(judge_beat(
            self.project,
            &self.world,
            &mut eval,
            beat,
            member,
            member,
        ))
    }

    /// Raise `occasion` (for `target`): its candidates' verdicts, what it
    /// presents (`pick` on a `select: all` occasion; `choose` over the
    /// script's), every quest settle after each presentation, and the
    /// quests' answer to the raise.
    pub fn occasion(
        &mut self,
        n: usize,
        occasion: &String,
        target: &Option<String>,
        pick: &Option<Pick>,
        choose: &BTreeMap<String, Vec<String>>,
    ) -> StepOutcome {
        self.world.step = n;
        // dsl 0.27.0 §4: a raise the engine would not make (its gate is
        // false, or the game is over) is refused.
        if let Some(why) =
            super::seam::closed(self.project, &self.world, occasion, target.as_deref())
        {
            let prefix = format!("{}.", lute_check::occasion_bind::OCCASION_PAYLOAD);
            self.world.state.retain(|k, _| !k.starts_with(&prefix));
            let body = StepBody::Occasion {
                occasion: occasion.clone(),
                target: target.clone(),
                select: self.project.select_of(occasion),
                pick: pick.clone(),
                candidates: Vec::new(),
                winner: None,
                decided: false,
                presented: Vec::new(),
                judged: Vec::new(),
                not_raised: Some(match why {
                    super::seam::Closed::Gate { .. } => "gate false",
                    super::seam::Closed::Terminal(_) => "the game is over",
                    super::seam::Closed::Unknown(_) => "undecided",
                }),
            };
            let halt = super::seam::refusal(n, occasion, target.as_deref(), &why, self.project);
            return (body, Vec::new(), Some(halt));
        }
        let out = run_occasion(
            self.project,
            &mut self.world,
            n,
            occasion,
            target,
            pick,
            choose,
        );
        // dsl 0.27.0 §3: a payload lives only for the raise it came with.
        let prefix = format!("{}.", lute_check::occasion_bind::OCCASION_PAYLOAD);
        self.world.state.retain(|k, _| !k.starts_with(&prefix));
        out
    }

    /// dsl 0.27.0 §3: bind the payload the next [`Session::occasion`] raise
    /// carries ([`typed_payload`]) — `occasion.payload.<field>` for that
    /// raise only.
    pub fn bind_payload(&mut self, payload: &BTreeMap<String, Value>) {
        for (path, value) in payload {
            self.world.state.insert(path.clone(), value.clone());
        }
    }

    /// dsl 0.24.0 §1: one `advance:` of the declared clock ([`run_advance`]).
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        n: usize,
        by: lute_manifest::clock::Advance,
        engine: &Writes,
        raise: &lute_manifest::clock::RaiseMoments,
        pick: &Option<Pick>,
        choose: &BTreeMap<String, Vec<String>>,
    ) -> StepOutcome {
        self.world.step = n;
        run_advance(
            self.project,
            &mut self.world,
            n,
            by,
            engine,
            raise,
            pick,
            choose,
        )
    }

    /// dsl 0.27.0 §4: whether the project's `terminal:` holds — the game is
    /// over and the engine raises no occasion (an undecided condition does
    /// not end the game).
    pub fn terminal(&self) -> bool {
        matches!(
            super::seam::terminal_holds(self.project, &self.world),
            Ok(true)
        )
    }

    /// `newRun` ([`new_run`]), then the settle.
    pub fn new_run(&mut self, n: usize, seed: &Writes) -> StepOutcome {
        let (p, w) = (self.project, &mut self.world);
        let result = new_run(p, w, seed);
        refresh_clock(p, w);
        match result {
            Ok(NewRunReport {
                writes,
                reset_quests,
                accepted,
                prev_run,
                unjudged,
            }) => {
                let (quests, stop) = advance_quests(p, w);
                let body = StepBody::NewRun {
                    writes,
                    reset_quests,
                    prev_run,
                    unjudged,
                    accepted,
                };
                (body, quests, stop)
            }
            Err(e) => (
                StepBody::NewRun {
                    writes: Vec::new(),
                    reset_quests: Vec::new(),
                    prev_run: Vec::new(),
                    unjudged: Vec::new(),
                    accepted: Vec::new(),
                },
                Vec::new(),
                Some(PlayHalt::Error(format!("step {n}: {e}"))),
            ),
        }
    }

    /// An `engine:` step's writes ([`apply_writes`]) — the clock never moves
    /// backward (dsl 0.24.0 §1) — then the settle.
    pub fn engine(&mut self, n: usize, writes: &Writes) -> StepOutcome {
        let (p, w) = (self.project, &mut self.world);
        let before = clock_at(p, w);
        match apply_writes(w, writes) {
            Ok(writes) => {
                refresh_clock(p, w);
                if let (Some(clock), Some(from), Some(to)) =
                    (&p.index.clock, before, clock_at(p, w))
                {
                    if to < from {
                        let halt = PlayHalt::Fatal(format!(
                            "step {n}: `engine:` moves the clock backward, from {} to {} \
                             (clock.index {} → {}) — the clock only moves forward; \
                             `advance:` moves it, a `newRun` starts it over",
                            clock.describe(from),
                            clock.describe(to),
                            clock.index(from),
                            clock.index(to)
                        ));
                        return (StepBody::Engine { writes }, Vec::new(), Some(halt));
                    }
                }
                let (quests, stop) = advance_quests(p, w);
                (StepBody::Engine { writes }, quests, stop)
            }
            Err(e) => (
                StepBody::Engine { writes: Vec::new() },
                Vec::new(),
                Some(PlayHalt::Error(format!("step {n}: {e}"))),
            ),
        }
    }

    /// Fire a declared world event (dsl 0.22.0 §9): the quests answer it.
    pub fn event(&mut self, event: &str) -> StepOutcome {
        let (quests, stop) = raise(self.project, &mut self.world, Raise::Event(event));
        (
            StepBody::Event {
                event: event.to_string(),
            },
            quests,
            stop,
        )
    }
}
