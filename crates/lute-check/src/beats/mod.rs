//! dsl 0.21.0 beats and occasions: authored beat attributes, target domains,
//! project collection, selection validation, and diagnostics.

mod collection;
mod diagnostics;
mod helpers;
mod ordering;
mod project;
mod validation;

use std::collections::BTreeMap;
use std::path::PathBuf;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::ident::is_name;
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use lute_syntax::ast::{AttrValue, CelKind, CelSlot, Document, Entry, Meta, Node, Quest};

use crate::cel_expand::DefTable;
use crate::check::FoldedEnv;
use crate::decide::{decide_slot, DecideCtx, Decided};
use crate::fact_env::{FactEnv, FactScope};
use crate::lore::{is_beat_target, is_entry_target, kind_target};

use collection::*;
use diagnostics::*;
use helpers::*;
use ordering::*;

pub use collection::{
    advances_from_attr, parse_beat_priority, AdvanceSpec, BeatMeta, BeatOnce, BEAT_KEYS,
    E_BEAT_ATTR, E_BEAT_UNREACHABLE, E_OCCASION_UNKNOWN, ENTRY_ONCE_VALUES, ONCE_VALUES,
    W_BEAT_SHADOWED,
};
pub use diagnostics::{always_eligible, presence_ladder, shadowers_at};
pub use helpers::{coverers, SPENDING_ONCE};
pub use ordering::{
    check_project_beats, ReportedErrors, W_BEAT_ONCE_RUN_USER, W_BEAT_PRIORITY_TIE,
};
pub use project::{
    in_selection_order, project_beats, reorder, selection_order, BeatCells, ProjectBeat,
    ProjectBeatKind,
};
pub use validation::{
    beat_target_restricts, kind_target_members, occasion_target_members, occasion_target_ok,
    OCCASION_TARGET,
};
pub(crate) use collection::{
    check_entry_beat_attrs, check_entry_occasions, lift_scene_beat,
};
pub(crate) use diagnostics::{read_tiers, ReadTier, UserTier, top_key_span};
pub(crate) use helpers::{
    malformed_target, nested_value_span, occasion_malformed, share_malformed, share_with_spent_by,
    share_without_once, spent_by_once_false, top_value_span, top_value_text,
};
pub(crate) use validation::{beat_unreachable_message, scene_beat_name};
pub(crate) use validation::{
    also_fault, check_beat_target_domains, check_objective_occasions, check_occasion,
    check_occasion_target_scope, occasion_target_scope_message, scene_when_scene_reads,
};
