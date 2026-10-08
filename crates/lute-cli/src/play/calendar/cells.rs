//! Per-cell evaluation: the columns a run evaluates, each column's outcome
//! at a cell, and the `--facts` relations that hold there.

use super::*;

/// Per `--facts` relation, the facts of it that hold over the settled
/// cell: the runner's fixpoint, as [`world_view`] derives them.
///
/// [`world_view`]: lute_runtime::session::world_view
pub(super) fn cell_facts(p: &ExecProject, w: &World, rels: &[FactsRel]) -> Vec<Vec<Fact>> {
    if rels.is_empty() {
        return Vec::new();
    }
    let eval = w.evaluator(&p.eval_json);
    rels.iter()
        .map(|r| {
            eval.all_facts()
                .iter()
                .filter(|(rel, _)| *rel == r.name)
                .cloned()
                .collect()
        })
        .collect()
}

/// The columns: every listed occasion (default: every occasion a beat
/// answers, by name), each targeted one once per target — the `--target`s
/// it takes, else every target its beats name (0.23.1: never the rest of
/// its declared domain, where no beat answers and every cell would read as
/// a hole). A targeted occasion none of whose beats names a target gets one
/// column its untargeted beats answer.
pub(super) fn columns(
    p: &ExecProject,
    occasions: &[String],
    targets: &[String],
) -> Result<Vec<Column>, String> {
    let answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    let listed: Vec<&str> = if occasions.is_empty() {
        answered.iter().copied().collect()
    } else {
        occasions.iter().map(String::as_str).collect()
    };
    for occ in &listed {
        let known = if p.occasions.is_empty() {
            answered.contains(occ) || p.objective_occasions.contains(*occ)
        } else {
            p.occasions.contains_key(*occ)
        };
        if !known {
            let vocab: Vec<&str> = if p.occasions.is_empty() {
                answered.iter().copied().collect()
            } else {
                p.occasions.keys().map(String::as_str).collect()
            };
            return Err(format!(
                "`--occasion {occ}` is not an occasion of this project (known: {})",
                vocab.join(", ")
            ));
        }
    }
    let mut used_targets = BTreeSet::new();
    let mut out = Vec::new();
    for occ in listed {
        let select = p.select_of(occ);
        let decl = p.occasions.get(occ);
        // dsl 0.26.0 §5: a kind beat stands for a column per member.
        let beat_targets: BTreeSet<String> = p
            .index
            .beats
            .iter()
            .filter(|b| b.on == occ)
            .flat_map(|b| match (&b.target_kind, &b.target) {
                (Some(k), _) => k
                    .members
                    .iter()
                    .map(|m| format!("{}.{m}", k.prefix))
                    .collect::<Vec<_>>(),
                (None, t) => t.iter().cloned().collect(),
            })
            .collect();
        let targeted = decl.map_or(!beat_targets.is_empty(), |d| d.target.takes_target());
        let column = |target: Option<String>| Column {
            occasion: occ.to_string(),
            any_target: targeted && target.is_none(),
            target,
            select,
            pins: None,
        };
        if !targeted {
            out.push(column(None));
            continue;
        }
        if !targets.is_empty() {
            for t in targets {
                let fits = match decl {
                    Some(d) => lute_check::occasion_target_ok(d, t, &p.kinds).is_ok(),
                    None => true,
                };
                if fits {
                    used_targets.insert(t.as_str());
                    out.push(column(Some(t.clone())));
                }
            }
        } else if beat_targets.is_empty() {
            out.push(column(None));
        } else {
            out.extend(beat_targets.iter().map(|t| column(Some(t.clone()))));
        }
    }
    if let Some(t) = targets.iter().find(|t| !used_targets.contains(t.as_str())) {
        return Err(format!(
            "`--target {t}` is a target of none of the listed targeted occasions"
        ));
    }
    Ok(out)
}

/// One column at one cell, recording every candidate's verdict in `seen`
/// (keyed by its `ProjectIndex.beats` row).
pub(super) fn evaluate(p: &ExecProject, w: &World, col: &Column, seen: &mut BTreeMap<usize, Seen>) -> Outcome {
    let cands = eligible_at(p, w, &col.occasion, col.target.as_deref());
    // A `for` beat is judged (and presented) once per member: its cell
    // names the member, as play's transcript does.
    let label = |c: &Candidate| match &c.for_member {
        Some(m) => format!("{} for {m}", c.id),
        None => c.id.clone(),
    };
    let rows: Vec<Option<usize>> = cands
        .iter()
        .map(|c| {
            p.index.beats.iter().position(|b| {
                b.id == c.id
                    && b.document == c.document
                    && is_candidate(b, &col.occasion, col.target.as_deref())
            })
        })
        .collect();
    for (c, row) in cands.iter().zip(&rows) {
        let Some(row) = *row else {
            continue;
        };
        let s = seen.entry(row).or_default();
        match &c.verdict {
            Verdict::Eligible => s.eligible = true,
            Verdict::Ineligible(why) => {
                s.reasons.insert(why.to_string());
            }
            Verdict::Unknown(why) => {
                s.reasons.insert(format!("when: unknown ({why})"));
            }
        }
    }
    let unknown: Vec<(String, String)> = cands
        .iter()
        .filter_map(|c| match &c.verdict {
            Verdict::Unknown(why) => Some((label(c), why.clone())),
            _ => None,
        })
        .collect();
    let undecided = deciding_unknown(&cands, col.select).is_some();
    let shown: Vec<usize> = if undecided {
        Vec::new()
    } else {
        presented(col.select, &cands)
    };
    let winner = (col.select == OccasionSelect::First)
        .then(|| {
            shown
                .iter()
                .map(|&i| &cands[i])
                .find(|c| !c.also)
                .map(label)
        })
        .flatten();
    let presented: Vec<String> = shown.iter().map(|&i| label(&cands[i])).collect();
    // What a shadowed beat lost to: the winner, else the presented list.
    let beater = if undecided {
        format!("an undecided cell ({})", undecided_why(&unknown))
    } else {
        winner.clone().unwrap_or_else(|| presented.join(", "))
    };
    let mut shadowed = Vec::new();
    for (i, c) in cands.iter().enumerate() {
        if !matches!(c.verdict, Verdict::Eligible) {
            continue;
        }
        let s = rows[i].map(|row| seen.entry(row).or_default());
        if shown.contains(&i) {
            if let Some(s) = s {
                s.presented = true;
            }
        } else {
            shadowed.push(label(c));
            if let Some(s) = s {
                s.beaten_by.insert(beater.clone());
            }
        }
    }
    let gated = matches!(
        lute_runtime::seam::closed(p, w, &col.occasion, col.target.as_deref()),
        Some(lute_runtime::seam::Closed::Gate { .. })
    );
    Outcome {
        winner,
        presented,
        shadowed,
        unknown,
        undecided,
        gated,
        not_raised: false,
    }
}
