//! Plugin stamp attributes and the surfaces they apply to (0.50.0 §3).
//!
//! A `stampAttrs` entry is an [`AttrDecl`] plus an optional `on:` list of
//! surface names. Without `on:` the entry is *cross-cutting*, as in 0.49: it
//! is admissible on every directive with a stamp carrier and on every content
//! line, never on `<branch>`, `<hub>` or `<choice>`. With `on:` it is
//! admissible exactly on the named surfaces. The loader keeps the raw names
//! ([`DeclaredStampAttr`]); assembly resolves them against the active
//! snapshot ([`StampSurface::parse`]) and indexes the result once per snapshot
//! in [`StampAttrs`], which every reader consults through a surface.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use sha2::{Digest, Sha256};

use crate::schema::AttrDecl;
use crate::snapshot::CapabilitySnapshot;

/// The four `on:` names that always mean a core surface. A plugin directive
/// tagged `line` cannot be named in `on:`; `branch`, `hub` and `choice` are
/// reserved plugin directive names already (`E-PLUGIN-RESERVED-NAME`).
pub const CORE_SURFACES: [&str; 4] = ["line", "branch", "hub", "choice"];

/// A place an author writes attributes on, as a stamp's `on:` names it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StampSurface {
    /// Every content line (`@speaker{…}: text`), wherever it is written.
    Line,
    /// The `<branch>` element.
    Branch,
    /// The `<hub>` element.
    Hub,
    /// Every `<choice>`, in a `<branch>` or a `<hub>`.
    Choice,
    /// A directive of the resolved snapshot that has a stamp carrier, by tag.
    Directive(String),
}

/// Why an `on:` name is not a stampable surface (`E-PLUGIN-STAMP-SURFACE`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SurfaceError {
    /// Neither a core surface nor a directive of the resolved snapshot.
    Unknown {
        /// The nearest surface name of the snapshot, for a did-you-mean.
        near: Option<String>,
        /// The installed but inactive plugin that declares a directive of
        /// this tag, when one does.
        inactive_owner: Option<String>,
    },
    /// A directive with no stamp carrier
    /// ([`crate::core::UNSTAMPED_DIRECTIVES`], 0.49.0 §4.1).
    Unstamped,
}

impl StampSurface {
    /// Resolve one `on:` name against the resolved snapshot.
    ///
    /// # Errors
    ///
    /// [`SurfaceError::Unknown`] for a name that is neither a core surface
    /// nor a directive of `snapshot`, and [`SurfaceError::Unstamped`] for a
    /// directive with no stamp carrier.
    pub fn parse(name: &str, snapshot: &CapabilitySnapshot) -> Result<Self, SurfaceError> {
        match name {
            "line" => return Ok(Self::Line),
            "branch" => return Ok(Self::Branch),
            "hub" => return Ok(Self::Hub),
            "choice" => return Ok(Self::Choice),
            _ => {}
        }
        if snapshot.directive(name).is_some() {
            if crate::core::UNSTAMPED_DIRECTIVES.contains(&name) {
                return Err(SurfaceError::Unstamped);
            }
            return Ok(Self::Directive(name.to_string()));
        }
        let candidates = CORE_SURFACES.into_iter().chain(
            snapshot
                .directives
                .keys()
                .map(String::as_str)
                .filter(|tag| !crate::core::UNSTAMPED_DIRECTIVES.contains(tag)),
        );
        Err(SurfaceError::Unknown {
            near: crate::suggest::nearest(name, candidates, 2).map(str::to_string),
            inactive_owner: snapshot.inactive.get(name).cloned(),
        })
    }

    /// The name `on:` spells this surface with: `line`, `branch`, `bg`.
    pub fn name(&self) -> &str {
        match self {
            Self::Line => "line",
            Self::Branch => "branch",
            Self::Hub => "hub",
            Self::Choice => "choice",
            Self::Directive(tag) => tag,
        }
    }

    /// Whether a cross-cutting entry (no `on:`) applies here: content lines
    /// and directives, never the logic elements (0.50.0 §3.1).
    pub fn admits_cross_cutting(&self) -> bool {
        matches!(self, Self::Line | Self::Directive(_))
    }

    /// The surface as an author reads it in a sentence: `content lines`,
    /// `` `<branch>` ``, `` `::bg` ``.
    pub fn quoted(&self) -> String {
        match self {
            Self::Line => "content lines".to_string(),
            _ => format!("`{self}`"),
        }
    }

