//! The runtime's view of a document's state schema and relation vocabulary:
//! exactly what evaluation reads — a state path's `default:` and each
//! relation's declaration (`derive:`, `excludes:`).

use std::collections::BTreeMap;

use lute_manifest::relations::RelationDecl;
use lute_manifest::Literal;

/// One declared state path, as evaluation reads it.
#[derive(Clone, Debug, Default)]
pub struct StateDecl {
    /// The path's `default:` — its effective value when nothing wrote it.
    pub default: Option<Literal>,
}

/// The document's `state:` schema (dsl §9), path -> decl.
#[derive(Clone, Debug, Default)]
pub struct StateSchema {
    pub decls: BTreeMap<String, StateDecl>,
}

/// The document's declared relations, by name.
#[derive(Clone, Debug, Default)]
pub struct RelVocab {
    pub relations: BTreeMap<String, RelationDecl>,
}

impl RelVocab {
    /// dsl 0.25.0 §1: relations `a` and `b` can never hold together on the
    /// same arguments (`excludes:`, closed symmetrically).
    pub fn excludes(&self, a: &str, b: &str) -> bool {
        lute_manifest::relations::relations_exclude(&self.relations, a, b)
    }
}
