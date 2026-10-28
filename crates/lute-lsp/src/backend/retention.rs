//! The project input caches the server keeps across requests (spec 0.53.0
//! §3.5): one tracked [`InputCache`] per project root (and one for documents
//! under no root), revalidated before every request and reclaimed after it.
//!
//! **Validation.** A request first revalidates every dependency record of its
//! root's cache ([`InputCache::revalidate`]): each lookup re-resolved, each
//! listing re-listed, each file `stat`ed and, when its fingerprint changed,
//! a notification marked it dirty or the record is racy, hashed. Any failed
//! record replaces the cache with a fresh one; a fresh cache rebuilds only
//! what the requests ask for.
//!
//! **Retention.** A document's *configuration* is the set of memo keys its
//! last assembly used ([`Configuration`]). The retained configurations are
//! the **live** ones (of the open documents) plus at most
//! [`SUPERSEDED_RETAINED`] **superseded** ones (no open document uses them
//! any more), most recent first. After every request (and every close) the
//! cache keeps exactly the entries reachable from the retained
//! configurations and from the configurations **pinned** by requests still
//! running ([`InputCache::retain`]); the sweep waits while a request has
//! not yet pinned its configuration (it may be building entries no
//! configuration names yet). When the last open document of a root closes,
//! the root's cache is dropped.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use lute_load::cache::{Configuration, RetainedCounts};
use lute_load::{InputCache, InputCacheStats};

/// How many superseded configurations a root keeps besides the live ones.
pub const SUPERSEDED_RETAINED: usize = 4;

/// A project root, or `None` for documents under no `lute.project.yaml`.
type RootKey = Option<PathBuf>;

/// The cache of one root and what it retains.
#[derive(Debug)]
struct RootEntry {
    cache: Arc<InputCache>,
    /// Paths a notification marked: every record naming one compares
    /// content at the next validation.
    dirty: Vec<PathBuf>,
    /// The configuration of each open document (by its key, the URI).
    live: HashMap<String, Configuration>,
    /// Superseded configurations, most recent first.
    superseded: VecDeque<Configuration>,
    /// The configurations of running requests, by request.
    pins: HashMap<u64, Configuration>,
    /// Running requests that have not pinned a configuration yet.
    assembling: usize,
    /// The counters of the caches this root replaced.
    retired: InputCacheStats,
}

impl RootEntry {
    fn new() -> Self {
        Self {
            cache: Arc::new(InputCache::tracked()),
            dirty: Vec::new(),
            live: HashMap::new(),
            superseded: VecDeque::new(),
            pins: HashMap::new(),
            assembling: 0,
            retired: InputCacheStats::default(),
        }
    }

    /// `config` is no longer live for some document: keep it as the most
    /// recent superseded configuration unless another document lives on it.
    fn supersede(&mut self, config: Configuration) {
        if self.live.values().any(|c| *c == config) {
            return;
        }
        self.superseded.retain(|c| *c != config);
        self.superseded.push_front(config);
        self.superseded.truncate(SUPERSEDED_RETAINED);
    }

    /// Reclaim every entry outside the retained closure, unless a request is
    /// still assembling.
    fn sweep(&self) {
        if self.assembling > 0 {
            return;
        }
        self.cache.retain(
            self.live
                .values()
                .chain(&self.superseded)
                .chain(self.pins.values()),
        );
    }
}

#[derive(Debug, Default)]
struct Inner {
    roots: HashMap<RootKey, RootEntry>,
    /// The open documents.
    open: HashSet<String>,
    /// The root each open document's live configuration sits in.
    doc_root: HashMap<String, RootKey>,
    next_request: u64,
}

impl Inner {
    /// Drop `doc`'s live configuration from its root (it moves to that
    /// root's superseded list); drop the root's cache when no open document
    /// is left in it.
    fn release(&mut self, doc: &str) {
        let Some(root) = self.doc_root.remove(doc) else {
            return;
        };
        let Some(entry) = self.roots.get_mut(&root) else {
            return;
        };
        if let Some(config) = entry.live.remove(doc) {
            entry.supersede(config);
        }
        if entry.live.is_empty() {
            self.roots.remove(&root);
        } else {
            entry.sweep();
        }
    }
}

/// Every root's input cache, its retained configurations and the open
/// documents (spec 0.53.0 §3.5).
#[derive(Debug, Default)]
pub struct ProjectInputs {
    inner: Mutex<Inner>,
}

impl ProjectInputs {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `doc` was opened: its configuration is live from its next request.
    pub fn open(&self, doc: &str) {
        self.lock().open.insert(doc.to_string());
    }

    /// `doc` was closed: its configuration is superseded, and its root's
    /// cache is dropped when it was the root's last open document.
    pub fn close(&self, doc: &str) {
        let mut inner = self.lock();
        inner.open.remove(doc);
        inner.release(doc);
    }

