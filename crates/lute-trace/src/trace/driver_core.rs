use super::*;
impl<'a> TraceDriver<'a> {
    pub(super) fn new(cx: TraceContext<'a>) -> Self {
        let cmds = cx
            .art
            .get("commands")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| Some((c.get("position")?.as_str()?.to_string(), c.clone())))
            .collect();
        let headings = cx
            .art
            .get("sections")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some((
                    s.get("section")?.as_i64()?,
                    s.get("heading")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                ))
            })
            .collect();
        TraceDriver {
            cx,
            cmds,
            headings,
            branch_cursor: BTreeMap::new(),
            bridge_cursor: BTreeMap::new(),
            bridge_unanswered: BTreeMap::new(),
            bridge_answer: None,
            menu: None,
            head: None,
            shot: 0,
            unit: None,
            jumped: false,
            reached: None,
            last_write: None,
            exclusive: Vec::new(),
            refused: None,
            steps: Vec::new(),
            decisions: Vec::new(),
            unresolved: Vec::new(),
            forced_unknown: Vec::new(),
            coverage_choices: BTreeMap::new(),
            coverage_arms: BTreeMap::new(),
            coverage_seen: BTreeSet::new(),
            said: Vec::new(),
            spent_accepts: Vec::new(),
            premises: BTreeMap::new(),
            not_raised: BTreeMap::new(),
            ineligible_by: BTreeMap::new(),
        }
    }

    pub(super) fn info(&self, addr: &str) -> Option<&SourceInfo> {
        self.cx.map.by_addr.get(addr)
    }

    pub(super) fn span_at(&self, addr: &str) -> Span {
        self.info(addr)
            .map(|i| i.span)
            .unwrap_or_else(mock::synthetic_span)
    }

    pub(super) fn arm(&self, addr: &str, i: usize) -> Option<&ArmSource> {
        self.info(addr)?.arms.get(i)
    }

    /// The option index of `option` in the menu record at `addr`.
    pub(super) fn option_index(&self, addr: &str, option: &str) -> Option<usize> {
        self.cmds
            .get(addr)?
            .get("options")?
            .as_array()?
            .iter()
            .position(|o| o.get("id").and_then(Json::as_str) == Some(option))
    }

    pub(super) fn option_span(&self, addr: &str, option: &str) -> Span {
        self.option_index(addr, option)
            .and_then(|i| self.arm(addr, i))
            .map(|a| a.span)
            .unwrap_or_else(|| self.span_at(addr))
    }

    pub(super) fn option_guard(&self, addr: &str, option: &str) -> (Option<String>, Option<String>) {
        match self
            .option_index(addr, option)
            .and_then(|i| self.arm(addr, i))
        {
            Some(a) => (a.guard.clone(), a.authored_guard.clone()),
            None => (None, None),
        }
    }

    /// An `UnresolvedAtom` → its §4.6 "supply it as a mock" hint: a read of a
    /// result slot an unanswered plugin call left unknown (dsl 0.24.0 §5)
    /// names the `bridges:` answer that would decide it; an unbound
    /// `occasion.target` names its members (T1-9).
    pub(super) fn render_atom(&self, a: &UnresolvedAtom) -> String {
        match a {
            UnresolvedAtom::Path(p) => match self.bridge_unanswered.get(p) {
                Some((tag, field, shape)) => format!(
                    "bridges: {{ {tag}: [ {shape} ] }} (plugin `{tag}` call unanswered; `{p}` \
                     reads its `{field}` result)"
                ),
                None if p == lute_manifest::semantics::beats::OCCASION_TARGET && !self.cx.members.is_empty() => {
                    format!("--state {p}=<{}>", self.cx.members.join("|"))
                }
                None => render_atom(a),
            },
            _ => render_atom(a),
        }
    }

    /// The mock that changes one read of a guard that decided false
    /// (round-5 T3-12), as a `lute trace` flag where one exists — the
    /// unresolved-atom hint ([`Self::render_atom`]) — else the `--mock`
    /// file key. A single-file trace knows no earlier scene: a
    /// `visited(…)` or quest-state read is only what the mock says.
    pub(super) fn guard_hint(&self, r: &GuardRead) -> String {
        const EARLIER: &str =
            "in a `--mock` file; a single-file trace does not know what earlier scenes did";
        match r {
            GuardRead::Path(p, _) if exec::session::quest_state_id(p).is_some() => {
                format!("{} {EARLIER}", r.yaml_mock())
            }
            GuardRead::Path(p, _) => {
                format!("`{}`", self.render_atom(&UnresolvedAtom::Path(p.clone())))
            }
            // A derived fact is mocked by what its rules miss, never by the
            // conclusion they would draw (it is re-derived over the mocks).
            GuardRead::Derived { base, .. } if !base.is_empty() => base
                .iter()
                .map(|b| format!("`{}`", render_atom(&UnresolvedAtom::Fact(b.clone()))))
                .collect::<Vec<_>>()
                .join(" and "),
            GuardRead::Fact(f) | GuardRead::Derived { fact: f, .. } => {
                format!("`{}`", render_atom(&UnresolvedAtom::Fact(f.clone())))
            }
            GuardRead::Visited(_) | GuardRead::Seen(_) => format!("{} {EARLIER}", r.yaml_mock()),
            GuardRead::Holds(_) => r.yaml_mock(),
        }
    }

    /// §3.2 de-duplication: the unresolved set reports a byte-identical
    /// entry (same construct, id, span, expression) once, not once per
    /// re-evaluation pass.
    pub(super) fn record_unresolved(
        &mut self,
        construct: &str,
        id: &str,
        span: Span,
        expression: String,
        atoms: &[UnresolvedAtom],
    ) {
        let dup = self.unresolved.iter().any(|u| {
            u.construct == construct && u.id == id && u.span == span && u.expression == expression
        });
        if dup {
            return;
        }
        // One hint per mock: an expression reading `occasion.target` twice
        // (`holds(owned(occasion.target)) && user.bond[occasion.target]`)
        // records the atom twice.
        let mut rendered: Vec<String> = Vec::new();
        for hint in atoms.iter().map(|a| self.render_atom(a)) {
            if !rendered.contains(&hint) {
                rendered.push(hint);
            }
        }
        self.unresolved.push(UnresolvedEntry {
            construct: construct.to_string(),
            id: id.to_string(),
            span,
            expression,
            atoms: rendered,
        });
    }

    /// A scripted `choose:` list ran out at `menu`: recorded as unresolved,
    /// with the longer list that would decide it (`--choose id=a,<b|c>`).
    pub(super) fn record_exhausted(
        &mut self,
        menu: &Menu<'_>,
        construct: &str,
        expression: String,
        list: &[String],
    ) {
        let span = self.span_at(menu.addr);
        self.record_unresolved(construct, menu.id, span, expression, &[]);
        let open: Vec<&str> = menu
            .options
            .iter()
            .filter(|o| !matches!(o.verdict, Verdict::Spent | Verdict::Closed(_)))
            .map(|o| o.id.as_str())
            .collect();
        let hint = format!(
            "--choose {}={},<{}>",
            menu.id,
            list.join(","),
            open.join("|")
        );
        if let Some(entry) = self.unresolved.last_mut() {
            if entry.id == menu.id && entry.atoms.is_empty() {
                entry.atoms.push(hint);
            }
        }
    }

    pub(super) fn push_decision(&mut self, d: Decision) {
        self.decisions.push(d.clone());
        self.steps.push(Step::Decision(d));
    }

    /// The walk's refusal: the driver's own ruling, else the exclusivity
    /// the last write broke, else the Machine's message.
    pub(super) fn refusal(&mut self, msg: String, refused: bool) -> Diagnostic {
        if let Some(d) = self.refused.take() {
            return d;
        }
        if refused && !self.exclusive.is_empty() {
            let mut diagnostic = logic_diag(
                lute_check::fact_check::E_FACT_EXCLUSIVE,
                format!(
                    "this write makes exclusive relations hold together: {} (dsl 0.25.0 §1)",
                    self.exclusive.join("; ")
                ),
                self.last_write.unwrap_or_else(mock::synthetic_span),
            );
            diagnostic.evidence = Some(lute_core_span::Evidence::Witnessed);
            return diagnostic;
        }
        logic_diag("E-COMPILE-INTERNAL", msg, mock::synthetic_span())
    }
}
