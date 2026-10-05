use super::*;

impl Presence<'_> {
    /// Walk `body` under `conds` added to the guards pushed so far; they
    /// are popped when it ends. A write inside still reaches the outer
    /// guards: the region runs before what follows it.
    fn region(&mut self, conds: Vec<(Expr, String)>, body: &[Node]) {
        let depth = self.guards.len();
        let before = self.changed.clone();
        for c in conds {
            let required = self.required(&c.0, EXPAND_DEPTH);
            self.changed.extend(required);
            self.push(Some(c));
        }
        self.walk(body);
        self.guards.truncate(depth);
        self.changed = before;
    }

    /// Alternative paths (a branch's choices, a match's arms): each starts
    /// from the guards as they are here, and an outer guard is still live
    /// after them only when it is live at the end of every one.
    fn fork(&mut self, arms: Vec<(Vec<(Expr, String)>, &[Node])>) {
        let saved: Vec<bool> = self.guards.iter().map(|g| g.live).collect();
        let mut joined = saved.clone();
        for (conds, body) in arms {
            for (g, &l) in self.guards.iter_mut().zip(&saved) {
                g.live = l;
            }
            self.region(conds, body);
            for (j, g) in joined.iter_mut().zip(&self.guards) {
                *j &= g.live;
            }
        }
        for (g, l) in self.guards.iter_mut().zip(joined) {
            g.live = l;
        }
    }

    pub(super) fn choice_conds(&self, choices: &[lute_syntax::ast::Choice]) -> Vec<Vec<(Expr, String)>> {
        choices
            .iter()
            .map(|c| {
                c.when
                    .as_ref()
                    .and_then(|w| self.parse(&w.raw, None))
                    .into_iter()
                    .collect()
            })
            .collect()
    }

    pub(super) fn walk(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    if let Some((id, _)) = literal_attr(&l.attrs, "id") {
                        self.label(id);
                    }
                    self.line(l);
                }
                Node::Directive(d) => self.directive(d),
                Node::Set(s) => self.kill_path(&s.path),
                Node::Timeline(t) => {
                    for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                        match &clip.node {
                            ClipNode::Set(s) => self.kill_path(&s.path),
                            ClipNode::Directive(d) => self.directive(d),
                        }
                    }
                }
                Node::Assert(a) => self.kill_fact(&a.pattern, true),
                Node::Retract(r) => self.kill_fact(&r.pattern, false),
                Node::Branch(b) => {
                    let conds = self.choice_conds(&b.choices);
                    self.fork(
                        conds
                            .into_iter()
                            .zip(b.choices.iter().map(|c| &c.body[..]))
                            .collect(),
                    );
                }
                Node::Hub(h) => {
                    // A hub body runs again and again: whatever one round
                    // writes, the next round's guards may not survive.
                    for b in h.bodies() {
                        self.kill_writes(b);
                    }
                    let conds = self.choice_conds(&h.choices);
                    // dsl 0.28.0 §5: the `<return>` block is one more path,
                    // under no guard of its own.
                    let back = h.on_return.iter().map(|r| (Vec::new(), &r.body[..]));
                    self.fork(
                        conds
                            .into_iter()
                            .zip(h.choices.iter().map(|c| &c.body[..]))
                            .chain(back)
                            .collect(),
                    );
                }
                Node::Match(m) => self.match_arms(m),
                Node::On(o) => {
                    let mut conds = Vec::new();
                    if let Some(q) = &self.quest {
                        let state = match o.event.as_str() {
                            "questComplete" => "complete",
                            "questFailed" => "failed",
                            _ => "active",
                        };
                        conds.extend(self.parse(&format!("quest.{q}.state == '{state}'"), None));
                    }
                    conds.extend(o.when.as_ref().and_then(|w| self.parse(&w.raw, None)));
                    // Raising an occasion fires the same-named event: the
                    // handler runs on it (dsl 0.25.0 §6).
                    let before = self.follow(&o.event);
                    let depth = self.guards.len();
                    if o.event == "questComplete" {
                        self.assume_completion(o.span.byte_start);
                    }
                    self.region(conds, &o.body);
                    self.guards.truncate(depth);
                    self.changed = before;
                }
                Node::Objective(o) => {
                    let cond = self.parse(&o.done.raw, None);
                    // An `on=` objective completes when its occasion judges it.
                    let before = o.on.as_ref().map(|(on, _)| self.follow(on));
                    self.region(cond.into_iter().collect(), &o.body);
                    if let Some(before) = before {
                        self.changed = before;
                    }
                }
            }
        }
    }

    fn directive(&mut self, d: &Directive) {
        use lute_manifest::core::MARK_DIRECTIVE;
        if d.tag == MARK_DIRECTIVE {
            if let Some((id, _)) = literal_attr(&d.attrs, "id") {
                self.label(id);
            }
        } else if let Some((quest, _)) = d.accept_quest() {
            self.kill_path(&format!("quest.{quest}"));
        } else if d.tag == "use" {
            self.use_lines(d);
        } else if let Some(facts) = self.folded.env.rel_vocab.call_facts(&d) {
            // dsl 0.27.0 §4: a call's declared fact effects.
            for (pattern, up) in facts.writes() {
                self.kill_fact(pattern, up);
            }
        }
    }

    /// dsl 0.26.0 §3.2: the `@@p:` lines a `::use` speaks, each by the
    /// member it binds, under the `::use`'s guard and the line's own —
    /// reported at the `::use`, once per member and guard.
    pub(super) fn use_lines(&mut self, d: &Directive) {
        let folded = self.folded;
        let Some(lines) = folded.use_lines.get(&d.span.byte_start) else {
            return;
        };
        let component = literal_attr(&d.attrs, "component").map(|(c, _)| c);
        let mut seen = std::collections::BTreeSet::new();
        for l in lines {
            if l.attrs.iter().any(|a| a.key == "vo") {
                continue;
            }
            let raw = match (&d.when, &l.when) {
                (Some(g), Some(a)) => Some(format!("({}) && ({})", g.raw, a.raw)),
                (Some(g), None) => Some(g.raw.clone()),
                (None, Some(a)) => Some(a.raw.clone()),
                (None, None) => None,
            };
            if !seen.insert((l.speaker.as_str(), raw.clone())) {
                continue;
            }
            let own = raw.and_then(|r| self.parse(&r, None));
            let before = self.changed.clone();
            if let Some((e, _)) = &own {
                let required = self.required(e, EXPAND_DEPTH);
                self.changed.extend(required);
            }
            self.decide_line(l, d.span, own, component);
            self.changed = before;
        }
    }

    /// Each arm assumes its own pattern and that no earlier arm matched.
    fn match_arms(&mut self, m: &Match) {
        let subject = self.expand_text(&m.subject.raw, None);
        let path = crate::match_check::subject_path(m);
        let s = subject_text(&subject);
        let mut earlier: Vec<Option<(Expr, String)>> = Vec::new();
        let mut arms = Vec::new();
        for arm in &m.arms {
            let mut conds: Vec<(Expr, String)> = earlier
                .iter()
                .flatten()
                .map(|(expr, text)| {
                    (
                        call(op::LOGICAL_NOT, vec![expr.clone()]),
                        format!("!({text})"),
                    )
                })
                .collect();
            let own = match arm {
                Arm::When { is, test, .. } => {
                    let is = match is {
                        Some(p) => is_condition(&p.raw, &s, path.as_deref()).map(Some),
                        None => Some(None),
                    };
                    let test = (!test.raw.trim().is_empty()).then(|| test.raw.as_str());
                    match (is, test) {
                        (Some(Some(is)), Some(t)) => {
                            self.parse(&format!("({is}) && ({t})"), Some(&subject))
                        }
                        (Some(Some(is)), None) => self.parse(&is, Some(&subject)),
                        (Some(None), Some(t)) => self.parse(t, Some(&subject)),
                        // No pattern at all, or one that does not classify.
                        (Some(None), None) | (None, _) => None,
                    }
                }
                Arm::Otherwise { .. } => None,
            };
            conds.extend(own.clone());
            let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
            arms.push((conds, &body[..]));
            earlier.push(own);
        }
        self.fork(arms);
    }

    /// A `::next` target: the jump may arrive from outside the enclosing
    /// regions, so their guards stop counting from here.
    fn label(&mut self, id: &str) {
        if self.jumps.contains(id) {
            for g in &mut self.guards[self.base..] {
                g.live = false;
            }
        }
    }

    /// A write of state `path`: every guard reading it, a record it sits
    /// in, or a field under it stops counting.
    fn kill_path(&mut self, path: &str) {
        let segments: Vec<&str> = path.split('.').collect();
        let prefixes: Vec<String> = (2..=segments.len())
            .map(|n| segments[..n].join("."))
            .collect();
        for g in &mut self.guards {
            if g.text.contains(path) || prefixes.iter().any(|p| g.text.contains(p.as_str())) {
                g.live = false;
            }
        }
    }

    /// An `::assert` (`up`) or `::retract` of `pattern`: every guard it may
    /// falsify ([`write_effects`], [`affected`]) stops counting.
    fn kill_fact(&mut self, pattern: &FactPattern, up: bool) {
        if pattern.relation.is_empty() {
            return;
        }
        let args = pattern_args(pattern);
        let effects = write_effects(&self.folded.env.rel_vocab, &pattern.relation, args, up);
        for g in &mut self.guards {
            if g.live && affected(&effects, g) {
                g.live = false;
            }
        }
    }

    /// Every write anywhere in `nodes`.
    pub(super) fn kill_writes(&mut self, nodes: &[Node]) {
        let mut paths = Vec::new();
        let mut facts = Vec::new();
        let mut quests = Vec::new();
        let mut calls = Vec::new();
        visit(nodes, &mut |node| match node {
            Node::Set(s) => paths.push(s.path.clone()),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Set(s) = &clip.node {
                        paths.push(s.path.clone());
                    }
                }
            }
            Node::Assert(a) => facts.push((&a.pattern, true)),
            Node::Retract(r) => facts.push((&r.pattern, false)),
            Node::Directive(d) => {
                if let Some((q, _)) = d.accept_quest() {
                    quests.push(format!("quest.{q}"));
                }
                if let Some(facts) = self.folded.env.rel_vocab.call_facts(&d) {
                    calls.push(facts);
                }
            }
            _ => {}
        });
        for p in paths.iter().chain(&quests) {
            self.kill_path(p);
        }
        for (pattern, up) in facts {
            self.kill_fact(pattern, up);
        }
        for facts in &calls {
            for (pattern, up) in facts.writes() {
                self.kill_fact(pattern, up);
            }
        }
    }

}
