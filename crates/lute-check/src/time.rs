//! The millisecond domain (dsl 0.10.0 §10, **D-H**/**D-T**).
//!
//! A time value's canonical representation is a **non-negative integer number
//! of milliseconds**, obtained from the authored decimal **by shifting the
//! decimal point three places** — never by multiplying a binary floating-point
//! value. `0.35` is `350`; `0.8` is `800`; `1.2` is `1200`. Exactly, for every
//! value the grammar admits, with no rounding step to specify and no
//! representation error to inherit.
//!
//! There is no `f64` anywhere in the parse. That is not pedantry: `0.8 + 0.4`
//! in IEEE-754 is `1.2000000000000002`, which is what put a workaround into
//! `docs/examples/anseo/scenes/spine-a.lute`, and converting through the parsed
//! double would reintroduce that class at the one point an implementer would
//! never test.
//!
//! ## The three outcomes
//! - [`TimeParse::Ms`] — a well-formed decimal, converted exactly.
//! - [`TimeParse::TooFine`] — a decimal finer than a millisecond.
//!   `E-TIME-RESOLUTION`. There is no rounding: a timeline the author cannot see
//!   the difference in is a timeline whose diagnostics they cannot predict, and
//!   `1.2000001` must be rejected rather than quietly honoured.
//! - [`TimeParse::NotANumber`] — not a decimal at all. **Pre-existing behaviour
//!   is preserved** (§10.2): a clip `at` is treated as absent and a `duration`
//!   resolves to zero. That is underspecified, it is not what #26 is about, and
//!   it is recorded rather than changed here.
//!
//! ## Unit suffixes
//! `1.5s` and `250ms` are accepted, because the pre-0.10.0 barrier/duration
//! reader accepted them (`timeline.rs`'s `parse_f64`) and this is a `LANG-SOFT`
//! change that may not redden a green document. Zero corpus values use them.
//! `0.5ms` is sub-millisecond and is [`TimeParse::TooFine`], like any other
//! value below the resolution limit.
//!
//! ## Sign
//! A leading `-` is accepted and yields negative milliseconds. §10.2 says the
//! canonical representation is non-negative, but `f64::from_str` accepted a
//! negative today and rejecting one here would be a new red on a green
//! document, which `LANG-SOFT` forbids. The deviation is deliberate.

/// The outcome of reading one authored time value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeParse {
    /// A well-formed decimal, converted exactly to milliseconds.
    Ms(i64),
    /// More fractional precision than a millisecond can carry —
    /// `E-TIME-RESOLUTION`.
    TooFine,
    /// Not a decimal number at all; pre-existing fallback behaviour applies.
    NotANumber,
}

/// The resolution limit, in fractional decimal digits of a seconds value.
pub const TIME_MAX_FRACTIONAL_DIGITS: usize = 3;

/// Read an authored time value into integer milliseconds by shifting its
/// decimal point three places (§10.2). Never parses an `f64`.
///
/// Grammar: optional sign, then a decimal (`123`, `1.5`, `.5`, `2.`), then an
/// optional `s` or `ms` unit. A seconds value carries at most
/// [`TIME_MAX_FRACTIONAL_DIGITS`] fractional digits; a `ms` value carries none.
pub fn parse_time_ms(raw: &str) -> TimeParse {
    let t = raw.trim();
    // Unit suffix first, `ms` before `s` — otherwise `250ms` reads as `250m`
    // plus `s`. A `ms` value is ALREADY milliseconds, so it admits no fraction.
    let (body, frac_limit) = if let Some(rest) = t.strip_suffix("ms") {
        (rest.trim_end(), 0usize)
    } else if let Some(rest) = t.strip_suffix('s') {
        (rest.trim_end(), TIME_MAX_FRACTIONAL_DIGITS)
    } else {
        (t, TIME_MAX_FRACTIONAL_DIGITS)
    };
    let (neg, digits) = match body.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    let Some((whole, frac)) = split_decimal(digits) else {
        return TimeParse::NotANumber;
    };
    if frac.len() > frac_limit {
        return TimeParse::TooFine;
    }
    // The shift: pad the fraction out to exactly `frac_limit` digits, then read
    // the concatenation as one integer. No division, no float multiply.
    let mut shifted = String::with_capacity(whole.len() + frac_limit + 1);
    shifted.push_str(if whole.is_empty() { "0" } else { whole });
    shifted.push_str(frac);
    for _ in frac.len()..frac_limit {
        shifted.push('0');
    }
    let Ok(ms) = shifted.parse::<i64>() else {
        return TimeParse::NotANumber; // out of `i64` range
    };
    TimeParse::Ms(if neg { -ms } else { ms })
}

