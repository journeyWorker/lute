//! `check()` assembly + the LSP-facing `Resolved` view (Task 4.9).
//!
//! This is the single validation core the CLI (Phase 5) and LSP (Phase 6) both
//! wrap — "`check()` is the contract, not the LSP protocol". It wires the whole
//! pipeline together and owns NO validation logic of its own: every diagnostic
//! comes from a Phase-3/Phase-4 validator that already has its own tests.
//!
//! ```text
//! parse (syntax)                      -> Document + parse diags
//!   -> fill_document (cel)            -> CEL asts in arena + E-CEL-PARSE diags
//!   -> parse_meta                     -> StateSchema + defs + meta diags
//!   -> fold <branch>/<hub> decls      -> folded schema (scene.choices.* + scene.visited.*) + E-DUP-BRANCH
//!   -> per-node walk                  -> directive/cel-slot/set/match/timeline diags
//!   -> document-level defassign       -> E-UNDECLARED / E-MAYBE-UNSET (whole stream)
//!   -> injection fold (lower_node)    -> InjectedCommand[] + E-DOMAIN-UNKNOWN
//!   -> suppress / dedup / normalize / sort
//! ```
//!
//! ## The five binding carry-forwards (see the T4.9 brief)
//! 1. **Document-level definite-assignment.** `scene.*`/`run.*` persist across
//!    shots within the episode (dsl §9.1), so `check_definite_assignment` runs
//!    ONCE over the whole-document concatenated node stream (all shots' bodies in
//!    source order), never per-shot — a path written in shot 1 and read in shot 2
//!    must not read as maybe-unset.
//! 2. **`Ctx` construction.** `state` = the [`StateSchema`] from `parse_meta`
//!    (folded with the implicit `scene.choices.*` / `scene.visited.*` decls from
//!    every `<branch>` and `<hub>`);
//!    `defs` = declared `defs:` names; `mode` from the input; `in_match` /
//!    `match_subject` set as the walk enters a `<match>` arm (for T4.3's `$`).
//! 3. **Determinism.** All diagnostics are sorted by `span.byte_start` then
//!    `code` before returning — the ordering the Phase-6 divergence golden
//!    (headless vs LSP) compares byte-for-byte. Every span's line/column/utf16 is
//!    re-derived from its bytes through one [`TextIndex`] so the two surfaces
//!    agree. A CEL parse failure is reported ONCE here (as `E-CEL-PARSE`,
//!    [`Layer::Cel`]) from `fill_document`'s errors and never aborts the walk.
//! 4. **`E-UNDECLARED` dedup.** A `::set` to an undeclared target is flagged by
//!    BOTH `check_set` ([`Layer::Staging`], the precise `path_span`) and
//!    `check_definite_assignment` ([`Layer::Logic`]). We collapse overlapping
//!    `E-UNDECLARED` spans to the narrowest (most precise) one — BEFORE the sort.
//! 5. **`E-REF-TYPE`.** Still deferred: the check needs per-def type info threaded
//!    into `Ctx` AND an expected-type per CEL slot (neither exists yet in T4.3's
//!    `check_cel_slot`), so wiring it here would touch T4.3. `defs` names ARE
//!    threaded (so `@ref` existence resolves); the type-context match is future.
//!
//! Plus: `is_exhaustive` (T4.6) suppresses T4.4's known false positive — a
//! maybe-unset `<match>` SUBJECT read on a domain-exhaustive match (the arms'
//! join covers every case, so the subject read cannot escape unhandled).
//!
//! ## `Resolved` view (Some-vs-None policy)
//! The resolved view is best-effort and computed unconditionally (parse, fill,
//! `resolve_timeline`, and the injection fold never panic). `resolved` is `Some`
//! unless a STRUCTURAL parse error corrupts the node stream itself
//! (`E-UNCLASSIFIED` / `E-UNCLOSED-TAG` / `E-COMMENT-UNTERMINATED` /
//! `E-META-PARSE`) — then the view would be misleading, so it is `None`. Semantic
//! errors (unknown directive, undeclared state, non-exhaustive match, …) still
//! yield a resolved view. A clean document is always `Some`.

use std::path::PathBuf;

