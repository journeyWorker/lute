//! Menus and decisions: `choice` and `hub` presentation through
//! [`Driver::choose`], option verdicts, the refusal of a scripted pick that
//! is not offered, and `match` arms.

use std::collections::BTreeSet;
use std::sync::Arc;

use serde_json::{json, Value as Json};

use super::format::value_to_json;
use super::{addr, Machine, Site, Step, NOTE_NO_DECISION, NOTE_NO_ELIGIBLE, NOTE_SKIPPED};
use crate::eval::Read;
use crate::driver::{
    guard_premise, Driver, Forced, GuardRead, Menu, MenuKind, MenuOption, Pick, SiteKind, Verdict,
};
use crate::expr::{guard_atoms, Expr, Func, GuardAtom, Slot};
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

/// The base premises `attempts` miss, each rule followed down to a missing
/// atom no rule explains further (`recruited(wren)` under `inParty(wren)`)
/// — what a play or a mock has to produce.
fn base_missing(attempts: &[crate::datalog::Attempt], out: &mut Vec<String>) {
    use crate::datalog::Premise;
    for p in attempts.iter().flat_map(|a| &a.premises) {
        match p {
            Premise::Missing { atom, why } if why.is_empty() => {
                if !out.contains(atom) {
                    out.push(atom.clone());
                }
            }
            Premise::Missing { why, .. } => base_missing(why, out),
            _ => {}
        }
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

    /// A menu option's `when` slot.
    fn option_when(&self, option: &Json) -> Option<Arc<Slot>> {
        self.slot(option.get("when"))
    }

    /// Judge one option guard for a menu: `Open` / `Closed`, or `Unknown`
    /// with the atoms this evaluation recorded (left in
    /// [`Machine::unresolved`]).
    fn option_verdict(&mut self, when: &Slot) -> Verdict {
        match self.eval_atoms(when) {
            (Value::Bool(true), _) => Verdict::Open,
            (Value::Bool(false), _) => Verdict::Closed(Vec::new()),
            (_, atoms) => Verdict::Unknown(atoms),
        }
    }

    /// [`Machine::option_verdict`] for a scripted pick the driver rules on:
    /// a guard that decided false names the reads it is false over
    /// ([`Machine::false_reads`], round-5 T3-12).
    fn picked_verdict(&mut self, when: &Slot) -> Verdict {
        match self.option_verdict(when) {
            Verdict::Closed(_) => Verdict::Closed(self.false_reads(&when.expr)),
            verdict => verdict,
        }
    }

    /// The premises a guard that decided false is false over (round-5
    /// T3-12, HW27-10), in document order: every state path it reads with
    /// its value, every fact pattern that does not hold (its path arguments
    /// read, `canEnter(occasion.target)` → `canEnter(office)`; a derived
    /// one with each rule that could conclude it and the premises that rule
    /// misses), every scene `visited(…)` has not seen. What a refusal
    /// names — a scripted pick's (`E-TRACE-CHOICE`), an occasion gate's
    /// (`E-OCCASION-GATE`) — so each tool says what to change.
    pub(crate) fn false_reads(&mut self, expr: &Expr) -> Vec<GuardRead> {
        let mut atoms = Vec::new();
        guard_atoms(expr, &mut atoms);
        atoms
            .into_iter()
            .filter_map(|a| match a {
                GuardAtom::Path(p) => Some(self.path_read(p)),
                GuardAtom::Indexed(family) => Some(self.indexed_read(&family)),
                GuardAtom::Fact(f) => {
                    if self.fact_value(&f) != Value::Bool(false) {
                        return None;
                    }
                    Some(self.fact_read(&f))
                }
                GuardAtom::Visited(k) => {
                    (!self.store.visited.contains(&k)).then_some(GuardRead::Visited(k))
                }
            })
            .collect()
    }

    /// OT-F-10: the conjuncts of a guard that decided false which are false
    /// themselves, in document order — every top-level `&&`, a
    /// parenthesized conjunction split again — each with the reads it is
    /// false over ([`Machine::conjunct_reads`]). The guard is the one
    /// conjunct when its top level is no conjunction; empty unless it is
    /// false. Each conjunct is named by its text, cut from the slot's CEL
    /// at its top-level `&&`s, and evaluated as the matching operand of the
    /// slot's left-associated `&&` chain.
    pub fn false_conjuncts(&mut self, slot: &Slot) -> Vec<(String, Vec<GuardRead>)> {
        let mut out = Vec::new();
        if self.store.eval(&slot.expr).0 == Value::Bool(false) {
            self.push_false_conjuncts(&slot.raw, &slot.expr, &mut out);
            if out.is_empty() {
                out.push((slot.raw.clone(), self.conjunct_reads(&slot.expr)));
            }
        }
        out
    }

    /// [`Machine::false_conjuncts`] of a `raw` / `expr` known false.
    fn push_false_conjuncts(
        &mut self,
        raw: &str,
        expr: &Expr,
        out: &mut Vec<(String, Vec<GuardRead>)>,
    ) {
        use lute_manifest::semantics::templates::{top_level_and, unparen};
        let c = unparen(raw);
        let parts = top_level_and(c);
        let operands = (parts.len() >= 2)
            .then(|| expr.conjuncts(parts.len()))
            .flatten();
        let Some(operands) = operands else {
            out.push((c.to_string(), self.conjunct_reads(expr)));
            return;
        };
        for (part, operand) in parts.into_iter().zip(operands) {
            if self.store.eval(operand).0 == Value::Bool(false) {
                self.push_false_conjuncts(part, operand, out);
            }
        }
    }

    /// Every read of one false conjunct, in document order: each state path
    /// with its value, each fact pattern with whether it holds (one that
    /// does not with [`Machine::fact_read`]'s why), each scene `visited(…)`
    /// asks about with whether it is visited. Undecided facts are left out.
    fn conjunct_reads(&mut self, expr: &Expr) -> Vec<GuardRead> {
        let mut atoms = Vec::new();
        guard_atoms(expr, &mut atoms);
        atoms
            .into_iter()
            .filter_map(|a| match a {
                GuardAtom::Path(p) => Some(self.path_read(p)),
                GuardAtom::Indexed(family) => Some(self.indexed_read(&family)),
                GuardAtom::Fact(f) => match self.fact_value(&f) {
                    Value::Bool(true) => {
                        let (rel, args) = self.fact_args(&f);
                        Some(GuardRead::Holds(format!("{rel}({})", args.join(", "))))
                    }
                    Value::Bool(false) => Some(self.fact_read(&f)),
                    _ => None,
                },
                GuardAtom::Visited(k) if self.store.visited.contains(&k) => {
                    Some(GuardRead::Seen(k))
                }
                GuardAtom::Visited(k) => Some(GuardRead::Visited(k)),
            })
            .collect()
    }

    /// A state path `p` with the value it holds.
    fn path_read(&mut self, p: String) -> GuardRead {
        let v = match self.store.read(&p) {
            Read::Value(v) => v,
            Read::Unset => Value::Unknown,
        };
        GuardRead::Path(p, v)
    }

    /// dsl 0.27.0 §3: a family read by the bound member
    /// (`user.bond[occasion.target]`) is the member path it reads
    /// (`user.bond.ines`) with its value; unbound, `occasion.target` itself.
    fn indexed_read(&mut self, family: &str) -> GuardRead {
        let target = lute_manifest::semantics::beats::OCCASION_TARGET;
        match self.store.read(target) {
            Read::Value(Value::Str(member)) => self.path_read(format!("{family}.{member}")),
            _ => self.path_read(target.to_string()),
        }
    }

    /// A fact pattern `rel(a, b)` as its relation and arguments, its path
    /// arguments read (an unset one kept as written) and a quoted name
    /// (`"lab-b2"`) as the name.
    fn fact_args<'p>(&mut self, pattern: &'p str) -> (&'p str, Vec<String>) {
        let (rel, args) = pattern
            .strip_suffix(')')
            .and_then(|p| p.split_once('('))
            .unwrap_or((pattern, ""));
        let args = args
            .split(", ")
            .filter(|a| !a.is_empty())
            .map(|a| {
                if let Some(name) = a.strip_prefix('"').and_then(|a| a.strip_suffix('"')) {
                    return name.to_string();
                }
                match self.store.read(a) {
                    Read::Value(v) if a.contains('.') => {
                        crate::value_text(&v).unwrap_or_else(|| a.to_string())
                    }
                    _ => a.to_string(),
                }
            })
            .collect();
        (rel, args)
    }

    fn fact_value(&mut self, pattern: &str) -> Value {
        let (rel, args) = self.fact_args(pattern);
        let args = args
            .into_iter()
            .map(|arg| {
                if arg.contains('.') {
                    Expr::Path(arg.into())
                } else {
                    Expr::Lit(Value::Str(arg))
                }
            })
            .collect();
        let query = Expr::Call(
            Func::Holds,
            vec![Expr::Lit(Value::Str(rel.to_string())), Expr::List(args)],
        );
        self.store.eval(&query).0
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
            Some(attempts) => {
                let mut base = Vec::new();
                base_missing(&attempts, &mut base);
                GuardRead::Derived {
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
                    base,
                }
            }
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
            let verdict = match self.option_when(o) {
                Some(when) => self.option_verdict(&when),
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

    /// The `E-TRACE-CHOICE` text for a picked option the driver refused; a
    /// closed guard's reads each carry the driver's hint
    /// ([`Driver::premise_hint`]).
    fn refusal(
        &self,
        construct: &str,
        id: &str,
        option: &str,
        when: &str,
        verdict: &Verdict,
    ) -> String {
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
                        format!(
                            ": {}",
                            guard_premise(reads, |r| self.driver.premise_hint(r))
                        )
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
            .get("selectionKey")
            .and_then(Json::as_str)
            .map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let options: &[Json] = cmd
            .get("options")
            .and_then(Json::as_array)
            .map_or(&[], Vec::as_slice);

        let judged = self.branch_verdicts(options);
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
        };
        let incomplete_rec = |note: &str| {
            let mut rec = serde_json::Map::new();
            rec.insert("position".into(), Json::String(addr(cmd).to_string()));
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
            Some(o) => o,
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
            if let Some(when) = self.option_when(opt) {
                let verdict = self.picked_verdict(&when);
                if verdict != Verdict::Open {
                    match self.driver.forced(&menu, &forced, &verdict) {
                        Forced::Take => {}
                        Forced::Refuse => {
                            let msg = self.refusal("branch", &branch, &forced, &when.raw, &verdict);
                            self.refuse(msg);
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
        rec.insert("position".into(), Json::String(addr(cmd).to_string()));
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
            .get("selectionKey")
            .and_then(Json::as_str)
            .map(str::to_string);
        // dsl 0.23.0 §4: `<hub prompt>` rides every presentation record.
        let prompt = cmd.get("prompt").and_then(Json::as_str).map(str::to_string);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let converge_idx = self.resolve(converge);
        let options: &[Json] = cmd
            .get("options")
            .and_then(Json::as_array)
            .map_or(&[], Vec::as_slice);

        // Segment starts in stream order: every option target, then the
        // `<return>` segment (dsl 0.28.0 §5), then the converge. A segment
        // runs from its start up to the NEXT start in this order (a non-`exit`
        // option never falls through into the next option's body or the
        // return block). Two starts on one address mean an empty segment —
        // never the following segment's body.
        let return_idx = cmd
            .get("return")
            .and_then(Json::as_str)
            .map(|t| self.resolve(t));
        let mut starts: Vec<usize> = options
            .iter()
            .map(|o| {
                o.get("target")
                    .and_then(Json::as_str)
                    .map_or(converge_idx, |t| self.resolve(t))
            })
            .collect();
        starts.extend(return_idx);
        starts.push(converge_idx);

        let mut presentation = 0usize;
        // [`Pick::HubAutoPass`]: the next menu position the pass considers.
        let mut auto_at = 0usize;
        loop {
            // Eligible = not a `once` option already taken (its
            // `scene.visited.<hub>.<option>` is true, D8: the memory is the
            // reserved visit record, so a hub `::jump` re-enters still knows
            // it), and its guard does not DECIDE false right now (an unknown
            // guard stays eligible — the same three-valued discipline
            // `do_choice`'s guard refusal uses). The spent and guard-closed
            // options ride the visit record, so a menu shows what was really
            // offered.
            let mut judged = Vec::new();
            for o in options {
                let Some(oid) = o.get("id").and_then(Json::as_str) else {
                    continue;
                };
                let once = o.get("once").and_then(Json::as_bool).unwrap_or(false);
                let verdict = if once && self.hub_visited(&id, oid) {
                    Verdict::Spent
                } else {
                    match self.option_when(o) {
                        Some(w) => self.option_verdict(&w),
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
                rec.insert("position".into(), Json::String(addr(cmd).to_string()));
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
            let (choice_id, auto, scripted) = match self.driver.choose(&menu) {
                Pick::Option(c) => (Some(c), false, 0),
                Pick::Unscripted { scripted } => (None, false, scripted),
                Pick::AutoFirst => (first_open(None, 0).map(|(_, o)| o.id.clone()), true, 0),
                Pick::HubAutoPass => match first_open(Some(false), auto_at) {
                    Some((i, o)) => {
                        auto_at = i + 1;
                        (Some(o.id.clone()), true, 0)
                    }
                    None => {
                        auto_at = judged.len();
                        (
                            first_open(Some(true), 0).map(|(_, o)| o.id.clone()),
                            true,
                            0,
                        )
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
                if scripted > 0 {
                    rec.insert("scripted".into(), json!(scripted));
                }
                marks(&mut rec);
                self.driver.emit(Json::Object(rec));
                return Step::Halt;
            };

            let Some(at) = options
                .iter()
                .position(|o| o.get("id").and_then(Json::as_str) == Some(&choice_id))
            else {
                self.fatal = Some(format!("hub `{id}` has no option `{choice_id}`"));
                return Step::Halt;
            };
            let opt = &options[at];
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
                            self.refuse(self.refusal("hub", &id, &choice_id, "", &Verdict::Spent));
                            return Step::Halt;
                        }
                    }
                }
                // Same rule as `do_choice` (#20, D-C). A hub option is
                // presented repeatedly, so this is evaluated per visit
                // against live state — a guard false on the first pass may
                // be true on the third, which is precisely what a hub is for.
                if let Some(when) = self.option_when(opt) {
                    let verdict = self.picked_verdict(&when);
                    if verdict != Verdict::Open {
                        match self.driver.forced(&menu, &choice_id, &verdict) {
                            Forced::Take => {}
                            Forced::Skip => continue,
                            Forced::Refuse => {
                                let msg =
                                    self.refusal("hub", &id, &choice_id, &when.raw, &verdict);
                                self.refuse(msg);
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
            self.run_range(starts[at], starts[at + 1]);
            if self.stopped() {
                return Step::Halt;
            }
            if is_exit {
                break;
            }
            // dsl 0.28.0 §5: control returns to the hub through its
            // `<return>` block, before the menu is judged and presented again
            // — marked, so its lines do not read as the option's.
            if let Some(back) = return_idx {
                self.driver.emit(json!({
                    "position": addr(cmd),
                    "kind": "hubReturn",
                    "hub": id,
                }));
                self.run_range(back, converge_idx);
                if self.stopped() {
                    return Step::Halt;
                }
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
    /// `otherwise`, else converge. Every arm is judged by its `test` slot.
    /// An `is` pattern over a bare path subject that is unset matches only
    /// `unset` (definite, as `isSet` is), so an arm the unset subject cannot
    /// match is false, never unknown — except `occasion.target`, a binding,
    /// which is unknown until bound (T1-9). An undecided arm is an
    /// [`UnknownSite`] ([`SiteKind::Arm`], or [`SiteKind::OccasionTarget`]
    /// when the unbound target is what is missing): a driver that halts
    /// stops the walk here; one that continues skips the arm.
    pub(super) fn do_match(&mut self, cmd: &Json) -> Step {
        let arms: &[Json] = cmd
            .get("arms")
            .and_then(Json::as_array)
            .map_or(&[], Vec::as_slice);
        let converge = cmd.get("converge").and_then(Json::as_str).unwrap_or("");
        let subject_slot = self.slot(cmd.get("subject"));
        let subject = subject_slot.as_ref().map_or("", |s| s.raw.as_str());
        let subject_path = subject_slot
            .as_ref()
            .and_then(|s| s.expr.static_path())
            .filter(|p| p != lute_manifest::semantics::beats::OCCASION_TARGET);
        let subject_unset = subject_path
            .as_deref()
            .is_some_and(|p| self.store.read(p) == Read::Unset);
        for (i, arm) in arms.iter().enumerate() {
            let test_slot = arm.get("test");
            let test = self.slot(test_slot);
            let raw = test.as_ref().map_or("", |t| t.raw.as_str());
            // `is` is semantic IR data; `test.authored` remains diagnostic only.
            let is_arm = arm.get("is").and_then(Json::as_str);
            let (v, atoms) = match &test {
                Some(t) => self.eval_atoms(t),
                None => (Value::Unknown, Vec::new()),
            };
            if self.probe_arms {
                if let Some(expr) = test_slot.and_then(|t| t.get("expr")) {
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
                        "kind": "armExpr", "position": addr(cmd), "arm": i, "expr": expr,
                        "held": held, "reads": reads,
                    }));
                }
            }
            let matched = match v {
                Value::Bool(b) => b,
                _ if subject_unset && is_arm.is_some_and(|s| s != "unset") => false,
                _ if subject_unset
                    && !self.driver.is_preview()
                    && subject_path.as_deref().is_some_and(|path| {
                        atoms.iter().any(|a| {
                            matches!(a, UnresolvedAtom::Path(p) if p == path)
                        })
                    }) => false,
                _ => {
                    let target_unbound = atoms.iter().any(|a| {
                        matches!(a, UnresolvedAtom::Path(p) if p == lute_manifest::semantics::beats::OCCASION_TARGET)
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
                    if self.at_unknown(site, raw, &atoms) {
                        return Step::Halt;
                    }
                    false
                }
            };
            if matched {
                let target = arm.get("target").and_then(Json::as_str).unwrap_or(converge);
                self.driver.emit(json!({
                    "position": addr(cmd),
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
            "position": addr(cmd),
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

/// Every state path an IR `expr` node names (`path`, `isSet`, `has`).
fn expr_paths(node: &Json, out: &mut BTreeSet<String>) {
    match node {
        Json::Object(map) => {
            for (k, v) in map {
                match (k.as_str(), v) {
                    ("path" | "isSet" | "has", Json::String(p)) => {
                        out.insert(p.clone());
                    }
                    ("lit" | "int" | "double" | "bool" | "string", _) => {}
                    _ => expr_paths(v, out),
                }
            }
        }
        Json::Array(items) => items.iter().for_each(|n| expr_paths(n, out)),
        _ => {}
    }
}
