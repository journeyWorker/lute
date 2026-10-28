//! dsl 0.37.0 §4 / §7: the directive surface renamed or removed by the 0.37
//! clean cutover. Every old spelling is diagnosed with a message naming the
//! new form — the old spelling is never read as an alias. The lossless ones
//! are also rewritten by `lute fix` (`crate::fix`), which reads the SAME
//! tables so the diagnostic and the codemod cannot drift apart.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::stamps::StampSurface;
use lute_syntax::ast::{Attr, Directive};

/// `E-RENAMED-DIRECTIVE`: an old directive tag (`::auto`, `::cut`, `::next`,
/// `::mark`); the message names the new tag.
pub const E_RENAMED_DIRECTIVE: &str = "E-RENAMED-DIRECTIVE";
/// `E-RENAMED-ATTR`: an old directive or content-line attribute key.
pub use crate::content_line::E_RENAMED_ATTR;
/// `E-REMOVED-ATTR`: a removed attribute with no lossless rewrite.
pub const E_REMOVED_ATTR: &str = "E-REMOVED-ATTR";
/// `E-CAMERA-REMOVED`: a removed numeric/legacy camera attribute.
pub const E_CAMERA_REMOVED: &str = "E-CAMERA-REMOVED";
/// `E-CAMERA-EMPTY`: a `::camera` with none of `focus`/`framing`/`move`/
/// `transition`.
pub const E_CAMERA_EMPTY: &str = "E-CAMERA-EMPTY";
/// `E-CG-LAYOUT`: the removed `::cg{full}`, which only a project `cgLayout`
/// member can replace.
pub const E_CG_LAYOUT: &str = "E-CG-LAYOUT";

/// Renamed directive tags: `(old tag, new tag)`.
pub const RENAMED_DIRECTIVES: &[(&str, &str)] = &[
    ("auto", lute_manifest::core::ACTOR_DIRECTIVE),
    ("cut", "cg"),
    ("next", lute_manifest::core::JUMP_DIRECTIVE),
    ("mark", lute_manifest::core::LABEL_DIRECTIVE),
];

/// Renamed directive attributes, keyed on the CURRENT tag:
/// `(tag, old key, new key)`. `lute fix` also applies the entry to a
/// directive written under the tag's old name (`::cut{action}` →
/// `::cg{display}`, `::mark{id}` → `::label{name}`).
pub const RENAMED_DIRECTIVE_ATTRS: &[(&str, &str, &str)] = &[
    ("music", "action", "playback"),
    ("video", "action", "display"),
    ("cg", "action", "display"),
    (
        lute_manifest::core::LABEL_DIRECTIVE,
        "id",
        lute_manifest::core::LABEL_NAME_ATTR,
    ),
];

/// A domain (enum or entity kind) renamed by a clean cutover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenamedDomain {
    /// The name a project written before `since` declares.
    pub old: &'static str,
    /// The name the toolchain reads since `since`.
    pub new: &'static str,
    /// The release that renamed it.
    pub since: &'static str,
}

/// Renamed domains (0.41.0 §3.1). The only source for both `lute fix`'s YAML
/// domain rename and the `E-DOMAIN-UNKNOWN` rename hint.
pub const RENAMED_DOMAINS: &[RenamedDomain] = &[RenamedDomain {
    old: "musicAction",
    new: "musicPlayback",
    since: "0.37.0",
}];

/// The tail an `E-DOMAIN-UNKNOWN` for `name` appends when `name` is a renamed
/// domain's new name and the document's folded vocabulary `declared` still
/// declares its old name (0.41.0 §3.1); empty otherwise.
pub fn renamed_domain_hint<V>(
    name: &str,
    declared: &std::collections::BTreeMap<String, V>,
) -> String {
    RENAMED_DOMAINS
        .iter()
        .find(|r| r.new == name && declared.contains_key(r.old))
        .map(|r| {
            format!(
                " — this project declares `{}`, renamed `{}` in {}; run `lute fix`",
                r.old, r.new, r.since
            )
        })
        .unwrap_or_default()
}

/// Removed directive attributes that need a hand migration:
/// `(tag, key, code, remedy)`.
const REMOVED_DIRECTIVE_ATTRS: &[(&str, &str, &str, &str)] = &[
    (
        "music",
        "track",
        E_REMOVED_ATTR,
        "use `assetId` for the concrete asset; hand migration is required",
    ),
    (
        "sfx",
        "name",
        E_REMOVED_ATTR,
        "use `sound` for the semantic sound and/or `assetId` for the concrete asset; hand \
         migration is required",
    ),
    (
        "camera",
        "zoom",
        E_CAMERA_REMOVED,
        "declare a `framing` member and write `framing=…`; hand migration is required",
    ),
    (
        "camera",
        "moveX",
        E_CAMERA_REMOVED,
        "declare a `cameraMove` member and write `move=…`; hand migration is required",
    ),
    (
        "camera",
        "moveY",
        E_CAMERA_REMOVED,
        "declare a `cameraMove` member and write `move=…`; hand migration is required",
    ),
    (
        "camera",
        "shake",
        E_CAMERA_REMOVED,
        "declare a `cameraMove` member (e.g. `shake`) and write `move=…`; hand migration is \
         required",
    ),
    (
        "camera",
        "reset",
        E_CAMERA_REMOVED,
        "declare a `framing` member for the resting shot and write `framing=…`; hand migration \
         is required",
    ),
    (
        "camera",
        "easing",
        E_CAMERA_REMOVED,
        "declare a `transition` member and write `transition=…`; hand migration is required",
    ),
    (
        "cg",
        "full",
        E_CG_LAYOUT,
        "declare a `cgLayout` member and write `layout=…`; hand migration is required",
    ),
];