use lute_cel::{fill_document, parse_slot, scan_refs, CelArena, CelParseError};
use lute_core_span::{Diagnostic, Fixit, Layer, Severity, Span, TextEdit, TextIndex};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::{SlotDecl, StateShape};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::{type_accepts, Literal, Type};
use lute_syntax::ast::{
    Arm, Attr, AttrValue, CelKind, CelSlot, Choice, ClipNode, Directive, Document, Interp,
    InterpKind, Node,
};
use lute_syntax::parse;
/// Delegate: the label-interp scanner now lives in `lute-syntax` (single source
/// of truth with the parser's content-line scan). Re-exported `pub(crate)` so
/// `crate::check::scan_label_interps` (used by `defassign`) and the local call
/// sites keep resolving without change.
pub(crate) use lute_syntax::scan_label_interps;
use lute_syntax::walk::for_each_cel_slot;

use crate::cel_expand::DefTable;
use crate::cel_message::translate_cel_parse;
use crate::cel_resolve::{check_rule_guards, compatible};
use crate::component_import::ComponentSet;
use crate::ctx::{Ctx, Env, ExpectedType, Mode};
use crate::decide::DecideCtx;
use crate::directives::{at_context, check_directive};
use crate::inject::{lower_node, InjectedCommand, StageState};
use crate::match_check::{check_param_match, param_domain};
use crate::reachability::check_reachability;
use crate::schema_import::{merge_domains, SchemaImports};
use crate::set_op::resolve_type;
use crate::timeline::{resolve_timeline, ResolvedTimeline};
use crate::{
    check_branch, check_cel_slot, check_definite_assignment, check_hub, check_line_codes,
    check_quest, check_quest_guard_defassign, check_quest_rewards, check_set, DomainInfo,
};

mod choice_record;
mod component_body;
mod fold;
mod guard;
mod injections;
mod interp;
mod literal_text;
mod pipeline;
mod postprocess;
mod use_site;
mod version;
mod walker;

use choice_record::{check_choice_record, into_literal};
pub(crate) use component_body::{bare_param_ref, directive_writes_state};
use component_body::{
    component_def_reads, component_own_slots, component_use_sites, use_target, validate_components,
    walk_component_body, BodyScope,
};
pub use fold::fold_env;
use fold::{attr_str, fold_directive_slots, params_from_yaml};
pub(crate) use guard::directive_when_refused;
use guard::{check_beat_when, check_directive_when, check_guard};
use injections::fold_injections;
pub use interp::W_TEXT_LOOKS_LIKE_REF;
use interp::{
    check_interp_format, check_interp_referent, check_interps, interp_grammar_diag, is_bare_ref,
    is_renderable, text_looks_like_ref,
};
use pipeline::cel_parse_diagnostics;
pub use pipeline::{check, check_parsed};
use postprocess::{
    collapse_same_root, dedup_rehomed, dedup_undeclared, first_backtick_token, normalize_spans,
    suppress_dead_arm_overlaps, suppress_exhaustive_subject_reads,
    suppress_unparsed_child_list_verdicts, suppress_unproven_absence,
};
pub use postprocess::{diagnostic_order, order_diagnostics};
pub(crate) use use_site::literal_arg_ok;
pub use use_site::use_speaker_lines;
use use_site::{
    check_param_literal_defaults, check_speaker_args, check_use, check_use_def_enum_args,
    check_use_interp_args, check_use_speaker_args, check_use_typed_args, collect_use_directives,
    dedup_guarded_use_reports, use_diag, E_COMPONENT_ARG, E_COMPONENT_BODY, E_COMPONENT_CYCLE,
    E_COMPONENT_STATE,
};
use version::check_lute_version_stale;
pub use version::{INHERITED_LUTE_VERSION, W_LUTE_VERSION_STALE};
use walker::Walker;

