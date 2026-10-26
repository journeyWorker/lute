use std::path::Path;
use std::sync::LazyLock;

use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

use crate::meta::TypedMeta;

/// An empty capability snapshot: the [`ProjectDoc::snapshot`] of a caller
/// that resolves no reward and reads no plugin declaration (identity-only
/// passes).
pub static EMPTY_SNAPSHOT: LazyLock<CapabilitySnapshot> =
    LazyLock::new(CapabilitySnapshot::default);

/// Borrowed view of one parsed project document and its already-lifted metadata.
///
/// Project-wide checker passes use this view instead of carrying an owning path
/// and document tuple, so every pass observes the same typed frontmatter parse.
/// `snapshot` is the capability snapshot the document was checked against: its
/// `rewardKinds` resolve each `<reward>` to the effects a grant applies (0.50.0
/// §5.4, [`crate::reward_effects`]), and its directive declarations tell a
/// `text` attribute (0.52.0 §6.5, [`crate::directives::text_values`]).
#[derive(Clone, Copy, Debug)]
pub struct ProjectDoc<'a> {
    pub path: &'a Path,
    pub doc: &'a Document,
    pub meta: &'a TypedMeta,
    pub snapshot: &'a CapabilitySnapshot,
}

impl<'a> ProjectDoc<'a> {
    pub fn new(
        path: &'a Path,
        doc: &'a Document,
        meta: &'a TypedMeta,
        snapshot: &'a CapabilitySnapshot,
    ) -> Self {
        Self {
            path,
            doc,
            meta,
            snapshot,
        }
    }
}

/// Owned project documents with one typed frontmatter parse per document.
///
/// Use [`ProjectDocs::views`] when passing the project to project-wide passes.
/// The owned form is intended for callers that build project inputs outside
/// the model, especially tests and command implementations.
#[derive(Debug)]
pub struct ProjectDocs<'s> {
    docs: Vec<(std::path::PathBuf, Document)>,
    metas: Vec<TypedMeta>,
    /// The snapshot the documents were parsed with, which every one shares.
    snapshot: &'s CapabilitySnapshot,
}

impl<'s> ProjectDocs<'s> {
    /// Parse project documents and retain their typed metadata alongside
    /// them, lifted with the same kind resolution [`crate::fold_env`] uses (a
    /// lore document keeps its `series`, a quest document its quest keys).
    pub fn parse(
        docs: Vec<(std::path::PathBuf, Document)>,
        snapshot: &'s CapabilitySnapshot,
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
            snapshot,
        }
    }

    /// Borrow each owned document as the view consumed by project passes.
    pub fn views(&self) -> Vec<ProjectDoc<'_>> {
        self.docs
            .iter()
            .zip(&self.metas)
            .map(|((path, doc), meta)| ProjectDoc::new(path, doc, meta, self.snapshot))
            .collect()
    }

    pub fn documents(&self) -> &[(std::path::PathBuf, Document)] {
        &self.docs
    }
}
