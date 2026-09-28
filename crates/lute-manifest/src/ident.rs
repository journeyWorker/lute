//! The one name rule. Every name a condition reaches by a path, an index or
//! a fact argument — a scene, beat, entry, quest, objective, branch, hub,
//! choice or mark id; a document id segment; a `share` key; a season,
//! relation, enum or entity-kind name; an enum or entity member; an occasion
//! or event a plugin declares; a target or a category — is a
//! [name](is_name): letters, digits, `_` or `-`, not starting with `-`. A
//! dotted id joins names with `.`.
//!
//! A name that is also an [identifier](is_ident) may be written bare in a
//! condition (`quest.lampOut.state`); any name may be written quoted
//! (`quest["lamp-out"].state`). The two spellings are the same name.
//!
//! A name a condition reads bare, like a JavaScript variable — a def, a
//! def's param, a component or template param (`@name`) — has no quoted
//! spelling, so it is an identifier ([`ident_fault`]).
//!
//! Each slot reports a bad name under its own code; this module owns the
//! predicates and the wording, so every slot says the same thing.

/// `true` when `s` is an identifier — a name that may be written bare in a
/// condition: an ASCII letter or `_`, then ASCII letters, digits or `_`.
pub fn is_ident(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `true` when `s` is a name: one or more ASCII letters, digits, `_` or `-`,
/// not starting with `-` (`lampOut`, `lamp-out`, `001`).
pub fn is_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `true` when `s` is names joined by `.` (no empty segment).
pub fn is_dotted_name(s: &str) -> bool {
    s.split('.').all(is_name)
}

/// The name rule, in words, for messages.
const NAME_RULE: &str = "letters, digits, `_` or `-`, not starting with `-`";

/// Why `name` (the `what` of its slot: "scene id", "`share` key", …) is not
/// a name; `None` when it is.
pub fn name_fault(what: &str, name: &str) -> Option<String> {
    (!is_name(name)).then(|| format!("{what} {} is not a name: {NAME_RULE}", shown(name)))
}

/// [`name_fault`] for a dotted id: every `.`-separated segment must be a
/// name.
pub fn dotted_name_fault(what: &str, name: &str) -> Option<String> {
    (!is_dotted_name(name)).then(|| {
        format!(
            "{what} {} is not a dotted id: names joined by `.`, each {NAME_RULE}",
            shown(name)
        )
    })
}

/// Why `name` (the `what` of its slot) is not an [identifier](is_ident);
/// `None` when it is. For a name a condition reads bare, like a JavaScript
/// variable: a def and a component or template param (read as `@name`,
/// `sigil` `"@"`) and a def's param (read in the def's body, `sigil` `""`).
/// Names the identifier spelling when there is one (`lamp-lit` →
/// `lampLit`).
pub fn ident_fault(what: &str, name: &str, sigil: &str) -> Option<String> {
    if is_ident(name) {
        return None;
    }
    let mut message = format!(
        "{what} {} is not an identifier: it is read bare as `{sigil}{name}` — a letter or `_`, \
         then letters, digits or `_`",
        shown(name)
    );
    let suggestion = ident_spelling(name);
    if is_ident(&suggestion) {
        message.push_str(&format!("; write `{suggestion}`"));
    }
    Some(message)
}

/// `name` as one identifier: split at every character an identifier cannot
/// hold, each later part capitalised (`lamp-lit` → `lampLit`).
fn ident_spelling(name: &str) -> String {
    name.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|part| !part.is_empty())
        .enumerate()
        .map(|(i, part)| {
            let mut cs = part.chars();
            match cs.next() {
                Some(c) if i > 0 => c.to_ascii_uppercase().to_string() + cs.as_str(),
                Some(c) => c.to_string() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn shown(name: &str) -> String {
    if name.is_empty() {
        String::from("``")
    } else {
        format!("`{name}`")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_letters_digits_underscore_or_hyphen_not_leading_hyphen() {
        for good in [
            "a",
            "lampDuty",
            "lamp_duty",
            "lamp-duty",
            "zero-coke-001",
            "001",
            "9-lives",
            "_x",
            "a-",
        ] {
            assert!(is_name(good), "{good}");
        }
        for bad in [
            "", "-a", "a.b", "a b", "\"a\"", "'a'", "é", "a:b", "kind:npc",
        ] {
            assert!(!is_name(bad), "{bad}");
        }
        assert!(is_dotted_name("door-notes.lamp-duty") && is_dotted_name("mira.001"));
        assert!(!is_dotted_name("a..b") && !is_dotted_name("") && !is_dotted_name("a.-b"));
    }

    #[test]
    fn an_identifier_is_the_bare_writable_name() {
        for good in ["a", "lampDuty", "lamp_duty", "s01ep01", "A9", "_x"] {
            assert!(is_ident(good), "{good}");
        }
        for bad in ["", "lamp-duty", "9a", "001", "a.b", "a b", "é"] {
            assert!(!is_ident(bad), "{bad}");
        }
    }

    #[test]
    fn a_fault_lists_the_allowed_characters() {
        assert_eq!(
            name_fault("`share` key", "nana report").unwrap(),
            "`share` key `nana report` is not a name: letters, digits, `_` or `-`, not \
             starting with `-`"
        );
        assert_eq!(
            dotted_name_fault("scene id", "door notes.a").unwrap(),
            "scene id `door notes.a` is not a dotted id: names joined by `.`, each letters, \
             digits, `_` or `-`, not starting with `-`"
        );
        assert_eq!(
            name_fault("id", ""),
            Some("id `` is not a name: letters, digits, `_` or `-`, not starting with `-`".into())
        );
        assert_eq!(name_fault("id", "lamp-out"), None);
        assert_eq!(dotted_name_fault("id", "lore.tomas-doc"), None);
    }

    #[test]
    fn a_name_read_bare_is_an_identifier_and_the_fault_names_its_spelling() {
        assert_eq!(
            ident_fault("def", "lamp-lit", "@").unwrap(),
            "def `lamp-lit` is not an identifier: it is read bare as `@lamp-lit` — a letter or \
             `_`, then letters, digits or `_`; write `lampLit`"
        );
        // No identifier spelling to offer: the rule alone.
        assert!(!ident_fault("def `f` param", "9-lives", "")
            .unwrap()
            .contains("write"));
        assert_eq!(ident_fault("def", "lampLit", "@"), None);
    }
}
