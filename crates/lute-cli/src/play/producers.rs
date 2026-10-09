//! What a play names when a scripted pick's guard reads a fact that does
//! not hold: every place in the compiled project that asserts it (its
//! scene, entry, beat or quest, and the `choice` option it sits under), and
//! what the playthrough chose at those choices so far.

use std::collections::BTreeMap;

use serde_json::Value as Json;

use lute_runtime::session::parse_ground_fact;
use lute_runtime::store::json_arg_to_string;
use lute_runtime::GuardRead;

/// One asserting command of the compiled project.
#[derive(Clone, Debug)]
struct Site {
    relation: String,
    args: Vec<String>,
    /// ``scene `counter` ``, ``entry `craneLamps` ``, ``beat `road.ford` ``.
    owner: String,
    /// The innermost `choice` / `hub` option the assert sits under.
    choice: Option<ChoiceAt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChoiceAt {
    document: String,
    addr: String,
    branch: String,
    option: String,
}

/// Every asserting site of a project, read off its compiled artifacts.
#[derive(Clone, Debug, Default)]
pub struct Producers {
    sites: Vec<Site>,
    /// Relations the engine asserts (`reserved: true`).
    reserved: Vec<String>,
}

/// One decision a playthrough made: at script step `step`, the `choice` /
/// `hub` at `addr` of `document` took `option`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub step: usize,
    pub document: String,
    pub addr: String,
    pub option: String,
}

