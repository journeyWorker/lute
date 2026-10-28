//! What a long-lived input cache observed on disk (spec 0.53.0 §3.5).
//!
//! Every function that builds a cached input from the filesystem records,
//! through a [`DependencySink`], each fact its result depends on: a path it
//! resolved ([`Dependency::Lookup`]), a directory it enumerated
//! ([`Dependency::Listing`]) and a file whose bytes it read
//! ([`Dependency::File`]). A cache that outlives one command (the language
//! server's) revalidates the records before each reuse
//! ([`Dependency::revalidate`]); a one-shot command passes
//! [`DependencySink::none`], which records nothing and performs exactly the
//! I/O the site performed before.
//!
//! A file is identified by its [`Fingerprint`] (one `stat`); its content is
//! compared (SHA-256) only when the fingerprint differs, when a client
//! notification marked it dirty, or while the record is *racy* — the file
//! was written within [`RACY_NS`] of being read, so a further write could
//! leave every timestamp unchanged on a file system with coarse clocks. The
//! one change no validation sees is an edit that preserves every fingerprint
//! field of a file older than that window, which takes tampering with file
//! system metadata (setting the clock back so `ctime` lands on its old
//! value); a dirty mark or a server restart revalidates it.
#![expect(
    clippy::disallowed_methods,
    reason = "the recorder: the probes input assembly makes through a `DependencySink` are performed here"
)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use sha2::{Digest, Sha256};

/// A record whose file was written less than this long (nanoseconds) before
/// it was read compares content on every validation until it is older: two
/// writes inside one tick of a coarse file-system clock leave equal
/// timestamps.
pub const RACY_NS: i128 = 2_000_000_000;

/// One `stat` of a file: on Unix `(length, mtime, ctime, inode, device)`, on
/// Windows `(length, last-write time, file index, volume serial)`. `ctime`
/// cannot be set by `touch` or an editor, and a replace-by-rename changes the
/// inode (file index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    len: u64,
    /// mtime (last-write time on Windows), nanoseconds since the Unix epoch.
    modified_ns: i128,
    /// ctime on Unix; the last-write time again on Windows.
    changed_ns: i128,
    /// The inode (file index on Windows).
    id: u64,
    /// The device (volume serial number on Windows).
    device: u64,
}

impl Fingerprint {
    /// The fingerprint of the file at `path` (symbolic links followed).
    ///
    /// # Errors
    ///
    /// The `stat` (on Windows, opening the file) failed.
    #[cfg(unix)]
    pub fn of(path: &Path) -> io::Result<Self> {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path)?;
        let ns = |secs: i64, nsec: i64| i128::from(secs) * 1_000_000_000 + i128::from(nsec);
        Ok(Self {
            len: meta.size(),
            modified_ns: ns(meta.mtime(), meta.mtime_nsec()),
            changed_ns: ns(meta.ctime(), meta.ctime_nsec()),
            id: meta.ino(),
            device: meta.dev(),
        })
    }

    /// The fingerprint of the file at `path` (symbolic links followed).
    ///
    /// # Errors
    ///
    /// The `stat` (on Windows, opening the file) failed.
    #[cfg(windows)]
    #[expect(
        unsafe_code,
        reason = "GetFileInformationByHandle is the only stable source of the file index and volume serial"
    )]
    pub fn of(path: &Path) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = std::fs::File::open(path)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the handle is the open `file`'s, valid for the call, and
        // `info` is a properly aligned, writable struct of the expected type.
        let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        let wide = |high: u32, low: u32| (u64::from(high) << 32) | u64::from(low);
        // FILETIME: 100 ns intervals since 1601-01-01.
        let written = wide(
            info.ftLastWriteTime.dwHighDateTime,
            info.ftLastWriteTime.dwLowDateTime,
        );
        let written_ns = (i128::from(written) - 116_444_736_000_000_000) * 100;
        Ok(Self {
            len: wide(info.nFileSizeHigh, info.nFileSizeLow),
            modified_ns: written_ns,
            changed_ns: written_ns,
            id: wide(info.nFileIndexHigh, info.nFileIndexLow),
            device: u64::from(info.dwVolumeSerialNumber),
        })
    }

    /// The fingerprint of the file at `path` (symbolic links followed).
    ///
    /// # Errors
    ///
    /// The `stat` failed.
    #[cfg(not(any(unix, windows)))]
    pub fn of(path: &Path) -> io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| i128::try_from(d.as_nanos()).unwrap_or(i128::MAX));
        Ok(Self {
            len: meta.len(),
            modified_ns,
            changed_ns: modified_ns,
            id: 0,
            device: 0,
        })
    }

    /// Whether the file was written within [`RACY_NS`] of now.
    fn racy(self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i128::try_from(d.as_nanos()).unwrap_or(i128::MAX));
        self.modified_ns.max(self.changed_ns) > now - RACY_NS
    }
}

