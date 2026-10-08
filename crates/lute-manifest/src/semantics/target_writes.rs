//! dsl 0.28.0 §3: writing through `occasion.target`.

/// A `::set` path's `[occasion.target]` index.
pub const TARGET_INDEX: &str = "[occasion.target]";

/// The family a `::set` path indexes by `occasion.target` — `run.count` for
/// `run.count[occasion.target]` — or `None` for any other path.
pub fn indexed_family(path: &str) -> Option<&str> {
    path.strip_suffix(TARGET_INDEX).filter(|f| !f.is_empty())
}

/// `path` with its `[occasion.target]` index read as `member`
/// (`run.count[occasion.target]` → `run.count.cod`); any other path as is.
pub fn member_path(path: &str, member: &str) -> String {
    match indexed_family(path) {
        Some(family) => format!("{family}.{member}"),
        None => path.to_string(),
    }
}
