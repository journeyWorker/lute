pub mod accept;
pub mod admission;
pub mod beats;
pub mod bundles;
pub mod cast;
pub mod cel_expand;
pub mod cel_message;
pub mod cel_paths;
pub mod cel_resolve;
pub mod check;
pub mod builtin_lowering;
pub mod clock;
pub mod component_effects;
pub mod component_import;
pub mod connectivity;
pub mod content_line;
pub mod ctx;
pub mod datalog_check;
pub mod decide;
pub mod def_decl;
pub mod def_inline;
pub mod defassign;
pub mod directives;
pub mod envelope;
pub mod fact_check;
pub mod fact_env;
pub mod fact_must;
pub mod fact_write;
pub mod fix;
pub mod inject;
pub mod logic_attrs;
pub mod lore;
pub mod match_check;
pub mod meta;
pub mod next_labels;
pub mod on;
pub mod permissions;
pub mod prereq;
pub mod producible;
pub mod project_check;
pub mod reachability;
pub mod rel_schema;
pub mod rule_index;
pub mod schema_import;
pub mod set_op;
pub mod set_type;
pub(crate) mod solution;
pub mod tag;
pub mod temporal;
pub mod time;
pub mod timeline;
pub mod usage;
pub mod when_test_literal;

/// The canonical Lute language-version string (dsl 0.6.1 §3, Appendix B).
/// `lute-compile` stamps it into the artifact envelope's `lute` field (and
/// re-exports it as `LUTE_LANG_VERSION`); `check()` compares it against a
/// document's frontmatter `luteVersion` stamp for the `W-LUTE-VERSION-STALE`
/// freshness signal (spec §3). Defined HERE, not in `lute-compile`, so the
/// checker can read it WITHOUT depending on the compiler — the crate
/// dependency runs the other way (`lute-compile` → `lute-check`).
pub const LUTE_LANG_VERSION: &str = "0.23.1";

