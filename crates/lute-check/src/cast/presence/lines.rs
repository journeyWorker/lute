use super::*;

impl Presence<'_> {
    /// `present`'s conjuncts for `speaker`, parsed once, each with its
    /// rule-read form under the relations [`Self::changed`] holds (dsl
    /// 0.25.0 §6). A plugin `present:` that is not a condition is reported
    /// here, once, at `span`.
    pub(super) fn present_of(&mut self, speaker: &str, span: Span) -> Option<PresentConjuncts> {
        let written = match self.present.get(speaker) {
            Some(cached) => cached.clone()?,
            None => {
                let written = self.written_present(speaker, span);
                self.present.insert(speaker.to_string(), written.clone());
                written?
            }
        };
        let assume = self.folded.cast.get(speaker)?.assume == Some(true);
        let changed = if assume {
            self.changed.clone()
        } else {
            BTreeSet::new()
        };
        let key = (speaker.to_string(), changed);
        if let Some(read) = self.reads.get(&key) {
            return Some(read.clone());
        }
        let side = Side::Present { assume };
        let read: PresentConjuncts = written
            .into_iter()
            .map(|p| {
                let read = self.expand(&p, Pol::Pos, side, EXPAND_DEPTH, &mut Vec::new());
                (p, read)
            })
            .collect();
        self.reads.insert(key, read.clone());
        Some(read)
    }

    /// `present`'s top-level conjuncts for `speaker` as written; `None` (the
    /// faults reported at `span`, single-file) when it is not a condition.
    /// A cast entry for `narrator` is refused (`E-RESERVED-NAME`): narration
    /// is no cast member, so its `present:` gates nothing.
    pub(super) fn written_present(&mut self, speaker: &str, span: Span) -> Option<Vec<Expr>> {
        if speaker == "narrator" {
            return None;
        }
        let member = self.folded.cast.get(speaker)?;
        let raw = member.present.clone()?;
        let mut faults = present_faults(speaker, &raw, span);
        // A `present:` reading a state path this document does not declare
        // is no condition here — `W-CAST-ABSENT` would suggest a guard that
        // is itself `E-UNDECLARED`.
        if faults.is_empty() {
            if let Some((e, _)) = self.parse(&raw, None) {
                let mut paths = Vec::new();
                read_paths(&e, &mut paths);
                paths.sort();
                paths.dedup();
                for p in paths.iter().filter(|p| {
                    crate::meta::namespace_of(p).is_some()
                        && !crate::defassign::is_declared(p, &self.folded.env.state)
                }) {
                    faults.push(cast_diag(
                        "E-UNDECLARED",
                        Severity::Error,
                        Layer::Cel,
                        format!(
                            "cast `{speaker}` `present: \"{raw}\"` reads `{p}`, which this \
                             document's `state:` does not declare — declare it where every \
                             document the member speaks in imports it, or fix the condition"
                        ),
                        span,
                    ));
                }
            }
        }
        if faults.is_empty() {
            return self.parse(&raw, None).map(|(e, _)| {
                let mut out = Vec::new();
                conjuncts(&e, &mut out);
                out
            });
        }
        if self.facts.is_none() {
            self.out.extend(faults.into_iter().map(|mut d| {
                d.message.push_str(if d.code == "E-UNDECLARED" {
                    " (dsl 0.24.0 §4)"
                } else {
                    " — fix the plugin's `cast` export (dsl 0.24.0 §4)"
                });
                d
            }));
        }
        None
    }

    pub(super) fn line(&mut self, l: &Line) {
        // A voice-over (a letter, a log page, a memory) does not put its
        // speaker in the room. An `{os}` line still does: the speaker is
        // there, only out of frame.
        if l.attrs.iter().any(|a| a.key == "vo") {
            return;
        }
        // The speaker id sits just past the line's leading `@`.
        let start = l.span.byte_start + 1;
        let span = Span {
            byte_start: start,
            byte_end: start + l.speaker.len(),
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        };
        let own = l.when.as_ref().and_then(|w| self.parse(&w.raw, None));
        // dsl 0.25.0 §6: a line whose own guard needs a `changedOn` fact runs
        // after the occasion that writes it.
        let before = self.changed.clone();
        if let Some((e, _)) = &own {
            let required = self.required(e, EXPAND_DEPTH);
            self.changed.extend(required);
        }
        self.decide_line(l, span, own, None);
        self.changed = before;
    }

    /// [`Self::line`] once its own `when` is parsed (and [`Self::changed`]
    /// includes what it requires). `via`: the component whose `@@p:` line
    /// this is, bound at a `::use` ([`Self::use_lines`]).
    pub(super) fn decide_line(
        &mut self,
        l: &Line,
        span: Span,
        own: Option<(Expr, String)>,
        via: Option<&str>,
    ) {
        let Some(present) = self.present_of(&l.speaker, span) else {
            return;
        };
        let own =
            own.map(|(e, _)| self.expand(&e, Pol::Pos, Side::Guard, EXPAND_DEPTH, &mut Vec::new()));
        // The Must slot `fact_must` records for this line.
        let slot = l.when.as_ref().map_or(l.span, |w| w.span);
        if self.implied(&present, own.as_ref(), slot) {
            return;
        }
        let raw = self.folded.cast[&l.speaker]
            .present
            .clone()
            .unwrap_or_default();
        let mut message = match via {
            None => format!(
                "`{who}` may not be here: the cast declares `present: \"{raw}\"` for `{who}`, and \
                 the guards around this line do not imply it (dsl 0.24.0 §4). Guard the line — \
                 `@{who}{{when=\"{raw}\"}}` — or move it under a guard that implies it",
                who = l.speaker
            ),
            Some(component) => format!(
                "`{who}` may not be here: component `{component}` speaks as `{who}` at this \
                 `::use`, the cast declares `present: \"{raw}\"` for `{who}`, and the guards \
                 around the `::use` do not imply it (dsl 0.24.0 §4, 0.26.0 §3.2). Guard it — \
                 `::use{{… when=\"{raw}\"}}` — or move it under a guard that implies it",
                who = l.speaker
            ),
        };
        // dsl 0.25.0 §6: say so when only `changedOn` took `assume` away.
        if !self.changed.is_empty() && self.folded.cast[&l.speaker].assume == Some(true) {
            let changed = std::mem::take(&mut self.changed);
            let assumed = self.present_of(&l.speaker, span);
            let rels: Vec<String> = changed.iter().map(|r| format!("`{r}`")).collect();
            self.changed = changed;
            if assumed.is_some_and(|a| self.implied(&a, own.as_ref(), slot)) {
                message.push_str(&format!(
                    "; `assume: true` does not cover {}: this line runs after an occasion its \
                     `changedOn:` names — it follows one, or its guards need a fact only one \
                     writes (dsl 0.25.0 §6)",
                    rels.join(", ")
                ));
            }
        }
        if self.facts.is_none() && (raw.contains("holds(") || raw.contains("count(")) {
            message.push_str(
                " (a single-file check does not see facts asserted on every route to this \
                 line; `lute check-project` does)",
            );
        }
        self.out.push(cast_diag(
            W_CAST_ABSENT,
            Severity::Warning,
            Layer::Logic,
            message,
            span,
        ));
    }

    /// `true` iff the live guards (and the line's own `when`) imply every
    /// conjunct of `present`: `guards && !p` is refuted, for `p` as written
    /// or read through the rules.
    pub(super) fn implied(&self, present: &[(Expr, Expr)], own: Option<&Expr>, slot: Span) -> bool {
        let ctx = DecideCtx {
            schema: &self.folded.env.state,
            dollar: None,
            params: self.params,
            facts: self.facts.map(|env| FactScope {
                env,
                vocab: &self.folded.env.rel_vocab,
                path: self.path,
                span: slot,
                wip: false,
            }),
        };
        let guards: Vec<Expr> = self
            .guards
            .iter()
            .filter(|g| g.live)
            .map(|g| g.expr.clone())
            .chain(own.cloned())
            .collect();
        let refutes = |p: &Expr| {
            let mut items = guards.clone();
            items.push(call(op::LOGICAL_NOT, vec![p.clone()]));
            conjoin(&items).is_some_and(|e| refuted(&e, &ctx))
        };
        present
            .iter()
            .all(|(p, read)| refutes(p) || (read != p && refutes(read)))
    }
}
