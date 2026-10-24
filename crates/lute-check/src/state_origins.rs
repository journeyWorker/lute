//! Where each imported `state:` path is declared (dsl 0.26 §2.1/§2.7), with
//! `per:` families kept as families (spec 0.49.0 §9.3).
//!
//! A path's place in its schema is [`state_key_span`]'s, an O(file) scan.
//! Until 0.49 every member of every family was located that way when its
//! schema was read, so an imported kind of `n` members cost `n` scans of a
//! file that grows with `n` (#189). [`StateOrigins`] locates an explicit
//! path when its schema is read, as before, and keeps a family as its
//! members and its file: a member is located when a diagnostic asks where
//! it is, by the same function, so every reported position is the one the
//! per-member map held.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_core_span::TextIndex;
use lute_syntax::ast::Meta;

use crate::meta::StateDecls;
use crate::rel_schema::{state_key_span, DeclOrigin};

/// The imported `state:` paths, path → [`DeclOrigin`]. A path several
/// files declare keeps the first file merged ([`Self::merge_first`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateOrigins {
    explicit: BTreeMap<String, DeclOrigin>,
    /// Family path → the families declared there, the first merged first.
    /// A member path an explicit entry names is that entry's.
    families: BTreeMap<String, Vec<FamilyOrigin>>,
}

/// The members of one file's `per:` family at a path.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FamilyOrigin {
    /// Sorted, as [`crate::meta::StateFamily::members`].
    members: Arc<[String]>,
    home: FamilyHome,
}

/// Where a family's members are declared.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FamilyHome {
    /// In the file's `state:`, each member located on demand.
    File(Arc<StateHome>),
    /// At the family's key: a `per:` over a kind another schema declares
    /// lists no member in its own file (dsl 0.28.0).
    Key(DeclOrigin),
}

impl FamilyOrigin {
    fn has(&self, member: &str) -> bool {
        self.members
            .binary_search_by(|m| m.as_str().cmp(member))
            .is_ok()
    }

    fn origin(&self) -> StateOrigin<'_> {
        match &self.home {
            FamilyHome::File(home) => StateOrigin::Member(home),
            FamilyHome::Key(at) => StateOrigin::At(at),
        }
    }
}

/// A schema file, kept to locate its family members on demand.
pub struct StateHome {
    file: PathBuf,
    text: String,
    meta: Meta,
}

impl StateHome {
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// The frontmatter its paths are located in.
    pub(crate) fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Where `path` is declared here, positioned.
    pub fn locate(&self, path: &str) -> DeclOrigin {
        locate(&self.file, &TextIndex::new(&self.text), &self.meta, path)
    }
}

/// Where `path` is declared in `meta`, the frontmatter of `idx`'s text in
/// `file`, positioned.
fn locate(file: &Path, idx: &TextIndex<'_>, meta: &Meta, path: &str) -> DeclOrigin {
    DeclOrigin::located(file, idx, state_key_span(meta, path))
}

impl PartialEq for StateHome {
    fn eq(&self, other: &Self) -> bool {
        self.file == other.file
            && self.meta.span == other.meta.span
            && self.meta.raw_yaml == other.meta.raw_yaml
            && self.text == other.text
    }
}

impl Eq for StateHome {}

impl fmt::Debug for StateHome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StateHome")
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

/// Where one imported path is declared: located, or a family member of a
/// file, located on demand.
#[derive(Clone, Copy, Debug)]
pub enum StateOrigin<'a> {
    At(&'a DeclOrigin),
    Member(&'a StateHome),
}

impl<'a> StateOrigin<'a> {
    pub fn file(&self) -> &'a Path {
        match self {
            Self::At(o) => &o.file,
            Self::Member(home) => home.file(),
        }
    }

    /// Where `path`, the path this origin is of, is declared.
    pub fn resolve(&self, path: &str) -> Cow<'a, DeclOrigin> {
        match self {
            Self::At(o) => Cow::Borrowed(*o),
            Self::Member(home) => Cow::Owned(home.locate(path)),
        }
    }
}

impl StateOrigins {
    /// The paths `decls` declares, read from `meta`, the frontmatter of
    /// `idx`'s text in `file` (canonical): each explicit path located now,
    /// each family kept with a copy of the file.
    pub(crate) fn declared_in(
        file: &Path,
        idx: &TextIndex<'_>,
        meta: &Meta,
        decls: &StateDecls,
    ) -> Self {
        let explicit = decls
            .explicit()
            .keys()
            .map(|p| (p.clone(), locate(file, idx, meta, p)))
            .collect();
        if decls.families().is_empty() {
            return Self {
                explicit,
                families: BTreeMap::new(),
            };
        }
        let home = Arc::new(StateHome {
            file: file.to_path_buf(),
            text: idx.text().to_string(),
            meta: meta.clone(),
        });
        let families = decls
            .families()
            .iter()
            .map(|(path, family)| {
                let origin = FamilyOrigin {
                    members: Arc::clone(family.shared_members()),
                    home: FamilyHome::File(Arc::clone(&home)),
                };
                (path.clone(), vec![origin])
            })
            .collect();
        Self { explicit, families }
    }

