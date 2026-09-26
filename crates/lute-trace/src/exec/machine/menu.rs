//! Menus and decisions: `choice` and `hub` presentation through
//! [`Driver::choose`], option verdicts, the refusal of a scripted pick that
//! is not offered, and `match` arms (an `is` arm's structured `expr` read
//! as CEL, [`expr_to_cel`]).

use std::collections::BTreeSet;

use serde_json::{json, Value as Json};

use super::format::value_to_json;
use super::{addr, Machine, Site, Step, NOTE_NO_DECISION, NOTE_NO_ELIGIBLE, NOTE_SKIPPED};
use crate::eval::Read;
use crate::exec::driver::{
    guard_premise, Driver, Forced, GuardRead, Menu, MenuKind, MenuOption, Pick, SiteKind, Verdict,
};
use crate::{UnresolvedAtom, Value};

/// One premise of a failed rule attempt, named for a refusal when it is
/// what the attempt misses (HW27-10).
fn missing(p: &crate::datalog::Premise) -> Option<String> {
    use crate::datalog::{render_fact, Premise};
    match p {
        Premise::Missing { atom, .. } => Some(format!("`{atom}` does not hold")),
        Premise::Present(proof) => {
            let f = render_fact(proof.fact());
            Some(format!("`not {f}` fails: `{f}` holds"))
        }
        Premise::Test {
            text,
            holds: Some(false),
        } => Some(format!("`{text}` is false")),
        Premise::Test { text, holds: None } => Some(format!("`{text}` is undecided")),
        _ => None,
    }
}

impl<D: Driver> Machine<D> {
    /// A walk-time `E-TRACE-CHOICE`: the script forced an option that is not
    /// offered at this presentation point. Halts like a fatal error, flagged
    /// so `lute play` can report it as the error (exit 1) it is.
    pub(super) fn refuse(&mut self, msg: String) {
        self.fatal = Some(msg);
        self.refused = true;
    }

    /// Judge one option guard for a menu: `Open` / `Closed`, or `Unknown`
    /// with the atoms this evaluation recorded (left in
    /// [`Machine::unresolved`]).
    fn option_verdict(&mut self, when: &str) -> Verdict {
        match self.eval_atoms(when) {
            (Value::Bool(true), _) => Verdict::Open,
            (Value::Bool(false), _) => Verdict::Closed(Vec::new()),
            (_, atoms) => Verdict::Unknown(atoms),
        }
    }

    /// [`Machine::option_verdict`] for a scripted pick the driver rules on:
    /// a guard that decided false names the reads it is false over
    /// ([`Machine::false_reads`], round-5 T3-12).
    fn picked_verdict(&mut self, when: &str) -> Verdict {
        match self.option_verdict(when) {
            Verdict::Closed(_) => Verdict::Closed(self.false_reads(when)),
            verdict => verdict,
        }
    }

    /// The premises a guard `raw` that decided false is false over (round-5
    /// T3-12, HW27-10), in document order: every state path it reads with
    /// its value, every fact pattern that does not hold (its path arguments
    /// read, `canEnter(occasion.target)` → `canEnter(office)`; a derived
    /// one with each rule that could conclude it and the premises that rule
    /// misses), every scene `visited(…)` has not seen. What a refusal
    /// names — a scripted pick's (`E-TRACE-CHOICE`), an occasion gate's
    /// (`E-OCCASION-GATE`) — so each tool says what to change.
    pub fn false_reads(&mut self, raw: &str) -> Vec<GuardRead> {
        let mut atoms = Vec::new();
        if let Some(expr) = crate::exec::store::parse(raw) {
            crate::eval::guard_atoms(&expr, &mut atoms);
        }
        atoms
            .into_iter()
            .filter_map(|a| match a {
                crate::eval::GuardAtom::Path(p) => {
                    let v = match self.store.read(&p) {
                        Read::Value(v) => v,
                        Read::Unset => Value::Unknown,
                    };
                    Some(GuardRead::Path(p, v))
                }
                crate::eval::GuardAtom::Fact(f) => {
                    if self.store.eval(&format!("holds({f})")).0 != Value::Bool(false) {
                        return None;
                    }
                    Some(self.fact_read(&f))
                }
                crate::eval::GuardAtom::Visited(k) => {
                    (!self.store.visited.contains(&k)).then_some(GuardRead::Visited(k))
                }
            })
            .collect()
    }

