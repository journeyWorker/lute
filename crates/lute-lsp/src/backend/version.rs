use std::path::Path;
use lute_manifest::project::MetaDefaults;
use tower_lsp_server::ls_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity, MessageType, Range, Uri};
use super::Backend;

/// What identifies one build of a binary file: its length, mtime and (on
/// unix) inode — a reinstall writes a new file or renames one over it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct BinaryId {
    len: u64,
    modified: Option<std::time::SystemTime>,
    inode: u64,
}

pub(super) fn binary_id(path: &Path) -> Option<BinaryId> {
    let m = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(&m);
    #[cfg(not(unix))]
    let inode = 0;
    Some(BinaryId {
        len: m.len(),
        modified: m.modified().ok(),
        inode,
    })
}

/// A `luteVersion:` YAML value as its string (a stamp is a quoted or plain
/// `x.y.z` string).
fn yaml_version(v: &serde_yaml::Value) -> Option<String> {
    v.as_str().map(|s| s.trim().to_string())
}

/// `MAJOR.MINOR.PATCH` as a comparable triple; `None` for anything else.
fn version_triple(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim().split('.').map(|p| p.parse::<u64>().ok());
    let t = (it.next()??, it.next()??, it.next()??);
    it.next().is_none().then_some(t)
}

impl Backend {
    /// dsl 0.26.0 §8 (T3-12): `Some(message)` once the binary this server
    /// was started from has been replaced or removed — its results would be
    /// an older build's, so it publishes this one diagnostic instead.
    pub(super) fn stale(&self) -> Option<String> {
        let (path, id) = self.binary.as_ref()?;
        (binary_id(path).as_ref() != Some(id)).then(|| {
            format!(
                "stale server: lute-lsp {} was started from {}, which has been replaced since, so \
                 its diagnostics would come from an older build — restart the language server",
                env!("CARGO_PKG_VERSION"),
                path.display()
            )
        })
    }

    /// Publish the [`Self::stale`] diagnostic alone for `uri`; `false` when
    /// the server is current.
    pub(super) async fn publish_if_stale(&self, uri: &Uri, version: i32) -> bool {
        let Some(message) = self.stale() else {
            return false;
        };
        self.diagnostics.insert(uri.clone(), Vec::new());
        let diag = LspDiagnostic {
            range: Range::default(),
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(tower_lsp_server::ls_types::NumberOrString::String(
                "lute-lsp-stale".into(),
            )),
            source: Some("lute-lsp".into()),
            message,
            ..Default::default()
        };
        self.client
            .publish_diagnostics(uri.clone(), vec![diag], Some(version))
            .await;
        true
    }

    /// Round-5 T3-19: a document (or the project manifest's `defaults:`)
    /// stamped with a `luteVersion` NEWER than this server's language means
    /// the editor runs an older `lute-lsp` than the project targets — every
    /// writer in round 5 met a 0.17 server that way and found it only through
    /// `lute doctor`. The checker's `W-LUTE-VERSION-STALE` marks the stamp;
    /// this also tells the user once per session, in the client's own UI, so
    /// a wall of bogus diagnostics is not trusted over the terminal.
    pub(super) async fn warn_if_older_than_stamp(&self, text: &str, defaults: &MetaDefaults) {
        use std::sync::atomic::Ordering;
        if self.warned_older.load(Ordering::Relaxed) {
            return;
        }
        let (doc, _) = lute_syntax::parse(text);
        let own = serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
            .ok()
            .and_then(|m| m.get("luteVersion").and_then(yaml_version));
        let Some(stamp) = own.or_else(|| defaults.get("luteVersion").and_then(yaml_version)) else {
            return;
        };
        let ours = lute_check::LUTE_LANG_VERSION;
        let newer = matches!(
            (version_triple(&stamp), version_triple(ours)),
            (Some(s), Some(o)) if s > o
        );
        if !newer || self.warned_older.swap(true, Ordering::Relaxed) {
            return;
        }
        self.client
            .show_message(
                MessageType::WARNING,
                format!(
                    "Lute: this editor's lute-lsp ({ours}) is older than the project targets \
                     (luteVersion {stamp}), so its diagnostics may be wrong — run `lute doctor` \
                     in a terminal to find the stale install, then restart the language server."
                ),
            )
            .await;
    }
}
