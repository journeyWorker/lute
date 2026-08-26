//! Minimal project-root-relative glob matcher for `lute.lint.yaml`'s
//! `ignore:` list.
//!
//! Supports two wildcards, sufficient for the "drafts/**", "**/*.lute",
//! "chapters/prologue/*.lute" style patterns real projects use:
//! - `*` — any run of characters WITHIN one path segment (no `/`).
//! - `**` — any run of characters INCLUDING `/`.
//!
//! Every other character matches literally. Paths are normalized to
//! forward-slash-separated relative strings before matching; callers strip
//! the project root prefix themselves.

/// True iff `path` (project-root-relative, forward-slash-separated) matches
/// `pattern`. Non-recursive back-tracking matcher — patterns are short and
/// paths shallow, so the extra generality of a full glob crate is not worth
/// the dependency (spec §Constraints "no new external dependencies").
pub fn matches(pattern: &str, path: &str) -> bool {
    match_from(pattern.as_bytes(), 0, path.as_bytes(), 0)
}

fn match_from(pat: &[u8], mut pi: usize, s: &[u8], mut si: usize) -> bool {
    while pi < pat.len() {
        let c = pat[pi];
        if c == b'*' {
            // Distinguish `*` from `**`.
            let double = pi + 1 < pat.len() && pat[pi + 1] == b'*';
            let next = pi + if double { 2 } else { 1 };
            // Try every split point for the wildcard, growing greedily.
            let mut probe = si;
            loop {
                if match_from(pat, next, s, probe) {
                    return true;
                }
                if probe >= s.len() {
                    return false;
                }
                // `*` stops at a path separator; `**` swallows it.
                if !double && s[probe] == b'/' {
                    return false;
                }
                probe += 1;
            }
        }
        if si >= s.len() {
            return false;
        }
        if pat[pi] != s[si] {
            return false;
        }
        pi += 1;
        si += 1;
    }
    si == s.len()
}

/// True iff `path` matches ANY of `patterns`. Empty list → never.
pub fn matches_any(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|p| matches(p, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_match() {
        assert!(matches("drafts/prologue.lute", "drafts/prologue.lute"));
        assert!(!matches("drafts/prologue.lute", "drafts/other.lute"));
    }

    #[test]
    fn star_stops_at_slash() {
        assert!(matches("drafts/*.lute", "drafts/foo.lute"));
        assert!(!matches("drafts/*.lute", "drafts/sub/foo.lute"));
    }

    #[test]
    fn double_star_matches_dirs() {
        assert!(matches("drafts/**", "drafts/foo.lute"));
        assert!(matches("drafts/**", "drafts/sub/dir/foo.lute"));
        assert!(matches("**/*.lute", "a/b/c/foo.lute"));
    }

    #[test]
    fn anchored_root_only() {
        assert!(!matches("drafts/**", "src/drafts/foo.lute"));
    }

    #[test]
    fn any_of_list() {
        let pats = vec!["drafts/**".into(), "wip.lute".into()];
        assert!(matches_any(&pats, "drafts/x.lute"));
        assert!(matches_any(&pats, "wip.lute"));
        assert!(!matches_any(&pats, "src/main.lute"));
    }
}
