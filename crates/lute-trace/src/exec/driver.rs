//! The [`Driver`] seam of the [`crate::exec::Machine`]: the only places the
//! runtimes (`lute run`, `lute play`, `lute trace`) may differ
//! (`docs/design/runtime-unification.md` §3.3). Everything else — guards,
//! effects, derivation, quests, handlers, jumps — is one rule in the Machine.

use std::collections::{BTreeMap, VecDeque};

use serde_json::Value as Json;

use crate::mock::BridgeAnswer;
use crate::value::UnresolvedAtom;

/// What a runtime decides for the walk the [`crate::exec::Machine`] drives.
pub trait Driver {
    /// A `<branch>` / `<hub>` presentation. Option verdicts are already judged.
    fn choose(&mut self, menu: &Menu<'_>) -> Pick;
    /// A picked option whose verdict is not [`Verdict::Open`].
    fn forced(&mut self, menu: &Menu<'_>, option: &str, verdict: &Verdict) -> Forced;
    /// A plugin call that reads a bridge result.
    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply;
    /// A value the walk needs but cannot decide.
    fn unknown(&mut self, site: &UnknownSite<'_>) -> OnUnknown;
    /// Every transcript record, in execution order.
    fn emit(&mut self, rec: Json);
}

impl<D: Driver + ?Sized> Driver for &mut D {
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        (**self).choose(menu)
    }
    fn forced(&mut self, menu: &Menu<'_>, option: &str, verdict: &Verdict) -> Forced {
        (**self).forced(menu, option, verdict)
    }
    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        (**self).bridge(call)
    }
    fn unknown(&mut self, site: &UnknownSite<'_>) -> OnUnknown {
        (**self).unknown(site)
    }
    fn emit(&mut self, rec: Json) {
        (**self).emit(rec)
    }
}

/// Which construct presents a [`Menu`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKind {
    Branch,
    Hub,
}

/// One presentation of a `choice` (`<branch>`) or `hub` record.
#[derive(Debug)]
pub struct Menu<'a> {
    pub construct: MenuKind,
    /// The branch id (`branchId`) or hub id.
    pub id: &'a str,
    /// The presenting record's `addr`.
    pub addr: &'a str,
    /// A hub: how many presentations this visit of the hub made before this
    /// one (`0` first). A branch: always `0` — a branch presented again is a
    /// new visit, and its scripted cursor lives in the driver.
    pub presentation: usize,
    pub options: &'a [MenuOption],
}

/// One option of a [`Menu`], judged at this presentation point.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuOption {
    pub id: String,
    pub verdict: Verdict,
    pub exit: bool,
    pub once: bool,
}

/// An option's eligibility at a presentation point.
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Open,
    /// Its guard decided false.
    Closed,
    /// Its guard is undecided; the atoms say why.
    Unknown(Vec<UnresolvedAtom>),
    /// A hub `once` option already taken in this visit of the hub.
    Spent,
}

/// The driver's answer to a [`Menu`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Take this option id (a scripted decision).
    Option(String),
    /// Take the first [`Verdict::Open`] option; none → the walk is incomplete.
    AutoFirst,
    /// A hub without a script: one document-order pass over the open
    /// non-`exit` options (each judged fresh at its turn), then the first
    /// open `exit`; none → the walk is incomplete.
    HubAutoPass,
    /// No decision. A branch halts incomplete; a hub converges when nothing
    /// is open and halts incomplete otherwise. `scripted` is how many
    /// decisions a scripted list held when it ran out (`0`: no list).
    Unscripted { scripted: usize },
}

/// The driver's ruling on a picked option that is not [`Verdict::Open`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Forced {
    /// Take it anyway.
    Take,
    /// Drop the decision: a hub moves to its next presentation (the decision
    /// is consumed); a branch halts incomplete.
    Skip,
    /// Refuse it (`E-TRACE-CHOICE`): the walk halts, `refused`.
    Refuse,
}

/// A plugin call whose effects read bridge results.
#[derive(Debug)]
pub struct BridgeCall<'a> {
    /// The plugin directive tag.
    pub tag: &'a str,
    pub addr: &'a str,
    /// `(bridgeResult field, state path it lands on)`, in effect order.
    pub reads: &'a [(String, String)],
}

