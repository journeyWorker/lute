//! The one "did you mean" helper in the workspace.
//!
//! It lived in `lute-check` (`cel_paths.rs`, dsl 0.5.0 §2.2's suggestion on
//! `E-UNDECLARED`), which depends on this crate — so a manifest-layer
//! diagnostic could not reach it. Rather than mint a second copy for
//! `E-DEFAULTS-KEY` (0.10.0 §6.1), the algorithm moved down and `lute-check`
//! calls it here.

/// Levenshtein edit distance between two strings. Character-wise, not
/// byte-wise — the inputs are ASCII identifiers in practice, but this stays
/// correct for any UTF-8.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// The nearest candidate to `needle` within `max_dist` edits. `None` when
/// nothing is close enough or `needle` matches a candidate exactly (distance
/// 0 is excluded — never suggest the input back). Ties break on the FIRST
/// candidate in iteration order, so a sorted `haystack` gives a deterministic
/// suggestion.
pub fn nearest<'a>(
    needle: &str,
    haystack: impl IntoIterator<Item = &'a str>,
    max_dist: usize,
) -> Option<&'a str> {
    haystack
        .into_iter()
        .map(|c| (c, levenshtein(needle, c)))
        .filter(|&(_, d)| d > 0 && d <= max_dist)
        .min_by_key(|&(_, d)| d)
        .map(|(c, _)| c)
}
