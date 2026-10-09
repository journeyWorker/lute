use super::*;
impl<'a> TraceDriver<'a> {
    // -- shots ------------------------------------------------------------

    /// A record of addressing unit `unit` is about to be reported: open the
    /// heads of the shots the walk entered since the last one (every shot
    /// in between on a straight walk, only the landing shot after a
    /// `::jump`), with the trailing source-only steps of the shots it left.
    pub(super) fn enter_unit(&mut self, unit: i64) {
        if !self.cx.shots || unit <= self.shot {
            return;
        }
        if self.jumped {
            self.shot_head(unit);
        } else {
            let from = self.shot.max(0);
            if from > 0 {
                self.trailing(from);
            }
            for n in from + 1..=unit {
                self.shot_head(n);
                if n < unit {
                    self.trailing(n);
                }
            }
        }
        self.shot = unit;
    }

    pub(super) fn shot_head(&mut self, n: i64) {
        let heading = self.headings.get(&n).cloned().unwrap_or_default();
        self.steps.push(Step::Shot { number: n, heading });
    }

    pub(super) fn trailing(&mut self, unit: i64) {
        let markers = self.cx.map.trailing.get(&unit).cloned().unwrap_or_default();
        for mk in &markers {
            self.marker(mk);
        }
    }

    /// A scene walk that ran to its end: the rest of the current shot and
    /// every later (empty) shot.
    pub(super) fn finish_shots(&mut self) {
        if !self.cx.shots {
            return;
        }
        let last = self.headings.keys().copied().max().unwrap_or(0);
        if self.shot > 0 {
            self.trailing(self.shot);
        }
        for n in self.shot + 1..=last {
            self.shot_head(n);
            self.trailing(n);
        }
        self.shot = last;
    }

    /// The source-only steps after the last record of the unit the walk
    /// ran off the end of — a presented entry's or bundle beat's own unit:
    /// a body ending in a `::use` closes its component frame there.
    pub(super) fn close_unit(&mut self) {
        if let Some(unit) = self.unit.take() {
            self.trailing(unit);
        }
    }

    pub(super) fn marker(&mut self, mk: &SourceMarker) {
        use lute_compile::source_map::ComponentBoundary as B;
        let component_boundary = mk.component.map(|c| match c {
            B::Begin => ComponentBoundary::Begin,
            B::End => ComponentBoundary::End,
            B::Body => ComponentBoundary::Body,
            B::BodyEnd => ComponentBoundary::BodyEnd,
        });
        self.steps.push(Step::Directive {
            tag: mk.tag.clone(),
            component_boundary,
            component: mk.name.clone(),
            call: None,
            exit: mk.tag == lute_manifest::core::CLEAR_DIRECTIVE,
            reason: None,
        });
    }

    // -- records ----------------------------------------------------------

    /// The walk reached the record at `addr`: open the shot heads it
    /// entered and show the source-only steps that precede it.
    pub(super) fn reach(&mut self, addr: &str) {
        if let Some(unit) = addr.split('-').next().and_then(|u| u.parse::<i64>().ok()) {
            self.enter_unit(unit);
            self.unit = Some(unit);
        }
        let before = self
            .info(addr)
            .map(|i| i.before.clone())
            .unwrap_or_default();
        for mk in &before {
            self.marker(mk);
        }
    }

    /// [`Self::reach`] ahead of the record: a pick or a halt at `addr`
    /// happens before (or instead of) its record.
    pub(super) fn reach_once(&mut self, addr: &str) {
        if self.reached.as_deref() != Some(addr) {
            self.reach(addr);
            self.reached = Some(addr.to_string());
        }
    }