/// The driver's answer to a [`BridgeCall`].
#[derive(Clone, Debug, PartialEq)]
pub enum BridgeReply {
    Answer(BridgeAnswer),
    Unanswered,
}

/// A place the walk met a value it cannot decide.
#[derive(Debug)]
pub struct UnknownSite<'a> {
    pub kind: SiteKind,
    /// The construct's id (a branch/hub/quest/objective id, a plugin tag…).
    pub id: &'a str,
    pub addr: &'a str,
    /// The undecided CEL text (empty when the site has none).
    pub raw: &'a str,
    pub atoms: &'a [UnresolvedAtom],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SiteKind {
    Arm,
    BranchAllUnknown,
    HubAllUnknown,
    Guard,
    SetValue,
    QuestStart,
    QuestFail,
    ObjectiveDone,
    ObjectiveBy,
    ObjectiveUntil,
    Handler,
    Reward,
    /// A plugin call without an answer whose result content reads.
    BridgeResult,
    OccasionTarget,
    EntryWhen,
    BeatWhen,
}

/// Whether the walk halts (incomplete) at an [`UnknownSite`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnUnknown {
    Halt,
    Continue,
}

/// Scripted `choose:` decisions (a mock's, a play step's), shared by every
/// driver that replays a script.
#[derive(Clone, Debug, Default)]
pub struct ScriptedChoices {
    /// branch/hub id → the ordered decisions.
    pub choose: BTreeMap<String, Vec<String>>,
    /// Per `<branch>` id: how many decisions of a multi-decision list
    /// earlier presentations consumed.
    pub cursor: BTreeMap<String, usize>,
}

impl ScriptedChoices {
    pub fn new(choose: BTreeMap<String, Vec<String>>, cursor: BTreeMap<String, usize>) -> Self {
        ScriptedChoices { choose, cursor }
    }

    /// The scripted decision for `menu`. A branch: a single decision answers
    /// every presentation; a list of two or more is consumed one decision
    /// per presentation, in order (the cursor), never silently truncated to
    /// its head. A hub: the list is its visit sequence — presentation `n`
    /// of one visit takes decision `n`.
    pub fn pick(&mut self, menu: &Menu<'_>) -> Pick {
        let list = self.choose.get(menu.id).map(Vec::as_slice).unwrap_or(&[]);
        match menu.construct {
            MenuKind::Hub => match list.get(menu.presentation) {
                Some(id) => Pick::Option(id.clone()),
                None => Pick::Unscripted { scripted: 0 },
            },
            MenuKind::Branch => match list {
                [] => Pick::Unscripted { scripted: 0 },
                [only] => Pick::Option(only.clone()),
                list => {
                    let used = self.cursor.entry(menu.id.to_string()).or_insert(0);
                    match list.get(*used) {
                        Some(id) => {
                            *used += 1;
                            Pick::Option(id.clone())
                        }
                        None => Pick::Unscripted {
                            scripted: list.len(),
                        },
                    }
                }
            },
        }
    }
}

/// dsl 0.24.0 §5: the bridge answers a walk consumes — per plugin directive
/// tag, one answer per call, in call order. `step` (a `lute play` step's own
/// `bridges:`) is consumed before `top` (the play's top-level `bridges:`, or
/// a `lute run --mock`'s).
#[derive(Clone, Debug, Default)]
pub struct BridgeQueues {
    pub step: BTreeMap<String, VecDeque<BridgeAnswer>>,
    pub top: BTreeMap<String, VecDeque<BridgeAnswer>>,
}

impl BridgeQueues {
    /// A `bridges:` surface as a queue.
    pub fn queue(
        answers: &BTreeMap<String, Vec<BridgeAnswer>>,
    ) -> BTreeMap<String, VecDeque<BridgeAnswer>> {
        answers
            .iter()
            .filter(|(_, list)| !list.is_empty())
            .map(|(tag, list)| (tag.clone(), list.iter().cloned().collect()))
            .collect()
    }

    /// The next answer for a call of `tag`: the step's first, then the top
    /// level's.
    pub fn next(&mut self, tag: &str) -> Option<BridgeAnswer> {
        for tier in [&mut self.step, &mut self.top] {
            if let Some(q) = tier.get_mut(tag) {
                if let Some(a) = q.pop_front() {
                    if q.is_empty() {
                        tier.remove(tag);
                    }
                    return Some(a);
                }
            }
        }
        None
    }
}