    /// The attributes the surface itself owns, which a scoped stamp may not
    /// shadow (0.50.0 §3.2 rule 3). `None` for a directive the snapshot
    /// does not hold.
    pub fn own_attrs<'a>(&'a self, snapshot: &'a CapabilitySnapshot) -> Option<Vec<&'a str>> {
        use crate::core::{
            BRANCH_ATTRS, BRANCH_CHOICE_ATTRS, CHOICE_RETIRED_ATTRS, HUB_ATTRS, HUB_CHOICE_ATTRS,
            LINE_ATTRS, LINE_GUARD_ATTR, LINE_RETIRED_ATTRS,
        };
        Some(match self {
            Self::Line => LINE_ATTRS
                .iter()
                .chain(LINE_RETIRED_ATTRS)
                .copied()
                .chain([LINE_GUARD_ATTR])
                .collect(),
            Self::Branch => BRANCH_ATTRS.to_vec(),
            Self::Hub => HUB_ATTRS.to_vec(),
            Self::Choice => {
                let mut own: Vec<&str> = BRANCH_CHOICE_ATTRS
                    .iter()
                    .chain(HUB_CHOICE_ATTRS)
                    .chain(CHOICE_RETIRED_ATTRS)
                    .copied()
                    .collect();
                own.sort_unstable();
                own.dedup();
                own
            }
            Self::Directive(tag) => {
                let decl = snapshot.directive(tag)?;
                let mut own: Vec<&str> = decl.attrs.iter().map(|a| a.name.as_str()).collect();
                if tag == crate::core::JUMP_DIRECTIVE {
                    own.push("when");
                }
                own
            }
        })
    }
}

impl fmt::Display for StampSurface {
    /// The author's spelling of the surface: `content lines`, `<branch>`,
    /// `::bg`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Line => f.write_str("content lines"),
            Self::Branch => f.write_str("<branch>"),
            Self::Hub => f.write_str("<hub>"),
            Self::Choice => f.write_str("<choice>"),
            Self::Directive(tag) => write!(f, "::{tag}"),
        }
    }
}

/// A `stampAttrs` entry as one package declares it, before assembly: its
/// declaration and the raw `on:` names (`None`: cross-cutting).
#[derive(Clone, Debug)]
pub struct DeclaredStampAttr {
    pub decl: AttrDecl,
    pub on: Option<BTreeSet<String>>,
}

/// One assembled stamp declaration of [`StampAttrs`].
#[derive(Clone, Debug)]
pub struct StampAttrDecl {
    pub decl: AttrDecl,
    /// The surfaces it applies to; `None` for a cross-cutting entry.
    pub on: Option<BTreeSet<StampSurface>>,
    /// The declaring plugin's id.
    pub plugin: String,
}

impl StampAttrDecl {
    /// A cross-cutting declaration (no `on:`) of `plugin`.
    pub fn cross_cutting(plugin: impl Into<String>, decl: AttrDecl) -> Self {
        Self {
            decl,
            on: None,
            plugin: plugin.into(),
        }
    }

    /// A declaration of `plugin` scoped to `on`.
    pub fn scoped(
        plugin: impl Into<String>,
        decl: AttrDecl,
        on: impl IntoIterator<Item = StampSurface>,
    ) -> Self {
        Self {
            decl,
            on: Some(on.into_iter().collect()),
            plugin: plugin.into(),
        }
    }

    /// Whether the declaration applies to `surface`.
    pub fn admits(&self, surface: &StampSurface) -> bool {
        match &self.on {
            Some(on) => on.contains(surface),
            None => surface.admits_cross_cutting(),
        }
    }
}

/// Where two declarations of one name overlap: `None` when disjoint, else
/// the shared surfaces — empty when both are cross-cutting, so the whole
/// name is declared twice. An entry without `on:` overlaps every entry
/// (0.50.0 §3.2); against a scoped one it shares that one's surfaces.
pub fn overlap<T: Ord + Clone>(a: Option<&BTreeSet<T>>, b: Option<&BTreeSet<T>>) -> Option<Vec<T>> {
    match (a, b) {
        (None, None) => Some(Vec::new()),
        (None, Some(on)) | (Some(on), None) => Some(on.iter().cloned().collect()),
        (Some(a), Some(b)) => {
            let shared: Vec<T> = a.intersection(b).cloned().collect();
            (!shared.is_empty()).then_some(shared)
        }
    }
}

/// [`StampAttrs::insert`] refused a declaration that overlaps one already
/// held.
#[derive(Clone, Debug, PartialEq)]
pub struct StampOverlap {
    /// The plugin of the declaration already held.
    pub first: String,
    /// The shared surfaces; empty when both declarations are cross-cutting.
    pub shared: Vec<StampSurface>,
}

