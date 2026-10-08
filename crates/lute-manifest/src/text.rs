//! Author-facing text: CEL source-text scanning, interpolation format hints,
//! and state-path spellings.

/// For each byte of `raw`, whether that byte lies inside a CEL **string literal**
/// (§4.4). The opening and closing quote bytes are themselves marked `true`, so a
/// scanner can treat any position with `mask[i] == true` as string content to be
/// skipped.
///
/// CEL string literals are single- (`'…'`) or double-quoted (`"…"`) with `\`
/// escaping; an escaped quote (`\'`) does not close the literal. Every byte of a
/// multibyte character inside a string is marked, so indexing the returned `Vec`
/// by any byte offset into `raw` is always valid. An unterminated literal marks
/// through end-of-input (a malformed fragment degrades safely: its bytes are
/// treated as string content rather than mis-tokenized as DSL `@`/`$`).
///
/// Shared with the LSP feature layer (`lute_lsp::features::path_tokens`/`path_at`,
/// S3) so DSL-token and state-path scanning agree on string boundaries.
pub fn cel_string_mask(raw: &str) -> Vec<bool> {
    let b = raw.as_bytes();
    let mut mask = vec![false; b.len()];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'\'' || c == b'"' {
            let quote = c;
            mask[i] = true; // opening quote
            i += 1;
            while i < b.len() {
                mask[i] = true;
                if b[i] == b'\\' {
                    // Escape: the next byte is literal string content, not a close.
                    i += 1;
                    if i < b.len() {
                        mask[i] = true;
                        i += 1;
                    }
                    continue;
                }
                if b[i] == quote {
                    i += 1; // closing quote (already marked)
                    break;
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    mask
}

/// A quoted name at the start of `text` — `"…"` or `'…'`, `\` escaping the
/// next character — and the bytes it spans, quotes included. `None` when
/// `text` does not open with a quote or the quote is never closed.
pub fn read_quoted(text: &str) -> Option<(String, usize)> {
    let b = text.as_bytes();
    let quote = *b.first().filter(|&&q| q == b'"' || q == b'\'')?;
    let mut out = String::new();
    let mut chars = text[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?.1),
            c if c as u32 == quote as u32 => return Some((out, i + 2)),
            c => out.push(c),
        }
    }
    None
}

/// The canonical dotted form of a path: its segments joined by `.`.
pub fn render_path<S: AsRef<str>>(segs: &[S]) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str(seg.as_ref());
    }
    out
}

/// How an author writes the path in a condition: the root bare, then each
/// segment after a `.` when it is an identifier, else as a double-quoted
/// index (`quest["zero-coke-001"].state`).
pub fn bracket_spelling<S: AsRef<str>>(segs: &[S]) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        let seg = seg.as_ref();
        if i == 0 {
            out.push_str(seg);
        } else if crate::ident::is_ident(seg) {
            out.push('.');
            out.push_str(seg);
        } else {
            out.push_str("[\"");
            out.push_str(seg);
            out.push_str("\"]");
        }
    }
    out
}

/// [`bracket_spelling`] of a canonical dotted path.
pub fn bracket_spelling_of(path: &str) -> String {
    bracket_spelling(&path.split('.').collect::<Vec<_>>())
}

/// dsl 0.24.0 §4: `{{user.deaths:ordinal}}` renders the number as an
/// English ordinal ([`english_ordinal`]).
pub const INTERP_FORMAT_ORDINAL: &str = "ordinal";

/// dsl 0.25.0 §8: `{{run.day:ordinalWord}}` renders the number as an
/// English ordinal word ([`english_ordinal_word`]).
pub const INTERP_FORMAT_ORDINAL_WORD: &str = "ordinalWord";

/// dsl 0.27.0 §7: `{{user.terms:plural(lantern|lanterns)}}` renders the
/// singular or the plural form by the number ([`english_plural`]); the IR
/// placeholder carries the forms so an engine can localize.
pub const INTERP_FORMAT_PLURAL: &str = "plural";

/// `{{run.wagons:cardinalWord}}` renders the number as an English cardinal
/// word ([`english_cardinal_word`]): `one` … `twenty`, then digits.
pub const INTERP_FORMAT_CARDINAL_WORD: &str = "cardinalWord";