/// The input to one `check()` invocation — the document text plus the resolved
/// capability surface it is validated against.
pub struct CheckInput {
    /// Raw `.lute` document source.
    pub text: String,
    /// Document identity (LSP uri / CLI path); carried through for diagnostics.
    pub uri: String,
    /// The resolved capability snapshot (directives, enums, defs, frontmatter).
    pub snapshot: CapabilitySnapshot,
    /// Pinned provider snapshots for `providerRef` id resolution (plugin §10).
    pub providers: ProviderSet,
    /// Author (interactive LSP) vs. Ci (batch) analysis mode.
    pub mode: Mode,
    /// Resolved `uses:` schema imports (dsl §9.2): imported state/defs merged
    /// into this document's schema, plus resolution diagnostics. Empty when the
    /// scene has no `uses:` (or on a surface that cannot resolve files).
    pub imports: SchemaImports,
    /// Resolved `components:` imports (dsl §13): the component name -> definition
    /// table (params + presentational body) plus resolution diagnostics. Empty
    /// when the scene has no `components:` (or on a surface that cannot resolve
    /// files); validated against `::use` invocations in [`check`].
    pub components: ComponentSet,
    /// The governing manifest's `defaults:` (0.10.0 §6): frontmatter this
    /// document did not have to retype. Empty for a loose document, for a
    /// project with no `defaults:` block, and on any surface with no manifest.
    pub defaults: lute_manifest::project::MetaDefaults,
}

/// Project-wide domain-read accounting (dsl 0.10.0 §11.1, **D-V**).
///
/// `check()` computes both halves per document, from ITS resolved snapshot and
/// ITS merged vocabulary; `check-project` unions them across the root and
/// reports the difference. Split this way because the question is only
/// answerable project-wide: a domain declared in a shared schema is read by
/// SOME document, so a single-document verdict would be a false positive on the
/// most common layout in the language.
///
/// The field carrying this is `#[serde(skip)]`: it is analysis input for the
/// project pass, not part of the `check --json` contract, and adding it must
/// not move a golden.
#[derive(Clone, Debug)]
pub struct DomainUse {
    /// Every domain name this document resolves — inline `enums:`, imported
    /// schema, plugin, and the snapshot baseline.
    pub declared: std::collections::BTreeSet<String>,
    /// Every domain name some active construct in the resolved snapshot reads.
    pub read: std::collections::BTreeSet<String>,
    /// This document's frontmatter span, where a project-wide domain diagnostic
    /// anchors when this is the first document to declare an unread domain
    /// and [`Self::homes`] does not place it.
    pub at: Span,
    /// Where each project-declared domain this document resolves is written:
    /// the imported schema's line (dsl 0.24 T3-6), or this document's own
    /// `enums:` / `entities:` key.
    pub homes: std::collections::BTreeMap<String, DomainHome>,
}

/// The declaration site of a domain (see [`DomainUse::homes`]).
#[derive(Clone, Debug)]
pub enum DomainHome {
    /// Declared in an imported schema file, at this line.
    Imported(crate::rel_schema::DeclOrigin),
    /// Declared in the document's own frontmatter, at this key.
    Local(Span),
}

/// Hand-written rather than derived: [`Span`] carries no `Default`, and adding
/// one to a foundation type for this field's benefit would be a wider change
/// than the field is worth.
impl Default for DomainUse {
    fn default() -> Self {
        Self {
            declared: std::collections::BTreeSet::new(),
            read: std::collections::BTreeSet::new(),
            at: Span {
                byte_start: 0,
                byte_end: 0,
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
            homes: std::collections::BTreeMap::new(),
        }
    }
}

/// The result of one `check()`: every diagnostic (deduped, byte-sorted) plus the
/// best-effort resolved view when the document is structurally intact.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CheckResult {
    /// `true` when no `Error`-severity diagnostic is present (drives the CLI exit
    /// code and the LSP "problems" gutter).
    pub ok: bool,
    /// All diagnostics, deduped and sorted by `(span.byte_start, code)`.
    pub diagnostics: Vec<Diagnostic>,
    /// The resolved view; `None` when a structural parse error corrupts the tree.
    pub resolved: Option<Resolved>,
    /// dsl 0.10.0 §11.1: this document's half of the project-wide domain-read
    /// question. Not serialized — see [`DomainUse`].
    #[serde(skip)]
    pub domain_use: DomainUse,
}