/// The assembled `stampAttrs` of a snapshot, grouped by name (0.50.0 §3.3).
///
/// The declarations of one name never overlap: either one cross-cutting
/// declaration, or scoped declarations with disjoint surfaces. So each
/// (surface, name) pair resolves to at most one declaration.
#[derive(Clone, Debug, Default)]
pub struct StampAttrs {
    by_name: BTreeMap<String, Vec<StampAttrDecl>>,
}

impl StampAttrs {
    /// Add `decl`, unless it overlaps a declaration of its name already
    /// held.
    ///
    /// # Errors
    ///
    /// [`StampOverlap`] naming the held declaration's plugin and the shared
    /// surfaces; `decl` is not added.
    pub fn insert(&mut self, decl: StampAttrDecl) -> Result<(), StampOverlap> {
        let held = self.by_name.entry(decl.decl.name.clone()).or_default();
        if let Some((first, shared)) = held
            .iter()
            .find_map(|h| Some((h, overlap(h.on.as_ref(), decl.on.as_ref())?)))
        {
            return Err(StampOverlap {
                first: first.plugin.clone(),
                shared,
            });
        }
        held.push(decl);
        held.sort_by(|a, b| a.on.cmp(&b.on));
        Ok(())
    }

    /// The declaration of `name` that applies to `surface`.
    pub fn entry(&self, surface: &StampSurface, name: &str) -> Option<&StampAttrDecl> {
        self.by_name.get(name)?.iter().find(|d| d.admits(surface))
    }

    /// The [`AttrDecl`] of `name` on `surface`: the entry whose `on:` holds
    /// `surface`, else the cross-cutting entry when `surface` is a content
    /// line or a directive.
    pub fn get(&self, surface: &StampSurface, name: &str) -> Option<&AttrDecl> {
        self.entry(surface, name).map(|d| &d.decl)
    }

    /// Every declaration admissible on `surface`, by name.
    pub fn for_surface<'a>(
        &'a self,
        surface: &'a StampSurface,
    ) -> impl Iterator<Item = &'a StampAttrDecl> + 'a {
        self.iter().filter(move |d| d.admits(surface))
    }

    /// The scoped declarations of `name` that do not admit `surface`.
    pub fn scoped_elsewhere(&self, surface: &StampSurface, name: &str) -> Vec<&StampAttrDecl> {
        self.by_name.get(name).map_or_else(Vec::new, |decls| {
            decls
                .iter()
                .filter(|d| d.on.is_some() && !d.admits(surface))
                .collect()
        })
    }

    /// Every declaration, by name, cross-cutting first within a name.
    pub fn iter(&self) -> impl Iterator<Item = &StampAttrDecl> {
        self.by_name.values().flatten()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Every declared name, once.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    /// Whether any declaration, on any surface, has this name.
    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Fold into a `capabilitySnapshot` hash: per name and per entry,
    /// `name=<AttrDecl Debug>`, then `@on=[…]` (sorted surface names) for a
    /// scoped entry only, then `;`. A snapshot with no `on:` therefore
    /// hashes exactly as in 0.49 (0.50.0 §3.3).
    pub(crate) fn fold_into(&self, h: &mut Sha256) {
        for d in self.iter() {
            h.update(d.decl.name.as_bytes());
            h.update(b"=");
            h.update(format!("{:?}", d.decl).as_bytes());
            if let Some(on) = &d.on {
                let mut names: Vec<&str> = on.iter().map(StampSurface::name).collect();
                names.sort_unstable();
                h.update(format!("@on=[{}]", names.join(", ")).as_bytes());
            }
            h.update(b";");
        }
    }
}

