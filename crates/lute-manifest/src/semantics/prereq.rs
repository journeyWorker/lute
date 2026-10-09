//! The `after` prerequisite formula shared by the checker, compiler and
//! runtime: a pure boolean formula over `visited("id")` / `completed("id")` /
//! `active("id")` atoms combined with `&&` / `||`. Parsing and its diagnostics
//! live in `lute-check` (`parse_prereq`); the compiler serializes the parsed
//! formula into the IR so the runtime never parses `after` text.
//!
//! JSON shape (externally tagged, lowercase): `{"visited":"id"}`,
//! `{"completed":"q"}`, `{"active":"q"}`, `{"and":[l,r]}`, `{"or":[l,r]}`.

use serde::{Deserialize, Serialize};

/// The parsed `after` prerequisite formula: a boolean expression over
/// `visited`/`completed`/`active` atoms, closed under `&&`/`||`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrereqFormula {
    Visited(String),
    Completed(String),
    /// lang 0.8.0 `active("questId")`: the quest reached the `active`
    /// lifecycle state. A STRICTLY WEAKER claim than [`Self::Completed`] —
    /// graph-wise identical, envelope-wise weaker (only that the quest
    /// started, never that its completion writes landed).
    Active(String),
    And(Box<PrereqFormula>, Box<PrereqFormula>),
    Or(Box<PrereqFormula>, Box<PrereqFormula>),
}

/// A single leaf condition flattened out of a [`PrereqFormula`] by [`atoms`]
/// (edge-extraction helper for later connectivity tasks).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Atom {
    Visited(String),
    Completed(String),
    /// lang 0.8.0: see [`PrereqFormula::Active`]. Targets a QUEST node,
    /// exactly like [`Self::Completed`].
    Active(String),
}

/// Flatten a [`PrereqFormula`] into its leaf atoms (edge-extraction helper for
/// later connectivity tasks — graph assembly reads these to know which
/// `visited`/`completed`/`active` targets an `after` formula depends on).
pub fn atoms(f: &PrereqFormula) -> Vec<Atom> {
    let mut out = Vec::new();
    collect_atoms(f, &mut out);
    out
}

fn collect_atoms(f: &PrereqFormula, out: &mut Vec<Atom>) {
    match f {
        PrereqFormula::Visited(id) => out.push(Atom::Visited(id.clone())),
        PrereqFormula::Completed(id) => out.push(Atom::Completed(id.clone())),
        PrereqFormula::Active(id) => out.push(Atom::Active(id.clone())),
        PrereqFormula::And(l, r) | PrereqFormula::Or(l, r) => {
            collect_atoms(l, out);
            collect_atoms(r, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_shape_is_externally_tagged_lowercase() {
        let f = PrereqFormula::And(
            Box::new(PrereqFormula::Visited("s".into())),
            Box::new(PrereqFormula::Or(
                Box::new(PrereqFormula::Completed("q".into())),
                Box::new(PrereqFormula::Active("r".into())),
            )),
        );
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"and": [
                {"visited": "s"},
                {"or": [{"completed": "q"}, {"active": "r"}]}
            ]})
        );
        assert_eq!(serde_json::from_value::<PrereqFormula>(json).unwrap(), f);
    }
}
