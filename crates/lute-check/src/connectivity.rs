//! Project-wide graph assembly across parsed `.lute` documents.

mod graph_assembly;
mod graph_nodes;
mod graph_sources;
mod identity;
mod reachability;
#[cfg(test)]
mod tests;

pub use graph_assembly::assemble_graph;
pub use graph_nodes::{
    cycle_diag, quest_id_set, resolve_nodes, Anchor, ConnGraph, EdgeKind, NodeId, NodeInfo,
    PrereqState, E_CONN_CYCLE, E_CONN_UNKNOWN_NODE,
};
pub use graph_sources::{omitted_refs, OmittedRef};
pub use identity::{
    bundle_beat_after, bundle_beat_key_set, bundle_id, check_conn_episode_dup, scene_key,
    scene_key_set, when_visited_unanchored, E_CONN_EPISODE_ID_DUP,
};
pub use reachability::{
    ambiguous_quest_ids, assert_relations_per_doc, check_reachability, collect_asserted,
    collect_asserts, live_assert_relations, live_assert_sites, unreachable_quest_ids, Reachability,
    E_CONN_FORMULA_TOO_COMPLEX, E_CONN_UNREACHABLE,
};