impl FromIterator<StampAttrDecl> for StampAttrs {
    /// Collect declarations; one overlapping a declaration already
    /// collected is a broken invariant of the caller (assembly drops
    /// overlaps before they reach a snapshot).
    fn from_iter<I: IntoIterator<Item = StampAttrDecl>>(iter: I) -> Self {
        let mut out = Self::default();
        for d in iter {
            let name = d.decl.name.clone();
            if let Err(e) = out.insert(d) {
                panic!(
                    "stamp attribute `{name}` overlaps `{}`'s declaration; assembly refuses \
                     overlapping declarations before they are collected",
                    e.first
                );
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    fn decl(name: &str, ty: Type) -> AttrDecl {
        AttrDecl {
            name: name.to_string(),
            required: false,
            ty,
            default: None,
        }
    }

    fn bg() -> StampSurface {
        StampSurface::Directive("bg".to_string())
    }

    #[test]
    fn get_resolves_by_surface_and_keeps_cross_cutting_off_the_logic_elements() {
        let mut stamps = StampAttrs::default();
        stamps
            .insert(StampAttrDecl::scoped(
                "p",
                decl("assetName", Type::Str),
                [bg()],
            ))
            .unwrap();
        stamps
            .insert(StampAttrDecl::scoped(
                "p",
                decl("assetName", Type::Int),
                [StampSurface::Directive("actor".to_string())],
            ))
            .unwrap();
        stamps
            .insert(StampAttrDecl::cross_cutting("p", decl("cue", Type::Str)))
            .unwrap();
        assert_eq!(stamps.get(&bg(), "assetName").unwrap().ty, Type::Str);
        let actor = StampSurface::Directive("actor".to_string());
        assert_eq!(stamps.get(&actor, "assetName").unwrap().ty, Type::Int);
        assert!(stamps.get(&StampSurface::Line, "assetName").is_none());
        assert!(stamps.get(&StampSurface::Line, "cue").is_some());
        assert!(stamps.get(&bg(), "cue").is_some());
        for logic in [
            StampSurface::Branch,
            StampSurface::Hub,
            StampSurface::Choice,
        ] {
            assert!(stamps.get(&logic, "cue").is_none(), "{logic}");
            assert!(stamps.scoped_elsewhere(&logic, "cue").is_empty());
        }
        let elsewhere = stamps.scoped_elsewhere(&StampSurface::Line, "assetName");
        assert_eq!(elsewhere.len(), 2);
        assert_eq!(stamps.names().collect::<Vec<_>>(), ["assetName", "cue"]);
    }

    #[test]
    fn insert_refuses_overlap_and_names_the_shared_surfaces() {
        let mut stamps = StampAttrs::default();
        stamps
            .insert(StampAttrDecl::scoped(
                "a",
                decl("x", Type::Str),
                [bg(), StampSurface::Line],
            ))
            .unwrap();
        let err = stamps
            .insert(StampAttrDecl::scoped("b", decl("x", Type::Str), [bg()]))
            .unwrap_err();
        assert_eq!(err.first, "a");
        assert_eq!(err.shared, [bg()]);
        let err = stamps
            .insert(StampAttrDecl::cross_cutting("b", decl("x", Type::Str)))
            .unwrap_err();
        assert_eq!(err.shared, [StampSurface::Line, bg()]);
        stamps
            .insert(StampAttrDecl::scoped(
                "b",
                decl("x", Type::Str),
                [StampSurface::Choice],
            ))
            .unwrap();
        let mut both = StampAttrs::default();
        both.insert(StampAttrDecl::cross_cutting("a", decl("y", Type::Str)))
            .unwrap();
        let err = both
            .insert(StampAttrDecl::cross_cutting("b", decl("y", Type::Str)))
            .unwrap_err();
        assert!(err.shared.is_empty());
    }

    #[test]
    fn surfaces_parse_against_the_snapshot() {
        let snap = crate::core::load_core_snapshot();
        assert_eq!(StampSurface::parse("line", &snap), Ok(StampSurface::Line));
        assert_eq!(StampSurface::parse("bg", &snap), Ok(bg()));
        assert_eq!(
            StampSurface::parse("jump", &snap),
            Err(SurfaceError::Unstamped)
        );
        assert_eq!(
            StampSurface::parse("bgg", &snap),
            Err(SurfaceError::Unknown {
                near: Some("bg".to_string()),
                inactive_owner: None,
            })
        );
        assert_eq!(StampSurface::Line.to_string(), "content lines");
        assert_eq!(StampSurface::Branch.quoted(), "`<branch>`");
        assert_eq!(bg().quoted(), "`::bg`");
    }

    /// Every core directive is a stamp surface exactly when it has a stamp
    /// carrier: those in `UNSTAMPED_DIRECTIVES` are refused, every other one
    /// resolves.
    #[test]
    fn every_core_directive_is_classified() {
        let snap = crate::core::load_core_snapshot();
        for tag in snap.directives.keys() {
            let parsed = StampSurface::parse(tag, &snap);
            if crate::core::UNSTAMPED_DIRECTIVES.contains(&tag.as_str()) {
                assert_eq!(parsed, Err(SurfaceError::Unstamped), "{tag}");
            } else {
                assert_eq!(parsed, Ok(StampSurface::Directive(tag.clone())), "{tag}");
            }
        }
        for tag in crate::core::UNSTAMPED_DIRECTIVES {
            assert!(snap.directive(tag).is_some(), "{tag} is a core directive");
        }
    }
}