/// `{{run.rival:capitalize}}` renders the text with its first letter in
/// upper case ([`format_text`]).
pub const INTERP_FORMAT_CAPITALIZE: &str = "capitalize";

/// `{{occasion.target:start}}` renders a member's sentence-start label form
/// (`start:` of a kind's `labels:` entry), else the text capitalized.
pub const INTERP_FORMAT_START: &str = "start";

/// `{{occasion.target:indefinite}}` renders a member's label with its
/// indefinite article (`indefinite:` of a kind's `labels:` entry), else
/// `a` / `an` by the text's first letter.
pub const INTERP_FORMAT_INDEFINITE: &str = "indefinite";

/// The hints that format a number ([`format_number`]).
pub const INTERP_NUMBER_FORMATS: [&str; 4] = [
    INTERP_FORMAT_ORDINAL,
    INTERP_FORMAT_ORDINAL_WORD,
    INTERP_FORMAT_CARDINAL_WORD,
    INTERP_FORMAT_PLURAL,
];

/// The hints that format text — a string, an enum or entity label, a cast
/// display name ([`format_text`]).
pub const INTERP_TEXT_FORMATS: [&str; 3] = [
    INTERP_FORMAT_CAPITALIZE,
    INTERP_FORMAT_START,
    INTERP_FORMAT_INDEFINITE,
];

/// Every interpolation format hint the language defines; the checker
/// rejects any other.
pub const INTERP_FORMATS: [&str; 7] = [
    INTERP_FORMAT_ORDINAL,
    INTERP_FORMAT_ORDINAL_WORD,
    INTERP_FORMAT_CARDINAL_WORD,
    INTERP_FORMAT_PLURAL,
    INTERP_FORMAT_CAPITALIZE,
    INTERP_FORMAT_START,
    INTERP_FORMAT_INDEFINITE,
];

/// `n` rendered in number hint `format` ([`INTERP_NUMBER_FORMATS`]) with the
/// hint's `forms` — the one rule the reference runner, `lute trace` and a
/// component's compile-time literal splice all render with. `shown` is the
/// number as it renders unformatted (what a `#` in a plural form becomes).
/// `None` for a text or unknown hint, malformed forms, or a number the hint
/// does not cover: the renderer then shows the number unchanged.
pub fn format_number(
    format: &str,
    forms: Option<&[String]>,
    n: f64,
    shown: &str,
) -> Option<String> {
    match format {
        INTERP_FORMAT_ORDINAL => english_ordinal(n),
        INTERP_FORMAT_ORDINAL_WORD => english_ordinal_word(n),
        INTERP_FORMAT_CARDINAL_WORD => english_cardinal_word(n),
        INTERP_FORMAT_PLURAL => english_plural(forms?, n, shown),
        _ => None,
    }
}

