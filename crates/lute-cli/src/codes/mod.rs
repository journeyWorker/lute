//! The diagnostic-code registry (dsl 0.27.0 §9, T3-17): every code a `lute`
//! surface emits — `check`, `check-project`, `compile`, `trace`, `play`,
//! `test`, `lint`, the editor — with its grade, one plain sentence saying what
//! raises it, and the spec sections behind it. Messages carry no spec
//! citations (`lute_core_span::plain_message`); this is where they went.
//!
//! Three things read it: `lute --explain <CODE>` ([`explain`]), the `--deny
//! <CODE>` universe ([`parse_deny_code`]), and the website's diagnostics
//! reference, whose English page is [`reference_page`] verbatim and whose
//! Korean twin keeps one section per code in the same order (both pinned by
//! the tests below, which also fail on a code some crate emits and this
//! table lacks).

use std::process::ExitCode;

use lute_core_span::DIAGNOSTICS_REFERENCE;

/// One registered code.
pub(crate) struct Code {
    pub(crate) code: &'static str,
    /// One plain sentence: the situation that raises the code.
    pub(crate) summary: &'static str,
    /// The spec sections behind it, most relevant first (may be empty).
    pub(crate) spec: &'static [&'static str],
}

impl Code {
    /// `error` for an `E-` code, `warning` for a `W-` one. The severity a
    /// run prints can only be raised (`--deny` promotes a warning); an `E-`
    /// code never prints as a warning — `--wip` reports its downgrades as
    /// `W-WIP`.
    pub(crate) fn grade(&self) -> &'static str {
        if self.code.starts_with("W-") {
            "warning"
        } else {
            "error"
        }
    }
}

/// The registered code named `code` (exact, upper-case).
pub(crate) fn lookup(code: &str) -> Option<&'static Code> {
    CODES
        .binary_search_by(|c| c.code.cmp(code))
        .ok()
        .map(|i| &CODES[i])
}

/// ` — did you mean `CODE`?` for the registered code nearest a mistyped one,
/// else nothing.
fn code_suggestion(raw: &str) -> String {
    lute_manifest::suggest::nearest(raw, CODES.iter().map(|c| c.code), 3)
        .map(|c| format!(" — did you mean `{c}`?"))
        .unwrap_or_default()
}

/// clap `value_parser` for `--deny <CODE>`: any registered code (spec §5). A
/// typo or a made-up string is a clap usage error (exit 2), never a promotion
/// that silently protects nothing.
pub(crate) fn parse_deny_code(raw: &str) -> Result<String, String> {
    match lookup(raw) {
        Some(c) => Ok(c.code.to_string()),
        None => Err(format!(
            "unknown diagnostic code `{raw}`{}; a typo'd `--deny` would silently protect \
             nothing, and every code is listed at {DIAGNOSTICS_REFERENCE}",
            code_suggestion(raw)
        )),
    }
}

/// clap `value_parser` for `lute --explain <CODE>`: a registered code, in
/// any letter case.
pub(crate) fn parse_explain_code(raw: &str) -> Result<&'static Code, String> {
    let code = raw.trim().to_ascii_uppercase();
    lookup(&code).ok_or_else(|| {
        format!(
            "unknown diagnostic code `{raw}`{}; every code is listed at {DIAGNOSTICS_REFERENCE}",
            code_suggestion(&code)
        )
    })
}

