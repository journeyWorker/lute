use super::*;
impl Driver for TraceDriver<'_> {
    fn is_preview(&self) -> bool {
        true
    }
    /// an automatic pick: a branch takes its first open option; a hub makes
    /// one document-order pass over its open non-`exit` options, then takes
    /// the first open `exit`. A hub's scripted list is its visit sequence,
    /// as in `lute play`: running out while the hub is still open halts the
    /// walk incomplete.
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        self.reach_once(menu.addr);
        let eligible = menu
            .options
            .iter()
            .filter(|o| o.verdict == Verdict::Open)
            .map(|o| o.id.clone())
            .collect();
        let list = self.cx.mocks.choose.get(menu.id).map(Vec::as_slice);
        let pick = match (menu.construct, list) {
            (MenuKind::Branch, None | Some([])) => Pick::AutoFirst,
            (MenuKind::Branch, Some([only])) => Pick::Option(only.clone()),
            (MenuKind::Branch, Some(list)) => {
                let used = self.branch_cursor.entry(menu.id.to_string()).or_insert(0);
                match list.get(*used) {
                    Some(next) => {
                        *used += 1;
                        Pick::Option(next.clone())
                    }
                    None => {
                        let expression = format!(
                            "choose list exhausted — {} decision(s) scripted, presented again",
                            list.len()
                        );
                        self.record_exhausted(menu, "branch", expression, list);
                        Pick::Unscripted {
                            scripted: list.len(),
                        }
                    }
                }
            }
            (MenuKind::Hub, None) => Pick::HubAutoPass,
            (MenuKind::Hub, Some(list)) => match list.get(menu.presentation) {
                Some(next) => Pick::Option(next.clone()),
                None => {
                    // Nothing left to present: the hub converges on its own
                    // (the machine's natural convergence), not a gap.
                    let open = menu
                        .options
                        .iter()
                        .any(|o| !matches!(o.verdict, Verdict::Spent | Verdict::Closed(_)));
                    if open {
                        let expression = format!(
                            "choose list exhausted — the hub is still open after {} scripted pick(s)",
                            list.len()
                        );
                        self.record_exhausted(menu, "hub", expression, list);
                    }
                    Pick::Unscripted {
                        scripted: list.len(),
                    }
                }
            },
        };
        if let Pick::Option(id) = &pick {
            if !menu.options.iter().any(|o| &o.id == id) {
                let what = match menu.construct {
                    MenuKind::Branch => "names no choice in this branch",
                    MenuKind::Hub => "names no choice in this hub",
                };
                let span = self.span_at(menu.addr);
                self.refused = Some(choice_diag(span, menu.id, id, what));
            }
        }
        self.menu = Some(MenuSeen {
            addr: menu.addr.to_string(),
            eligible,
            auto: matches!(pick, Pick::AutoFirst | Pick::HubAutoPass),
            forced: None,
        });
        pick
    }

    /// A scripted pick past an undecided guard is taken and reported as
    /// forced (§4.4); past a guard decided false — or a spent `once` hub
    /// option — it is refused (`E-TRACE-CHOICE`).
    fn forced(&mut self, menu: &Menu<'_>, option: &str, verdict: &Verdict) -> Forced {
        match verdict {
            Verdict::Open => Forced::Take,
            Verdict::Unknown(atoms) => {
                let (guard, _) = self.option_guard(menu.addr, option);
                let construct = match menu.construct {
                    MenuKind::Branch => "branch",
                    MenuKind::Hub => "hub",
                };
                let atoms = atoms.iter().map(|a| self.render_atom(a)).collect();
                self.forced_unknown.push(UnresolvedEntry {
                    construct: construct.to_string(),
                    id: format!("{} -> {option}", menu.id),
                    span: self.option_span(menu.addr, option),
                    expression: guard.unwrap_or_default(),
                    atoms,
                });
                if let Some(seen) = &mut self.menu {
                    seen.forced = Some(option.to_string());
                }
                Forced::Take
            }
            Verdict::Spent => {
                let reason = format!(
                    "it is `once` and already taken in this visit of hub `{}`",
                    menu.id
                );
                let span = self.option_span(menu.addr, option);
                self.refused = Some(choice_diag(span, menu.id, option, &reason));
                Forced::Refuse
            }
            Verdict::Closed(reads) => {
                let (guard, authored) = self.option_guard(menu.addr, option);
                let guard = authored.or(guard).unwrap_or_default();
                let premise = guard_premise(reads, |r| format!("mock {}", self.guard_hint(r)));
                let reason = if premise.is_empty() {
                    format!("its guard `{}` decided false", guard.trim())
                } else {
                    format!("its guard `{}` decided false: {premise}", guard.trim())
                };
                let span = self.option_span(menu.addr, option);
                self.refused = Some(choice_diag(span, menu.id, option, &reason));
                Forced::Refuse
            }
        }
    }

    /// The mock's `bridges:` answers, one per call of the tag, in order. A
    /// result field left unanswered reads UNKNOWN, and a read of it is hinted
    /// as the missing answer: every field of the tag content reads, typed.
    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        let used = self.bridge_cursor.entry(call.tag.to_string()).or_insert(0);
        let answer = self
            .cx
            .mocks
            .bridges
            .get(call.tag)
            .and_then(|list| list.get(*used))
            .cloned();
        if answer.is_some() {
            *used += 1;
        }
        let shape = self
            .cx
            .snapshot
            .directives
            .get(call.tag)
            .map(|decl| {
                let writes = lute_runtime::bridge_result_writes(decl);
                // A read the textual scan cannot see (a component body) still
                // halts on UNKNOWN; its hint then names every field.
                let read = mock::bridge_fields_read(&writes, self.cx.content_reads);
                let decls = &self.cx.folded.env.state.decls;
                lute_runtime::bridge_answer_shape(
                    call.reads
                        .iter()
                        .filter(|(f, _)| read.is_empty() || read.contains(f.as_str()))
                        .map(|(f, p)| (f.as_str(), decls.get(p).map(|d| &d.ty))),
                )
            })
            .unwrap_or_default();
        for (field, path) in call.reads {
            let answered = answer
                .as_ref()
                .is_some_and(|a| a.iter().any(|(f, _)| f == field));
            if answered {
                self.bridge_unanswered.remove(path);
            } else {
                self.bridge_unanswered.insert(
                    path.clone(),
                    (call.tag.to_string(), field.clone(), shape.clone()),
                );
            }
        }
        self.bridge_answer = Some(answer.clone());
        match answer {
            Some(a) => BridgeReply::Answer(a),
            None => BridgeReply::Unanswered,
        }
    }

    /// Trace halts where control flow needs the value (an arm, a menu with
    /// nothing open, an unbound `occasion.target`) and records every other
    /// unknown that decides something — a quest's `start`, an objective, a
    /// handler, a presentation head — without halting: the walk goes on
    /// and the exit is incomplete.
    fn unknown(&mut self, site: &UnknownSite<'_>) -> OnUnknown {
        if matches!(
            site.kind,
            SiteKind::Arm
                | SiteKind::OccasionTarget
                | SiteKind::BranchAllUnknown
                | SiteKind::HubAllUnknown
                | SiteKind::Guard
        ) && self.cx.map.by_addr.contains_key(site.addr)
        {
            self.reach_once(site.addr);
        }
        let raw = site.raw.trim().to_string();
        match site.kind {
            // A write through an unbound `occasion.target` (`::set`,
            // `::assert`/`::retract`, a plugin call) or a line interpolating
            // it is no `<match>`: it has no arms to count.
            SiteKind::OccasionTarget
                if self
                    .cmds
                    .get(site.addr)
                    .and_then(|c| c.get("kind"))
                    .and_then(Json::as_str)
                    != Some("match") =>
            {
                let span = self.span_at(site.addr);
                let cmd = self.cmds.get(site.addr);
                let kind = cmd.and_then(|c| c.get("kind")).and_then(Json::as_str);
                let (construct, id, expr) = match kind {
                    Some("line") => ("line", "placeholder".to_string(), site.id.to_string()),
                    Some("plugin") => {
                        ("write", format!("::{}", site.id), plugin_call(site.id, cmd))
                    }
                    Some(k) => ("write", format!("::{k}"), site.id.to_string()),
                    None => ("write", String::new(), site.id.to_string()),
                };
                // One cause, one report: the beat's own `when` already
                // names the missing member when it read it first.
                let hints: Vec<String> = site.atoms.iter().map(|a| self.render_atom(a)).collect();
                let named = self
                    .unresolved
                    .iter()
                    .any(|u| !hints.is_empty() && hints.iter().all(|h| u.atoms.contains(h)));
                if !named {
                    self.record_unresolved(construct, &id, span, expr, site.atoms);
                }
                OnUnknown::Halt
            }
            SiteKind::Arm | SiteKind::OccasionTarget => {
                let span = self.span_at(site.addr);
                let info = self.info(site.addr).cloned();
                let guard_site = info.as_ref().is_some_and(|i| i.guard);
                let expr = if guard_site {
                    site.id.to_string()
                } else {
                    site.arm
                        .and_then(|i| self.arm(site.addr, i))
                        .and_then(|a| a.guard.clone())
                        .unwrap_or_default()
                };
                let construct = if guard_site { "guard" } else { "match" };
                self.record_unresolved(construct, site.id, span, expr, site.atoms);
                let component = self.component_site(info.as_ref());
                let count = CoverageCount {
                    visited: 0,
                    total: info.as_ref().map_or(0, |i| i.arms.len()),
                    label: site.id.to_string(),
                    authored_label: info.as_ref().and_then(|i| i.authored_id.clone()),
                    guard: guard_site,
                    component: component.clone(),
                };
                self.coverage_arms
                    .entry(report::site_key_in(&span, component.as_ref()))
                    .or_insert(count);
                OnUnknown::Halt
            }
            SiteKind::BranchAllUnknown | SiteKind::HubAllUnknown => {
                let (construct, expression) = if site.kind == SiteKind::BranchAllUnknown {
                    ("branch", "eligibility")
                } else {
                    ("hub", "exit eligibility")
                };
                let span = self.span_at(site.addr);
                self.record_unresolved(
                    construct,
                    site.id,
                    span,
                    expression.to_string(),
                    site.atoms,
                );
                let total = self
                    .cmds
                    .get(site.addr)
                    .and_then(|c| c.get("options"))
                    .and_then(Json::as_array)
                    .map_or(0, Vec::len);
                self.coverage_choices
                    .entry(site.id.to_string())
                    .or_insert(CoverageCount {
                        visited: 0,
                        total,
                        label: site.id.to_string(),
                        authored_label: None,
                        guard: false,
                        component: None,
                    });
                OnUnknown::Halt
            }
            SiteKind::Guard => OnUnknown::Halt,
            SiteKind::EntryWhen => {
                if !matches!(self.head, Some(Head::Judged { spent: Some(_), .. })) {
                    let span = self
                        .cx
                        .ast
                        .entry_when
                        .get(site.id)
                        .copied()
                        .unwrap_or_else(|| self.span_at(site.addr));
                    self.record_unresolved("entry", site.id, span, raw, site.atoms);
                }
                OnUnknown::Continue
            }
            SiteKind::BeatWhen => {
                let span = self
                    .cx
                    .ast
                    .beat_when
                    .iter()
                    .find(|(suffix, _)| site.id.ends_with(suffix.as_str()))
                    .map(|(_, s)| *s)
                    .unwrap_or_else(|| self.span_at(site.addr));
                self.record_unresolved("beat", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            // dsl 0.28.0 (T1-13): the line needs a payload field the mocks
            // did not seed — halt before printing its marker raw.
            SiteKind::Payload => {
                let span = self.span_at(site.addr);
                self.record_unresolved("line", "payload", span, raw, site.atoms);
                OnUnknown::Halt
            }
            SiteKind::QuestStart => {
                let span = self
                    .cx
                    .map
                    .quests
                    .get(site.id)
                    .map(|q| q.span)
                    .unwrap_or_else(mock::synthetic_span);
                self.record_unresolved("quest", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            SiteKind::ObjectiveDone | SiteKind::ObjectiveBy | SiteKind::ObjectiveUntil => {
                let span = site
                    .quest
                    .and_then(|q| self.cx.map.quests.get(q))
                    .and_then(|q| q.objectives.get(site.id))
                    .map(|o| o.span)
                    .unwrap_or_else(mock::synthetic_span);
                self.record_unresolved("objective", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            SiteKind::Handler => {
                let span = site
                    .quest
                    .and_then(|q| self.cx.map.quests.get(q))
                    .and_then(|q| q.handlers.get(site.addr))
                    .copied()
                    .unwrap_or_else(mock::synthetic_span);
                self.record_unresolved("on", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            // Trace reports none of these: an undecided `fail` does not fail
            // the quest, an undecided reward is not granted, an undecided
            // write or bridge result is written unknown and hinted where read.
            SiteKind::QuestFail
            | SiteKind::Reward
            | SiteKind::SetValue
            | SiteKind::BridgeResult => OnUnknown::Continue,
        }
    }

    fn emit(&mut self, rec: Json) {
        self.record(rec);
    }

    fn observe(&mut self, rec: Json) {
        if rec.get("kind").and_then(Json::as_str) == Some("jump") {
            self.record(rec);
        } else {
            self.observation(rec);
        }
    }
}
