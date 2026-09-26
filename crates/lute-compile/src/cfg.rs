//! Symbolic-label machinery for branch/match flattening (§7). A [`Label`] is
//! a compiler-internal temporary: flattening writes `"@<n>"` into target
//! fields, [`Emitter::bind`] parks a label on the NEXT pushed record, and the
//! addressing pass (Task 11) rewrites every `"@<n>"` to a concrete `addr` —
//! labels are never serialized.

use lute_core_span::Span;

use crate::ir::Command;
use crate::source_map::{SourceInfo, SourceMarker};

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
    /// Where the record came from — `Some` only when the emitter maps
    /// ([`Emitter::mapped`], `compile_mapped`); never serialized.
    pub origin: Option<SourceInfo>,
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
    /// `Some` when this emitter records a [`SourceInfo`] per record.
    map: Option<MapState>,
}

/// The source-map bookkeeping of a mapping [`Emitter`].
#[derive(Default)]
struct MapState {
    /// Source-only steps waiting for the next record.
    markers: Vec<SourceMarker>,
    /// The `into=` attrs (`path`, span) of the choices being walked: the
    /// `::set` normalize synthesized from one carries that exact span.
    into: Vec<(String, Span)>,
}

impl Emitter {
    /// An emitter that, when `mapped`, records where every record came from
    /// (`compile_mapped`); otherwise [`Emitter::default`].
    pub fn new(mapped: bool) -> Self {
        Emitter {
            map: mapped.then(MapState::default),
            ..Emitter::default()
        }
    }

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

    /// Push `cmd`. `origin` runs only on a mapping emitter.
    pub fn push(&mut self, cmd: Command, origin: impl FnOnce() -> SourceInfo) {
        let labels = std::mem::take(&mut self.pending);
        let named = std::mem::take(&mut self.pending_named);
        let origin = self.map.as_mut().map(|m| {
            let mut info = origin();
            info.before = std::mem::take(&mut m.markers);
            info
        });
        self.recs.push(Rec {
            labels,
            named,
            cmd,
            origin,
        });
    }

    /// Record a source-only step, attached to the next pushed record (or
    /// trailing past the end). `marker` runs only on a mapping emitter.
    pub fn marker(&mut self, marker: impl FnOnce() -> SourceMarker) {
        if let Some(m) = self.map.as_mut() {
            m.markers.push(marker());
        }
    }

    /// Enter / leave a `<choice>` body whose `into=` synthesized a `::set`.
    pub fn enter_into(&mut self, into: Option<(String, Span)>) {
        if let (Some(m), Some(into)) = (self.map.as_mut(), into) {
            m.into.push(into);
        }
    }

    pub fn leave_into(&mut self, into: Option<&(String, Span)>) {
        if let (Some(m), Some(_)) = (self.map.as_mut(), into) {
            m.into.pop();
        }
    }

    /// Whether the `::set` of `path` at `span` is a choice's `into=` sugar.
    pub fn is_into_sugar(&self, path: &str, span: Span) -> bool {
        self.map
            .as_ref()
            .is_some_and(|m| m.into.iter().any(|(p, s)| p == path && *s == span))
    }

    /// The records plus any labels still pending past the last record (an
    /// end-of-shot convergence, plan spec-gap note 2) — anonymous, then
    /// named (dsl 0.12.0) — and the source-only steps after the last record.
    pub fn finish(self) -> Finished {
        Finished {
            recs: self.recs,
            trailing: self.pending,
            trailing_named: self.pending_named,
            trailing_markers: self.map.map(|m| m.markers).unwrap_or_default(),
        }
    }
}

/// What an [`Emitter`] leaves for the addressing pass.
pub struct Finished {
    pub recs: Vec<Rec>,
    pub trailing: Vec<Label>,
    pub trailing_named: Vec<String>,
    pub trailing_markers: Vec<SourceMarker>,
}
