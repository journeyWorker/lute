//! `lute play --explain <atom>` (dsl 0.22.0 §6): why a ground atom holds at
//! the end of a playthrough — the rule used and each premise's own support,
//! a negated premise shown absent — or, when it does not, every rule that
//! could conclude it with its failing premises. The answer comes from
//! [`lute_trace::datalog::Program::explain`], over the SAME fixpoint the
//! playthrough's guards read; this module only renders it.

use std::collections::{BTreeMap, BTreeSet};

use lute_check::StateSchema;
use lute_trace::datalog::{
    parse_ground, render_fact, Attempt, Explanation, Premise, Program, Proof,
};
use lute_trace::{EffectiveState, Value};
use serde_json::{json, Value as Json};

use crate::runner::Fact;

/// Explain every atom in `atoms` over `rules` (the project's IR rules
/// array) applied to `base_facts` with rule guards reading `state`.
/// `seed_facts` only labels a base fact `seed` rather than `asserted`.
/// Returns the human text and the `--json` value (an array, one object per
/// atom); `Err` names an atom that is not a ground `rel(a, …)` (a usage
/// error).
pub(crate) fn render(
    rules: &Json,
    seed_facts: &BTreeSet<Fact>,
    state: &BTreeMap<String, Value>,
    base_facts: &BTreeSet<Fact>,
    atoms: &[String],
) -> Result<(String, Json), String> {
    let goals = atoms
        .iter()
        .map(|a| {
            parse_ground(a).ok_or_else(|| {
                format!("`{a}` is not a ground atom such as `rel(a, b)` (variables and `_` are not allowed)")
            })
        })
        .collect::<Result<Vec<Fact>, String>>()?;
    let program = Program::from_ir(Some(rules));
    let schema = StateSchema::default();
    let eff = EffectiveState::new(&schema, state.clone());
    let closure = program.fixpoint(base_facts, &eff);
    let r = Renderer { seed_facts };
    let mut text = String::new();
    let mut json = Vec::new();
    for goal in &goals {
        let ex = program.explain(&closure, goal, &eff);
        r.human(&mut text, goal, &ex);
        json.push(r.json(goal, &ex));
    }
    Ok((text, Json::Array(json)))
}

struct Renderer<'a> {
    seed_facts: &'a BTreeSet<Fact>,
}

