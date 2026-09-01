//! The `auto-anchor-on-show` rule's IMPLICIT `anchor` domain read (dsl 0.9.0
//! D-D), end-to-end through `check()`.
//!
//! `::auto` without an `anchor` attribute is injected at the `anchor` domain's
//! declared `default:`. Nothing in the document names `anchor` on that path, so
//! directive validation — which walks AUTHORED attrs only — cannot see the
//! dependency: before this pin, a project that declared `action` but forgot
//! `anchor` checked CLEAN and silently lost the default-anchor command 0.8.0
//! emitted unconditionally. An undeclared slot must be an error, never a silent
//! behavior change, so the reducer reports the implicit read itself.
//!
//! All three rows below use the INLINE declaration route (a document's own
//! frontmatter); the imported-`uses:` and plugin-`enums` routes feed the SAME
//! merged `domains` map the reducer reads, so one route pins the behavior.

use lute_check::{check, CheckInput, Mode};
use lute_core_span::Severity;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;

/// `action` declared in the 0.9.0 long form. Every scene below needs it, since
/// `::auto{action=…}` is itself a domain-typed attr.
const ACTION: &str = "  action:\n    members: [show, hide]\n    exits: [hide]\n";

/// `anchor` declared in the 0.9.0 long form (`default:` mandatory for the slot).
const ANCHOR: &str = "  anchor:\n    members: [left, center, right]\n    default: center\n";

/// A scene whose frontmatter declares `enums:` inline, staging `bianca` through
/// the `::auto` in `body`.
fn scene(enums: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 1\nenums:\n{enums}---\n\
         ## Shot 1.\n{body}\n@bianca: Here I am.\n"
    )
}

/// Run the real `check()` over `text` and return its error-severity codes.
fn error_codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    let mut codes: Vec<String> = check(&input)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    codes.sort();
    codes.dedup();
    codes
}

/// Row 3 of the release measurement: `action` declared, `anchor` NOT declared,
/// no `anchor` attribute. The `::auto` still reads `anchor`'s `default:`, so the
/// missing declaration is an error — it used to check clean and drop the
/// command.
#[test]
fn implicit_anchor_read_without_the_domain_is_an_error() {
    let codes = error_codes(&scene(
        ACTION,
        "::auto{character=\"bianca\" action=\"show\"}",
    ));
    assert!(
        codes.contains(&"E-DOMAIN-UNKNOWN".to_string()),
        "expected E-DOMAIN-UNKNOWN for the implicit `anchor` read, got {codes:?}"
    );
}

/// Row 2: the same document with `anchor` declared checks clean. The regression
/// guard — the diagnostic above must not cost the working path.
#[test]
fn implicit_anchor_read_with_the_domain_checks_clean() {
    let text = scene(
        &format!("{ACTION}{ANCHOR}"),
        "::auto{character=\"bianca\" action=\"show\"}",
    );
    assert_eq!(error_codes(&text), Vec::<String>::new());
}

/// An EXPLICIT `anchor` with no declared `anchor` domain already errored before
/// this change (at the attribute). Pinned so the authored and implicit reads of
/// the same domain stay consistent — one error each, from one owner each.
#[test]
fn explicit_anchor_without_the_domain_is_still_an_error() {
    let codes = error_codes(&scene(
        ACTION,
        "::auto{character=\"bianca\" action=\"show\" anchor=\"center\"}",
    ));
    assert!(
        codes.contains(&"E-DOMAIN-UNKNOWN".to_string()),
        "expected E-DOMAIN-UNKNOWN at the `anchor` attribute, got {codes:?}"
    );
}
