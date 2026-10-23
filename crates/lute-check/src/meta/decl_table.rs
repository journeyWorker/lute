//! A state schema's declarations, with `per:` families kept as families
//! (spec 0.49.0 §9.3).
//!
//! `run.bond: { type: int, per: companion }` declares `run.bond.<m>` for
//! every member `m` of `companion`. Until 0.49 each member was its own
//! declaration, so a family over a large kind cost a decl per member in
//! every state that held it. [`StateDecls`] stores the family once — its
//! shared declaration, its members and the members whose `default:` differs
//! — and answers a member path by splitting it. Iteration expands the
//! families lazily, in the byte order the per-member map had.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::{btree_map, BTreeMap, BinaryHeap};
use std::fmt;
use std::iter::Peekable;
use std::ops::Bound;
use std::sync::Arc;

use super::StateDecl;

/// The declarations of a state schema, path → decl: explicit paths and
/// `per:` families. A path an explicit declaration and a family member both
/// name resolves to the one written last, as one per-member map did:
/// [`StateDecls::insert`] after a family shadows its member, and a family
/// inserted with [`Precedence::Replace`] drops the explicit paths it
/// redeclares.
#[derive(Clone, Default)]
pub struct StateDecls {
    explicit: BTreeMap<String, StateDecl>,
    /// Family path → family. Members never contain a dot and are never
    /// empty, so two families never declare the same path.
    families: BTreeMap<String, StateFamily>,
}

/// A `per: <kind>` family: `<path>.<m>` for every member `m` of the kind,
/// sub-kind members included.
#[derive(Clone, Debug, PartialEq)]
pub struct StateFamily {
    kind: String,
    /// The members' declaration; its `default` is the one a member without
    /// its own takes (a scalar `default:`, or a map's `_:` fallback).
    decl: StateDecl,
    /// Sorted and deduplicated.
    members: Arc<[String]>,
    /// The members whose own `default:` differs from [`Self::decl`]'s.
    overrides: BTreeMap<String, StateDecl>,
}

/// What [`StateDecls::insert_family`] does with a member path already
/// declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precedence {
    /// The family's member replaces it (a map `insert`).
    Replace,
    /// The existing declaration stays (a map `entry().or_insert`).
    KeepExisting,
}

impl StateFamily {
    /// A family over `members` (any order, repeats allowed). `decl.default`
    /// is the fallback; `defaults` gives the members with their own value.
    pub fn new(
        kind: String,
        decl: StateDecl,
        members: Arc<[String]>,
        defaults: BTreeMap<String, lute_manifest::types::Literal>,
    ) -> Self {
        let members = if members.windows(2).all(|w| w[0] < w[1]) {
            members
        } else {
            let mut sorted = members.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            sorted.into()
        };
        let overrides = defaults
            .into_iter()
            .filter(|(m, d)| members.binary_search(m).is_ok() && decl.default.as_ref() != Some(d))
            .map(|(m, d)| {
                let decl = StateDecl {
                    default: Some(d),
                    ..decl.clone()
                };
                (m, decl)
            })
            .collect();
        Self {
            kind,
            decl,
            members,
            overrides,
        }
    }

    /// The entity kind the family is indexed by.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The members' declaration, with the fallback default.
    pub fn decl(&self) -> &StateDecl {
        &self.decl
    }

    /// The members, sorted.
    pub fn members(&self) -> &[String] {
        &self.members
    }

    /// The declaration of member `m`, when it is one.
    pub fn member(&self, m: &str) -> Option<&StateDecl> {
        self.members.binary_search_by(|x| x.as_str().cmp(m)).ok()?;
        Some(self.overrides.get(m).unwrap_or(&self.decl))
    }

    /// The same family with `decl` mapped (defaults kept per member).
    pub fn map_decl(&self, f: impl Fn(&StateDecl) -> StateDecl) -> Self {
        Self {
            kind: self.kind.clone(),
            decl: f(&self.decl),
            members: Arc::clone(&self.members),
            overrides: self
                .overrides
                .iter()
                .map(|(m, d)| (m.clone(), f(d)))
                .filter(|(_, d)| *d != f(&self.decl))
                .collect(),
        }
    }