    /// Where `path` is declared.
    pub fn get(&self, path: &str) -> Option<Cow<'_, DeclOrigin>> {
        self.origin(path).map(|o| o.resolve(path))
    }

    pub fn contains_key(&self, path: &str) -> bool {
        self.origin(path).is_some()
    }

    /// Where `path` is declared, without locating a family member.
    pub fn origin(&self, path: &str) -> Option<StateOrigin<'_>> {
        if let Some(o) = self.explicit.get(path) {
            return Some(StateOrigin::At(o));
        }
        let (family, member) = path.rsplit_once('.')?;
        self.families
            .get(family)?
            .iter()
            .find(|f| f.has(member))
            .map(FamilyOrigin::origin)
    }

    /// Where `path` is declared, else where the first declared path under
    /// it (byte order) is: a `per:` family's first member.
    pub fn at_or_under(&self, path: &str) -> Option<Cow<'_, DeclOrigin>> {
        if let Some(o) = self.get(path) {
            return Some(o);
        }
        let below = format!("{path}.");
        let explicit = self
            .explicit
            .range::<str, _>((Bound::Included(below.as_str()), Bound::Unbounded))
            .next()
            .map(|(k, _)| Cow::Borrowed(k.as_str()))
            .filter(|k| k.starts_with(&below));
        // A family at `path` or under it; a member has no dot, so no family
        // above `path` declares a path under it.
        let at = self.families.get_key_value(path);
        let under = self
            .families
            .range::<str, _>((Bound::Included(below.as_str()), Bound::Unbounded))
            .take_while(|(k, _)| k.starts_with(&below));
        let members = at.into_iter().chain(under).filter_map(|(k, origins)| {
            let first = origins.iter().filter_map(|f| f.members.first()).min()?;
            Some(Cow::Owned(format!("{k}.{first}")))
        });
        let first = explicit.into_iter().chain(members).min()?;
        self.get(&first)
    }

    /// Every declared path with its origin, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (Cow<'_, str>, StateOrigin<'_>)> + '_ {
        let explicit = self
            .explicit
            .iter()
            .map(|(k, o)| (Cow::Borrowed(k.as_str()), StateOrigin::At(o)));
        let members = self.families.iter().flat_map(move |(path, origins)| {
            origins.iter().enumerate().flat_map(move |(i, f)| {
                f.members
                    .iter()
                    // A member an explicit entry or an earlier family names
                    // is theirs.
                    .filter(move |m| !origins[..i].iter().any(|e| e.has(m)))
                    .map(move |m| format!("{path}.{m}"))
                    .filter(|key| !self.explicit.contains_key(key))
                    .map(move |key| (Cow::Owned(key), f.origin()))
            })
        });
        explicit.chain(members)
    }

    /// Add every path of `other` this does not declare yet.
    pub(crate) fn merge_first(&mut self, other: &Self) {
        for (path, origin) in &other.explicit {
            if !self.contains_key(path) {
                self.explicit.insert(path.clone(), origin.clone());
            }
        }
        for (path, origins) in &other.families {
            self.families
                .entry(path.clone())
                .or_default()
                .extend(origins.iter().cloned());
        }
    }

    /// Add the `per:` family at `path` over `members` (sorted) whose members
    /// are declared at `at`, its key, after every family already there; an
    /// explicit path or an earlier family keeps a member it names.
    pub(crate) fn add_family_at(&mut self, path: &str, members: Arc<[String]>, at: DeclOrigin) {
        self.families
            .entry(path.to_string())
            .or_default()
            .push(FamilyOrigin {
                members,
                home: FamilyHome::Key(at),
            });
    }

    /// Drop every path `decls` declares.
    pub(crate) fn remove_declared(&mut self, decls: &StateDecls) {
        let mut gone: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for path in decls.explicit().keys() {
            self.explicit.remove(path);
            if let Some((family, member)) = path.rsplit_once('.') {
                gone.entry(family).or_default().insert(member);
            }
        }
        for (path, family) in decls.families() {
            let below = format!("{path}.");
            let named: Vec<String> = self
                .explicit
                .range::<str, _>((Bound::Included(below.as_str()), Bound::Unbounded))
                .take_while(|(k, _)| k.starts_with(&below))
                .filter(|(k, _)| family.member(&k[below.len()..]).is_some())
                .map(|(k, _)| k.clone())
                .collect();
            for k in named {
                self.explicit.remove(&k);
            }
            gone.entry(path.as_str())
                .or_default()
                .extend(family.members().iter().map(String::as_str));
        }
        for (path, members) in gone {
            let Some(origins) = self.families.get_mut(path) else {
                continue;
            };
            for f in origins.iter_mut() {
                if f.members.iter().any(|m| members.contains(m.as_str())) {
                    f.members = f
                        .members
                        .iter()
                        .filter(|m| !members.contains(m.as_str()))
                        .cloned()
                        .collect();
                }
            }
            origins.retain(|f| !f.members.is_empty());
            if origins.is_empty() {
                self.families.remove(path);
            }
        }
    }
}
