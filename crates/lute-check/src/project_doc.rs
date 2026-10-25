use std::collections::BTreeMap;
use std::path::Path;

use lute_manifest::schema::RewardKindDecl;
use lute_syntax::ast::Document;

use crate::meta::TypedMeta;

/// Borrowed view of one parsed project document and its already-lifted metadata.
///
/// Project-wide checker passes use this view instead of carrying an owning path
/// and document tuple, so every pass observes the same typed frontmatter parse.
/// `reward_kinds` is the reward vocabulary the document was checked against
/// (its capability snapshot's `rewardKinds`): what resolves each of its
/// `<reward>`s to the effects a grant applies (0.50.0 §5.4,
/// [`crate::reward_effects`]).
#[derive(Clone, Copy, Debug)]
pub struct ProjectDoc<'a> {
    pub path: &'a Path,
    pub doc: &'a Document,
    pub meta: &'a TypedMeta,
    pub reward_kinds: &'a BTreeMap<String, RewardKindDecl>,
}

impl<'a> ProjectDoc<'a> {
    pub fn new(
        path: &'a Path,
        doc: &'a Document,
        meta: &'a TypedMeta,
        reward_kinds: &'a BTreeMap<String, RewardKindDecl>,
    ) -> Self {
        Self {
            path,
            doc,
            meta,
            reward_kinds,
        }
    }
}

/// Owned project documents with one typed frontmatter parse per document.
///
/// Use [`ProjectDocs::views`] when passing the project to project-wide passes.
/// The owned form is intended for callers that build project inputs outside
/// the model, especially tests and command implementations.
#[derive(Debug)]
pub struct ProjectDocs {
    docs: Vec<(std::path::PathBuf, Document)>,
    metas: Vec<TypedMeta>,
    /// The snapshot's reward vocabulary every document shares.
    reward_kinds: BTreeMap<String, RewardKindDecl>,
}

impl ProjectDocs {
    /// Parse project documents and retain their typed metadata alongside
    /// them, lifted with the same kind resolution [`crate::fold_env`] uses (a
    /// lore document keeps its `series`, a quest document its quest keys).
    pub fn parse(
        docs: Vec<(std::path::PathBuf, Document)>,
        snapshot: &lute_manifest::snapshot::CapabilitySnapshot,
    ) -> Self {
        let defaults = lute_manifest::project::MetaDefaults::default();
        let metas = docs
            .iter()
            .map(|(_, doc)| {
                let (_, kind, _) = crate::check::resolve_kinds(doc, &defaults);
                crate::meta::parse_meta_kind(&doc.meta, snapshot, kind).0
            })
            .collect();
        Self {
            docs,
            metas,
            reward_kinds: snapshot.reward_kinds.clone(),
        }
    }

    /// Borrow each owned document as the view consumed by project passes.
    pub fn views(&self) -> Vec<ProjectDoc<'_>> {
        self.docs
            .iter()
            .zip(&self.metas)
            .map(|((path, doc), meta)| ProjectDoc::new(path, doc, meta, &self.reward_kinds))
            .collect()
    }

    pub fn documents(&self) -> &[(std::path::PathBuf, Document)] {
        &self.docs
    }
}
