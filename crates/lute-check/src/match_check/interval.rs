//! Numeric coverage over [`Domain::Number`] (dsl 0.18.0 §4): closed intervals
//! and their merged union.

use super::*;

/// A closed interval over the extended reals (dsl 0.18.0 §4): a point `n` is
/// `[n, n]`, an open range end is `±∞`. Both ends inclusive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Interval {
    pub(crate) lo: f64,
    pub(crate) hi: f64,
}

impl Interval {
    /// The interval a classified numeric `is=` literal matches — a point or
    /// a range — or `None` for every non-numeric literal kind.
    pub(crate) fn of(lit: &IsLiteral) -> Option<Self> {
        match lit {
            IsLiteral::Num(n) => Some(Self { lo: *n, hi: *n }),
            IsLiteral::Range(r) => Some(Self {
                lo: r.lo.unwrap_or(f64::NEG_INFINITY),
                hi: r.hi.unwrap_or(f64::INFINITY),
            }),
            IsLiteral::Bool(_) | IsLiteral::Unset | IsLiteral::Str(_) => None,
        }
    }

    pub(super) fn is_point(self) -> bool {
        self.lo == self.hi
    }

    /// The closed interval both `self` and `other` contain, `None` when
    /// they are disjoint.
    pub(super) fn intersect(self, other: Self) -> Option<Self> {
        let lo = self.lo.max(other.lo);
        let hi = self.hi.min(other.hi);
        (lo <= hi).then_some(Self { lo, hi })
    }
}

/// Coverage over [`Domain::Number`] (dsl 0.18.0 §4): the union of closed
/// intervals, kept sorted, disjoint, and maximally merged — two intervals
/// sharing even one point fuse (`..0` + `0..` is the whole line). The reals
/// are dense, so any two separated spans leave a non-empty open gap.
#[derive(Clone, Debug, Default)]
pub(crate) struct NumCoverage {
    spans: Vec<Interval>,
}

impl NumCoverage {
    pub(crate) fn add(&mut self, iv: Interval) {
        let at = self.spans.partition_point(|s| s.lo < iv.lo);
        self.spans.insert(at, iv);
        let mut merged: Vec<Interval> = Vec::with_capacity(self.spans.len());
        for s in self.spans.drain(..) {
            match merged.last_mut() {
                Some(last) if s.lo <= last.hi => last.hi = last.hi.max(s.hi),
                _ => merged.push(s),
            }
        }
        self.spans = merged;
    }

    /// Whether `iv` lies wholly inside the union (a connected interval
    /// inside a union of maximal spans lies inside ONE span).
    pub(crate) fn contains(&self, iv: Interval) -> bool {
        self.spans.iter().any(|s| s.lo <= iv.lo && iv.hi <= s.hi)
    }

    /// The `W-OVERLAP-ARMS` test (dsl 0.18.0 §4): a point literal (or a
    /// degenerate `n..n` range) overlaps when it lies inside the union. A
    /// wider range NEVER overlaps: partial overlap is the descending-threshold
    /// cascade idiom (`3..` then `1..`, first match wins), and full
    /// containment is `E-ARM-DEAD`'s subsumption, not this warning.
    pub(crate) fn overlaps(&self, iv: Interval) -> bool {
        iv.is_point() && self.contains(iv)
    }

    /// Whether the union is the whole real line.
    pub(crate) fn covers_all(&self) -> bool {
        matches!(self.spans.as_slice(), [s] if s.lo == f64::NEG_INFINITY && s.hi == f64::INFINITY)
    }

    /// The first (lowest) uncovered stretch of the line, phrased for the
    /// `E-NONEXHAUSTIVE` message; `None` when [`Self::covers_all`].
    pub(crate) fn first_gap(&self) -> Option<String> {
        let Some(first) = self.spans.first() else {
            return Some("no number is covered".to_string());
        };
        if first.lo > f64::NEG_INFINITY {
            return Some(format!(
                "numbers below {} are not covered",
                fmt_num(first.lo)
            ));
        }
        if let Some(pair) = self.spans.windows(2).next() {
            return Some(format!(
                "numbers strictly between {} and {} are not covered",
                fmt_num(pair[0].hi),
                fmt_num(pair[1].lo)
            ));
        }
        (first.hi < f64::INFINITY)
            .then(|| format!("numbers above {} are not covered", fmt_num(first.hi)))
    }

    /// The whole numbers in `lo..=hi` the union leaves uncovered.
    pub(crate) fn uncovered_ints(&self, lo: i64, hi: i64) -> Vec<i64> {
        (lo..=hi)
            .filter(|k| {
                let p = *k as f64;
                !self.contains(Interval { lo: p, hi: p })
            })
            .collect()
    }
}

/// Shortest round-trip rendering of a finite bound (`2`, not `2.0`; `-0`
/// prints as `0`).
fn fmt_num(n: f64) -> String {
    if n == 0.0 {
        "0".to_string()
    } else {
        n.to_string()
    }
}