    /// Whether every member can stand as a family member: a dotted or
    /// empty member would make member paths of two families collide.
    fn well_formed(&self) -> bool {
        self.members
            .iter()
            .all(|m| !m.is_empty() && !m.contains('.'))
    }
}

/// `path.member`.
fn member_path(path: &str, member: &str) -> String {
    let mut key = String::with_capacity(path.len() + 1 + member.len());
    key.push_str(path);
    key.push('.');
    key.push_str(member);
    key
}

impl StateDecls {
    /// The declaration of `path`.
    pub fn get(&self, path: &str) -> Option<&StateDecl> {
        self.explicit
            .get(path)
            .or_else(|| self.family_member(path).map(|(_, _, d)| d))
    }

    pub fn contains_key(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    /// The declaration of `path`, to change it; a family member is first
    /// declared explicitly (its family is unchanged).
    pub fn get_mut(&mut self, path: &str) -> Option<&mut StateDecl> {
        if !self.explicit.contains_key(path) {
            let decl = self.family_member(path)?.2.clone();
            self.explicit.insert(path.to_string(), decl);
        }
        self.explicit.get_mut(path)
    }

    /// The family `path` is a member of, with the family's path, when no
    /// explicit declaration shadows it.
    pub fn family_of(&self, path: &str) -> Option<(&str, &StateFamily)> {
        if self.explicit.contains_key(path) {
            return None;
        }
        self.family_member(path).map(|(p, f, _)| (p, f))
    }

    fn family_member(&self, path: &str) -> Option<(&str, &StateFamily, &StateDecl)> {
        let (family, member) = path.rsplit_once('.')?;
        let (p, f) = self.families.get_key_value(family)?;
        Some((p.as_str(), f, f.member(member)?))
    }

    /// Declare `path` explicitly; it shadows a family member of that path.
    pub fn insert(&mut self, path: String, decl: StateDecl) {
        self.explicit.insert(path, decl);
    }

    /// Declare `path` unless it is declared; whether it was inserted.
    pub fn insert_if_absent(&mut self, path: String, decl: StateDecl) -> bool {
        if self.contains_key(&path) {
            return false;
        }
        self.explicit.insert(path, decl);
        true
    }

    /// Declare `family` at `path`. A family without members declares
    /// nothing. A family already at `path` keeps its members as explicit
    /// declarations, which `precedence` then treats like any other.
    pub fn insert_family(&mut self, path: String, family: StateFamily, precedence: Precedence) {
        if family.members.is_empty() {
            return;
        }
        if !family.well_formed() {
            for m in family.members.iter() {
                let decl = family
                    .member(m)
                    .cloned()
                    .unwrap_or_else(|| family.decl.clone());
                let key = member_path(&path, m);
                match precedence {
                    Precedence::Replace => self.insert(key, decl),
                    Precedence::KeepExisting => {
                        self.insert_if_absent(key, decl);
                    }
                }
            }
            return;
        }
        if let Some(old) = self.families.remove(&path) {
            for m in old.members.iter() {
                let decl = old.member(m).unwrap_or(&old.decl);
                self.explicit
                    .entry(member_path(&path, m))
                    .or_insert_with(|| decl.clone());
            }
        }
        if precedence == Precedence::Replace {
            let below = format!("{path}.");
            let shadowed: Vec<String> = self
                .explicit
                .range::<str, _>((Bound::Included(below.as_str()), Bound::Unbounded))
                .take_while(|(k, _)| k.starts_with(&below))
                .filter(|(k, _)| family.member(&k[below.len()..]).is_some())
                .map(|(k, _)| k.clone())
                .collect();
            for k in shadowed {
                self.explicit.remove(&k);
            }
        }
        self.families.insert(path, family);
    }

    /// Add every declaration of `other`, each replacing one of its path.
    pub fn append(&mut self, other: StateDecls) {
        for (path, family) in other.families {
            self.insert_family(path, family, Precedence::Replace);
        }
        self.explicit.extend(other.explicit);
    }

    /// Whether some member of the family `family` at `path` is declared here.
    pub fn declares_member_of(&self, path: &str, family: &StateFamily) -> bool {
        if self.families.contains_key(path) {
            return true;
        }
        self.explicit_members(path, family).next().is_some()
    }

    /// How many members of the family at `path` no explicit declaration
    /// shadows.
    pub fn family_len(&self, path: &str, family: &StateFamily) -> usize {
        family.members.len() - self.explicit_members(path, family).count()
    }

    /// The explicit paths that name a member of `family` at `path`.
    fn explicit_members<'a>(
        &'a self,
        path: &str,
        family: &'a StateFamily,
    ) -> impl Iterator<Item = &'a String> + 'a {
        let below = format!("{path}.");
        let skip = below.len();
        self.explicit
            .range(below.clone()..)
            .take_while(move |(k, _)| k.starts_with(&below))
            .filter(move |(k, _)| family.member(&k[skip..]).is_some())
            .map(|(k, _)| k)
    }

