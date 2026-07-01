use crate::schema::DirectiveDecl;

/// plugin §8.1 closed vocabulary — owned by the core; a plugin MUST NOT invent flags.
pub const SEMANTICS_VOCAB: &[&str] = &[
    "writes.sceneState", "writes.characterState", "reads.onStage",
    "mayExitCharacter", "usesAnchor", "isExit", "isStateful",
    "mutatesScene", "requiresAnchor", "cancelsPrevious", "bridgeCall",
];

#[derive(Clone, Debug, PartialEq)]
pub enum ManifestError {
    UnknownSemanticsFlag { directive: String, flag: String },
    DuplicateAttr { directive: String, attr: String },
}

pub fn validate_directive(d: &DirectiveDecl) -> Vec<ManifestError> {
    let mut errs = Vec::new();
    for flag in &d.semantics {
        if !SEMANTICS_VOCAB.contains(&flag.as_str()) {
            errs.push(ManifestError::UnknownSemanticsFlag { directive: d.name.clone(), flag: flag.clone() });
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for a in &d.attrs {
        if !seen.insert(a.name.clone()) {
            errs.push(ManifestError::DuplicateAttr { directive: d.name.clone(), attr: a.name.clone() });
        }
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DirectiveDecl, Lowering, AttrDecl};
    use crate::types::Type;

    fn dir(name: &str, semantics: &[&str]) -> DirectiveDecl {
        DirectiveDecl {
            name: name.into(), layer: None,
            attrs: vec![AttrDecl { name: "x".into(), required: false, ty: Type::Bool, default: None }],
            semantics: semantics.iter().map(|s| s.to_string()).collect(),
            state: None, effects: None, bridge: None,
            lower: Lowering::Builtin { kind: "builtin".into(), name: "noop".into() },
        }
    }

    #[test]
    fn unknown_semantics_flag_is_error() {
        let errs = validate_directive(&dir("d", &["writes.sceneState", "totallyMadeUp"]));
        assert!(errs.iter().any(|e| matches!(e, ManifestError::UnknownSemanticsFlag { flag, .. } if flag == "totallyMadeUp")));
    }

    #[test]
    fn known_semantics_flags_pass() {
        let errs = validate_directive(&dir("d", &["writes.sceneState", "bridgeCall"]));
        assert!(errs.is_empty());
    }

    #[test]
    fn duplicate_attr_name_is_error() {
        let mut d = dir("d", &[]);
        d.attrs.push(d.attrs[0].clone());
        let errs = validate_directive(&d);
        assert!(errs.iter().any(|e| matches!(e, ManifestError::DuplicateAttr { .. })));
    }
}