/// The attributes of which a `::camera` needs at least one.
const CAMERA_SLOTS: &[&str] = &["focus", "framing", "move", "transition"];

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The new tag of a renamed directive tag.
pub fn renamed_directive(tag: &str) -> Option<&'static str> {
    RENAMED_DIRECTIVES
        .iter()
        .find(|(old, _)| *old == tag)
        .map(|(_, new)| *new)
}

/// The current name of `key` on `::tag` when `key` is a renamed core
/// attribute (dsl 0.37.0 §4) AND the snapshot does not admit `key` on
/// `::tag` — neither the directive's own attribute nor a stamp on its
/// surface (plugin §14.1, 0.50.0 §3.4). `None` otherwise.
///
/// The one predicate for an old attribute spelling (spec 0.53.0 §4.1): the
/// checker's `E-RENAMED-ATTR`, its default and required-attribute judgments,
/// and `lute fix`'s rule `renamed-attr` all ask it, so a key the active
/// profile declares as a stamp is never read, reported or rewritten as the
/// 0.37 spelling.
pub fn renamed_attr(tag: &str, key: &str, snapshot: &CapabilitySnapshot) -> Option<&'static str> {
    renamed_key(tag, key).filter(|_| !admits_attr(tag, key, snapshot))
}

/// The new key of `key` on `::tag` in [`RENAMED_DIRECTIVE_ATTRS`], whatever
/// the snapshot admits. `lute fix` reads it to report the sites
/// [`renamed_attr`] keeps.
pub(crate) fn renamed_key(tag: &str, key: &str) -> Option<&'static str> {
    RENAMED_DIRECTIVE_ATTRS
        .iter()
        .find(|(t, old, _)| *t == tag && *old == key)
        .map(|(_, _, new)| *new)
}

/// Whether `snapshot` admits `key` on `::tag`: an attribute of the
/// directive's own declaration, or a stamp admissible on its surface (a
/// directive with no stamp carrier admits none, 0.49.0 §4.1).
fn admits_attr(tag: &str, key: &str, snapshot: &CapabilitySnapshot) -> bool {
    let own = snapshot
        .directive(tag)
        .is_some_and(|decl| decl.attrs.iter().any(|a| a.name == key));
    own || (!lute_manifest::core::UNSTAMPED_DIRECTIVES.contains(&tag)
        && snapshot
            .stamp_attrs
            .get(&StampSurface::Directive(tag.to_string()), key)
            .is_some())
}

/// `E-RENAMED-DIRECTIVE` for an old tag no snapshot declares. The hint
/// naming the tag's renamed attribute judges it on the new tag's surface
/// ([`renamed_attr`]).
pub fn renamed_directive_diag(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
) -> Option<Diagnostic> {
    let new = renamed_directive(&dir.tag)?;
    let attrs = dir
        .attrs
        .iter()
        .find_map(|a| Some((a.key.as_str(), renamed_attr(new, &a.key, snapshot)?)))
        .map(|(old, key)| format!(" (its `{old}=` is now `{key}=`)"))
        .unwrap_or_default();
    Some(diag(
        E_RENAMED_DIRECTIVE,
        format!(
            "`::{}` is now `::{new}`{attrs} (dsl 0.37.0 §2.2) — `lute fix` rewrites it",
            dir.tag
        ),
        dir.span,
    ))
}

/// The diagnostic for a renamed or removed attribute of `tag`, if `attr` is
/// one.
pub fn attr_diag(tag: &str, attr: &Attr, snapshot: &CapabilitySnapshot) -> Option<Diagnostic> {
    if let Some(new) = renamed_attr(tag, &attr.key, snapshot) {
        return Some(diag(
            E_RENAMED_ATTR,
            format!(
                "`::{tag}{{{old}=…}}` is now `{new}=` (dsl 0.37.0 §2.2) — `lute fix` rewrites it",
                old = attr.key
            ),
            attr.span,
        ));
    }
    let (_, key, code, remedy) = REMOVED_DIRECTIVE_ATTRS
        .iter()
        .find(|(t, k, _, _)| *t == tag && *k == attr.key)?;
    Some(diag(
        code,
        format!("`::{tag}` no longer takes `{key}` (dsl 0.37.0 §3.3): {remedy}"),
        attr.span,
    ))
}

/// `E-CAMERA-EMPTY` for a `::camera` naming none of its slots. A camera whose
/// only attrs are removed ones already draws `E-CAMERA-REMOVED` per attr, so
/// it is not reported a second time.
pub fn camera_empty(dir: &Directive) -> Option<Diagnostic> {
    let removed = |a: &Attr| {
        REMOVED_DIRECTIVE_ATTRS
            .iter()
            .any(|(t, k, _, _)| *t == "camera" && *k == a.key)
    };
    (dir.tag == "camera"
        && !dir
            .attrs
            .iter()
            .any(|a| CAMERA_SLOTS.contains(&a.key.as_str()) || removed(a)))
    .then(|| {
        diag(
            E_CAMERA_EMPTY,
            "`::camera` needs at least one of `focus`, `framing`, `move` or `transition` \
             (dsl 0.37.0 §3.3)"
                .to_string(),
            dir.span,
        )
    })
}