    /// A client notification (`didChangeWatchedFiles`, `didSave`) for
    /// `path`: every root whose records name it compares content at its next
    /// validation. A root that never read it is left alone.
    pub fn mark_dirty(&self, path: &Path) {
        let canonical = std::fs::canonicalize(path).ok();
        let paths: Vec<&Path> = std::iter::once(path).chain(canonical.as_deref()).collect();
        let mut inner = self.lock();
        for entry in inner.roots.values_mut() {
            for p in &paths {
                if entry.cache.names(p) && !entry.dirty.iter().any(|d| d == p) {
                    entry.dirty.push(p.to_path_buf());
                }
            }
        }
    }

    /// Start a request on `root` for the open document `doc` (`None` for a
    /// request no document owns): revalidate the root's cache, replacing it
    /// when a record no longer holds, and hand it out.
    pub fn begin(&self, root: Option<&Path>, doc: Option<&str>) -> Request<'_> {
        let mut inner = self.lock();
        inner.next_request += 1;
        let id = inner.next_request;
        let key: RootKey = root.map(Path::to_path_buf);
        let entry = inner
            .roots
            .entry(key.clone())
            .or_insert_with(RootEntry::new);
        let dirty = std::mem::take(&mut entry.dirty);
        let names_dirty = |d: &lute_load::cache::Dependency| dirty.iter().any(|p| d.names(p));
        if !entry.cache.revalidate(&names_dirty) {
            entry.retired += entry.cache.stats();
            entry.cache = Arc::new(InputCache::tracked());
        }
        entry.assembling += 1;
        Request {
            inputs: self,
            root: key,
            doc: doc.map(str::to_string),
            id,
            cache: Arc::clone(&entry.cache),
            config: None,
        }
    }

    /// The bases, vocabularies and assemblies `root`'s caches built, the
    /// replaced ones included; zero when the root has no cache.
    pub fn stats(&self, root: Option<&Path>) -> InputCacheStats {
        let inner = self.lock();
        let Some(entry) = inner.roots.get(&root.map(Path::to_path_buf)) else {
            return InputCacheStats::default();
        };
        let mut stats = entry.retired;
        stats += entry.cache.stats();
        stats
    }

    /// What `root`'s cache retains, or `None` when the root has none.
    pub fn retained(&self, root: Option<&Path>) -> Option<RetainedCounts> {
        let inner = self.lock();
        inner
            .roots
            .get(&root.map(Path::to_path_buf))
            .map(|entry| entry.cache.retained())
    }

    /// The configurations `root` retains: live, then superseded (most
    /// recent first), then pinned.
    pub fn retained_configurations(&self, root: Option<&Path>) -> Vec<Configuration> {
        let inner = self.lock();
        inner
            .roots
            .get(&root.map(Path::to_path_buf))
            .map(|e| {
                let mut all: Vec<Configuration> = Vec::new();
                for c in e.live.values().chain(&e.superseded).chain(e.pins.values()) {
                    if !all.contains(c) {
                        all.push(c.clone());
                    }
                }
                all
            })
            .unwrap_or_default()
    }
}

/// One running request on a root's cache. Pin the configuration the
/// assembly used ([`Self::pin`]) as soon as it is known; dropping the
/// request makes it the document's live configuration (while the document
/// is open), unpins it and reclaims what no retained configuration reaches.
#[derive(Debug)]
pub struct Request<'a> {
    inputs: &'a ProjectInputs,
    root: RootKey,
    doc: Option<String>,
    id: u64,
    cache: Arc<InputCache>,
    config: Option<Configuration>,
}

impl Request<'_> {
    /// The root's cache, valid for this request.
    pub fn cache(&self) -> &InputCache {
        &self.cache
    }

    /// The configuration this request's assembly used: its entries stay
    /// until the request ends, whatever other requests retain.
    pub fn pin(&mut self, config: Configuration) {
        let mut inner = self.inputs.lock();
        if let Some(entry) = inner.roots.get_mut(&self.root) {
            if self.config.is_none() {
                entry.assembling = entry.assembling.saturating_sub(1);
            }
            entry.pins.insert(self.id, config.clone());
        }
        self.config = Some(config);
    }
}

impl Drop for Request<'_> {
    fn drop(&mut self) {
        let mut inner = self.inputs.lock();
        let open = self.doc.as_ref().is_some_and(|d| inner.open.contains(d));
        // A document that moved to another root (a manifest created or
        // removed) leaves its old root.
        if let (true, Some(doc)) = (open && self.config.is_some(), &self.doc) {
            if inner.doc_root.get(doc).is_some_and(|r| *r != self.root) {
                inner.release(doc);
            }
        }
        let Some(entry) = inner.roots.get_mut(&self.root) else {
            return;
        };
        if self.config.is_none() {
            entry.assembling = entry.assembling.saturating_sub(1);
        }
        entry.pins.remove(&self.id);
        let mut live_doc = None;
        if let (true, Some(doc), Some(config)) = (open, &self.doc, &self.config) {
            entry.superseded.retain(|c| c != config);
            if let Some(old) = entry.live.insert(doc.clone(), config.clone()) {
                if old != *config {
                    entry.supersede(old);
                }
            }
            live_doc = Some(doc.clone());
        }
        entry.sweep();
        if let Some(doc) = live_doc {
            inner.doc_root.insert(doc, self.root.clone());
        }
    }
}
