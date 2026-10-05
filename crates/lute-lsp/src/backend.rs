//! The `LanguageServer` backend (Task 6.1).
//!
//! Holds the open-document map and runs the shared [`lute_check::check`] core on
//! every open/change, republishing the result via `publishDiagnostics`. It is a
//! pass-through to `check()` — no validation logic lives here.
//!
//! ## Sync model
//! We advertise `TextDocumentSyncKind::FULL`: the client resends the entire
//! document on each edit, so `did_change` simply takes the last content-change's
//! `text` as the new snapshot. Incremental sync (range-scoped edits) is out of
//! scope for 6.1.
//!
//! ## The divergence invariant
//! [`analyze`](Backend::analyze) builds a `TextIndex` from the *same* document
//! text `check()` saw and maps each diagnostic's byte offsets through it (via
//! [`crate::convert::to_lsp_diagnostic`]). Since `check()` already re-derived every
//! span from its bytes through one shared `TextIndex`, the positions the LSP
//! publishes match the headless CLI byte-for-byte — the property Task 6.2's
//! golden asserts.

use std::path::PathBuf;
use dashmap::DashMap;
use lute_core_span::Diagnostic;
use tower_lsp_server::ls_types::{Diagnostic as LspDiagnostic, Uri};
use tower_lsp_server::Client;

mod analysis;
mod declaration;
mod diagnostics;
mod document;
mod positions;
mod requests;
mod version;
mod yaml_spans;

pub use document::DocumentSnapshot;
pub(crate) use positions::{byte_to_position, span_to_range};
use version::{binary_id, BinaryId};

/// The Lute language server: an LSP client handle plus the concurrent map of open
/// documents keyed by their [`Uri`].
pub struct Backend {
    client: Client,
    docs: DashMap<Uri, DocumentSnapshot>,
    /// The ORIGINAL `Vec<Diagnostic>` (fixits + `covered` intact) the last
    /// `analyze`/`analyze_declaration` run produced for each open document —
    /// kept beside the published LSP `Diagnostic`s (which drop `fixits`
    /// entirely, `crate::convert`'s doc comment). `textDocument/codeAction`
    /// (Task 15) has no other way to recover a fixit: the wire-form
    /// `Diagnostic` the client echoes back in `CodeActionContext` never
    /// carried one. Cleared on `did_close` alongside `docs`.
    diagnostics: DashMap<Uri, Vec<Diagnostic>>,
    /// What the last `analyze` published for each open document from its own
    /// check, and the version it was stamped with — republished together
    /// with [`Self::imported`] when another document's analysis changes the
    /// latter.
    published: DashMap<Uri, (Vec<LspDiagnostic>, Option<i32>)>,
    /// Round-6 T3-68: diagnostics located in ANOTHER file — a schema import's
    /// or plugin file's fault an importer reports — keyed by that file's
    /// canonical path, each with the importer it came from. Published on the
    /// file's own URI while it is open.
    imported: DashMap<PathBuf, Vec<(Uri, LspDiagnostic)>>,
    /// dsl 0.26.0 §8: the binary this server runs, as it was at start.
    binary: Option<(PathBuf, BinaryId)>,
    /// Round-5 T3-19: whether [`Self::warn_if_older_than_stamp`] has shown
    /// its one `window/showMessage` this session.
    warned_older: std::sync::atomic::AtomicBool,
}

impl Backend {
    /// Build a backend bound to `client` with empty document/diagnostic maps.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            diagnostics: DashMap::new(),
            published: DashMap::new(),
            imported: DashMap::new(),
            binary: std::env::current_exe()
                .ok()
                .and_then(|p| binary_id(&p).map(|id| (p, id))),
            warned_older: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
mod tests;