/// `text` rendered in text hint `format` ([`INTERP_TEXT_FORMATS`]):
/// `capitalize` upper-cases the first letter; `start` is the declared
/// sentence-start form `start` when there is one, else `capitalize`;
/// `indefinite` is the declared form `indefinite` when there is one, else
/// the text itself when it already starts with an article (`the smugglers'
/// cut`, `a lantern`, `an owl`), else `an` before a text whose first letter
/// is a vowel (`a e i o u`) and `a` before any other (`an ashwraith`, `a
/// wagon`) — declare `indefinite:` for the words that rule gets wrong (`an
/// hour`, `a unicorn`). `None` for a number or unknown hint.
pub fn format_text(
    format: &str,
    text: &str,
    start: Option<&str>,
    indefinite: Option<&str>,
) -> Option<String> {
    match format {
        INTERP_FORMAT_CAPITALIZE => Some(capitalize(text)),
        INTERP_FORMAT_START => Some(start.map_or_else(|| capitalize(text), str::to_string)),
        INTERP_FORMAT_INDEFINITE => Some(indefinite.map_or_else(
            || {
                let first = text.split_whitespace().next().unwrap_or("");
                if text.contains(char::is_whitespace)
                    && ["the", "a", "an"]
                        .iter()
                        .any(|a| first.eq_ignore_ascii_case(a))
                {
                    return text.to_string();
                }
                let vowel = text
                    .chars()
                    .find(|c| c.is_alphanumeric())
                    .is_some_and(|c| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u'));
                format!("{} {text}", if vowel { "an" } else { "a" })
            },
            str::to_string,
        )),
        _ => None,
    }
}

/// `text` with its first character in upper case (`the cut` → `The cut`).
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// dsl 0.27.0 §7: the English form of `forms` (`[singular, plural]`) for
/// `n` — the singular exactly when `n` is 1 — with the number written into
/// it: `#word` is the number as a cardinal word ([`english_cardinal_word`],
/// digits outside `one` … `twenty`), `#Word` the same word capitalized, and
/// any other `#` is `shown` (`plural(# lantern|# lanterns)` → `3 lanterns`,
/// `plural(One wagon|#Word wagons)` → `Eleven wagons`). `None` unless there
/// are exactly two forms.
pub fn english_plural(forms: &[String], n: f64, shown: &str) -> Option<String> {
    let [one, other] = forms else {
        return None;
    };
    let mut rest = if n == 1.0 {
        one.as_str()
    } else {
        other.as_str()
    };
    let word = || english_cardinal_word(n).unwrap_or_else(|| shown.to_string());
    let mut out = String::with_capacity(rest.len() + shown.len());
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        rest = if let Some(r) = after.strip_prefix("word") {
            out.push_str(&word());
            r
        } else if let Some(r) = after.strip_prefix("Word") {
            out.push_str(&capitalize(&word()));
            r
        } else {
            out.push_str(shown);
            after
        };
    }
    out.push_str(rest);
    Some(out)
}

