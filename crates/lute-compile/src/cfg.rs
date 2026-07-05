//! Symbolic-label machinery for branch/match flattening (§7). A [`Label`] is
//! a compiler-internal temporary: flattening writes `"@<n>"` into target
//! fields, [`Emitter::bind`] parks a label on the NEXT pushed record, and the
//! addressing pass (Task 11) rewrites every `"@<n>"` to a concrete `addr` —
//! labels are never serialized.

use crate::ir::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Label(pub u32);

impl Label {
    /// Symbolic target text: `"@<n>"` — cannot collide with a real addr
    /// (`"{shot:03}-{idx:04}"`).
    pub fn sym(self) -> String {
        format!("@{}", self.0)
    }

    /// Parse a symbolic target back to its label number.
    pub fn parse_sym(s: &str) -> Option<u32> {
        s.strip_prefix('@').and_then(|n| n.parse().ok())
    }
}

/// One emitted record plus the labels bound AT it (its future `addr` is the
/// labels' resolution).
#[derive(Clone, Debug)]
pub struct Rec {
    pub labels: Vec<Label>,
    pub cmd: Command,
}

/// Per-shot record emitter (labels never cross shots).
#[derive(Default)]
pub struct Emitter {
    pub recs: Vec<Rec>,
    pending: Vec<Label>,
    next: u32,
}

impl Emitter {
    pub fn fresh(&mut self) -> Label {
        let l = Label(self.next);
        self.next += 1;
        l
    }

    /// Park `l` to bind on the next pushed record (or trail past the end).
    pub fn bind(&mut self, l: Label) {
        self.pending.push(l);
    }

    pub fn push(&mut self, cmd: Command) {
        let labels = std::mem::take(&mut self.pending);
        self.recs.push(Rec { labels, cmd });
    }

    /// The records plus any labels still pending past the last record (an
    /// end-of-shot convergence, plan spec-gap note 2).
    pub fn finish(self) -> (Vec<Rec>, Vec<Label>) {
        (self.recs, self.pending)
    }
}