    pub(super) fn record(&mut self, rec: Json) {
        let kind = rec
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let addr = rec
            .get("position")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if kind == "exclusive" {
            let text = rec
                .get("text")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            self.exclusive.push(text.clone());
            self.steps.push(Step::Exclusive { text });
            return;
        }
        if kind == "grant" {
            self.grant(&rec);
            return;
        }
        if addr.is_empty() {
            // `quest` / `objective` / `occasion` records: the observations
            // carry what trace reports of them.
            return;
        }
        let info = self.info(&addr).cloned();
        // A plugin call's declared effects are recorded at the call's own
        // `addr`, after it: its source-only steps were shown with the call.
        let effect_of = rec
            .get("effectOf")
            .and_then(Json::as_str)
            .map(str::to_string);
        if effect_of.is_none() && self.reached.take().as_deref() != Some(addr.as_str()) {
            self.reach(&addr);
        }
        self.jumped = false;
        if info.as_ref().is_some_and(|i| i.injected) {
            return;
        }
        let str_of = |k: &str| rec.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        match kind.as_str() {
            "line" => {
                let span = info.as_ref().map(|i| i.span);
                let delivery = span
                    .and_then(|s| self.cx.ast.deliveries.get(&key(&s)).cloned())
                    .flatten();
                self.said.push(crate::exec::record::said_line(
                    &rec,
                    self.cmds.get(&addr),
                ));
                self.steps.push(Step::Line {
                    speaker: str_of("speaker"),
                    text: str_of("text"),
                    delivery,
                });
            }
            "set" => {
                self.last_write = info.as_ref().map(|i| i.span);
                self.exclusive.clear();
                self.steps.push(Step::Set {
                    path: str_of("path"),
                    value: json_text(rec.get("value")),
                    sugar: info.as_ref().is_some_and(|i| i.sugar),
                    effect_of,
                });
            }
            "assert" => {
                self.last_write = info.as_ref().map(|i| i.span);
                self.exclusive.clear();
                self.steps.push(Step::Assert {
                    text: str_of("fact"),
                    effect_of,
                });
            }
            "retract" => {
                self.last_write = info.as_ref().map(|i| i.span);
                self.exclusive.clear();
                self.steps.push(Step::Retract {
                    text: str_of("pattern"),
                    effect_of,
                });
            }
            "skipped" => {
                let text = info
                    .as_ref()
                    .and_then(|i| i.write_text.clone())
                    .unwrap_or_else(|| {
                        ["path", "fact", "pattern"]
                            .iter()
                            .find_map(|k| rec.get(*k).and_then(Json::as_str))
                            .unwrap_or("")
                            .to_string()
                    });
                self.steps.push(Step::Skipped {
                    effect: str_of("effect"),
                    text,
                });
            }
            "jump" => {
                if info.as_ref().is_some_and(|i| i.authored_jump) {
                    let to = info
                        .as_ref()
                        .and_then(|i| self.cx.ast.next_to.get(&key(&i.span)).cloned())
                        .unwrap_or_default();
                    self.steps.push(Step::Jump { to });
                    self.jumped = true;
                }
            }
            "choice" | "hub" => self.menu_record(&rec, &kind, &addr),
            "hubReturn" => self.steps.push(Step::HubReturn { hub: str_of("hub") }),
            "match" => self.match_record(&rec, &addr),
            "end" => self.steps.push(Step::Directive {
                tag: lute_manifest::core::END_DIRECTIVE.to_string(),
                component_boundary: None,
                component: None,
                call: None,
                exit: false,
                reason: rec.get("reason").and_then(Json::as_str).map(str::to_string),
            }),
            "plugin" => {
                let tag = str_of("tag");
                self.steps.push(Step::Directive {
                    tag: tag.clone(),
                    component_boundary: None,
                    component: None,
                    call: Some(plugin_call(&tag, self.cmds.get(&addr))),
                    exit: false,
                    reason: None,
                });
                if let Some(answer) = self.bridge_answer.take() {
                    self.steps.push(Step::Bridge {
                        tag,
                        answered: answer,
                    });
                }
            }
            "accept" => self.steps.push(Step::Accept {
                quest: str_of("quest"),
                next_run: rec.get("at").and_then(Json::as_str) == Some("nextRun"),
            }),
            "entry" => {
                let (eligible, spent) = match self.head.take() {
                    Some(Head::Judged {
                        eligible, spent, ..
                    }) => (eligible, spent),
                    None => (rec.get("eligible").and_then(Json::as_bool), None),
                };
                self.steps.push(Step::Entry {
                    id: str_of("id"),
                    first_read: rec.get("firstRead").and_then(Json::as_bool).unwrap_or(true),
                    eligible,
                    spent,
                });
            }
            "beat" => {
                let (eligible, after_unmet) = match self.head.take() {
                    Some(Head::Judged {
                        eligible,
                        after_unmet,
                        ..
                    }) => (eligible, after_unmet),
                    None => (rec.get("eligible").and_then(Json::as_bool), false),
                };
                self.steps.push(Step::Beat {
                    id: str_of("id"),
                    eligible,
                    after_unmet,
                });
            }
            "barrier" | "occasion" => {}
            _ => {
                // A staging record: the directive it was lowered from, as
                // authored (an exit or a reason keeps its marked tag).
                let tag = info
                    .as_ref()
                    .and_then(|i| i.directive.clone())
                    .unwrap_or(kind.clone());
                let exit = tag == lute_manifest::core::CLEAR_DIRECTIVE
                    || tag == lute_manifest::core::ACTOR_DIRECTIVE && self.auto_exits(&addr);
                let call = (!exit)
                    .then(|| self.cx.authored.get(&addr).cloned())
                    .flatten();
                self.steps.push(Step::Directive {
                    tag,
                    component_boundary: None,
                    component: None,
                    call,
                    exit,
                    reason: None,
                });
            }
        }
    }