    /// OT-F-10: the conjuncts of a guard `raw` that decided false which are
    /// false themselves, in document order — every top-level `&&`, a
    /// parenthesized conjunction split again — each with the reads it is
    /// false over ([`Machine::conjunct_reads`]). `raw` is the one conjunct
    /// when its top level is no conjunction; empty unless `raw` is false.
    pub fn false_conjuncts(&mut self, raw: &str) -> Vec<(String, Vec<GuardRead>)> {
        let mut out = Vec::new();
        if self.store.eval(raw).0 == Value::Bool(false) {
            self.push_false_conjuncts(raw, &mut out);
        }
        out
    }

    /// [`Machine::false_conjuncts`] of a `raw` known false.
    fn push_false_conjuncts(&mut self, raw: &str, out: &mut Vec<(String, Vec<GuardRead>)>) {
        use lute_check::templates::{top_level_and, unparen};
        let c = unparen(raw);
        let parts = top_level_and(c);
        if parts.len() < 2 {
            out.push((c.to_string(), self.conjunct_reads(c)));
            return;
        }
        for part in parts {
            if self.store.eval(part).0 == Value::Bool(false) {
                self.push_false_conjuncts(part, out);
            }
        }
    }

    /// Every read of one false conjunct `raw`, in document order: each state
    /// path with its value, each fact pattern with whether it holds (one
    /// that does not with [`Machine::fact_read`]'s why), each scene
    /// `visited(…)` asks about with whether it is visited. Undecided facts
    /// are left out.
    fn conjunct_reads(&mut self, raw: &str) -> Vec<GuardRead> {
        let mut atoms = Vec::new();
        if let Some(expr) = crate::exec::store::parse(raw) {
            crate::eval::guard_atoms(&expr, &mut atoms);
        }
        atoms
            .into_iter()
            .filter_map(|a| match a {
                crate::eval::GuardAtom::Path(p) => {
                    let v = match self.store.read(&p) {
                        Read::Value(v) => v,
                        Read::Unset => Value::Unknown,
                    };
                    Some(GuardRead::Path(p, v))
                }
                crate::eval::GuardAtom::Fact(f) => {
                    match self.store.eval(&format!("holds({f})")).0 {
                        Value::Bool(true) => {
                            let (rel, args) = self.fact_args(&f);
                            Some(GuardRead::Holds(format!("{rel}({})", args.join(", "))))
                        }
                        Value::Bool(false) => Some(self.fact_read(&f)),
                        _ => None,
                    }
                }
                crate::eval::GuardAtom::Visited(k) if self.store.visited.contains(&k) => {
                    Some(GuardRead::Seen(k))
                }
                crate::eval::GuardAtom::Visited(k) => Some(GuardRead::Visited(k)),
            })
            .collect()
    }

