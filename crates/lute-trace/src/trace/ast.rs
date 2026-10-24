use super::{
    line_delivery, Arm, AttrValue, BTreeMap, BTreeSet, BridgeAnswer, CoverageCount, Decision,
    Diagnostic, Document, FoldedEnv, HashMap, Json, MockSet, Node, SourceMap, Span, Step,
    UnresolvedEntry,
};
// ---------------------------------------------------------------------
// The AST texts the source map does not carry.
// ---------------------------------------------------------------------

/// A span's identity (the byte range).
pub(crate) fn key(span: &Span) -> (usize, usize) {
    (span.byte_start, span.byte_end)
}

/// Per content line its authored delivery, per `::jump` its label — keyed by
/// the node's span, which the source map gives every record.
#[derive(Default)]
pub(crate) struct AstIndex {
    pub(crate) deliveries: HashMap<(usize, usize), Option<String>>,
    pub(crate) next_to: HashMap<(usize, usize), String>,
    /// Per `<entry>` id, its `when` slot's span.
    pub(crate) entry_when: HashMap<String, Span>,
    /// Per bundle `<beat>` canonical id suffix (`.<beat id>`), its `when`
    /// slot's span.
    pub(crate) beat_when: Vec<(String, Span)>,
}

impl AstIndex {
    pub(crate) fn of(doc: &Document) -> Self {
        let mut ix = AstIndex::default();
        for shot in &doc.sections {
            ix.nodes(&shot.body);
        }
        for q in &doc.quests {
            ix.nodes(&q.body);
        }
        for e in &doc.entries {
            ix.nodes(&e.body);
            if let Some(w) = &e.when {
                ix.entry_when.insert(e.id.clone(), w.span);
            }
        }
        for b in &doc.beats {
            ix.nodes(&b.body);
            if let Some(w) = &b.when {
                ix.beat_when.push((format!(".{}", b.id), w.span));
            }
        }
        ix
    }

    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    self.deliveries.insert(key(&l.span), line_delivery(l));
                }
                Node::Directive(d) if d.tag == lute_manifest::core::JUMP_DIRECTIVE => {
                    let to = d
                        .attrs
                        .iter()
                        .find(|a| a.key == "to")
                        .and_then(|a| match &a.value {
                            AttrValue::Str(s) => Some(s.clone()),
                            _ => None,
                        });
                    if let Some(to) = to {
                        self.next_to.insert(key(&d.span), to);
                    }
                }
                Node::Branch(b) => {
                    for c in &b.choices {
                        self.nodes(&c.body);
                    }
                }
                Node::Hub(h) => {
                    for b in h.bodies() {
                        self.nodes(b);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.nodes(body);
                            }
                        }
                    }
                }
                Node::On(o) => self.nodes(&o.body),
                Node::Objective(o) => self.nodes(&o.body),
                Node::Directive(_)
                | Node::Set(_)
                | Node::Assert(_)
                | Node::Retract(_)
                | Node::Timeline(_) => {}
            }
        }
    }
}

// ---------------------------------------------------------------------
// The trace driver: trace's policies, and the report built from records.
// ---------------------------------------------------------------------

/// What a [`TraceDriver`] reads.
pub(crate) struct TraceContext<'a> {
    pub(crate) art: &'a Json,
    pub(crate) map: &'a SourceMap,
    pub(crate) ast: AstIndex,
    pub(crate) mocks: &'a MockSet,
    pub(crate) folded: &'a FoldedEnv,
    pub(crate) snapshot: &'a lute_manifest::snapshot::CapabilitySnapshot,
    pub(crate) content_reads: &'a BTreeSet<String>,
    /// The members an unbound `occasion.target` may take (its hint).
    pub(crate) members: Vec<String>,
    /// A scene walk: records open `Shot` heads.
    pub(crate) shots: bool,
    /// Every component the document imports, by name → its file.
    pub(crate) component_files: BTreeMap<String, String>,
    /// addr → a staging directive as authored (`::sfx{id="bell"}`), what
    /// `lute play` prints for it.
    pub(crate) authored: BTreeMap<String, String>,
}

