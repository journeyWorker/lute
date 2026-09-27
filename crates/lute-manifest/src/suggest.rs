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

/// Neighbouring spellings that are not typos of each other: a word authors
/// reach for when they mean its sibling (`<objective complete=>` for `done=`,
/// `<quest questTier=>` for `tier=`, occasion `when:` for `raisedWhen:`).
/// Each pair is directed — `(written, meant)` — and only fires when `meant`
/// is a candidate, so a table row never suggests a word the caller would
/// refuse.
const SIBLINGS: &[(&str, &str)] = &[
    ("complete", "done"),
    ("completed", "done"),
    ("finished", "done"),
    ("questDone", "questComplete"),
    ("questCompleted", "questComplete"),
    ("questFinished", "questComplete"),
    ("questTier", "tier"),
    ("test", "when"),
    ("if", "when"),
    ("condition", "when"),
    ("when", "raisedWhen"),
    ("when", "test"),
    ("rearm", "spentBy"),
    ("spentBy", "rearm"),
    ("until", "spentBy"),
    ("else", "otherwise"),
    ("default", "otherwise"),
    ("occasion", "on"),
    ("occasion", "event"),
    ("on", "occasion"),
    ("subject", "on"),
    ("emote", "emotion"),
    ("entity", "entities"),
    ("dependencies", "depends"),
    ("requires", "depends"),
    ("kind", "entity"),
    ("domain", "entity"),
    ("goto", "next"),
    ("jump", "next"),
    ("divert", "next"),
    ("status", "state"),
    ("seen", "read"),
    ("everSeen", "everRead"),
];

/// The nearest candidate to `needle`. `None` when nothing is close enough or
/// `needle` matches a candidate exactly (never suggest the input back). In
/// order:
///
/// 1. a candidate equal to `needle` ignoring case (`gold` → `GOLD`,
///    `Scene` → `scene`, `changed_on` → `changedOn` also counts: `_`/`-` are
///    ignored with case);
/// 2. the [`SIBLINGS`] word `needle` is known to stand for;
/// 3. the one dotted candidate whose last segment is the bare `needle`
///    (`lit` → `ending.lit`: a local id for its canonical one) — none when
///    several end in it;
/// 4. the candidate within `max_dist` edits, compared case-insensitively.
///
/// Ties break on the FIRST candidate in iteration order, so a sorted
/// `haystack` gives a deterministic suggestion.
///
/// The edit budget also shrinks with the needle: at most one edit per three
/// characters (never less than one). Two edits turn any short word into any
/// other — `the` and `oven` are both two edits from `when` — so a fixed
/// budget suggested unrelated words for short needles.
pub fn nearest<'a>(
    needle: &str,
    haystack: impl IntoIterator<Item = &'a str>,
    max_dist: usize,
) -> Option<&'a str> {
    let candidates: Vec<&'a str> = haystack.into_iter().filter(|c| *c != needle).collect();
    let folded = fold(needle);
    if let Some(c) = candidates.iter().find(|c| fold(c) == folded) {
        return Some(c);
    }
    if let Some(c) = SIBLINGS
        .iter()
        .filter(|(written, _)| *written == needle)
        .find_map(|(_, meant)| candidates.iter().find(|c| **c == *meant))
    {
        return Some(c);
    }
    if !needle.contains('.') {
        let mut tails = candidates
            .iter()
            .filter(|c| c.rsplit_once('.').is_some_and(|(_, last)| last == needle));
        if let (Some(c), None) = (tails.next(), tails.next()) {
            return Some(c);
        }
    }
    let len = needle.chars().count();
    // Every edit spent means nothing of the needle is left (`c` → `a`, `셋`
    // → `둘`): that is another word, not a typo of it.
    let max_dist = max_dist.min((len / 3).max(1)).min(len.saturating_sub(1));
    let lower = needle.to_lowercase();
    candidates
        .into_iter()
        .map(|c| (c, levenshtein(&lower, &c.to_lowercase())))
        .filter(|&(_, d)| d > 0 && d <= max_dist)
        .min_by_key(|&(_, d)| d)
        .map(|(c, _)| c)
}

