use super::*;

impl Presence<'_> {
    /// `raw` with its `@def`s (and `$`, as `subject`) expanded.
    pub(super) fn expand_text(&self, raw: &str, subject: Option<&str>) -> String {
        let mut stack = Vec::new();
        expand_cel(raw, &self.defs, subject, &mut stack).unwrap_or_else(|_| raw.to_string())
    }

    /// `raw` with its `@def`s (and `$`, as `subject`) expanded, and parsed.
    pub(super) fn parse(&self, raw: &str, subject: Option<&str>) -> Option<(Expr, String)> {
        if raw.trim().is_empty() {
            return None;
        }
        let text = self.expand_text(raw, subject);
        let mut arena = lute_cel::CelArena::default();
        let handle = lute_cel::parse_slot_marked_refs(&mut arena, &text)?;
        Some((arena.get(handle)?.expr.clone(), text))
    }

    pub(super) fn slot_cond(&self, slot: Option<&CelSlot>) -> Option<(Expr, String)> {
        slot.and_then(|s| self.parse(&s.raw, None))
    }

    /// What a unit answering occasion `on` (for `target`) may assume from
    /// the engine seam: the occasion's `raisedWhen` gate and `!terminal`
    /// ([`crate::gates::folded_seam`]) — one member's condition, or their
    /// disjunction when `occasion.target` ranges over several.
    pub(super) fn seam_cond(&self, on: &str, target: Option<&str>) -> Option<(Expr, String)> {
        let seam = crate::gates::folded_seam(self.folded, on, target, None)?;
        let text = match seam.conds.as_slice() {
            [] => return None,
            [one] => one.clone(),
            many => many
                .iter()
                .map(|c| format!("({c})"))
                .collect::<Vec<_>>()
                .join(" || "),
        };
        self.parse(&text, None)
    }

    /// `e` with each list-form `holds('rel', [args])` of a derived relation
    /// reached through `!`/`&&`/`||` read through its rules
    /// ([`definition`]): on the guard side a positive query becomes
    /// `holds('rel', [args]) && D` (what it implies) and, when `D` is exact,
    /// a negative one `holds('rel', [args]) || D`; on the `present` side an
    /// exact `D` replaces it, and under `assume` a negative query of an
    /// engine-`reserved` relation reads `false` — unless (dsl 0.25.0 §6) the
    /// relation is in [`Self::changed`].
    /// The rule `cel()`s read are pushed onto `cels`.
    pub(super) fn expand(&self, e: &Expr, pol: Pol, side: Side, depth: u8, cels: &mut Vec<String>) -> Expr {
        let Expr::Call(c) = e else { return e.clone() };
        if c.target.is_some() {
            return e.clone();
        }
        match (c.func_name.as_str(), c.args.as_slice()) {
            (n, [a]) if n == op::LOGICAL_NOT => {
                call(n, vec![self.expand(&a.expr, pol.flip(), side, depth, cels)])
            }
            (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => call(
                n,
                vec![
                    self.expand(&a.expr, pol, side, depth, cels),
                    self.expand(&b.expr, pol, side, depth, cels),
                ],
            ),
            ("holds", [_, _]) if crate::cel_resolve::is_profile_fact_query(c) => {
                let Some(query) = crate::fact_env::QueryPattern::from_call(c) else {
                    return e.clone();
                };
                let relation = &query.relation;
                let vocab = &self.folded.env.rel_vocab;
                if side == (Side::Present { assume: true })
                    && pol == Pol::Neg
                    && vocab.relations.get(relation).is_some_and(|d| d.reserved)
                    && !self.changed.contains(relation)
                {
                    return Expr::Literal(Val::Boolean(false));
                }
                if depth == 0 || pol == Pol::Both {
                    return e.clone();
                }
                let Some(def) = definition(vocab, relation, &query.args) else {
                    return e.clone();
                };
                cels.extend(def.cels.iter().map(|c| self.expand_text(c, None)));
                let text = if def.disjuncts.is_empty() {
                    "false".to_string()
                } else {
                    def.disjuncts
                        .iter()
                        .map(|d| format!("({d})"))
                        .collect::<Vec<_>>()
                        .join(" || ")
                };
                let Some((d, _)) = self.parse(&text, None) else {
                    return e.clone();
                };
                let d = self.expand(&d, pol, side, depth - 1, cels);
                match (side, pol, def.exact) {
                    (Side::Guard, Pol::Pos, _) => call(op::LOGICAL_AND, vec![e.clone(), d]),
                    (Side::Guard, Pol::Neg, true) => call(op::LOGICAL_OR, vec![e.clone(), d]),
                    (Side::Present { .. }, _, true) => d,
                    _ => e.clone(),
                }
            }
            _ => e.clone(),
        }
    }

    /// Push `cond` as one guard per top-level conjunct, so a write that
    /// may falsify one conjunct leaves the others standing.
    pub(super) fn push(&mut self, cond: Option<(Expr, String)>) {
        let Some((expr, text)) = cond else { return };
        let mut parts = Vec::new();
        conjuncts(&expr, &mut parts);
        for part in parts {
            let mut paths = Vec::new();
            let mut text = if read_paths(&part, &mut paths) {
                paths.join(" ")
            } else {
                text.clone()
            };
            let mut cels = Vec::new();
            let expr = self.expand(&part, Pol::Pos, Side::Guard, EXPAND_DEPTH, &mut cels);
            for cel in cels {
                text.push_str(" ; ");
                text.push_str(&cel);
            }
            let mut atoms = Vec::new();
            fact_atoms(&expr, Pol::Pos, &mut atoms);
            self.guards.push(Guard {
                expr,
                text,
                atoms,
                live: true,
            });
        }
    }

    /// One unit (a scene's shots, a quest, an entry, a bundle beat) under
    /// its own assumptions and the ladder's; `absent` ([`Self::absent_facts`])
    /// holds at its start, as path state the body's writes may end. `after`
    /// are the occasions the unit follows (dsl 0.25.0 §6): the relations they
    /// change are no longer assumed unchanged — nor are those its own
    /// assumptions require a fact of ([`Self::required`]).
    pub(super) fn unit<'n>(
        &mut self,
        conds: Vec<(Expr, String)>,
        ladder: &[String],
        absent: Vec<String>,
        after: &[String],
        bodies: impl Iterator<Item = &'n [Node]>,
    ) {
        self.guards.clear();
        self.changed.clear();
        for occasion in after {
            self.follow(occasion);
        }
        for c in conds {
            let required = self.required(&c.0, EXPAND_DEPTH);
            self.changed.extend(required);
            self.push(Some(c));
        }
        for l in ladder {
            let c = self.parse(l, None);
            self.push(c);
        }
        self.base = self.guards.len();
        for a in absent {
            let c = self.parse(&a, None);
            self.push(c);
        }
        for body in bodies {
            self.walk(body);
        }
    }

    /// dsl 0.25.0 §6: the code walked from here on follows `occasion`, so the
    /// relations it changes join [`Self::changed`]. Returns the set before.
    pub(super) fn follow(&mut self, occasion: &str) -> BTreeSet<String> {
        let before = self.changed.clone();
        if let Some(rels) = self.changed_on.get(occasion) {
            self.changed.extend(rels.iter().cloned());
        }
        before
    }

    /// dsl 0.27.0 §9 (T3-25): what `quest` assumes at its completion. It
    /// completes the moment one of its required objectives does (`complete=
    /// "all"`: the last one; `"any"`: the first), and that objective's `done`
    /// held then — so the disjunction of the required objectives' `done`s.
    /// `None` (nothing assumed) when no objective is required or one's `done`
    /// is not written (a `quest=` objective's is synthesized).
    pub(super) fn completion(&self, quest: &lute_syntax::ast::Quest) -> Option<(Expr, String)> {
        let mut alts = Vec::new();
        for node in &quest.body {
            if let Node::Objective(o) = node {
                if !o.optional {
                    alts.push(format!("({})", self.parse(&o.done.raw, None)?.1));
                }
            }
        }
        if alts.is_empty() {
            return None;
        }
        self.parse(&alts.join(" || "), None)
    }

    /// A `questComplete` handler (at `at`) of the walked quest assumes its
    /// [`Self::completion`] — pushed as a guard the caller truncates — as
    /// far as what runs between that objective's completion and the handler
    /// leaves it standing: every objective body of the quest (those completed
    /// in the same settle play first) and the quest's earlier
    /// `questComplete` handlers.
    pub(super) fn assume_completion(&mut self, at: usize) {
        let Some(done) = self.completion.clone() else {
            return;
        };
        let required = self.required(&done.0, EXPAND_DEPTH);
        self.changed.extend(required);
        let outer: Vec<bool> = self.guards.iter().map(|g| g.live).collect();
        self.push(Some(done));
        let body = self.quest_body;
        for node in body {
            match node {
                Node::Objective(o) => self.kill_writes(&o.body),
                Node::On(h) if h.event == "questComplete" && h.span.byte_start < at => {
                    self.kill_writes(&h.body)
                }
                _ => {}
            }
        }
        for (g, live) in self.guards.iter_mut().zip(outer) {
            g.live = live;
        }
    }

    /// dsl 0.25.0 §6: the `changedOn` relations `e` cannot hold without a
    /// fact of — a positive list-form `holds('R', […])`, a `count('R', […])`
    /// compared to be at least one, or a derived relation every rule of which
    /// needs one, joined through `&&` (either side) and `||` (both sides).
    /// Such a fact is the engine's, written on one of R's occasions, so code
    /// guarded by `e` runs after that occasion and `assume: true` no longer
    /// covers R there.
    pub(super) fn required(&self, e: &Expr, depth: u8) -> BTreeSet<String> {
        let Expr::Call(c) = e else {
            return BTreeSet::new();
        };
        if c.target.is_some() {
            return BTreeSet::new();
        }
        let atom_rel = |x: &Expr| match x {
            Expr::Call(f)
                if f.target.is_none()
                    && matches!(f.func_name.as_str(), "count" | "countDistinct")
                    && crate::cel_resolve::is_profile_fact_query(f) =>
            {
                crate::fact_env::QueryPattern::from_call(f).map(|q| q.relation)
            }
            _ => None,
        };
        let num = |x: &Expr| match x {
            Expr::Literal(Val::Int(i)) => Some(*i as f64),
            Expr::Literal(Val::UInt(u)) => Some(*u as f64),
            Expr::Literal(Val::Double(d)) => Some(*d),
            _ => None,
        };
        let n = c.func_name.as_str();
        match c.args.as_slice() {
            [a, b] if n == op::LOGICAL_AND => {
                let mut out = self.required(&a.expr, depth);
                out.extend(self.required(&b.expr, depth));
                out
            }
            [a, b] if n == op::LOGICAL_OR => {
                let left = self.required(&a.expr, depth);
                let right = self.required(&b.expr, depth);
                left.intersection(&right).cloned().collect()
            }
            [_, _] if n == "holds" && crate::cel_resolve::is_profile_fact_query(c) => {
                crate::fact_env::QueryPattern::from_call(c)
                    .map(|q| self.required_rel(&q.relation, depth))
                    .unwrap_or_default()
            }
            [a, b] => {
                // `count(R) >= k` (k ≥ 1), `> k` (k ≥ 0), `== k` (k ≥ 1), either way round.
                let (rel, k, cmp) = match (
                    atom_rel(&a.expr),
                    num(&b.expr),
                    atom_rel(&b.expr),
                    num(&a.expr),
                ) {
                    (Some(r), Some(k), _, _) => (r, k, n.to_string()),
                    (_, _, Some(r), Some(k)) => {
                        let flipped = if n == op::LESS_EQUALS {
                            op::GREATER_EQUALS
                        } else if n == op::LESS {
                            op::GREATER
                        } else {
                            n
                        };
                        (r, k, flipped.to_string())
                    }
                    _ => return BTreeSet::new(),
                };
                let at_least_one = (cmp == op::GREATER_EQUALS && k >= 1.0)
                    || (cmp == op::GREATER && k >= 0.0)
                    || (cmp == op::EQUALS && k >= 1.0);
                if at_least_one {
                    self.required_rel(&rel, depth)
                } else {
                    BTreeSet::new()
                }
            }
            _ => BTreeSet::new(),
        }
    }

    /// [`Self::required`] for one relation's fact: the relation itself when
    /// some occasion changes it; for a derived relation without seed facts,
    /// what every one of its rules' positive premises requires.
    fn required_rel(&self, rel: &str, depth: u8) -> BTreeSet<String> {
        if self.changed_on.values().any(|rels| rels.contains(rel)) {
            return BTreeSet::from([rel.to_string()]);
        }
        let vocab = &self.folded.env.rel_vocab;
        let derived = vocab
            .relations
            .get(rel)
            .is_some_and(|d| d.derive && !d.reserved);
        if depth == 0
            || !derived
            || vocab.unparsed_heads.contains(rel)
            || vocab.facts.iter().any(|f| f.fact.relation == rel)
        {
            return BTreeSet::new();
        }
        let mut out: Option<BTreeSet<String>> = None;
        for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
            let mut needs = BTreeSet::new();
            for lit in &r.rule.body {
                if let BodyLiteral::Pos(a) = lit {
                    needs.extend(self.required_rel(&a.relation, depth - 1));
                }
            }
            out = Some(match out {
                None => needs,
                Some(acc) => acc.intersection(&needs).cloned().collect(),
            });
        }
        out.unwrap_or_default()
    }

    /// `check-project`: the facts known absent when unit `key` of this
    /// document starts, as `!holds(F)` ([`unit_facts`]).
    pub(super) fn absent_facts(&self, key: usize, once: BeatOnce) -> Vec<String> {
        let Some(producers) = self.producers else {
            return Vec::new();
        };
        let vocab = &self.folded.env.rel_vocab;
        let out: BTreeSet<String> = unit_facts(producers, vocab, self.path, key, &once)
            .into_iter()
            .filter(|f| f.persists.is_none())
            .map(|f| format!("!{}", f.query))
            .collect();
        out.into_iter().collect()
    }

}