/// dsl 0.24.0 §4: `n` as an English ordinal — `1st 2nd 3rd 4th … 11th 12th
/// 13th … 21st 22nd 23rd … 101st 111th 112th`. The suffix follows the last
/// digit, except that a number whose last two digits are `11`–`13` takes
/// `th`; `0` is `0th`. Defined for a non-negative integer only: any other
/// number (fractional, negative, non-finite, or ≥ 10^15 where a float stops
/// being an exact integer) is `None`, and the renderer then shows the number
/// unchanged. The one rule the reference runner, `lute trace` and a
/// component's compile-time literal splice all render with.
pub fn english_ordinal(n: f64) -> Option<String> {
    if !(n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n < 1e15) {
        return None;
    }
    let i = n as u64;
    let suffix = match (i % 10, i % 100) {
        (_, 11..=13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    Some(format!("{i}{suffix}"))
}

/// dsl 0.25.0 §8: `n` as an English ordinal word — `first` … `twentieth`
/// for 1–20, and [`english_ordinal`]'s digits (`0th`, `21st`, `101st`)
/// otherwise. `None` exactly where [`english_ordinal`] is.
pub fn english_ordinal_word(n: f64) -> Option<String> {
    const WORDS: [&str; 20] = [
        "first",
        "second",
        "third",
        "fourth",
        "fifth",
        "sixth",
        "seventh",
        "eighth",
        "ninth",
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
        "twentieth",
    ];
    let digits = english_ordinal(n)?;
    Some(match n as usize {
        i @ 1..=20 => WORDS[i - 1].to_string(),
        _ => digits,
    })
}

/// `n` as an English cardinal word — `one` … `twenty` for 1–20, and its
/// digits (`0`, `21`, `101`) otherwise; `None` exactly where
/// [`english_ordinal`] is (a fraction, a negative number), and the renderer
/// then shows the number unchanged.
pub fn english_cardinal_word(n: f64) -> Option<String> {
    const WORDS: [&str; 20] = [
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    english_ordinal(n)?;
    Some(match n as u64 {
        i @ 1..=20 => WORDS[i as usize - 1].to_string(),
        i => i.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_spelling_quotes_only_non_identifiers() {
        for (path, want) in [
            (
                "quest.zero-coke-001.state",
                "quest[\"zero-coke-001\"].state",
            ),
            ("run.visits.labB2", "run.visits.labB2"),
            ("run.visits.001", "run.visits[\"001\"]"),
            ("run._x", "run._x"),
        ] {
            assert_eq!(bracket_spelling_of(path), want);
        }
    }

    /// dsl 0.24.0 §4: the suffix follows the last digit, except 11–13 in the
    /// last two digits.
    #[test]
    fn english_ordinal_suffixes() {
        let cases = [
            (0.0, "0th"),
            (1.0, "1st"),
            (2.0, "2nd"),
            (3.0, "3rd"),
            (4.0, "4th"),
            (10.0, "10th"),
            (11.0, "11th"),
            (12.0, "12th"),
            (13.0, "13th"),
            (21.0, "21st"),
            (22.0, "22nd"),
            (23.0, "23rd"),
            (100.0, "100th"),
            (101.0, "101st"),
            (111.0, "111th"),
            (112.0, "112th"),
            (113.0, "113th"),
            (1002.0, "1002nd"),
        ];
        for (n, want) in cases {
            assert_eq!(english_ordinal(n).as_deref(), Some(want), "{n}");
        }
    }

    /// A number with no ordinal renders unchanged: the function says so.
    #[test]
    fn english_ordinal_is_undefined_off_the_non_negative_integers() {
        for n in [-1.0, 2.5, f64::NAN, f64::INFINITY, 1e15] {
            assert_eq!(english_ordinal(n), None, "{n}");
        }
    }

    /// dsl 0.25.0 §8: words for 1–20, digit ordinals on either side, and
    /// no ordinal where `:ordinal` has none.
    #[test]
    fn english_ordinal_word_spells_one_to_twenty_then_falls_back_to_digits() {
        for (n, want) in [
            (1.0, "first"),
            (2.0, "second"),
            (3.0, "third"),
            (12.0, "twelfth"),
            (20.0, "twentieth"),
            (21.0, "21st"),
            (0.0, "0th"),
            (112.0, "112th"),
        ] {
            assert_eq!(english_ordinal_word(n).as_deref(), Some(want), "{n}");
        }
        for n in [-1.0, 2.5, f64::NAN] {
            assert_eq!(english_ordinal_word(n), None, "{n}");
        }
    }

    #[test]
    fn cardinal_words_and_plural_word_forms() {
        for (n, want) in [
            (1.0, "one"),
            (11.0, "eleven"),
            (20.0, "twenty"),
            (0.0, "0"),
            (21.0, "21"),
        ] {
            assert_eq!(english_cardinal_word(n).as_deref(), Some(want), "{n}");
        }
        assert_eq!(english_cardinal_word(2.5), None);
        let forms = [
            "One wagon".to_string(),
            "#Word wagons, #word in all (#)".to_string(),
        ];
        assert_eq!(
            english_plural(&forms, 11.0, "11").as_deref(),
            Some("Eleven wagons, eleven in all (11)")
        );
        assert_eq!(
            english_plural(&forms, 1.0, "1").as_deref(),
            Some("One wagon")
        );
        assert_eq!(
            english_plural(&forms, 30.0, "30").as_deref(),
            Some("30 wagons, 30 in all (30)")
        );
    }

    #[test]
    fn text_hints_use_declared_forms_then_fall_back() {
        assert_eq!(
            format_text("capitalize", "the cut", None, None).as_deref(),
            Some("The cut")
        );
        assert_eq!(
            format_text("start", "the cut", Some("The smugglers' cut"), None).as_deref(),
            Some("The smugglers' cut")
        );
        assert_eq!(
            format_text("start", "élan", None, None).as_deref(),
            Some("Élan")
        );
        assert_eq!(
            format_text("indefinite", "ashwraith", None, None).as_deref(),
            Some("an ashwraith")
        );
        assert_eq!(
            format_text("indefinite", "wagon", None, None).as_deref(),
            Some("a wagon")
        );
        assert_eq!(
            format_text("indefinite", "hour", None, Some("an hour")).as_deref(),
            Some("an hour")
        );
        assert_eq!(
            format_text("indefinite", "the smugglers' cut", None, None).as_deref(),
            Some("the smugglers' cut")
        );
        assert_eq!(
            format_text("indefinite", "theatre", None, None).as_deref(),
            Some("a theatre")
        );
        assert_eq!(format_text("ordinal", "x", None, None), None);
    }
}
