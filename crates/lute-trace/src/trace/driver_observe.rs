use super::*;
impl<'a> TraceDriver<'a> {
    pub(super) fn observation(&mut self, rec: Json) {
        let str_of = |k: &str| rec.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        let guard = rec.get("guard").and_then(Json::as_str).map(str::to_string);
        let outcome = str_of("outcome");
        let quest = str_of("quest");
        let quest_src = self.cx.map.quests.get(&quest);
        match rec.get("kind").and_then(Json::as_str) {
            Some("quest") => {
                let span = quest_src
                    .map(|q| q.span)
                    .unwrap_or_else(mock::synthetic_span);
                if matches!(outcome.as_str(), "complete" | "failed") {
                    // §3.2: a terminal quest's objectives "can no longer
                    // affect any outcome" — their unresolved entries go.
                    let spans: Vec<Span> = quest_src
                        .map(|q| q.objectives.values().map(|o| o.span).collect())
                        .unwrap_or_default();
                    self.unresolved
                        .retain(|u| !(u.construct == "objective" && spans.contains(&u.span)));
                }
                let forced = rec.get("forced").and_then(Json::as_bool) == Some(true);
                self.push_decision(bare_decision("quest", &quest, span, outcome, guard, forced));
            }
            Some("objective") => {
                let objective = str_of("objective");
                let span = quest_src
                    .and_then(|q| q.objectives.get(&objective))
                    .map(|o| o.span)
                    .unwrap_or_else(mock::synthetic_span);
                self.push_decision(bare_decision(
                    "objective",
                    &objective,
                    span,
                    outcome,
                    guard,
                    false,
                ));
            }
            Some("on") => {
                let span = quest_src
                    .and_then(|q| q.handlers.get(&str_of("addr")))
                    .copied()
                    .unwrap_or_else(mock::synthetic_span);
                let event = str_of("event");
                self.push_decision(bare_decision("on", &event, span, outcome, guard, false));
            }
            Some("acceptSpent") => self.spent_accepts.push(format!(
                "{NOTE_ACCEPT_SPENT} `{quest}` spent: its parent quest `{}` is {} — an \
                 `activate=\"accept\"` child activates only while its parent is active",
                str_of("parent"),
                str_of("why"),
            )),
            _ => {}
        }
    }
}