/// The menu a pick answered, until its record arrives.
pub(crate) struct MenuSeen {
    pub(crate) addr: String,
    /// Every option offered at this presentation point.
    pub(crate) eligible: Vec<String>,
    pub(crate) auto: bool,
    /// The option a scripted pick forced past an undecided guard.
    pub(crate) forced: Option<String>,
}

/// A presentation head the pipeline judged before the walk, by the
/// session's rule: its eligibility, the `once` an earlier read spent (an
/// entry), and whether its `after=` is what is false (a bundle beat).
pub(crate) enum Head {
    Judged {
        eligible: Option<bool>,
        spent: Option<String>,
        after_unmet: bool,
    },
}

/// `lute trace`'s [`Driver`] (design §3.3) and report builder (§3.6).
pub(crate) struct TraceDriver<'a> {
    pub(crate) cx: TraceContext<'a>,
    /// `addr` → the artifact command.
    pub(crate) cmds: HashMap<String, Json>,
    /// Shot number → heading.
    pub(crate) headings: BTreeMap<i64, String>,
    /// Per `<branch>` id, how many decisions of a multi-decision `choose`
    /// list earlier presentations consumed.
    pub(crate) branch_cursor: BTreeMap<String, usize>,
    /// Per plugin directive tag, how many `bridges:` answers earlier calls
    /// consumed.
    pub(crate) bridge_cursor: BTreeMap<String, usize>,
    /// dsl 0.24.0 §5: result slot → `(tag, field, answer shape)` of a plugin
    /// call that found no answer for it — a read of it is hinted as the
    /// missing answer ([`TraceDriver::render_atom`]).
    pub(crate) bridge_unanswered: BTreeMap<String, (String, String, String)>,
    /// The answer the last bridge call consumed (`None`: none left), until
    /// its `plugin` record arrives.
    pub(crate) bridge_answer: Option<Option<BridgeAnswer>>,
    pub(crate) menu: Option<MenuSeen>,
    pub(crate) head: Option<Head>,
    /// The shot whose head was shown last (`0`: none yet).
    pub(crate) shot: i64,
    /// The addressing unit of the last record reached (an entry's or a
    /// bundle beat's own unit when one is presented).
    pub(crate) unit: Option<i64>,
    /// The menu `addr` a pick already reached (its heads and markers shown).
    pub(crate) reached: Option<String>,
    /// The last record was an authored `::jump` jump.
    pub(crate) jumped: bool,
    /// The span of the last write, assert or retract record.
    pub(crate) last_write: Option<Span>,
    /// `exclusive` records since the last write.
    pub(crate) exclusive: Vec<String>,
    /// A refusal the driver ruled (`E-TRACE-CHOICE`).
    pub(crate) refused: Option<Diagnostic>,

    pub(crate) steps: Vec<Step>,
    pub(crate) decisions: Vec<Decision>,
    pub(crate) unresolved: Vec<UnresolvedEntry>,
    pub(crate) forced_unknown: Vec<UnresolvedEntry>,
    pub(crate) coverage_choices: BTreeMap<String, CoverageCount>,
    pub(crate) coverage_arms: BTreeMap<String, CoverageCount>,
    /// (construct key, outcome) pairs already counted, so a hub picked
    /// again or a `<match>` re-run counts each option / arm once: coverage
    /// is how many distinct ones ran, never how many times.
    pub(crate) coverage_seen: BTreeSet<(String, String)>,
    pub(crate) said: Vec<String>,
    pub(crate) spent_accepts: Vec<String>,
    /// Round-5 T3-12: presented scene / entry / beat id → the false premise
    /// its eligibility verdict names ([`TraceReport::premises`]).
    pub(crate) premises: BTreeMap<String, String>,
    /// HW27-04: the same ids whose premise is a closed seam
    /// ([`TraceReport::not_raised`]).
    pub(crate) not_raised: BTreeMap<String, crate::report::NotRaised>,
    /// T1-25: the same ids → which premise failed
    /// ([`TraceReport::ineligible_by`]).
    pub(crate) ineligible_by: BTreeMap<String, &'static str>,
}
