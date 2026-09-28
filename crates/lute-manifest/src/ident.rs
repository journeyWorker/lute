//! The one identifier rule. Every name an author writes — a scene, beat,
//! entry, quest, objective, branch, hub, choice or mark id; a document id
//! segment; a `share` key; a season, relation, def, enum or entity-kind name;
//! an enum or entity member; a component or template param; an occasion or
//! event a plugin declares — is an identifier: a letter, then letters, digits
//! or `_`. A dotted id joins identifiers with `.`.
//!
//! A name Lute declares is an identifier; an id the ENGINE owns — external
//! data, like a CEL map key — is written as the engine spells it: an
//! [`is_engine_id`] / [`is_engine_ref`]. Those are a beat's or entry's
//! `target` where no entity kind lists the members (an untyped occasion, an
//! `open:` kind) and an entry's `category`.
//!
//! Each slot reports a bad name under its own code; this module owns the
//! predicates and the wording, so every slot says the same thing and names
//! the same camelCase spelling.

/// `true` when `s` is an identifier: an ASCII letter, then ASCII letters,
/// digits or `_`.
pub fn is_ident(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `true` when `s` is identifiers joined by `.` (no empty segment).
pub fn is_dotted_ident(s: &str) -> bool {
    s.split('.').all(is_ident)
}

/// `true` when `s` is an engine id: an ASCII letter, then ASCII letters,
/// digits, `_` or `-` (`rusty-key`). The shape of an id the engine owns —
/// Lute never reads it as a CEL name, so it keeps the engine's spelling.
pub fn is_engine_id(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic()) && bytes.all(is_engine_byte)
}

/// `true` when `s` is an engine reference: an [`is_engine_id`], then zero or
/// more `.`-separated parts of ASCII letters, digits, `_` or `-`
/// (`item.rusty-key`, `place.lab-b2`).
pub fn is_engine_ref(s: &str) -> bool {
    let mut parts = s.split('.');
    parts.next().is_some_and(is_engine_id)
        && parts.all(|part| !part.is_empty() && part.bytes().all(is_engine_byte))
}

fn is_engine_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// [`is_engine_ref`]'s shape, in words, for messages.
pub const ENGINE_REF_SHAPE: &str = "an engine id such as `npc.maud` or `item.rusty-key` — a \
     letter, then letters, digits, `_` or `-`, in `.`-separated parts";

/// Why `name` (the `what` of its slot) is not an [`is_engine_id`]; `None`
/// when it is.
pub fn engine_id_fault(what: &str, name: &str) -> Option<String> {
    (!is_engine_id(name)).then(|| {
        format!("{what} `{name}` is not an engine id: a letter, then letters, digits, `_` or `-`")
    })
}

/// `name` as one camelCase identifier: split at every character that is not
/// a letter, digit or `_`, each later part capitalised (`lamp-duty` →
/// `lampDuty`, `isolation.hush` → `isolationHush`).
pub fn camel_case(name: &str) -> String {
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

/// `name` with each `.`-separated segment made one camelCase identifier
/// (`door-notes.lamp-duty` → `doorNotes.lampDuty`).
pub fn camel_case_dotted(name: &str) -> String {
    name.split('.')
        .map(camel_case)
        .collect::<Vec<_>>()
        .join(".")
}

/// Why `name` (the `what` of its slot: "scene id", "`share` key", …) is not
/// an identifier; `None` when it is. Names the camelCase spelling when there
/// is one.
pub fn ident_fault(what: &str, name: &str) -> Option<String> {
    (!is_ident(name)).then(|| fault_message(what, name, camel_case(name), false))
}

/// [`ident_fault`] for a dotted id: every `.`-separated segment must be an
/// identifier; the suggestion keeps the dots.
pub fn dotted_ident_fault(what: &str, name: &str) -> Option<String> {
    (!is_dotted_ident(name)).then(|| fault_message(what, name, camel_case_dotted(name), true))
}

fn fault_message(what: &str, name: &str, suggestion: String, dotted: bool) -> String {
    let (rule, fits): (_, fn(&str) -> bool) = if dotted {
        (
            "a dotted id: identifiers joined by `.`, each a letter, then letters, digits or `_`",
            is_dotted_ident,
        )
    } else {
        (
            "an identifier: a letter, then letters, digits or `_`",
            is_ident,
        )
    };
    let shown = if name.is_empty() {
        String::from("``")
    } else {
        format!("`{name}`")
    };
    if fits(&suggestion) {
        format!("{what} {shown} is not {rule} — write `{suggestion}`")
    } else {
        format!("{what} {shown} is not {rule}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_a_letter_then_letters_digits_or_underscore() {
        for good in ["a", "lampDuty", "lamp_duty", "s01ep01", "A9"] {
            assert!(is_ident(good), "{good}");
        }
        for bad in ["", "lamp-duty", "_x", "9a", "a.b", "a b", "é"] {
            assert!(!is_ident(bad), "{bad}");
        }
        assert!(is_dotted_ident("mira.s01ep01") && !is_dotted_ident("door-notes.a"));
        assert!(!is_dotted_ident("a..b") && !is_dotted_ident(""));
    }

    #[test]
    fn engine_ids_keep_the_engines_spelling() {
        for good in [
            "item.rusty-key",
            "place.lab-b2",
            "npc.maud",
            "rusty-key",
            "a.9-lives",
        ] {
            assert!(is_engine_ref(good), "{good}");
        }
        for bad in [
            "",
            "9item.key",
            "item..key",
            "item.",
            "item.a b",
            "kind:npc",
            "_x.y",
        ] {
            assert!(!is_engine_ref(bad), "{bad}");
        }
        assert!(is_engine_id("side-quest") && !is_engine_id("a.b") && !is_engine_id("-a"));
    }

    #[test]
    fn the_suggestion_is_camel_case() {
        assert_eq!(camel_case("lamp-duty"), "lampDuty");
        assert_eq!(camel_case("fade-in-up"), "fadeInUp");
        assert_eq!(camel_case("isolation.hush"), "isolationHush");
        assert_eq!(
            camel_case_dotted("door-notes.lamp-duty"),
            "doorNotes.lampDuty"
        );
        assert_eq!(
            ident_fault("`share` key", "nana-report").unwrap(),
            "`share` key `nana-report` is not an identifier: a letter, then letters, digits or \
             `_` — write `nanaReport`"
        );
        // No identifier spelling to offer: the rule alone.
        assert!(!ident_fault("id", "9-lives").unwrap().contains("write"));
        assert_eq!(ident_fault("id", "ok"), None);
    }
}
