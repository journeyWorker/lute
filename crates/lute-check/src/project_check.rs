//! Project-wide checks split by responsibility.

mod branches;
mod components;
mod domains;
mod entries;
mod paths;
mod quest_ids;
mod quest_refs;
mod quest_tiers;
mod quest_tree;

pub use branches::{check_project_branch_ids, W_BRANCH_ID_SHARED};
pub use components::{component_unverified_diag, ComponentScope, W_COMPONENT_UNVERIFIED};
pub use domains::{check_project_domain_reads, domain_reading_set, domain_reads_from_kinds, domain_reads_from_relations, domain_reads_from_state, W_DOMAIN_UNREAD};
pub use entries::{check_project_entry_ids, check_project_entry_refs, colliding_entry_occurrences};
pub use quest_ids::{check_project_quest_ids, colliding_occurrences};
pub use quest_refs::{check_project_quest_refs, W_QUEST_REF_UNKNOWN};
pub use quest_tiers::{check_doc_quest_rearm, check_doc_quest_tiers, check_quest_tier_implicit, E_QUEST_TIER_MIX, E_SUBQUEST_REARM, W_QUEST_REARM_CONSTANT, W_QUEST_TIER_IMPLICIT};
pub use quest_tree::{check_project_quest_handlers, check_project_quest_tree, check_project_subquest_unsatisfiable, E_OBJECTIVE_UNSATISFIABLE_SUBQUEST, E_QUEST_MULTI_PARENT, E_QUEST_REF_UNKNOWN, E_QUEST_TREE_CYCLE, W_QUEST_HANDLER_DEAD};

#[cfg(test)]
mod tests;
