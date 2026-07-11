//! Three-valued (Kleene/K3) trace value (dsl 0.4.0 §4.3) and why a value
//! went unknown.

/// Three-valued trace value (§4.3). `Unknown` is a VALUE, not an error — it
/// is produced, compared, and propagated through K3 logic exactly like
/// `Bool`/`Num`/`Str` propagate through ordinary CEL evaluation
/// ([`crate::eval::eval`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Num(f64),
    Str(String),
    Unknown,
}

/// `decide()`'s decided-constant fragment (dsl §5.1) is a strict SUBSET of
/// what `trace` evaluates — a `Decided` is always ground, never unknown —
/// so every `Decided` converts straight across.
impl From<lute_check::Decided> for Value {
    fn from(d: lute_check::Decided) -> Self {
        match d {
            lute_check::Decided::Bool(b) => Value::Bool(b),
            lute_check::Decided::Num(n) => Value::Num(n),
            lute_check::Decided::Str(s) => Value::Str(s),
        }
    }
}

/// Why something was unknown — drives the §4.5 `unresolved[]` report and
/// the §4.6 "supply it as a mock" hints.
#[derive(Clone, Debug, PartialEq)]
pub enum UnresolvedAtom {
    /// A state-path read with no effective value (§4.3: trace-write → mock
    /// seed → schema `default:` all miss).
    Path(String),
    /// Reserved for a non-derived fact-pattern reported unknown for a
    /// reason other than the `derive:true` rule below. `eval.rs`'s bounded
    /// scan explains every `holds`/`count` miss it can produce today as
    /// [`UnresolvedAtom::DerivedFact`]; this variant is here for a future
    /// producer (mock validation / walk reporting, Tasks 18–19) rather than
    /// left undeclared.
    Fact(String),
    /// `holds`/`count` over a `derive: true` relation with zero matching
    /// supplied facts (§4.2 rule 3 / §4.3: the Datalog fixpoint is never
    /// run here — the rendered pattern is the "supply it as a mock" hint,
    /// §4.6).
    DerivedFact(String),
    /// `now()`/`validAt(...)` — narrative time is engine-minted (§4.3) and
    /// has no mock surface.
    Time,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_decided_converts_bool_num_str() {
        assert_eq!(Value::from(lute_check::Decided::Bool(true)), Value::Bool(true));
        assert_eq!(Value::from(lute_check::Decided::Num(3.5)), Value::Num(3.5));
        assert_eq!(
            Value::from(lute_check::Decided::Str("x".to_string())),
            Value::Str("x".to_string())
        );
    }

    #[test]
    fn unknown_is_a_first_class_value_not_an_error() {
        // Unknown participates in equality/Debug like any other Value
        // variant — it is a K3 value, never a Result::Err or a panic.
        let v = Value::Unknown;
        assert_eq!(v, Value::Unknown);
        assert_ne!(v, Value::Bool(false));
        assert_ne!(v, Value::Num(0.0));
    }

    #[test]
    fn unresolved_atom_variants_are_distinguishable() {
        assert_ne!(
            UnresolvedAtom::Path("run.tip".into()),
            UnresolvedAtom::Fact("run.tip".into())
        );
        assert_ne!(
            UnresolvedAtom::DerivedFact("believesLocation(player, halsin, grove)".into()),
            UnresolvedAtom::Time
        );
    }
}