impl Renderer<'_> {
    fn base_label(&self, f: &Fact) -> &'static str {
        if self.seed_facts.contains(f) {
            "seed fact"
        } else {
            "asserted"
        }
    }

    fn human(&self, out: &mut String, goal: &Fact, ex: &Explanation) {
        let atom = render_fact(goal);
        match ex {
            Explanation::Holds(proof) => {
                out.push_str(&format!("explain {atom}: holds\n"));
                out.push_str(&format!("  {}\n", self.proof_head(proof)));
                if let Proof::Derived { premises, .. } = proof {
                    self.premises(out, premises, "  ");
                }
            }
            Explanation::Fails { derived, attempts } => {
                out.push_str(&format!("explain {atom}: does not hold\n"));
                if attempts.is_empty() {
                    let why = if *derived {
                        "no rule for it matches these arguments"
                    } else {
                        "it is not a fact, and no rule concludes it"
                    };
                    out.push_str(&format!("  {why}\n"));
                }
                for a in attempts {
                    self.attempt(out, a, "  ");
                }
            }
        }
    }

    fn proof_head(&self, p: &Proof) -> String {
        match p {
            Proof::Base(f) => format!("{}  ({})", render_fact(f), self.base_label(f)),
            Proof::Derived { fact, rule, .. } => format!("{}  ⇐ {rule}", render_fact(fact)),
        }
    }

    fn attempt(&self, out: &mut String, a: &Attempt, indent: &str) {
        out.push_str(&format!("{indent}{}\n", a.rule));
        self.premises(out, &a.premises, indent);
    }

    /// One tree level: `├─`/`└─` connectors, children indented under `│`.
    fn premises(&self, out: &mut String, premises: &[Premise], indent: &str) {
        for (i, p) in premises.iter().enumerate() {
            let last = i + 1 == premises.len();
            let (branch, cont) = if last { ("└─ ", "   ") } else { ("├─ ", "│  ") };
            let child = format!("{indent}{cont}");
            match p {
                Premise::Holds(proof) => {
                    out.push_str(&format!("{indent}{branch}{}\n", self.proof_head(proof)));
                    if let Proof::Derived { premises, .. } = proof.as_ref() {
                        self.premises(out, premises, &child);
                    }
                }
                Premise::Missing { atom, why } => {
                    let note = if why.is_empty() { "absent" } else { "not derived" };
                    out.push_str(&format!("{indent}{branch}✗ {atom}  ({note})\n"));
                    for a in why {
                        self.attempt(out, a, &child);
                    }
                }
                Premise::Absent(f) => {
                    out.push_str(&format!("{indent}{branch}not {}  (absent)\n", render_fact(f)))
                }
                Premise::Present(proof) => {
                    let why = match proof.as_ref() {
                        Proof::Base(f) => self.base_label(f).to_string(),
                        Proof::Derived { rule, .. } => format!("⇐ {rule}"),
                    };
                    out.push_str(&format!(
                        "{indent}{branch}✗ not {}  (but it holds: {why})\n",
                        render_fact(proof.fact())
                    ));
                    if let Proof::Derived { premises, .. } = proof.as_ref() {
                        self.premises(out, premises, &child);
                    }
                }
                Premise::Test { text, holds } => {
                    let mark = match holds {
                        Some(true) => "",
                        Some(false) => "✗ ",
                        None => "? ",
                    };
                    let note = match holds {
                        Some(true) => "",
                        Some(false) => "  (false)",
                        None => "  (undecided)",
                    };
                    out.push_str(&format!("{indent}{branch}{mark}{text}{note}\n"));
                }
                Premise::Unreached(text) => {
                    out.push_str(&format!("{indent}{branch}· {text}  (not reached)\n"))
                }
            }
        }
    }

    fn json(&self, goal: &Fact, ex: &Explanation) -> Json {
        match ex {
            Explanation::Holds(proof) => json!({
                "atom": render_fact(goal),
                "holds": true,
                "proof": self.proof_json(proof),
            }),
            Explanation::Fails { derived, attempts } => json!({
                "atom": render_fact(goal),
                "holds": false,
                "derived": derived,
                "attempts": attempts.iter().map(|a| self.attempt_json(a)).collect::<Vec<_>>(),
            }),
        }
    }

    fn proof_json(&self, p: &Proof) -> Json {
        match p {
            Proof::Base(f) => json!({ "fact": render_fact(f), "support": self.base_label(f) }),
            Proof::Derived {
                fact,
                rule,
                premises,
            } => json!({
                "fact": render_fact(fact),
                "support": "derived",
                "rule": rule,
                "premises": premises.iter().map(|p| self.premise_json(p)).collect::<Vec<_>>(),
            }),
        }
    }

    fn attempt_json(&self, a: &Attempt) -> Json {
        json!({
            "rule": a.rule,
            "premises": a.premises.iter().map(|p| self.premise_json(p)).collect::<Vec<_>>(),
        })
    }

    fn premise_json(&self, p: &Premise) -> Json {
        match p {
            Premise::Holds(proof) => json!({ "status": "holds", "proof": self.proof_json(proof) }),
            Premise::Missing { atom, why } => json!({
                "status": "missing",
                "atom": atom,
                "attempts": why.iter().map(|a| self.attempt_json(a)).collect::<Vec<_>>(),
            }),
            Premise::Absent(f) => json!({ "status": "absent", "negated": render_fact(f) }),
            Premise::Present(proof) => json!({ "status": "present", "proof": self.proof_json(proof) }),
            Premise::Test { text, holds } => json!({ "status": "test", "test": text, "holds": holds }),
            Premise::Unreached(text) => json!({ "status": "unreached", "premise": text }),
        }
    }
}