    /// The explicit declarations (each family member excluded).
    pub fn explicit(&self) -> &BTreeMap<String, StateDecl> {
        &self.explicit
    }

    /// The families, by path.
    pub fn families(&self) -> &BTreeMap<String, StateFamily> {
        &self.families
    }

    /// The members of the family at `path` no explicit declaration
    /// shadows, as `(path.member, decl)` in byte order.
    pub fn family_entries<'a>(
        &'a self,
        path: &'a str,
        family: &'a StateFamily,
    ) -> impl Iterator<Item = (String, &'a StateDecl)> + 'a {
        family.members.iter().filter_map(move |m| {
            let key = member_path(path, m);
            if self.explicit.contains_key(&key) {
                return None;
            }
            let decl = family.member(m)?;
            Some((key, decl))
        })
    }

    /// Every declaration, in byte order of the path.
    pub fn iter(&self) -> Entries<'_> {
        self.with_prefix("")
    }

    /// The declarations whose path starts with `prefix`, in byte order of
    /// the path. Only the families that can declare such a path are
    /// visited: those at a path starting with `prefix`, and the one at each
    /// segment boundary of `prefix`, from its first member starting with
    /// the rest of `prefix`.
    pub fn with_prefix<'a>(&'a self, prefix: &str) -> Entries<'a> {
        let under = self
            .families
            .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
            .take_while(|(path, _)| path.starts_with(prefix))
            .filter_map(|(path, family)| Cursor::at(path, family, 0));
        let over = prefix.match_indices('.').filter_map(|(dot, _)| {
            let (path, family) = self.families.get_key_value(&prefix[..dot])?;
            let rest = &prefix[dot + 1..];
            let next = family.members.partition_point(|m| m.as_str() < rest);
            Cursor::at(path, family, next)
        });
        Entries {
            explicit: self
                .explicit
                .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
                .peekable(),
            families: under.chain(over).map(Reverse).collect(),
            prefix: prefix.to_string(),
            done: false,
        }
    }

    /// Every declared path, in byte order.
    pub fn keys(&self) -> impl Iterator<Item = Cow<'_, str>> {
        self.iter().map(|(k, _)| k)
    }

    /// How many paths are declared.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.explicit.is_empty() && self.families.is_empty()
    }
}

impl Extend<(String, StateDecl)> for StateDecls {
    fn extend<T: IntoIterator<Item = (String, StateDecl)>>(&mut self, iter: T) {
        self.explicit.extend(iter);
    }
}

impl FromIterator<(String, StateDecl)> for StateDecls {
    fn from_iter<T: IntoIterator<Item = (String, StateDecl)>>(iter: T) -> Self {
        Self {
            explicit: iter.into_iter().collect(),
            families: BTreeMap::new(),
        }
    }
}

