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
    /// (digits and `-` only, whatever width `address::addr_of` picked).
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
    /// dsl 0.12.0: NAMED labels bound at this record — `::mark{id}` / a
    /// content line's `id=` — mirrors `labels` but keyed by the author's
    /// own string rather than a compiler-fresh numeric [`Label`]. Resolved
    /// DOCUMENT-WIDE (not per-shot like `labels`) by
    /// `address::assign_addresses`'s named-label pass, since a `::next` may
    /// target a label in a LATER shot — see that function's doc comment.
    pub named: Vec<String>,
    pub cmd: Command,
}

/// Per-shot record emitter. Anonymous numeric [`Label`]s never cross shots
/// (converge targets are always local); NAMED labels (`Rec::named`) may —
/// `address::assign_addresses` resolves those against a document-wide table
/// built BEFORE any shot is consumed.
#[derive(Default)]
pub struct Emitter {
    pub recs: Vec<Rec>,
    pending: Vec<Label>,
    pending_named: Vec<String>,
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

    /// Park a NAMED label (dsl 0.12.0: `::mark{id}` / a line's `id=`) to
    /// bind on the next pushed record (or trail past the end) — mirrors
    /// [`Self::bind`] for the document-wide named-label table.
    pub fn bind_named(&mut self, id: String) {
        self.pending_named.push(id);
    }

    pub fn push(&mut self, cmd: Command) {
        let labels = std::mem::take(&mut self.pending);
        let named = std::mem::take(&mut self.pending_named);
        self.recs.push(Rec { labels, named, cmd });
    }

    /// The records plus any labels still pending past the last record (an
    /// end-of-shot convergence, plan spec-gap note 2) — anonymous, then
    /// named (dsl 0.12.0).
    pub fn finish(self) -> (Vec<Rec>, Vec<Label>, Vec<String>) {
        (self.recs, self.pending, self.pending_named)
    }
}