    /// A fact pattern `rel(a, b)` as its relation and arguments, its path
    /// arguments read (an unset one kept as written).
    fn fact_args<'p>(&mut self, pattern: &'p str) -> (&'p str, Vec<String>) {
        let (rel, args) = pattern
            .strip_suffix(')')
            .and_then(|p| p.split_once('('))
            .unwrap_or((pattern, ""));
        let args = args
            .split(", ")
            .filter(|a| !a.is_empty())
            .map(|a| match self.store.read(a) {
                Read::Value(v) if a.contains('.') => {
                    crate::report::value_text(&v).unwrap_or_else(|| a.to_string())
                }
                _ => a.to_string(),
            })
            .collect();
        (rel, args)
    }

    /// A fact pattern `rel(a, b)` that does not hold, its path arguments
    /// read ([`Machine::fact_args`]); a ground fact of a derived relation
    /// carries why no rule concludes it.
    fn fact_read(&mut self, pattern: &str) -> GuardRead {
        let (rel, args) = self.fact_args(pattern);
        let fact = format!("{rel}({})", args.join(", "));
        let ground = !args.iter().any(|a| a == "_" || a.contains('.'));
        match ground
            .then(|| self.store.why_not(&(rel.to_string(), args)))
            .flatten()
        {
            Some(attempts) => GuardRead::Derived {
                fact,
                rules: attempts
                    .iter()
                    .map(|a| {
                        (
                            a.rule.clone(),
                            a.premises.iter().filter_map(missing).collect(),
                        )
                    })
                    .collect(),
            },
            None => GuardRead::Fact(fact),
        }
    }

    /// A branch's options judged right now — what its menu shows. Display
    /// only: the evaluation never feeds [`Machine::unresolved`], so an
    /// unchosen option's unknown guard cannot halt a playthrough (an unknown
    /// guard counts as offered).
    fn branch_verdicts(&mut self, options: &[Json]) -> Vec<MenuOption> {
        let mark = self.unresolved.len();
        let mut out = Vec::new();
        for o in options {
            let Some(oid) = o.get("id").and_then(Json::as_str) else {
                continue;
            };
            let verdict = match o.get("when").and_then(Json::as_str) {
                Some(when) => self.option_verdict(when),
                None => Verdict::Open,
            };
            out.push(MenuOption {
                id: oid.to_string(),
                verdict,
                exit: o.get("exit").and_then(Json::as_bool).unwrap_or(false),
                once: o.get("once").and_then(Json::as_bool).unwrap_or(false),
            });
        }
        self.unresolved.truncate(mark);
        out
    }

    /// The `E-TRACE-CHOICE` text for a picked option the driver refused.
    fn refusal(construct: &str, id: &str, option: &str, when: &str, verdict: &Verdict) -> String {
        match verdict {
            Verdict::Spent => format!(
                "[E-TRACE-CHOICE] `choose: {id}: {option}` names a `once` option already \
                 taken at this {construct}, so it is no longer offered"
            ),
            Verdict::Unknown(_) => format!(
                "[E-TRACE-CHOICE] `choose: {id}: {option}` names an option whose guard \
                 `{when}` is undecided at this presentation point"
            ),
            Verdict::Open | Verdict::Closed(_) => {
                let premise = match verdict {
                    Verdict::Closed(reads) if !reads.is_empty() => {
                        format!(": {}", guard_premise(reads, GuardRead::yaml_mock))
                    }
                    _ => String::new(),
                };
                format!(
                    "[E-TRACE-CHOICE] `choose: {id}: {option}` names an option whose guard \
                     `{when}` decided false at this presentation point{premise}"
                )
            }
        }
    }

    pub(super) fn do_choice(&mut self, cmd: &Json) -> Step {
        let branch = cmd
            .get("branchId")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let record_key = cmd
            .get("recordKey")
            .and_then(Json::as_str)
            .map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let options = cmd
            .get("options")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();

        let judged = self.branch_verdicts(&options);
        // A menu marks what was not offered.
        let closed: Vec<&str> = judged
            .iter()
            .filter(|o| matches!(o.verdict, Verdict::Closed(_)))
            .map(|o| o.id.as_str())
            .collect();
        let menu = Menu {
            construct: MenuKind::Branch,
            id: &branch,
            addr: addr(cmd),
            presentation: 0,
            options: &judged,
        };
        let pick = self.driver.choose(&menu);
        let auto = matches!(pick, Pick::AutoFirst | Pick::HubAutoPass);
        let (chosen, scripted) = match pick {
            Pick::Option(id) => (Some(id), 0),
            Pick::AutoFirst | Pick::HubAutoPass => (
                judged
                    .iter()
                    .find(|o| o.verdict == Verdict::Open)
                    .map(|o| o.id.clone()),
                0,
            ),
            Pick::Unscripted { scripted } => (None, scripted),
            Pick::Leave => (None, 0),
        };
        let incomplete_rec = |note: &str| {
            let mut rec = serde_json::Map::new();
            rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
            rec.insert("kind".into(), Json::String("choice".into()));
            rec.insert("branch".into(), Json::String(branch.clone()));
            rec.insert("chose".into(), Json::Null);
            rec.insert("note".into(), Json::String(note.into()));
            if scripted > 1 {
                rec.insert("scripted".into(), json!(scripted));
            }
            if !closed.is_empty() {
                rec.insert("ineligible".into(), json!(closed));
            }
            Json::Object(rec)
        };
        let Some(forced) = chosen else {
            if auto {
                // Nothing open: every option closed or undecided.
                let atoms = unknown_atoms(&judged);
                let site = Site::new(SiteKind::BranchAllUnknown, &branch, addr(cmd));
                self.at_unknown(site, "", &atoms);
            }
            self.incomplete = true;
            let note = if auto {
                NOTE_NO_ELIGIBLE
            } else {
                NOTE_NO_DECISION
            };
            self.driver.emit(incomplete_rec(note));
            return Step::Halt;
        };
        let opt = match options
            .iter()
            .find(|o| o.get("id").and_then(Json::as_str) == Some(&forced))
        {
            Some(o) => o.clone(),
            None => {
                self.fatal = Some(format!("choice `{branch}` has no option `{forced}`"));
                return Step::Halt;
            }
        };
        // #20 / T8.5, D-C: `lute trace` refuses a forced selection whose guard
        // decided false, and `lute test` inherits that refusal. `run` played
        // it in full at exit 0 — one question, three tools, two answers. The
        // guard is in the artifact as `option.when` and this walk already
        // evaluates CEL everywhere else in it (`do_match`). The driver rules
        // on a pick that is not open ([`Driver::forced`]); `run` and `play`
        // refuse only a DECIDED false — an unknown guard read something with
        // no mock surface (`now()`/`validAt(...)`, a bridgeResult), and
        // refusing on that would refuse a legal replay.
        if !auto {
            if let Some(when) = opt.get("when").and_then(Json::as_str) {
                let verdict = self.picked_verdict(when);
                if verdict != Verdict::Open {
                    match self.driver.forced(&menu, &forced, &verdict) {
                        Forced::Take => {}
                        Forced::Refuse => {
                            self.refuse(Self::refusal("branch", &branch, &forced, when, &verdict));
                            return Step::Halt;
                        }
                        Forced::Skip => {
                            self.incomplete = true;
                            self.driver.emit(incomplete_rec(NOTE_SKIPPED));
                            return Step::Halt;
                        }
                    }
                }
            }
        }
        if let Some(key) = record_key {
            self.write(&key, Value::Str(forced.clone()));
        }
        let mut rec = serde_json::Map::new();
        rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
        rec.insert("kind".into(), Json::String("choice".into()));
        rec.insert("branch".into(), Json::String(branch.clone()));
        rec.insert("chose".into(), Json::String(forced));
        if !closed.is_empty() {
            rec.insert("ineligible".into(), json!(closed));
        }
        self.driver.emit(Json::Object(rec));
        let target = opt.get("target").and_then(Json::as_str).unwrap_or(converge);
        Step::Next(self.resolve(target))
    }

    /// The hub re-presentation loop: the driver answers ONE presentation at
    /// a time — a script's sequence is never consumed up front — so running
    /// out of scripted decisions can be told apart from a genuine natural
    /// convergence. Exhaustion with eligible options still standing halts
    /// incomplete (`self.incomplete = true`, exit 3) rather than silently
    /// converging. Every iteration consumes one decision or leaves the loop.
    pub(super) fn do_hub(&mut self, cmd: &Json) -> Step {
        let id = cmd
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let record_key = cmd
            .get("recordKey")
            .and_then(Json::as_str)
            .map(str::to_string);
        // dsl 0.23.0 §4: `<hub prompt>` rides every presentation record.
        let prompt = cmd.get("prompt").and_then(Json::as_str).map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let converge_idx = self.resolve(converge);
        let options = cmd
            .get("options")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();

        // Segment boundaries: every option target + the converge, so an option
        // body runs from its target up to the NEXT boundary (a non-`exit`
        // option falls through into the next option's body in the stream).
        let mut boundaries: Vec<usize> = options
            .iter()
            .filter_map(|o| o.get("target").and_then(Json::as_str))
            .map(|t| self.resolve(t))
            .collect();
        boundaries.push(converge_idx);
        boundaries.sort_unstable();
        boundaries.dedup();

        let mut presentation = 0usize;
        // [`Pick::HubAutoPass`]: the next menu position the pass considers.
        let mut auto_at = 0usize;
        loop {
            // Eligible = not a `once` option already taken (its
            // `scene.visited.<hub>.<option>` is true, D8: the memory is the
            // reserved visit record, so a hub `::next` re-enters still knows
            // it), and its guard does not DECIDE false right now (an unknown
            // guard stays eligible — the same three-valued discipline
            // `do_choice`'s guard refusal uses). The spent and guard-closed
            // options ride the visit record, so a menu shows what was really
            // offered.
            let mut judged = Vec::new();
            for o in &options {
                let Some(oid) = o.get("id").and_then(Json::as_str) else {
                    continue;
                };
                let once = o.get("once").and_then(Json::as_bool).unwrap_or(false);
                let verdict = if once && self.hub_visited(&id, oid) {
                    Verdict::Spent
                } else {
                    match o.get("when").and_then(Json::as_str) {
                        Some(w) => self.option_verdict(w),
                        None => Verdict::Open,
                    }
                };
                judged.push(MenuOption {
                    id: oid.to_string(),
                    verdict,
                    exit: o.get("exit").and_then(Json::as_bool).unwrap_or(false),
                    once,
                });
            }
            let with = |v: fn(&Verdict) -> bool| -> Vec<&str> {
                judged
                    .iter()
                    .filter(|o| v(&o.verdict))
                    .map(|o| o.id.as_str())
                    .collect()
            };
            let spent = with(|v| *v == Verdict::Spent);
            let closed = with(|v| matches!(v, Verdict::Closed(_)));
            let any_eligible = judged
                .iter()
                .any(|o| !matches!(o.verdict, Verdict::Spent | Verdict::Closed(_)));
            let marks = |rec: &mut serde_json::Map<String, Json>| {
                if !spent.is_empty() {
                    rec.insert("spent".into(), json!(spent));
                }
                if !closed.is_empty() {
                    rec.insert("ineligible".into(), json!(closed));
                }
            };
            let head = |chose: Json| {
                let mut rec = serde_json::Map::new();
                rec.insert("addr".into(), Json::String(addr(cmd).to_string()));
                rec.insert("kind".into(), Json::String("hub".into()));
                rec.insert("hub".into(), Json::String(id.clone()));
                if let Some(p) = &prompt {
                    rec.insert("prompt".into(), Json::String(p.clone()));
                }
                rec.insert("chose".into(), chose);
                rec
            };

            let menu = Menu {
                construct: MenuKind::Hub,
                id: &id,
                addr: addr(cmd),
                presentation,
                options: &judged,
            };
            presentation += 1;
            let first_open =
                |exit: Option<bool>, from: usize| {
                    judged.iter().enumerate().skip(from).find(|(_, o)| {
                        o.verdict == Verdict::Open && exit.is_none_or(|e| o.exit == e)
                    })
                };
            let (choice_id, auto) = match self.driver.choose(&menu) {
                Pick::Option(c) => (Some(c), false),
                Pick::Unscripted { .. } => (None, false),
                Pick::Leave => break,
                Pick::AutoFirst => (first_open(None, 0).map(|(_, o)| o.id.clone()), true),
                Pick::HubAutoPass => match first_open(Some(false), auto_at) {
                    Some((i, o)) => {
                        auto_at = i + 1;
                        (Some(o.id.clone()), true)
                    }
                    None => {
                        auto_at = judged.len();
                        (first_open(Some(true), 0).map(|(_, o)| o.id.clone()), true)
                    }
                },
            };

            let Some(choice_id) = choice_id else {
                if !auto && !any_eligible {
                    // Natural convergence: nothing left eligible to present.
                    break;
                }
                if auto {
                    // No open exit: the exits are closed or undecided.
                    let exits: Vec<MenuOption> =
                        judged.iter().filter(|o| o.exit).cloned().collect();
                    let atoms = unknown_atoms(&exits);
                    let site = Site::new(SiteKind::HubAllUnknown, &id, addr(cmd));
                    self.at_unknown(site, "", &atoms);
                }
                // The decisions ran out but the hub would still be
                // re-presented (eligible options remain) — halt incomplete
                // rather than silently converging.
                self.incomplete = true;
                let mut rec = head(Json::Null);
                let note = if auto {
                    NOTE_NO_ELIGIBLE
                } else {
                    NOTE_NO_DECISION
                };
                rec.insert("note".into(), Json::String(note.into()));
                marks(&mut rec);
                self.driver.emit(Json::Object(rec));
                return Step::Halt;
            };

            let opt = match options
                .iter()
                .find(|o| o.get("id").and_then(Json::as_str) == Some(&choice_id))
            {
                Some(o) => o.clone(),
                None => {
                    self.fatal = Some(format!("hub `{id}` has no option `{choice_id}`"));
                    return Step::Halt;
                }
            };
            let once = opt.get("once").and_then(Json::as_bool).unwrap_or(false);
            let is_exit = opt.get("exit").and_then(Json::as_bool).unwrap_or(false);
            if !auto {
                // A repeat force of a spent `once` option: `lute run` (the
                // conformance contract) skips it; `lute play` refuses it, as
                // `lute trace` does — a silent skip lets the rest of the
                // script drift out of step with what the player was offered.
                if once && self.hub_visited(&id, &choice_id) {
                    match self.driver.forced(&menu, &choice_id, &Verdict::Spent) {
                        Forced::Take => {}
                        Forced::Skip => continue,
                        Forced::Refuse => {
                            self.refuse(Self::refusal("hub", &id, &choice_id, "", &Verdict::Spent));
                            return Step::Halt;
                        }
                    }
                }
                // Same rule as `do_choice` (#20, D-C). A hub option is
                // presented repeatedly, so this is evaluated per visit
                // against live state — a guard false on the first pass may
                // be true on the third, which is precisely what a hub is for.
                if let Some(when) = opt.get("when").and_then(Json::as_str) {
                    let verdict = self.picked_verdict(when);
                    if verdict != Verdict::Open {
                        match self.driver.forced(&menu, &choice_id, &verdict) {
                            Forced::Take => {}
                            Forced::Skip => continue,
                            Forced::Refuse => {
                                self.refuse(Self::refusal("hub", &id, &choice_id, when, &verdict));
                                return Step::Halt;
                            }
                        }
                    }
                }
            }
            if let Some(key) = &record_key {
                self.write(key, Value::Str(choice_id.clone()));
            }
            // hub visit record slot (scene.visited.<hub>.<opt>, state-lifecycle.md).
            self.write(
                &format!("scene.visited.{id}.{choice_id}"),
                Value::Bool(true),
            );
            if self.stopped() {
                return Step::Halt;
            }
            let mut rec = head(Json::String(choice_id.clone()));
            marks(&mut rec);
            self.driver.emit(Json::Object(rec));
            let target = opt.get("target").and_then(Json::as_str).unwrap_or(converge);
            let start = self.resolve(target);
            let stop = boundaries
                .iter()
                .find(|&&b| b > start)
                .copied()
                .unwrap_or(self.commands.len());
            self.run_range(start, stop);
            if self.stopped() {
                return Step::Halt;
            }
            if is_exit {
                break;
            }
        }
        Step::Next(converge_idx)
    }

    /// Whether hub `hub`'s option `option` was taken (its reserved
    /// `scene.visited.<hub>.<option>` visit record).
    fn hub_visited(&self, hub: &str, option: &str) -> bool {
        matches!(
            self.store.read(&format!("scene.visited.{hub}.{option}")),
            Read::Value(Value::Bool(true))
        )
    }

    /// `<match>`: the first arm whose condition holds is taken, else
    /// `otherwise`, else converge. Every arm is judged by the one CEL
    /// evaluator: its `test` text, or — an `is` arm without one — its
    /// structured `expr` ([`expr_to_cel`]); S3 guarantees one of the two.
    /// An `is` pattern over a bare path subject that is unset matches only
    /// `unset` (definite, as `isSet` is), so an arm the unset subject cannot
    /// match is false, never unknown — except `occasion.target`, a binding,
    /// which is unknown until bound (T1-9). An undecided arm is an
    /// [`UnknownSite`] ([`SiteKind::Arm`], or [`SiteKind::OccasionTarget`]
    /// when the unbound target is what is missing): a driver that halts
    /// stops the walk here; one that continues skips the arm.
    pub(super) fn do_match(&mut self, cmd: &Json) -> Step {
        let arms = cmd
            .get("arms")
            .and_then(Json::as_array)
            .cloned()
            .unwrap_or_default();
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let subject = cmd.get("subject").and_then(Json::as_str).unwrap_or("");
        let subject_unset = crate::exec::store::parse(subject)
            .and_then(|e| crate::eval::expr_path(&e))
            .filter(|p| p != lute_check::beats::OCCASION_TARGET)
            .is_some_and(|p| self.store.read(&p) == Read::Unset);
        for (i, arm) in arms.iter().enumerate() {
            let test = arm.get("test").and_then(Json::as_str).unwrap_or("");
            let is_arm = test.trim().is_empty();
            let raw = if is_arm {
                arm.get("expr").and_then(expr_to_cel).unwrap_or_default()
            } else {
                test.to_string()
            };
            let (v, atoms) = self.eval_atoms(&raw);
            if self.probe_arms && is_arm {
                if let Some(expr) = arm.get("expr") {
                    let mut paths = BTreeSet::new();
                    expr_paths(expr, &mut paths);
                    let reads: serde_json::Map<String, Json> = paths
                        .into_iter()
                        .map(|p| {
                            let v = match self.store.read(&p) {
                                Read::Value(Value::Unknown) => json!({ "unknown": true }),
                                Read::Value(v) => value_to_json(&v),
                                Read::Unset => Json::Null,
                            };
                            (p, v)
                        })
                        .collect();
                    let held = match &v {
                        Value::Bool(b) => Json::Bool(*b),
                        _ => Json::Null,
                    };
                    self.driver.observe(json!({
                        "kind": "armExpr", "addr": addr(cmd), "arm": i, "expr": expr,
                        "held": held, "reads": reads,
                    }));
                }
            }
            let matched = match v {
                Value::Bool(b) => b,
                _ if is_arm && subject_unset => false,
                _ => {
                    let target_unbound = atoms.iter().any(|a| {
                        matches!(a, UnresolvedAtom::Path(p) if p == lute_check::beats::OCCASION_TARGET)
                    });
                    let kind = if target_unbound {
                        SiteKind::OccasionTarget
                    } else {
                        SiteKind::Arm
                    };
                    let site = Site {
                        arm: Some(i),
                        ..Site::new(kind, subject, addr(cmd))
                    };
                    if self.at_unknown(site, &raw, &atoms) {
                        return Step::Halt;
                    }
                    false
                }
            };
            if matched {
                let target = arm.get("target").and_then(Json::as_str).unwrap_or(converge);
                self.driver.emit(json!({
                    "addr": addr(cmd),
                    "kind": "match",
                    "result": format!("arm {}", i + 1),
                }));
                return Step::Next(self.resolve(target));
            }
        }
        // No arm matched → otherwise, else converge.
        let (result, target) = match cmd.get("otherwise").and_then(Json::as_str) {
            Some(o) => ("otherwise".to_string(), o),
            None => ("converge".to_string(), converge),
        };
        self.driver.emit(json!({
            "addr": addr(cmd),
            "kind": "match",
            "result": result,
        }));
        Step::Next(self.resolve(target))
    }
}

