//! The one "did you mean" helper in the workspace.
//!
//! It lived in `lute-check` (`cel_paths.rs`, dsl 0.5.0 §2.2's suggestion on
//! `E-UNDECLARED`), which depends on this crate — so a manifest-layer
//! diagnostic could not reach it. Rather than mint a second copy for
//! `E-DEFAULTS-KEY` (0.10.0 §6.1), the algorithm moved down and `lute-check`
//! calls it here.

/// Edit distance between two strings, counting an adjacent transposition
/// (`lable` → `label`) as ONE edit (optimal string alignment). Character-wise,
/// not byte-wise — the inputs are ASCII identifiers in practice, but this
/// stays correct for any UTF-8.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    // Rows i-2, i-1 and i of the DP table.
    let mut prev2 = vec![0usize; m + 1];
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0usize; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(prev2[j - 2] + 1);
            }
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// The nearest candidate to `needle` within `max_dist` edits. `None` when
/// nothing is close enough or `needle` matches a candidate exactly (distance
/// 0 is excluded — never suggest the input back). Ties break on the FIRST
/// candidate in iteration order, so a sorted `haystack` gives a deterministic
/// suggestion.
///
/// The budget also shrinks with the needle: at most one edit per three
/// characters (never less than one). Two edits turn any short word into any
/// other — `the` and `oven` are both two edits from `when` — so a fixed
/// budget suggested unrelated words for short needles (round-5 T3-1).
pub fn nearest<'a>(
    needle: &str,
    haystack: impl IntoIterator<Item = &'a str>,
    max_dist: usize,
) -> Option<&'a str> {
    let max_dist = max_dist.min((needle.chars().count() / 3).max(1));
    haystack
        .into_iter()
        .map(|c| (c, levenshtein(needle, c)))
        .filter(|&(_, d)| d > 0 && d <= max_dist)
        .min_by_key(|&(_, d)| d)
        .map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::nearest;

    // Round-5 T3-1: a short needle two edits from an unrelated word drew a
    // suggestion (`the` → `when`); a transposed typo still does.
    #[test]
    fn short_needles_need_a_close_match() {
        let attrs = ["id", "label", "when", "once"];
        assert_eq!(nearest("the", attrs, 2), None);
        assert_eq!(nearest("oven", attrs, 2), None);
        assert_eq!(nearest("bread", attrs, 2), None);
        assert_eq!(nearest("lable", attrs, 2), Some("label"));
        assert_eq!(nearest("wehn", attrs, 2), Some("when"));
        assert_eq!(nearest("onc", attrs, 2), Some("once"));
        assert_eq!(nearest("episod", ["episode"], 2), Some("episode"));
    }
}
