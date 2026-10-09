//! The wire types of Lute's compiled output that a writer and a reader share:
//! the `project.index.json` envelope and its rows, the IR row types they
//! embed, and the portable expression AST (`expr`). `lute-compile` builds
//! them; `lute-runtime` reads them. Each type's serialization is the byte
//! contract (field order, renames, omissions); its deserialization reads the
//! same shape back.
//!
//! Compiler logic (lowering, index assembly, checker-backed resolution) stays
//! in `lute-compile`; this crate depends only on `serde`, `serde_json` and
//! `lute-manifest`.

mod expr;
mod index;
mod ir;

pub use expr::{ExprNode, LitVal};
pub use index::{
    BeatKind, IndexBeat, IndexDocument, IndexEntry, IndexOccasion, ProjectIndex, StateDomain,
};
pub use ir::{
    AdvanceSpec, AtomEntry, BeatOnce, BodyEntry, CelPair, DocKind, EntityKindEntry, EnumEntry,
    ForKind, GateEntry, LabelForms, PrereqEdge, PrereqEdgeEntry, RelationEntry, RuleEntry,
    SeasonEntry, SeedFactEntry, TargetKind, TermEntry,
};