pub use accept::{
    check_accept_directive, check_project_accepts, check_project_never_accepted,
    E_ACCEPT_TARGET, W_QUEST_NEVER_ACCEPTED,
};
pub use admission::{check_admission, node_kind, NodeKind};
pub use bundles::{
    bundle_beat_also, bundle_beat_key, bundle_beat_once, bundle_beat_priority, check_bundle_beats,
    BUNDLE_BEAT_ATTRS,
};
pub use beats::{
    beat_target_restricts, check_project_beats, occasion_target_ok, parse_beat_priority,
    project_beats, BeatMeta,
    BeatOnce, ProjectBeat, ProjectBeatKind, BEAT_KEYS,
    E_BEAT_ATTR, E_BEAT_UNREACHABLE, E_OCCASION_UNKNOWN, W_BEAT_ONCE_RUN_USER,
    W_BEAT_PRIORITY_TIE, W_BEAT_SHADOWED,
};
pub use cast::{check_speakers, declared_cast, E_CAST_UNKNOWN};
pub use cel_expand::{expand_cel, DefTable};
pub use cel_message::{translate_cel_parse, Translation};
pub use cel_paths::{is_entry_ever_read, is_reserved_entry_read, reserved_entry_id, E_PATH_IDENT};
pub use cel_resolve::{
    check_cel_slot, check_rule_guards, visited_call_target, visited_targets, E_CEL_PROFILE,
    E_DATALOG_GUARD_FACT, E_MATCH_RELATION_SUBJECT, E_VALIDAT_DERIVED, VISITED_FN,
};
pub use check::{
    check, fold_env, CheckInput, CheckResult, DomainUse, FoldedEnv, Resolved, W_LUTE_VERSION_STALE,
};
pub use component_effects::{splice_component_effects, speaker_display_args};
pub use component_import::{resolve_components, ComponentDef, ComponentSet};
pub use ctx::{Ctx, Mode};
pub use datalog_check::{
    check_rules, check_stratification, E_DATALOG_UNSAFE, E_DATALOG_UNSTRATIFIED,
    E_DERIVE_UNDECLARED, W_DERIVE_NO_RULES,
};
pub use decide::{apply_op, decide, decide_slot, DecideCtx, Decided, DollarBinding};
pub use def_decl::E_DEF_DECL;
pub use def_inline::{
    attr_def_dynamic_diag, choice_attr_owned_elsewhere, decided_literal, fold_attr_ref,
    inline_interp_ref, interp_def_diag, E_ATTR_DEF_DYNAMIC, E_INTERP_DEF,
};
pub use defassign::{check_definite_assignment, check_quest_guard_defassign};
pub use directives::E_AT_CONTEXT;
pub use fact_write::{check_assert, check_retract, E_DERIVED_WRITE, E_FACT_TIER_WRITE};
pub use fix::{fix_document, FixResult};
pub use inject::{
    is_declared_exit, lower_node, Departure, InjectKind, InjectedCommand, Provenance, SpriteState,
    StageState,
};
pub use lore::{
    check_entries, document_series, entry_read_decl, entry_read_path, is_entry_ident,
    is_entry_target, parse_entry_order, resolve_entry_series, EntryRecord, EntrySeries,
    E_ENTRY_ATTR, E_ENTRY_ID_DUP, E_ENTRY_SERIES_ORDER, W_ENTRY_REF_UNKNOWN,
};
pub use match_check::{
    check_branch, check_hub, check_line_codes, check_match, check_quest, check_quest_rewards,
    is_exhaustive, is_pattern_literals, BranchRecord, DomainInfo, HubRecord, QuestRecord,
    E_REWARD_ATTR, E_REWARD_KIND, E_WHEN_RANGE,
};
pub use meta::{
    parse_meta, parse_meta_kind, resolve_doc_kind, DocKind, MetaKind, Namespace, StateDecl,
    StateSchema, TypedMeta, E_KIND_MISSING, E_STATE_COLLECTION, E_UNKNOWN_KIND,
};
pub use on::{check_on_event, E_ON_NO_EVENT, E_UNKNOWN_EVENT};
pub use permissions::{
    check_permissions, E_PERMISSION_BRIDGE, E_PERMISSION_DIRECTIVE, E_PERMISSION_FACT,
    E_PERMISSION_QUEST, E_PERMISSION_REWARD, E_PERMISSION_STATE,
};
pub use prereq::{atoms, parse_prereq, Atom, PrereqFormula, E_CONN_PROFILE};
pub use fact_check::{check_fact_guards, E_ENTRY_UNREACHABLE, W_FACT_GUARANTEED};
pub use fact_env::{FactEnv, FactScope, GroundFact, MaySet, MustMap, QueryPattern, RootVocab};
pub use fact_must::{compute_must, stable_seeds, unproduced_relations, FactMust};
pub use project_check::{
    check_project_domain_reads, check_project_entry_ids, check_project_entry_refs,
    check_project_quest_handlers, check_project_quest_ids, check_project_quest_refs,
    check_project_quest_tree, check_project_subquest_unsatisfiable, colliding_entry_occurrences,
    colliding_occurrences, component_unverified_diag, domain_reading_set, ComponentScope,
    E_QUEST_MULTI_PARENT, E_QUEST_REF_UNKNOWN, E_QUEST_TIER_MIX, E_QUEST_TREE_CYCLE,
    W_COMPONENT_UNVERIFIED,
    W_DOMAIN_UNREAD, W_QUEST_HANDLER_DEAD, W_QUEST_REF_UNKNOWN,
};
pub use rel_schema::{build_rel_vocab, check_atom, validate_rel_decls, RelVocab};
pub use rule_index::evaluable_rules;
pub use schema_import::{resolve_imports, RelImports, SchemaImports};
pub use set_op::{check_set, WriteOwner, E_ENGINE_OWNED_WRITE};
pub use tag::{codes_locked, retag_document, tag_document, RetagOutcome, TagOutcome};
pub use temporal::{check_temporal, E_TEMPORAL_ARG};
pub use time::{fmt_seconds, ms_to_seconds, parse_time_ms, TimeParse, TIME_MAX_FRACTIONAL_DIGITS};
pub use timeline::{
    resolve_timeline, time_resolution_diag, ResolvedRow, ResolvedTimeline, E_CLIP_TIMING,
    E_TIMELINE_DURATION, E_TIME_RESOLUTION,
};
pub use usage::{check_project_usage, schema_sources, UsageDoc, W_DEF_UNUSED, W_RELATION_UNREAD};
pub use when_test_literal::{check_when_test_literals, test_as_is_pattern, W_WHEN_TEST_LITERAL};
