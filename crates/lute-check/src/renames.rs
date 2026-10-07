//! dsl 0.37.0 §4 / §7: the directive surface renamed or removed by the 0.37
//! clean cutover. Every old spelling is diagnosed with a message naming the
//! new form — the old spelling is never read as an alias. The lossless ones
//! are also rewritten by `lute fix` (`crate::fix`), which reads the SAME
//! tables so the diagnostic and the codemod cannot drift apart.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
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
    (lute_manifest::core::LABEL_DIRECTIVE, "id", lute_manifest::core::LABEL_NAME_ATTR),
];

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

/// The new key of a renamed attribute of `tag` (the current tag).
pub fn renamed_directive_attr(tag: &str, key: &str) -> Option<&'static str> {
    RENAMED_DIRECTIVE_ATTRS
        .iter()
        .find(|(t, old, _)| *t == tag && *old == key)
        .map(|(_, _, new)| *new)
}

/// `E-RENAMED-DIRECTIVE` for an old tag no snapshot declares.
pub fn renamed_directive_diag(dir: &Directive) -> Option<Diagnostic> {
    let new = renamed_directive(&dir.tag)?;
    let attrs = RENAMED_DIRECTIVE_ATTRS
        .iter()
        .find(|(t, _, _)| *t == new)
        .filter(|(_, old, _)| dir.attrs.iter().any(|a| a.key == *old))
        .map(|(_, old, key)| format!(" (its `{old}=` is now `{key}=`)"))
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
pub fn attr_diag(tag: &str, attr: &Attr) -> Option<Diagnostic> {
    if let Some(new) = renamed_directive_attr(tag, &attr.key) {
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
