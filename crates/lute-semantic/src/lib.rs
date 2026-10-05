//! Stable semantic values shared by project analysis and source consumers.

pub mod fact;
pub mod graph;
pub mod identity;
pub mod location;
pub mod rename;
pub use fact::{fact_node, fact_overlap, format_fact};
pub use graph::{GraphEdge, GraphNode, NodeKey, NodeKind, SemanticGraph};
pub use identity::{IdentityMetadata, IdentitySource};
pub use location::SourceLocation;
pub use rename::RenameError;