    /// #32 / T2.5: an `::actor` whose `action=` names a declared exit member
    /// ENDS a presence — `Domain.exits`, the list `lute-check::inject` and
    /// `lute-compile::lower` read.
    pub(super) fn auto_exits(&self, addr: &str) -> bool {
        let action = self
            .cmds
            .get(addr)
            .and_then(|c| c.get("action"))
            .and_then(Json::as_str);
        action.is_some_and(|a| {
            self.cx
                .folded
                .domains
                .get("action")
                .is_some_and(|d| d.exits.iter().any(|e| e == a))
        })
    }

    pub(super) fn menu_record(&mut self, rec: &Json, kind: &str, addr: &str) {
        let seen = self.menu.take();
        let Some(chose) = rec.get("chose").and_then(Json::as_str) else {
            return;
        };
        let (construct, id) = if kind == "choice" {
            ("branch", rec.get("branch"))
        } else {
            ("hub", rec.get("hub"))
        };
        let id = id.and_then(Json::as_str).unwrap_or("").to_string();
        let (guard, authored_guard) = self.option_guard(addr, chose);
        let total = self
            .cmds
            .get(addr)
            .and_then(|c| c.get("options"))
            .and_then(Json::as_array)
            .map_or(0, Vec::len);
        let seen = seen.filter(|s| s.addr == addr);
        self.push_decision(Decision {
            construct: construct.to_string(),
            id: id.clone(),
            span: self.option_span(addr, chose),
            outcome: chose.to_string(),
            guard,
            forced: seen.as_ref().and_then(|s| s.forced.as_deref()) == Some(chose),
            auto: seen.as_ref().is_some_and(|s| s.auto),
            eligible: seen.map(|s| s.eligible).unwrap_or_default(),
            authored_id: None,
            authored_guard,
            component: None,
        });
        let fresh = self
            .coverage_seen
            .insert((format!("choice {id}"), chose.to_string()));
        self.coverage_choices
            .entry(id.clone())
            .and_modify(|c| c.visited += usize::from(fresh))
            .or_insert(CoverageCount {
                visited: 1,
                total,
                label: id,
                authored_label: None,
                guard: false,
                component: None,
            });
    }