/// Split a plain unsigned decimal into `(whole, frac)`, both possibly empty but
/// never both. `None` for anything that is not one — an exponent, a second dot,
/// a stray character. Digits only: deliberately narrower than `f64::from_str`,
/// which also accepts `inf`, `NaN` and `1e3`, none of which is a time an author
/// can mean.
fn split_decimal(s: &str) -> Option<(&str, &str)> {
    if s.is_empty() {
        return None;
    }
    let (whole, frac) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    if whole.is_empty() && frac.is_empty() {
        return None; // a lone "."
    }
    if frac.contains('.') {
        return None; // a second dot
    }
    let ok = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if !ok(whole) || !ok(frac) {
        return None;
    }
    Some((whole, frac))
}

/// Milliseconds to the seconds value the ARTIFACT carries (**D-T**, §10.3).
///
/// The artifact keeps seconds because an engine never performs cursor
/// arithmetic — the compiler resolves every clip's absolute `at` before
/// lowering — so exactness is a compile-time property and paying for it at the
/// boundary buys nothing. Changing these fields to milliseconds under the same
/// names and the same JSON type would be a break with **no detection surface at
/// all**: every conforming engine would place every effect 1000× late,
/// silently, and the artifact would still validate against a schema that says
/// `number`. `irVersion` gates on major.minor and the field shape is unchanged,
/// so there is no version gate that catches it either.
///
/// The division is exact for every value the grammar admits: `ms` is an integer,
/// so `ms as f64` is exact below 2^53, and dividing by `1000.0` is correctly
/// rounded to the nearest double — which, for a decimal of at most three
/// fractional digits, is the same double `f64::from_str` produces for that
/// decimal. The shortest round-tripping rendering is therefore the decimal
/// itself, and `seconds_emission_round_trips_every_admissible_value` proves it
/// over the whole admissible range rather than asserting it.
pub fn ms_to_seconds(ms: i64) -> f64 {
    ms as f64 / 1000.0
}