/// ` — did you mean `x`?` for the [`nearest`] candidate (edit budget 2), or
/// the empty string: the one phrasing every diagnostic appends.
pub fn did_you_mean<'a>(needle: &str, haystack: impl IntoIterator<Item = &'a str>) -> String {
    nearest(needle, haystack, 2)
        .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"))
}

/// The one candidate `needle` abbreviates — `Mon` for `Monday`, `morn` for
/// `morning` — compared case-insensitively; `None` for a needle shorter than
/// three characters or one that starts several candidates. For member
/// literals, where a label is often written short; [`nearest`] first.
pub fn abbreviated<'a>(
    needle: &str,
    haystack: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    if needle.chars().count() < 3 {
        return None;
    }
    let lower = needle.to_lowercase();
    let mut starts = haystack
        .into_iter()
        .filter(|c| *c != needle && c.to_lowercase().starts_with(&lower));
    match (starts.next(), starts.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// Case- and separator-insensitive form: `changed_on`, `changed-on` and
/// `changedOn` all fold to `changedon`.
fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '_' && *c != '-')
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{did_you_mean, nearest};

    // A short needle two edits from an unrelated word drew a suggestion
    // (`the` → `when`); a transposed typo still does.
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

    // A one-character needle one edit from a one-character member shares
    // nothing with it: no suggestion.
    #[test]
    fn a_single_character_is_no_typo_of_another() {
        assert_eq!(nearest("c", ["a", "b"], 2), None);
        assert_eq!(nearest("셋", ["하나", "둘"], 2), None);
        assert_eq!(nearest("ab", ["ac", "zz"], 2), Some("ac"));
    }

    // Case and separators are not edits: `gold` names `GOLD`, `changed_on`
    // names `changedOn`, `Emotion` names `emotion`.
    #[test]
    fn case_and_separators_fold() {
        assert_eq!(nearest("gold", ["GOLD", "SILVER"], 2), Some("GOLD"));
        assert_eq!(
            nearest("changed_on", ["tier", "changedOn"], 2),
            Some("changedOn")
        );
        assert_eq!(
            nearest("Emotion", ["emotion", "motion"], 2),
            Some("emotion")
        );
        assert_eq!(nearest("Scene", ["scene", "quest"], 2), Some("scene"));
        assert_eq!(nearest("scene", ["scene", "quest"], 2), None);
    }

    // A sibling word is not a typo of the word meant, but it is what the
    // author meant — and only when that word is a candidate.
    #[test]
    fn siblings_name_the_neighbour() {
        assert_eq!(nearest("complete", ["done", "failed"], 2), Some("done"));
        assert_eq!(nearest("questTier", ["tier", "after"], 2), Some("tier"));
        assert_eq!(
            nearest("when", ["raisedWhen", "target"], 2),
            Some("raisedWhen")
        );
        assert_eq!(nearest("else", ["when", "otherwise"], 2), Some("otherwise"));
        assert_eq!(nearest("complete", ["failed"], 2), None);
        assert_eq!(
            did_you_mean("questCompleted", ["questComplete", "questFail"]),
            " — did you mean `questComplete`?"
        );
        assert_eq!(did_you_mean("zzz", ["when"]), "");
    }

    // A bare id names the one dotted id it ends; two that end in it are
    // ambiguous, and fall through to the edit-distance rule.
    #[test]
    fn a_bare_id_names_its_canonical_dotted_id() {
        assert_eq!(
            nearest("lit", ["ending.dark", "ending.lit"], 2),
            Some("ending.lit")
        );
        assert_eq!(
            nearest("edith", ["isolation.edith", "isolation.bram"], 2),
            Some("isolation.edith")
        );
        assert_eq!(nearest("lit", ["ending.lit", "lamp.lit"], 2), None);
    }
}