impl<'a> IntoIterator for &'a StateDecls {
    type Item = (Cow<'a, str>, &'a StateDecl);
    type IntoIter = Entries<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Equal when they declare the same paths the same way, however each is
/// stored.
impl PartialEq for StateDecls {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

/// Printed as the per-member map.
impl fmt::Debug for StateDecls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// [`StateDecls::with_prefix`]: the explicit declarations and the families'
/// members, merged in byte order of the path.
pub struct Entries<'a> {
    explicit: Peekable<btree_map::Range<'a, String, StateDecl>>,
    /// The families with members left, least next path first.
    families: BinaryHeap<Reverse<Cursor<'a>>>,
    /// Every path yielded starts with it.
    prefix: String,
    /// A path past the prefix's range was reached.
    done: bool,
}

impl fmt::Debug for Entries<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entries")
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}

/// A family's next member and its path, ordered by the path. Two cursors
/// never share a path: a member has no dot, so a path names one family.
struct Cursor<'a> {
    path: &'a str,
    family: &'a StateFamily,
    next: usize,
    key: String,
}

impl<'a> Cursor<'a> {
    fn at(path: &'a str, family: &'a StateFamily, next: usize) -> Option<Self> {
        let member = family.members.get(next)?;
        Some(Self {
            path,
            family,
            next,
            key: member_path(path, member),
        })
    }

    /// The current member's path and declaration; the cursor moves on, or
    /// is `None` past the last member.
    fn take(self) -> (String, &'a StateDecl, Option<Self>) {
        let member = &self.family.members[self.next];
        let decl = self.family.member(member).unwrap_or(&self.family.decl);
        let rest = Cursor::at(self.path, self.family, self.next + 1);
        (self.key, decl, rest)
    }
}

impl PartialEq for Cursor<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for Cursor<'_> {}

impl PartialOrd for Cursor<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cursor<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