    pub(super) fn match_record(&mut self, rec: &Json, addr: &str) {
        let subject = self.subject(addr);
        let info = self.info(addr).cloned();
        let span = self.span_at(addr);
        let total = info.as_ref().map_or(0, |i| i.arms.len());
        let authored_id = info.as_ref().and_then(|i| i.authored_id.clone());
        let guard_site = info.as_ref().is_some_and(|i| i.guard);
        let component = self.component_site(info.as_ref());
        let result = rec.get("result").and_then(Json::as_str).unwrap_or("");
        let (outcome, arm) = match result.strip_prefix("arm ") {
            Some(n) => {
                let i = n.parse::<usize>().unwrap_or(1).saturating_sub(1);
                (
                    result.to_string(),
                    info.as_ref().and_then(|x| x.arms.get(i).cloned()),
                )
            }
            None if result == "otherwise" => ("otherwise".to_string(), None),
            None => ("no arm".to_string(), None),
        };
        let visited = usize::from(outcome != "no arm");
        let count_outcome = outcome.clone();
        // T3-22: a `when=` guard is taken or skipped; its lowered `$` arm
        // and empty `<otherwise>` are compiler plumbing.
        let decision = if guard_site {
            Decision {
                construct: "guard".to_string(),
                id: subject.clone(),
                span,
                outcome: if arm.is_some() {
                    report::GUARD_TAKEN
                } else {
                    report::GUARD_SKIPPED
                }
                .to_string(),
                guard: None,
                forced: false,
                auto: false,
                eligible: Vec::new(),
                authored_id: authored_id.clone(),
                authored_guard: None,
                component: component.clone(),
            }
        } else {
            Decision {
                construct: "match".to_string(),
                id: subject.clone(),
                span,
                outcome,
                guard: arm.as_ref().and_then(|a| a.guard.clone()),
                forced: false,
                auto: false,
                eligible: Vec::new(),
                authored_id: authored_id.clone(),
                authored_guard: arm.and_then(|a| a.authored_guard),
                component: component.clone(),
            }
        };
        self.push_decision(decision);
        let site = report::site_key_in(&span, component.as_ref());
        let count = CoverageCount {
            visited,
            total,
            label: subject,
            authored_label: authored_id,
            guard: guard_site,
            component,
        };
        if visited == 1 {
            let fresh = self
                .coverage_seen
                .insert((format!("arm {site}"), count_outcome));
            self.coverage_arms
                .entry(site)
                .and_modify(|c| c.visited += usize::from(fresh))
                .or_insert(count);
        } else {
            self.coverage_arms.entry(site).or_insert(count);
        }
    }

    /// Where a construct a component `::use` expanded is written: the
    /// component's file and which use (`None` for the document's own).
    pub(super) fn component_site(&self, info: Option<&SourceInfo>) -> Option<ComponentSite> {
        let c = info?.component.as_ref()?;
        Some(ComponentSite {
            name: c.name.clone(),
            file: self
                .cx
                .component_files
                .get(&c.name)
                .cloned()
                .unwrap_or_default(),
            scope: c.scope.clone(),
        })
    }

    /// A `match` record's subject text.
    pub(super) fn subject(&self, addr: &str) -> String {
        self.cmds
            .get(addr)
            .and_then(|c| c.get("subject"))
            .and_then(|v| v.get("cel").and_then(Json::as_str))
            .unwrap_or("")
            .to_string()
    }

    pub(super) fn grant(&mut self, rec: &Json) {
        let reward = rec.get("reward").cloned().unwrap_or(Json::Null);
        let str_of = |v: &Json, k: &str| v.get(k).and_then(Json::as_str).map(str::to_string);
        self.steps.push(Step::Grant {
            quest: str_of(rec, "quest").unwrap_or_default(),
            instance: rec.get("instance").and_then(Json::as_u64).unwrap_or(0),
            objective: str_of(rec, "objective"),
            index: rec.get("index").and_then(Json::as_u64).unwrap_or(0) as usize,
            reward: GrantReward {
                kind: str_of(&reward, "kind").unwrap_or_default(),
                target: str_of(&reward, "target"),
                amount: reward.get("amount").and_then(Json::as_i64),
                amount_min: reward.get("amountMin").and_then(Json::as_i64),
                amount_max: reward.get("amountMax").and_then(Json::as_i64),
            },
            on_failed: rec.get("onFailed").and_then(Json::as_bool) == Some(true),
            credited: rec.get("credited").map(|c| GrantCredit {
                path: str_of(c, "path").unwrap_or_default(),
                value: json_text(c.get("value")),
            }),
        });
    }
}