/// What a resolved path names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Dir,
    /// Neither (a device, a socket), or a link whose target cannot be read.
    Other,
}

impl EntryKind {
    fn of(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_file() => Self::File,
            Ok(meta) if meta.is_dir() => Self::Dir,
            _ => Self::Other,
        }
    }

    /// The suffix a [`Dependency::Listing`] entry carries.
    fn suffix(self) -> &'static str {
        match self {
            Self::File => "",
            Self::Dir => "/",
            Self::Other => "?",
        }
    }
}

/// How a lexical path resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing at the path (or a dangling link).
    Missing,
    /// The canonical target and what it is.
    Resolved { target: PathBuf, kind: EntryKind },
    /// Resolution failed otherwise.
    Error(io::ErrorKind),
}

impl Outcome {
    /// Resolve `path`: canonicalize it and `stat` the target.
    pub fn resolve(path: &Path) -> Self {
        match std::fs::canonicalize(path) {
            Ok(target) => {
                let kind = EntryKind::of(&target);
                Self::Resolved { target, kind }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::Missing,
            Err(e) => Self::Error(e.kind()),
        }
    }
}

/// Which entries of a directory a listing site reads: the names its result
/// depends on, and nothing else, so an unrelated file never invalidates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListFilter {
    /// `*.yaml`/`*.yml` files (provider catalogs, plugin export directories).
    Yaml,
    /// Subdirectories (the plugins directory's packages).
    Dirs,
    /// A `defaults:` glob segment: non-hidden names it matches (`*`, `?`).
    GlobSegment(String),
    /// A `defaults:` glob `**`: non-hidden subdirectories.
    GlobDirs,
    /// The component-file search of an unresolved `components:` entry:
    /// subdirectories other than hidden ones, `node_modules` and `target`,
    /// and every other entry named `*.component.lute`.
    ComponentTree,
}

impl ListFilter {
    /// Whether `name` can pass before its kind is known (one `stat` saved).
    fn admits_name(&self, name: &str) -> bool {
        match self {
            Self::Yaml => Path::new(name)
                .extension()
                .is_some_and(|e| e == "yaml" || e == "yml"),
            Self::Dirs => true,
            Self::GlobSegment(segment) => {
                !name.starts_with('.') && crate::project::segment_matches(segment, name)
            }
            Self::GlobDirs => !name.starts_with('.'),
            Self::ComponentTree => {
                (!name.starts_with('.') && name != "node_modules" && name != "target")
                    || name.ends_with(".component.lute")
            }
        }
    }

    fn admits(&self, name: &str, kind: EntryKind) -> bool {
        match self {
            Self::Yaml => kind == EntryKind::File,
            Self::Dirs | Self::GlobDirs => kind == EntryKind::Dir,
            Self::GlobSegment(_) => true,
            Self::ComponentTree => {
                let dir_ok = !name.starts_with('.') && name != "node_modules" && name != "target";
                if kind == EntryKind::Dir {
                    dir_ok
                } else {
                    name.ends_with(".component.lute")
                }
            }
        }
    }
}

/// One admitted directory entry.
#[derive(Clone, Debug)]
pub struct Listed {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

/// The admitted entries of one directory, sorted by path, and the per-entry
/// errors the enumeration met.
#[derive(Debug, Default)]
pub struct DirListing {
    pub entries: Vec<Listed>,
    pub entry_errors: Vec<io::Error>,
}

/// Enumerate `dir` through `filter`.
///
/// # Errors
///
/// `read_dir` failed.
pub fn list(dir: &Path, filter: &ListFilter) -> io::Result<DirListing> {
    let mut out = DirListing::default();
    for entry in std::fs::read_dir(dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                out.entry_errors.push(e);
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if !filter.admits_name(&name) {
            continue;
        }
        let path = entry.path();
        let kind = EntryKind::of(&path);
        if filter.admits(&name, kind) {
            out.entries.push(Listed { name, path, kind });
        }
    }
    out.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// The recorded form of a listing: each name with its kind suffix, and one
/// `!` per entry error; `None` when the directory could not be read.
fn listing_record(listing: &io::Result<DirListing>) -> Option<Vec<String>> {
    let listing = listing.as_ref().ok()?;
    let mut names: Vec<String> = listing
        .entries
        .iter()
        .map(|e| format!("{}{}", e.name, e.kind.suffix()))
        .collect();
    names.extend(listing.entry_errors.iter().map(|_| "!".to_string()));
    Some(names)
}

/// What a file's record holds about its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    /// The bytes were read: their SHA-256, the fingerprint taken before the
    /// read (`None` when the `stat` failed: compare content every time), and
    /// whether the record is racy ([`RACY_NS`]).
    Read {
        fingerprint: Option<Fingerprint>,
        hash: [u8; 32],
        racy: bool,
    },
    /// The read failed with this error.
    Unreadable(io::ErrorKind),
}

impl Content {
    fn read(fingerprint: Option<Fingerprint>, bytes: &[u8]) -> Self {
        Self::Read {
            fingerprint,
            hash: Sha256::digest(bytes).into(),
            racy: fingerprint.is_none_or(Fingerprint::racy),
        }
    }
}

/// One fact a cached value depends on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dependency {
    /// A lexical path (as authored and joined, before canonicalization) and
    /// how it resolved.
    Lookup { path: PathBuf, outcome: Outcome },
    /// A directory enumeration that decided which files exist.
    Listing {
        dir: PathBuf,
        filter: ListFilter,
        entries: Option<Vec<String>>,
    },
    /// A file whose bytes were read.
    File { path: PathBuf, content: Content },
}