/// `lute --explain <CODE>`: the code and its grade, its sentence, the spec
/// sections behind it, and its section of the website reference.
pub(crate) fn explain(code: &Code) -> ExitCode {
    match crate::write_stdout(&explain_text(code)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

/// The codes whose entry also lists every reserved name (the one table,
/// [`lute_manifest::reserved`]): `--explain` prints it, and the reference page
/// links the *Reserved names* page that shows it.
const RESERVED_NAME_CODES: [&str; 2] = ["E-RESERVED-NAME", "E-PLUGIN-RESERVED-NAME"];

/// [`explain`]'s text. A reserved-name code also lists every reserved name.
fn explain_text(code: &Code) -> String {
    let mut out = format!("{} ({})\n\n{}\n\n", code.code, code.grade(), code.summary);
    if RESERVED_NAME_CODES.contains(&code.code) {
        out.push_str(&lute_manifest::reserved::render_text());
        out.push('\n');
    }
    if !code.spec.is_empty() {
        out.push_str(&format!("Spec: {}\n", code.spec.join(", ")));
    }
    if let Some(url) = lute_core_span::doc_url(code.code) {
        out.push_str(&format!("More: {url}\n"));
    }
    out
}

/// The website's English diagnostics reference
/// (`packages/website/src/content/docs/reference/diagnostics.md`), rendered
/// from [`CODES`]: one `### <CODE>` section per code — the anchor
/// [`lute_core_span::doc_url`] and the editor link to — errors first.
#[cfg(test)]
pub(crate) fn reference_page() -> String {
    let mut out = String::from(
        "---\n\
         title: Diagnostics reference\n\
         description: \"Every diagnostic code Lute reports: what raises it, and the spec sections \
         behind it.\"\n\
         ---\n\n\
         <!-- Generated from crates/lute-cli/src/codes.rs. Edit the registry, then run\n     \
         LUTE_BLESS_DIAGNOSTICS=1 cargo test -p lute-cli --bins codes -->\n\n\
         Every diagnostic Lute prints carries a code. `E-` codes are errors: the document fails \
         the check and `lute check` exits 1. `W-` codes are warnings: the document passes, \
         unless `--deny <CODE>` or `--deny-warnings` promotes them. An `E-` code is never \
         printed as a warning: `check-project --wip` reports the dead guards it spares as \
         `W-WIP`, and the message names the error code the same guard has without the flag. \
         `lute --explain <CODE>` prints a code's entry below in the terminal, and an editor \
         links each code to its section here.\n\n\
         A message says what is wrong in plain words. The spec sections behind a code are listed \
         under it, each linked to its proposal, and `--json` output carries them in each \
         diagnostic's `spec` field.\n\n\
         A position `file:line:column` counts lines and columns from 1, and the column counts \
         characters, not bytes: a Korean syllable or an emoji before the error is one column. \
         `--json` `span.column` and `lute scenario … reach` `causes[].column` are the same \
         number. The language server reports UTF-16 positions, as LSP requires.\n",
    );
    for (heading, grade) in [("Errors", "error"), ("Warnings", "warning")] {
        out.push_str(&format!("\n## {heading}\n"));
        for c in CODES.iter().filter(|c| c.grade() == grade) {
            out.push_str(&format!("\n### {}\n\n{}\n", c.code, c.summary));
            if RESERVED_NAME_CODES.contains(&c.code) {
                out.push_str(
                    "\nEvery reserved name, where it is refused and what to write instead: \
                     [Reserved names](/reference/reserved-names/).\n",
                );
            }
            if !c.spec.is_empty() {
                let links: Vec<String> = c.spec.iter().map(|s| spec_link(s)).collect();
                out.push_str(&format!("\nSpec: {}\n", links.join(", ")));
            }
        }
    }
    out
}

/// A spec citation as a Markdown link: `dsl X.Y.Z …` to that proposal in
/// the repository (the normative text), anything else — an unversioned
/// `dsl §7.6`, a plugin-system section — to the site's specification index.
#[cfg(test)]
pub(crate) fn spec_link(spec: &str) -> String {
    let version = spec
        .strip_prefix("dsl ")
        .and_then(|rest| rest.split(' ').next())
        .filter(|v| {
            v.split('.').count() == 3
                && v.split('.')
                    .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        });
    let proposals =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/proposals/scenario-dsl");
    match version {
        Some(v) if proposals.join(format!("{v}.md")).is_file() => format!(
            "[{spec}](https://github.com/journeyWorker/lute/blob/main/docs/proposals/\
             scenario-dsl/{v}.md)"
        ),
        _ => format!("[{spec}](/spec/)"),
    }
}

mod registry;

pub(crate) use registry::CODES;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// `E-…` / `W-…`: upper-case letters, digits and inner dashes.
    fn is_code(c: &str) -> bool {
        let Some(rest) = c.strip_prefix("E-").or_else(|| c.strip_prefix("W-")) else {
            return false;
        };
        !rest.is_empty()
            && !rest.ends_with('-')
            && rest
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
    }

    /// Sorted and deduplicated (so [`lookup`] may binary-search), every code
    /// shaped `E-`/`W-`, and every sentence plain: one sentence, no spec
    /// citation — the citations are the `spec` column's job.
    #[test]
    fn registry_is_sorted_wellformed_and_plain() {
        for w in CODES.windows(2) {
            assert!(
                w[0].code < w[1].code,
                "{} / {} out of order",
                w[0].code,
                w[1].code
            );
        }
        for c in CODES {
            assert!(is_code(c.code), "malformed code {}", c.code);
            assert!(c.summary.ends_with('.'), "{}: {}", c.code, c.summary);
            assert_eq!(
                lute_core_span::plain_message(c.summary),
                c.summary,
                "{}: the sentence cites the spec",
                c.code
            );
            assert!(lookup(c.code).is_some_and(|l| l.code == c.code));
        }
    }

    /// The first spec citation or ticket id `text` carries: `§`, `dsl 0.`,
    /// `dsl 20…`, `Appendix`, or a whole-word `T3-26` / `ML-L15` /
    /// `D1-quarantined`.
    pub(crate) fn citation_in(text: &str) -> Option<&str> {
        for needle in ["§", "dsl 0.", "dsl 20", "Appendix"] {
            if text.contains(needle) {
                return Some(needle);
            }
        }
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .find(|w| {
                let w = w.trim_end_matches('-');
                w.strip_prefix('T')
                    .and_then(|r| r.split_once('-'))
                    .is_some_and(|(a, b)| digits(a) && digits(b))
                    || w.strip_prefix("ML-L").is_some_and(digits)
                    || w.strip_prefix('D')
                        .and_then(|r| r.strip_suffix("-quarantined"))
                        .is_some_and(digits)
            })
    }

    /// Every code's sentence is author-facing: no spec section, no ticket id.
    #[test]
    fn summaries_cite_no_spec_or_ticket() {
        for c in CODES {
            assert_eq!(citation_in(c.summary), None, "{}: {}", c.code, c.summary);
        }
        assert_eq!(citation_in("see (T3-26) here"), Some("T3-26"));
        assert_eq!(
            citation_in("the D1-quarantined evaluator"),
            Some("D1-quarantined")
        );
        assert_eq!(citation_in("UTF-8 and T-shirts, ML-Lx"), None);
    }

    /// Drift guard: every `"E-…"` / `"W-…"` literal in any crate's sources
    /// is a registered code, so a new code cannot ship without its sentence,
    /// its `--explain`, its reference section and its `--deny`. A registered
    /// code no crate emits any more is harmless; a missing one is the defect.
    #[test]
    fn every_emitted_code_is_registered() {
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut stack: Vec<PathBuf> = std::fs::read_dir(&crates)
            .unwrap()
            .flatten()
            .map(|e| e.path().join("src"))
            .filter(|p| p.is_dir())
            .collect();
        assert!(stack.len() >= 10, "found only {stack:?}");
        let mut missing = BTreeSet::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for literal in text.split('"').skip(1).step_by(2) {
                        if is_code(literal) && lookup(literal).is_none() {
                            missing.insert(format!("{literal} ({})", path.display()));
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "diagnostic code(s) emitted but not registered in crates/lute-cli/src/codes.rs: \
             {missing:?}"
        );
    }

    fn docs() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/website/src/content/docs")
    }

    /// The English reference is the registry, rendered — the anchors
    /// `doc_url` and the editor link to exist, and the text cannot drift.
    #[test]
    fn the_reference_page_is_the_registry() {
        let path = docs().join("reference/diagnostics.md");
        let want = reference_page();
        if std::env::var_os("LUTE_BLESS_DIAGNOSTICS").is_some() {
            std::fs::write(&path, &want).unwrap();
            return;
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == want,
            "{} is stale; run `LUTE_BLESS_DIAGNOSTICS=1 cargo test -p lute-cli --bins codes`",
            path.display()
        );
    }

    /// The Korean twin holds one `### <CODE>` section per registered code, in
    /// the registry's order, each naming the same spec sections with the same
    /// links ([`spec_link`]).
    #[test]
    fn the_korean_reference_page_has_every_code() {
        let path = docs().join("ko/reference/diagnostics.md");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut sections: Vec<(String, Vec<String>)> = Vec::new();
        for line in text.lines() {
            if let Some(code) = line.strip_prefix("### ") {
                sections.push((code.to_string(), Vec::new()));
            } else if let Some(spec) = line.strip_prefix("명세: ") {
                let (_, s) = sections.last_mut().expect("a spec line under a section");
                *s = spec.split(", ").map(str::to_string).collect();
            }
        }
        let want: Vec<(String, Vec<String>)> = ["error", "warning"]
            .iter()
            .flat_map(|g| CODES.iter().filter(move |c| c.grade() == *g))
            .map(|c| {
                (
                    c.code.to_string(),
                    c.spec.iter().map(|s| spec_link(s)).collect(),
                )
            })
            .collect();
        assert_eq!(
            sections,
            want,
            "{} drifted from the registry",
            path.display()
        );
    }

    #[test]
    fn every_registry_code_has_exactly_one_evidence_classification() {
        let registry: std::collections::BTreeSet<&str> = CODES.iter().map(|code| code.code).collect();
        let table = lute_check::evidence::DIAGNOSTIC_CLASSIFICATIONS;
        let mut classified = std::collections::BTreeSet::new();
        for (code, _) in table {
            assert!(classified.insert(*code), "duplicate evidence classification for {code}");
            assert!(registry.contains(code), "evidence table has unregistered code {code}");
        }
        assert_eq!(classified, registry, "registry and evidence table drifted");
    }

    /// The reserved-names reference is the one table: the English page holds
    /// its rows verbatim, the Korean twin names every reserved name, and
    /// `lute --explain E-RESERVED-NAME` prints the list.
    #[test]
    fn the_reserved_names_reference_is_the_table() {
        const BEGIN: &str = "<!-- reserved-names:begin -->\n";
        const END: &str = "<!-- reserved-names:end -->";
        let path = docs().join("reference/reserved-names.md");
        let en = std::fs::read_to_string(&path).unwrap();
        let table = lute_manifest::reserved::render_markdown();
        let (Some(b), Some(e)) = (en.find(BEGIN), en.find(END)) else {
            panic!("{} lacks the {BEGIN:?}…{END:?} markers", path.display());
        };
        let region = &en[b + BEGIN.len()..e];
        if std::env::var_os("LUTE_BLESS_DIAGNOSTICS").is_some() {
            let blessed = format!("{}{table}{}", &en[..b + BEGIN.len()], &en[e..]);
            std::fs::write(&path, blessed).unwrap();
        } else {
            assert!(
                region == table,
                "{} drifted from lute_manifest::reserved::GROUPS; run \
                 `LUTE_BLESS_DIAGNOSTICS=1 cargo test -p lute-cli --bins codes`",
                path.display()
            );
        }
        let ko = std::fs::read_to_string(docs().join("ko/reference/reserved-names.md")).unwrap();
        let explained = explain_text(lookup("E-RESERVED-NAME").unwrap());
        for group in lute_manifest::reserved::GROUPS {
            for name in group.names {
                assert!(ko.contains(&format!("`{name}`")), "ko page lacks `{name}`");
                assert!(explained.contains(name), "--explain lacks `{name}`");
            }
        }
    }
}