/// The decisions a finished walk of `document` made at step `step`, read
/// off its transcript (`choice` / `hub` records that chose an option).
pub fn decisions_of(document: &str, step: usize, transcript: &[Json]) -> Vec<Decision> {
    transcript
        .iter()
        .filter(|r| matches!(r.get("kind").and_then(Json::as_str), Some("choice" | "hub")))
        .filter_map(|r| {
            Some(Decision {
                step,
                document: document.to_string(),
                addr: r.get("position")?.as_str()?.to_string(),
                option: r.get("chose")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn text<'a>(c: &'a Json, key: &str) -> Option<&'a str> {
    c.get(key).and_then(Json::as_str)
}

impl Producers {
    /// The table over every artifact of the project. An assert's owner is
    /// the last `entry` / `beat` / `quest` head before it in its document,
    /// else the scene; its choice is the innermost option range holding it
    /// (an option runs from its target to the next option's, the last one
    /// to the converge point).
    pub fn of(artifacts: &BTreeMap<String, Json>, reserved: Vec<String>) -> Self {
        let mut sites = Vec::new();
        for (document, art) in artifacts {
            let Some(commands) = art.get("commands").and_then(Json::as_array) else {
                continue;
            };
            let at: BTreeMap<&str, usize> = commands
                .iter()
                .enumerate()
                .filter_map(|(i, c)| Some((text(c, "position")?, i)))
                .collect();
            // (start, end, branch, option, choice addr) per option body.
            let mut ranges: Vec<(usize, usize, &str, &str, &str)> = Vec::new();
            for c in commands {
                let branch = match text(c, "kind") {
                    Some("choice") => text(c, "branchId"),
                    Some("hub") => text(c, "id"),
                    _ => None,
                };
                let (Some(branch), Some(addr)) = (branch, text(c, "position")) else {
                    continue;
                };
                let mut starts: Vec<(usize, &str)> = c
                    .get("options")
                    .and_then(Json::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|o| Some((*at.get(text(o, "target")?)?, text(o, "id")?)))
                    .collect();
                starts.sort();
                let converge = text(c, "converge")
                    .and_then(|a| at.get(a).copied())
                    .unwrap_or(commands.len());
                for (k, (start, option)) in starts.iter().enumerate() {
                    let end = starts.get(k + 1).map_or(converge, |(s, _)| *s);
                    ranges.push((*start, end, branch, option, addr));
                }
            }
            let scene = match (
                text(art, "kind"),
                art.pointer("/meta/id").and_then(Json::as_str),
            ) {
                (Some("scene"), Some(id)) => format!("scene `{id}`"),
                _ => format!("`{document}`"),
            };
            let mut owner = scene.clone();
            for (i, c) in commands.iter().enumerate() {
                let facts: Vec<(&str, Vec<String>)> = match text(c, "kind") {
                    Some(kind @ ("entry" | "beat" | "quest")) => {
                        if let Some(id) = text(c, "id") {
                            owner = format!("{kind} `{id}`");
                        }
                        continue;
                    }
                    Some("assert") => fact_of(c).into_iter().collect(),
                    Some("plugin") => c
                        .get("asserts")
                        .and_then(Json::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(fact_of)
                        .collect(),
                    _ => continue,
                };
                let choice = ranges
                    .iter()
                    .filter(|(s, e, ..)| *s <= i && i < *e)
                    .max_by_key(|(s, ..)| *s)
                    .map(|(_, _, branch, option, addr)| ChoiceAt {
                        document: document.clone(),
                        addr: addr.to_string(),
                        branch: branch.to_string(),
                        option: option.to_string(),
                    });
                for (relation, args) in facts {
                    sites.push(Site {
                        relation: relation.to_string(),
                        args,
                        owner: owner.clone(),
                        choice: choice.clone(),
                    });
                }
            }
        }
        Producers { sites, reserved }
    }

    /// The producers of the ground fact `f` in a play's words: "asserted
    /// by scene `counter` choice `look: receipt`; step 1 chose `leave`", or
    /// "nothing in the project asserts it". `decisions` are the play's
    /// decisions so far, in order.
    fn of_fact(&self, f: &str, decisions: &[Decision]) -> String {
        let Some((rel, args)) = parse_ground_fact(f) else {
            return String::new();
        };
        if self.reserved.contains(&rel) {
            return "the engine asserts it — an `engine:` step writes it".to_string();
        }
        let matching: Vec<&Site> = self
            .sites
            .iter()
            .filter(|s| s.relation == rel && s.args == args)
            .collect();
        if matching.is_empty() {
            return "nothing in the project asserts it".to_string();
        }
        let mut labels: Vec<String> = Vec::new();
        let mut choices: Vec<&ChoiceAt> = Vec::new();
        for s in &matching {
            let label = match &s.choice {
                Some(c) => format!("{} choice `{}: {}`", s.owner, c.branch, c.option),
                None => s.owner.clone(),
            };
            if !labels.contains(&label) {
                labels.push(label);
            }
            if let Some(c) = &s.choice {
                if !choices
                    .iter()
                    .any(|k| k.document == c.document && k.addr == c.addr)
                {
                    choices.push(c);
                }
            }
        }
        // The last decision at each choice holding a producer.
        let chosen: Vec<(&ChoiceAt, &Decision)> = choices
            .into_iter()
            .filter_map(|c| {
                let d = decisions
                    .iter()
                    .rev()
                    .find(|d| d.document == c.document && d.addr == c.addr)?;
                Some((c, d))
            })
            .collect();
        let mut out = format!("asserted by {}", labels.join(", "));
        let one = chosen.len() == 1;
        for (c, d) in chosen {
            let option = if one {
                d.option.clone()
            } else {
                format!("{}: {}", c.branch, d.option)
            };
            out.push_str(&format!("; step {} chose `{option}`", d.step));
        }
        out
    }

    /// What produces one read of a closed guard, in a play's words: a fact
    /// that does not hold names its asserting sites; a derived one, the
    /// producers of each base premise it misses. Empty for every other read.
    pub fn hint(&self, read: &GuardRead, decisions: &[Decision]) -> String {
        match read {
            GuardRead::Fact(f) => self.of_fact(f, decisions),
            GuardRead::Derived { base, .. } => base
                .iter()
                .map(|b| format!("`{b}`: {}", self.of_fact(b, decisions)))
                .collect::<Vec<_>>()
                .join("; "),
            _ => String::new(),
        }
    }
}

/// `(relation, args)` of an `assert` command or a plugin call's `asserts`
/// entry.
fn fact_of(c: &Json) -> Option<(&str, Vec<String>)> {
    let rel = text(c, "relation")?;
    let args = c
        .get("args")
        .and_then(Json::as_array)
        .map(|a| a.iter().map(json_arg_to_string).collect())
        .unwrap_or_default();
    Some((rel, args))
}

/// A walk's view of [`Producers`]: the table (`None` for a walk no script
/// picks in — it refuses nothing), the decisions the play made before this
/// walk, and — for the decisions this walk makes — its document and step.
#[derive(Clone, Debug, Default)]
pub struct Premises {
    pub producers: Option<std::sync::Arc<Producers>>,
    pub decisions: Vec<Decision>,
    pub document: String,
    pub step: usize,
}