/// The LSP-facing resolved view (arch "resolved view"): the compiler's
/// best-effort read of what the document lowers to, WITHOUT final flat-record
/// codegen (scoped out of this plan).
#[derive(Clone, Debug, serde::Serialize)]
pub struct Resolved {
    /// A shallow, depth-1 textual preview of the authored top-level command
    /// stream in document order — one entry per top-level node (nested arm /
    /// choice / clip bodies are summarized by their opener, not expanded). Full
    /// desugaring is a Phase-5+ concern; this is a human-readable outline.
    pub commands_preview: Vec<String>,
    /// One resolved table per `<timeline>` in document order (dsl §11.4).
    pub timeline_tables: Vec<ResolvedTimeline>,
    /// Every command the injection reducer inserted, with provenance, folded over
    /// the document node stream (arch stateful resolution / auto-injection).
    pub injections: Vec<InjectedCommand>,
}

/// The folded compile inputs `check()` builds internally (compile-spec §11
/// reuse-input exposure): the typed frontmatter, the analysis [`Env`] whose
/// `state` is the FOLDED schema (imported ∪ inline ∪ implicit
/// `scene.choices.*` ∪ plugin-declared slots), and the merged def CEL bodies
/// (`plugin < imported < inline`, mirroring `def_types`). One source of
/// truth: `check()` itself consumes this fold.
#[derive(Clone, Debug)]
pub struct FoldedEnv {
    pub typed: crate::meta::TypedMeta,
    pub env: Env,
    /// def name -> raw CEL body, merged plugin < imported < inline (D4 input).
    pub def_bodies: std::collections::BTreeMap<String, String>,
    /// The resolved root document kind (dsl 0.2.0 §3.1): `Scene` when
    /// unresolved (missing/unknown `kind:`) so scene stays the degrade-safe
    /// path and downstream dispatch never panics.
    pub doc_kind: crate::meta::DocKind,
    /// The FULL merged domain vocabulary (data-catalog foundation A4):
    /// `snapshot.domains` UNION project-authored schema-import domains (A3's
    /// `merge_domains`) — computed ONCE here (0.3.0 T7 moved this from
    /// `check()`) so a caller that folds directly, like `lute-compile`, sees
    /// the SAME vocabulary without recomputing it or double-emitting
    /// `E-DOMAIN-DUP`.
    pub domains: std::collections::BTreeMap<String, Domain>,
    /// dsl 0.21.0 §2: the resolved occasion vocabulary this document was
    /// checked against (the snapshot's `occasions`; empty = shape-only) —
    /// what the project beat pass (`crate::beats::check_project_beats`) reads
    /// each occasion's `select` from.
    pub occasions: std::collections::BTreeMap<String, lute_manifest::schema::OccasionDecl>,
    /// The cast this document is checked against (dsl 0.23.0 §7,
    /// [`crate::cast::declared_cast`]) — what the project presence pass
    /// (`crate::cast::reconcile_presence`, dsl 0.24.0 §4) reads `present:`
    /// from.
    pub cast: std::collections::BTreeMap<String, lute_manifest::schema::CastMember>,
    /// dsl 0.26.0 §3.2: per `::use` of this document (keyed by its span
    /// start), the `@@p:` lines it speaks, bound to the members its
    /// arguments name ([`crate::component_effects::use_speaker_lines`]) —
    /// what the emotion and presence passes judge at that `::use`.
    pub use_lines: std::collections::BTreeMap<usize, Vec<lute_syntax::ast::Line>>,
    /// When the document's kind and `for=` beats answer different members:
    /// one environment per member list, `occasion.target` typed by that list
    /// alone ([`Self::env_at`]). Empty otherwise — [`Self::env`] is exact.
    pub member_envs: Vec<(Vec<String>, Env)>,
}

impl FoldedEnv {
    /// The environment a slot written at `span` is checked in: in a kind or
    /// `for=` beat, `occasion.target` is typed by that beat's own members,
    /// so a `<match>` over it needs only those and a literal from another
    /// kind is outside its domain; everywhere else, [`Self::env`].
    pub fn env_at(&self, span: lute_core_span::Span) -> &Env {
        if self.member_envs.is_empty() {
            return &self.env;
        }
        self.env
            .occasion_scopes
            .members_at(span.byte_start, span.byte_end)
            .and_then(|ms| self.member_envs.iter().find(|(list, _)| list == ms))
            .map_or(&self.env, |(_, env)| env)
    }
}