impl<'a> Entries<'a> {
    /// The least family member path's cursor, moved on.
    fn advance_family(&mut self) -> Option<(String, &'a StateDecl)> {
        let Reverse(cursor) = self.families.pop()?;
        let (key, decl, rest) = cursor.take();
        self.families.extend(rest.map(Reverse));
        Some((key, decl))
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = (Cow<'a, str>, &'a StateDecl);

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let order = match (self.explicit.peek(), self.families.peek()) {
            (Some((k, _)), Some(Reverse(c))) => k.as_str().cmp(c.key.as_str()),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => return None,
        };
        let (key, decl) = if order == Ordering::Greater {
            let (key, decl) = self.advance_family()?;
            (Cow::Owned(key), decl)
        } else {
            // An explicit declaration of a member's path shadows it.
            if order == Ordering::Equal {
                self.advance_family();
            }
            let (key, decl) = self.explicit.next()?;
            (Cow::Borrowed(key.as_str()), decl)
        };
        // Every path starts at or after `prefix`, so the first one without
        // it is past the paths with it.
        if !key.starts_with(self.prefix.as_str()) {
            self.done = true;
            return None;
        }
        Some((key, decl))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::Namespace;
    use lute_manifest::types::{Literal, Type};

    fn decl(default: Option<i64>) -> StateDecl {
        StateDecl {
            ty: Type::Int,
            default: default.map(Literal::Int),
            namespace: Namespace::Run,
            owner: None,
        }
    }

    fn family(members: &[&str], fallback: Option<i64>, own: &[(&str, i64)]) -> StateFamily {
        StateFamily::new(
            "k".into(),
            decl(fallback),
            members.iter().map(|m| m.to_string()).collect(),
            own.iter()
                .map(|(m, v)| (m.to_string(), Literal::Int(*v)))
                .collect(),
        )
    }

    /// The per-member map the 0.48.1 expansion built, by the same writes.
    fn reference(
        writes: &[(&str, &[&str], Option<i64>, Precedence)],
    ) -> BTreeMap<String, StateDecl> {
        let mut out = BTreeMap::new();
        for (path, members, default, precedence) in writes {
            let paths: Vec<String> = if members.is_empty() {
                vec![path.to_string()]
            } else {
                members.iter().map(|m| member_path(path, m)).collect()
            };
            for p in paths {
                match precedence {
                    Precedence::Replace => {
                        out.insert(p, decl(*default));
                    }
                    Precedence::KeepExisting => {
                        out.entry(p).or_insert_with(|| decl(*default));
                    }
                }
            }
        }
        out
    }

    /// Families and explicit paths, written in any order, iterate and look
    /// up as the per-member map the same writes built.
    #[test]
    fn families_read_as_the_per_member_map() {
        use Precedence::{KeepExisting, Replace};
        let writes: &[(&str, &[&str], Option<i64>, Precedence)] = &[
            ("run.a.y", &[], Some(9), Replace),
            ("run.a", &["z", "x", "y", "x"], Some(1), Replace),
            ("run.a.x", &[], Some(2), Replace),
            ("run.a-b", &["m"], None, Replace),
            ("run.a.x", &["n", "o"], Some(3), Replace),
            ("run.a", &["x", "w"], Some(4), KeepExisting),
            ("run.a.w", &[], Some(5), KeepExisting),
            ("run.ab", &[], None, Replace),
            ("run", &["a"], Some(6), KeepExisting),
        ];
        let mut decls = StateDecls::default();
        for (path, members, default, precedence) in writes {
            if members.is_empty() {
                match precedence {
                    Replace => decls.insert(path.to_string(), decl(*default)),
                    KeepExisting => {
                        decls.insert_if_absent(path.to_string(), decl(*default));
                    }
                }
            } else {
                decls.insert_family(
                    path.to_string(),
                    family(members, *default, &[]),
                    *precedence,
                );
            }
        }
        let want = reference(writes);
        let got: Vec<(String, StateDecl)> = decls
            .iter()
            .map(|(k, d)| (k.into_owned(), d.clone()))
            .collect();
        let want_rows: Vec<(String, StateDecl)> = want.clone().into_iter().collect();
        assert_eq!(got, want_rows);
        for (path, d) in &want {
            assert_eq!(decls.get(path), Some(d), "{path}");
        }
        assert_eq!(decls.get("run.a.q"), None);
        for prefix in [
            "", "r", "run", "run.", "run.a", "run.a.", "run.a.x", "run.a.x.", "run.a.xx",
            "run.a.y", "run.a.w", "run.a-", "run.ab", "s",
        ] {
            let got: Vec<String> = decls
                .with_prefix(prefix)
                .map(|(k, _)| k.into_owned())
                .collect();
            let want: Vec<String> = want
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned()
                .collect();
            assert_eq!(got, want, "prefix {prefix:?}");
        }
    }

    /// A member's own `default:` is its own; the rest take the fallback.
    #[test]
    fn member_defaults() {
        let f = family(&["a", "b", "c"], Some(0), &[("b", 2), ("c", 0), ("zz", 1)]);
        assert_eq!(
            f.member("a").and_then(|d| d.default.clone()),
            Some(Literal::Int(0))
        );
        assert_eq!(
            f.member("b").and_then(|d| d.default.clone()),
            Some(Literal::Int(2))
        );
        assert_eq!(
            f.member("c").and_then(|d| d.default.clone()),
            Some(Literal::Int(0))
        );
        assert_eq!(f.member("zz"), None);
        assert_eq!(f.overrides.len(), 1, "only `b` differs from the fallback");
    }

    /// A dotted member cannot be a family member path: it is declared
    /// member by member.
    #[test]
    fn dotted_members_are_explicit() {
        let mut decls = StateDecls::default();
        decls.insert_family(
            "run.a".into(),
            family(&["x.y", "z"], None, &[]),
            Precedence::Replace,
        );
        assert!(decls.families().is_empty());
        assert_eq!(decls.keys().collect::<Vec<_>>(), ["run.a.x.y", "run.a.z"]);
    }
}