impl Dependency {
    /// Whether a change notification for `path` concerns this record: the
    /// file itself, the lookup's path or target, or an entry of the listed
    /// directory.
    pub fn names(&self, path: &Path) -> bool {
        match self {
            Self::Lookup { path: p, outcome } => {
                p == path || matches!(outcome, Outcome::Resolved { target, .. } if target == path)
            }
            Self::Listing { dir, .. } => dir == path || path.parent() == Some(dir.as_path()),
            Self::File { path: p, .. } => p == path,
        }
    }

    /// Whether the record still holds. A lookup is re-resolved and a listing
    /// re-listed; a file is `stat`ed and, when its fingerprint differs, it is
    /// `dirty` or the record is racy, hashed. A file whose content is
    /// unchanged keeps the record with the new fingerprint.
    pub fn revalidate(&mut self, dirty: bool) -> bool {
        match self {
            Self::Lookup { path, outcome } => Outcome::resolve(path) == *outcome,
            Self::Listing {
                dir,
                filter,
                entries,
            } => listing_record(&list(dir, filter)) == *entries,
            Self::File { path, content } => match content {
                Content::Read {
                    fingerprint,
                    hash,
                    racy,
                } => {
                    let now = Fingerprint::of(path).ok();
                    if !dirty && !*racy && now.is_some() && now == *fingerprint {
                        return true;
                    }
                    let Ok(bytes) = std::fs::read(path) else {
                        return false;
                    };
                    let fresh = Content::read(now, &bytes);
                    let Content::Read { hash: new_hash, .. } = &fresh else {
                        return false;
                    };
                    if new_hash != hash {
                        return false;
                    }
                    *content = fresh;
                    true
                }
                Content::Unreadable(kind) => {
                    matches!(std::fs::read(path), Err(e) if e.kind() == *kind)
                }
            },
        }
    }
}

/// Where a filesystem site records what it observed. [`Self::none`] records
/// nothing: its methods are the plain `std::fs` calls the sites made before.
#[derive(Debug, Default)]
pub struct DependencySink {
    records: Option<Mutex<Vec<Dependency>>>,
}

static NO_SINK: DependencySink = DependencySink { records: None };

