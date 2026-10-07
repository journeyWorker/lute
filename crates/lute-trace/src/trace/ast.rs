use super::*;
// ---------------------------------------------------------------------
// The AST texts the source map does not carry.
// ---------------------------------------------------------------------

/// A span's identity (the byte range).
pub(super) fn key(span: &Span) -> (usize, usize) {
    (span.byte_start, span.byte_end)
}

/// Per content line its authored delivery, per `::next` its label — keyed by
/// the node's span, which the source map gives every record.
#[derive(Default)]
pub(super) struct AstIndex {
    pub(super) deliveries: HashMap<(usize, usize), Option<String>>,
    pub(super) next_to: HashMap<(usize, usize), String>,
    /// Per `<entry>` id, its `when` slot's span.
    pub(super) entry_when: HashMap<String, Span>,
    /// Per bundle `<beat>` canonical id suffix (`.<beat id>`), its `when`
    /// slot's span.
    pub(super) beat_when: Vec<(String, Span)>,
}

impl AstIndex {
    pub(super) fn of(doc: &Document) -> Self {
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
                Node::Directive(d) if d.tag == lute_manifest::core::NEXT_DIRECTIVE => {
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
                Node::Branch(b) => b.choices.iter().for_each(|c| self.nodes(&c.body)),
                Node::Hub(h) => h.bodies().for_each(|b| self.nodes(b)),
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.nodes(body)
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
pub(super) struct TraceContext<'a> {
    pub(super) art: &'a Json,
    pub(super) map: &'a SourceMap,
    pub(super) ast: AstIndex,
    pub(super) mocks: &'a MockSet,
    pub(super) folded: &'a FoldedEnv,
    pub(super) snapshot: &'a lute_manifest::snapshot::CapabilitySnapshot,
    pub(super) content_reads: &'a BTreeSet<String>,
    /// The members an unbound `occasion.target` may take (its hint).
    pub(super) members: Vec<String>,
    /// A scene walk: records open `Shot` heads.
    pub(super) shots: bool,
    /// Every component the document imports, by name → its file.
    pub(super) component_files: BTreeMap<String, String>,
    /// addr → a staging directive as authored (`::sfx{id="bell"}`), what
    /// `lute play` prints for it.
    pub(super) authored: BTreeMap<String, String>,
}

/// The menu a pick answered, until its record arrives.
pub(super) struct MenuSeen {
    pub(super) addr: String,
    /// Every option offered at this presentation point.
    pub(super) eligible: Vec<String>,
    pub(super) auto: bool,
    /// The option a scripted pick forced past an undecided guard.
    pub(super) forced: Option<String>,
}

/// A presentation head the pipeline judged before the walk, by the
/// session's rule: its eligibility, the `once` an earlier read spent (an
/// entry), and whether its `after=` is what is false (a bundle beat).
pub(super) enum Head {
    Judged {
        eligible: Option<bool>,
        spent: Option<String>,
        after_unmet: bool,
    },
}

/// `lute trace`'s [`Driver`] (design §3.3) and report builder (§3.6).
pub(crate) struct TraceDriver<'a> {
    pub(super) cx: TraceContext<'a>,
    /// `addr` → the artifact command.
    pub(super) cmds: HashMap<String, Json>,
    /// Shot number → heading.
    pub(super) headings: BTreeMap<i64, String>,
    /// Per `<branch>` id, how many decisions of a multi-decision `choose`
    /// list earlier presentations consumed.
    pub(super) branch_cursor: BTreeMap<String, usize>,
    /// Per plugin directive tag, how many `bridges:` answers earlier calls
    /// consumed.
    pub(super) bridge_cursor: BTreeMap<String, usize>,
    /// dsl 0.24.0 §5: result slot → `(tag, field, answer shape)` of a plugin
    /// call that found no answer for it — a read of it is hinted as the
    /// missing answer ([`TraceDriver::render_atom`]).
    pub(super) bridge_unanswered: BTreeMap<String, (String, String, String)>,
    /// The answer the last bridge call consumed (`None`: none left), until
    /// its `plugin` record arrives.
    pub(super) bridge_answer: Option<Option<BridgeAnswer>>,
    pub(super) menu: Option<MenuSeen>,
    pub(super) head: Option<Head>,
    /// The shot whose head was shown last (`0`: none yet).
    pub(super) shot: i64,
    /// The addressing unit of the last record reached (an entry's or a
    /// bundle beat's own unit when one is presented).
    pub(super) unit: Option<i64>,
    /// The menu `addr` a pick already reached (its heads and markers shown).
    pub(super) reached: Option<String>,
    /// The last record was an authored `::next` jump.
    pub(super) jumped: bool,
    /// The span of the last write, assert or retract record.
    pub(super) last_write: Option<Span>,
    /// `exclusive` records since the last write.
    pub(super) exclusive: Vec<String>,
    /// A refusal the driver ruled (`E-TRACE-CHOICE`).
    pub(super) refused: Option<Diagnostic>,

    pub(super) steps: Vec<Step>,
    pub(super) decisions: Vec<Decision>,
    pub(super) unresolved: Vec<UnresolvedEntry>,
    pub(super) forced_unknown: Vec<UnresolvedEntry>,
    pub(super) coverage_choices: BTreeMap<String, CoverageCount>,
    pub(super) coverage_arms: BTreeMap<String, CoverageCount>,
    /// (construct key, outcome) pairs already counted, so a hub picked
    /// again or a `<match>` re-run counts each option / arm once: coverage
    /// is how many distinct ones ran, never how many times.
    pub(super) coverage_seen: BTreeSet<(String, String)>,
    pub(super) said: Vec<String>,
    pub(super) spent_accepts: Vec<String>,
    /// Round-5 T3-12: presented scene / entry / beat id → the false premise
    /// its eligibility verdict names ([`TraceReport::premises`]).
    pub(super) premises: BTreeMap<String, String>,
    /// HW27-04: the same ids whose premise is a closed seam
    /// ([`TraceReport::not_raised`]).
    pub(super) not_raised: BTreeMap<String, crate::report::NotRaised>,
    /// T1-25: the same ids → which premise failed
    /// ([`TraceReport::ineligible_by`]).
    pub(super) ineligible_by: BTreeMap<String, &'static str>,
}