/// Milliseconds as the authored decimal, for a diagnostic message (§10.2:
/// `E-CLIP-OVERLAP` and `E-TIMELINE-DURATION` MUST print the authored decimal,
/// not a reconstructed float). Pure integer arithmetic.
pub fn fmt_seconds(ms: i64) -> String {
    let neg = ms < 0;
    let abs = ms.unsigned_abs();
    let whole = abs / 1000;
    let frac = abs % 1000;
    let sign = if neg { "-" } else { "" };
    if frac == 0 {
        return format!("{sign}{whole}");
    }
    let mut f = format!("{frac:03}");
    while f.ends_with('0') {
        f.pop();
    }
    format!("{sign}{whole}.{f}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifts_the_decimal_exactly() {
        assert_eq!(parse_time_ms("0.35"), TimeParse::Ms(350));
        assert_eq!(parse_time_ms("0.8"), TimeParse::Ms(800));
        assert_eq!(parse_time_ms("1.2"), TimeParse::Ms(1200));
        assert_eq!(parse_time_ms("0.1"), TimeParse::Ms(100));
        assert_eq!(parse_time_ms("0.2"), TimeParse::Ms(200));
        assert_eq!(parse_time_ms("1.005"), TimeParse::Ms(1005));
        assert_eq!(parse_time_ms("1"), TimeParse::Ms(1000));
        assert_eq!(parse_time_ms("1.0"), TimeParse::Ms(1000));
        assert_eq!(parse_time_ms("0"), TimeParse::Ms(0));
        assert_eq!(parse_time_ms(".5"), TimeParse::Ms(500));
        assert_eq!(parse_time_ms("2."), TimeParse::Ms(2000));
        assert_eq!(parse_time_ms("  0.75  "), TimeParse::Ms(750));
    }

    /// The class this whole section removes.
    #[test]
    fn addition_is_exact_where_f64_addition_is_not() {
        let (TimeParse::Ms(a), TimeParse::Ms(b)) = (parse_time_ms("0.1"), parse_time_ms("0.2"))
        else {
            panic!("both parse")
        };
        assert_eq!(a + b, 300);
        assert_ne!(0.1_f64 + 0.2_f64, 0.3_f64, "the premise: doubles do not");

        let (TimeParse::Ms(x), TimeParse::Ms(y)) = (parse_time_ms("0.8"), parse_time_ms("0.4"))
        else {
            panic!("both parse")
        };
        assert_eq!(x + y, 1200);
        assert_ne!(0.8_f64 + 0.4_f64, 1.2_f64, "the premise, again");
    }

    /// Never via the parsed double. `("0.35" as f64) * 1000.0` happens to give
    /// 350, so a naive implementation passes most of the table above — this is
    /// the test that separates them, on a value where the two disagree.
    ///
    /// `1.005` is the smallest such value: the nearest double to `1.005` is
    /// below it, so the product is `1004.9999999999999` and the `as i64`
    /// truncation drops a whole millisecond. 741 of the first 100_000
    /// millisecond values behave this way.
    #[test]
    fn never_multiplies_a_parsed_double() {
        assert_eq!(parse_time_ms("1.005"), TimeParse::Ms(1005));
        assert_eq!(("1.005".parse::<f64>().unwrap() * 1000.0) as i64, 1004);
        assert_eq!(parse_time_ms("2.002"), TimeParse::Ms(2002));
        assert_eq!(("2.002".parse::<f64>().unwrap() * 1000.0) as i64, 2001);
    }

    #[test]
    fn rejects_finer_than_a_millisecond() {
        assert_eq!(parse_time_ms("1.2000001"), TimeParse::TooFine);
        assert_eq!(parse_time_ms("0.0001"), TimeParse::TooFine);
        assert_eq!(parse_time_ms("1.2345"), TimeParse::TooFine);
        assert_eq!(parse_time_ms("0.5ms"), TimeParse::TooFine);
    }

    #[test]
    fn accepts_the_pre_existing_unit_suffixes() {
        assert_eq!(parse_time_ms("1.5s"), TimeParse::Ms(1500));
        assert_eq!(parse_time_ms("250ms"), TimeParse::Ms(250));
        assert_eq!(parse_time_ms("250 ms"), TimeParse::Ms(250));
    }

    #[test]
    fn non_numbers_keep_their_pre_existing_fallback() {
        assert_eq!(parse_time_ms(""), TimeParse::NotANumber);
        assert_eq!(parse_time_ms("   "), TimeParse::NotANumber);
        assert_eq!(parse_time_ms("soon"), TimeParse::NotANumber);
        assert_eq!(parse_time_ms("1.2.3"), TimeParse::NotANumber);
        assert_eq!(parse_time_ms("1e3"), TimeParse::NotANumber);
        assert_eq!(parse_time_ms("."), TimeParse::NotANumber);
        assert_eq!(parse_time_ms("-"), TimeParse::NotANumber);
    }

    /// A leading `-` was accepted by `f64::from_str` and stays accepted: a
    /// LANG-SOFT change may not redden a green document.
    #[test]
    fn keeps_the_pre_existing_negative_tolerance() {
        assert_eq!(parse_time_ms("-0.5"), TimeParse::Ms(-500));
        assert_eq!(fmt_seconds(-500), "-0.5");
    }

    /// §10.3, D-T: the artifact keeps SECONDS, emitted as ms / 1000. This is
    /// the test that a cursor-derived 1.2 stops emitting 1.2000000000000002.
    #[test]
    fn seconds_emission_is_exact() {
        assert_eq!(ms_to_seconds(1200), 1.2);
        assert_eq!(ms_to_seconds(350), 0.35);
        assert_eq!(ms_to_seconds(0), 0.0);
        assert_eq!(ms_to_seconds(1000), 1.0);
        assert_eq!(serde_json::to_string(&ms_to_seconds(1200)).unwrap(), "1.2");
        assert_ne!(
            serde_json::to_string(&(0.8_f64 + 0.4_f64)).unwrap(),
            "1.2",
            "the premise: the accumulated float serializes as 1.2000000000000002"
        );
    }

    /// Every value the grammar admits survives the seconds boundary, and the
    /// artifact's rendering agrees with the message's rendering at all of them.
    ///
    /// `serde_json` (ryu) appends a mandatory `.0` to an integral double, so
    /// `1000` ms renders `1.0` in the artifact and `1` in a message. That is the
    /// only difference the two renderings are permitted: the significant digits
    /// must agree everywhere, because a diagnostic that names a different
    /// decimal from the one the artifact carries is a diagnostic an author
    /// cannot act on.
    #[test]
    fn seconds_emission_round_trips_every_admissible_value() {
        for ms in 0..10_000_i64 {
            let s = ms_to_seconds(ms);
            assert_eq!((s * 1000.0).round() as i64, ms, "ms {ms} did not survive");
            let authored = fmt_seconds(ms);
            let expected = if authored.contains('.') {
                authored.clone()
            } else {
                format!("{authored}.0")
            };
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                expected,
                "artifact and message rendering disagree at ms {ms}"
            );
        }
    }

    /// Diagnostics MUST print the authored decimal, not a reconstructed float.
    #[test]
    fn formats_as_the_authored_decimal() {
        assert_eq!(fmt_seconds(1200), "1.2");
        assert_eq!(fmt_seconds(350), "0.35");
        assert_eq!(fmt_seconds(1005), "1.005");
        assert_eq!(fmt_seconds(1000), "1");
        assert_eq!(fmt_seconds(0), "0");
        assert_eq!(fmt_seconds(1), "0.001");
        assert_eq!(fmt_seconds(1600), "1.6");
    }
}
