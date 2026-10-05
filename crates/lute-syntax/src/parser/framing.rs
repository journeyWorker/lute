//! Document framing helpers: frontmatter-derived parser policy.
//!
//! Keeping these small, representation-preserving helpers separate from the
//! body grammar makes the document envelope policy independently reviewable.

/// The literal value of top-level frontmatter key `key` (`kind`, `component`),
/// when written.
pub(super) fn frontmatter_scalar(raw_yaml: &str, key: &str) -> Option<String> {
    raw_yaml.lines().find_map(|l| {
        let v = l.strip_prefix(key)?.strip_prefix(':')?.trim();
        let v = v.trim_matches(|c| c == '"' || c == '\'');
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// The component name of frontmatter that declares a `beat:` header template.
pub(super) fn template_component(raw_yaml: &str) -> Option<String> {
    raw_yaml
        .lines()
        .any(|l| l.starts_with("beat:"))
        .then(|| frontmatter_scalar(raw_yaml, "component"))
        .flatten()
}


/// The source span used when a document has no frontmatter envelope.
pub(super) fn zero_span() -> lute_core_span::Span {
    lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}