impl DependencySink {
    /// The sink that records nothing (one-shot commands).
    pub fn none() -> &'static Self {
        &NO_SINK
    }

    /// A sink that keeps every record.
    pub fn recording() -> Self {
        Self {
            records: Some(Mutex::new(Vec::new())),
        }
    }

    /// [`Self::recording`] when `record`, else a sink that records nothing.
    pub fn new(record: bool) -> Self {
        if record {
            Self::recording()
        } else {
            Self::default()
        }
    }

    /// Whether this sink keeps records.
    pub fn is_recording(&self) -> bool {
        self.records.is_some()
    }

    /// The records, in observation order.
    pub fn into_records(self) -> Vec<Dependency> {
        self.records
            .map(|r| r.into_inner().unwrap_or_else(PoisonError::into_inner))
            .unwrap_or_default()
    }

    fn push(&self, dependency: Dependency) {
        if let Some(records) = &self.records {
            records
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(dependency);
        }
    }

    /// [`std::fs::read_to_string`], recording a [`Dependency::File`]: the
    /// fingerprint is taken before the read and the hash is of the bytes
    /// read, so a write racing the read is seen on the next validation.
    ///
    /// # Errors
    ///
    /// As [`std::fs::read_to_string`].
    pub fn read_to_string(&self, path: &Path) -> io::Result<String> {
        if !self.is_recording() {
            return std::fs::read_to_string(path);
        }
        let fingerprint = Fingerprint::of(path).ok();
        match std::fs::read(path) {
            Ok(bytes) => {
                self.push(Dependency::File {
                    path: path.to_path_buf(),
                    content: Content::read(fingerprint, &bytes),
                });
                String::from_utf8(bytes).map_err(|_not_utf8| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "stream did not contain valid UTF-8",
                    )
                })
            }
            Err(e) => {
                self.push(Dependency::File {
                    path: path.to_path_buf(),
                    content: Content::Unreadable(e.kind()),
                });
                Err(e)
            }
        }
    }

    /// [`std::fs::canonicalize`], recording a [`Dependency::Lookup`].
    ///
    /// # Errors
    ///
    /// As [`std::fs::canonicalize`].
    pub fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let result = std::fs::canonicalize(path);
        if self.is_recording() {
            let outcome = match &result {
                Ok(target) => Outcome::Resolved {
                    target: target.clone(),
                    kind: EntryKind::of(target),
                },
                Err(e) if e.kind() == io::ErrorKind::NotFound => Outcome::Missing,
                Err(e) => Outcome::Error(e.kind()),
            };
            self.push(Dependency::Lookup {
                path: path.to_path_buf(),
                outcome,
            });
        }
        result
    }

    fn lookup(&self, path: &Path) -> Outcome {
        let outcome = Outcome::resolve(path);
        self.push(Dependency::Lookup {
            path: path.to_path_buf(),
            outcome: outcome.clone(),
        });
        outcome
    }

    /// [`Path::is_file`], recording a [`Dependency::Lookup`].
    pub fn is_file(&self, path: &Path) -> bool {
        if !self.is_recording() {
            return path.is_file();
        }
        matches!(
            self.lookup(path),
            Outcome::Resolved {
                kind: EntryKind::File,
                ..
            }
        )
    }

    /// [`Path::is_dir`], recording a [`Dependency::Lookup`].
    pub fn is_dir(&self, path: &Path) -> bool {
        if !self.is_recording() {
            return path.is_dir();
        }
        matches!(
            self.lookup(path),
            Outcome::Resolved {
                kind: EntryKind::Dir,
                ..
            }
        )
    }

    /// [`Path::exists`], recording a [`Dependency::Lookup`].
    pub fn exists(&self, path: &Path) -> bool {
        if !self.is_recording() {
            return path.exists();
        }
        matches!(self.lookup(path), Outcome::Resolved { .. })
    }

    /// [`list`], recording a [`Dependency::Listing`].
    ///
    /// # Errors
    ///
    /// `read_dir` failed.
    pub fn list(&self, dir: &Path, filter: ListFilter) -> io::Result<DirListing> {
        let listing = list(dir, &filter);
        if self.is_recording() {
            let entries = listing_record(&listing);
            self.push(Dependency::Listing {
                dir: dir.to_path_buf(),
                filter,
                entries,
            });
        }
        listing
    }
}

/// What one memo entry was built from: the entries of its own or another
/// memo it read (`children`, which a mark-and-sweep keeps alive with it) and
/// the dependency records its build observed.
#[derive(Debug)]
pub struct Recorded<N> {
    pub children: Vec<N>,
    pub deps: Mutex<Vec<Dependency>>,
}

impl<N> Default for Recorded<N> {
    fn default() -> Self {
        Self {
            children: Vec::new(),
            deps: Mutex::new(Vec::new()),
        }
    }
}

impl<N> Recorded<N> {
    /// The record of a build that read `children` and observed `sink`'s
    /// dependencies.
    pub fn new(children: Vec<N>, sink: DependencySink) -> Self {
        Self {
            children,
            deps: Mutex::new(sink.into_records()),
        }
    }

    /// A copy of the entry's dependency records.
    pub fn records(&self) -> Vec<Dependency> {
        self.deps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// How many dependency records the entry holds.
    pub fn dependency_count(&self) -> usize {
        self.deps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether any record names `path` ([`Dependency::names`]).
    pub fn names(&self, path: &Path) -> bool {
        self.deps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|d| d.names(path))
    }

    /// Whether every record still holds ([`Dependency::revalidate`]); `dirty`
    /// says which paths a notification marked.
    pub fn revalidate(&self, dirty: &dyn Fn(&Dependency) -> bool) -> bool {
        self.deps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
            .all(|d| {
                let is_dirty = dirty(d);
                d.revalidate(is_dirty)
            })
    }
}

#[cfg(test)]
mod tests;