/// The atoms of every undecided option of a menu.
fn unknown_atoms(options: &[MenuOption]) -> Vec<UnresolvedAtom> {
    options
        .iter()
        .filter_map(|o| match &o.verdict {
            Verdict::Unknown(atoms) => Some(atoms.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// An IR structured `expr` node (`lute_compile::expr::ExprNode`'s serialized
/// shape — `lit` / `path` / `op` / `cond` / `list` / `isSet` / `has`, a
/// subset of CEL) as CEL text, so an `is` arm is judged by the one CEL
/// evaluator (design §3.4). Every operand is parenthesized. `None` for a
/// shape outside the set.
/// Every state path an IR `expr` node names (`path`, `isSet`, `has`).
fn expr_paths(node: &Json, out: &mut BTreeSet<String>) {
    match node {
        Json::Object(map) => {
            for (k, v) in map {
                match (k.as_str(), v) {
                    ("path" | "isSet" | "has", Json::String(p)) => {
                        out.insert(p.clone());
                    }
                    ("lit", _) => {}
                    _ => expr_paths(v, out),
                }
            }
        }
        Json::Array(items) => items.iter().for_each(|n| expr_paths(n, out)),
        _ => {}
    }
}

pub fn expr_to_cel(node: &Json) -> Option<String> {
    if let Some(lit) = node.get("lit") {
        return Some(match lit {
            Json::String(s) => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
            Json::Bool(b) => b.to_string(),
            Json::Number(n) => match n.as_f64() {
                Some(f) if f.fract() == 0.0 && f.abs() < 9.007e15 => format!("{}", f as i64),
                Some(f) => f.to_string(),
                None => return None,
            },
            _ => return None,
        });
    }
    if let Some(path) = node.get("path").and_then(Json::as_str) {
        return Some(path.to_string());
    }
    if let Some(path) = node.get("isSet").and_then(Json::as_str) {
        return Some(format!("isSet({path})"));
    }
    if let Some(path) = node.get("has").and_then(Json::as_str) {
        return Some(format!("has({path})"));
    }
    if let Some(items) = node.get("list").and_then(Json::as_array) {
        let items: Option<Vec<String>> = items.iter().map(expr_to_cel).collect();
        return Some(format!("[{}]", items?.join(", ")));
    }
    if let (Some(c), Some(t), Some(e)) = (node.get("cond"), node.get("then"), node.get("else")) {
        return Some(format!(
            "({}) ? ({}) : ({})",
            expr_to_cel(c)?,
            expr_to_cel(t)?,
            expr_to_cel(e)?
        ));
    }
    let op = node.get("op").and_then(Json::as_str)?;
    let l = expr_to_cel(node.get("l")?)?;
    match node.get("r") {
        Some(r) => Some(format!("({l}) {op} ({})", expr_to_cel(r)?)),
        None => Some(format!("{op}({l})")),
    }
}
