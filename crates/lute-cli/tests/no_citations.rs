//! Author-facing help cites no spec section and names no ticket: `lute
//! --help` and every (sub)subcommand's `--help` are scanned.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn help(args: &[&str]) -> String {
    let out = Command::new(BIN).args(args).arg("--help").output().unwrap();
    assert!(out.status.success(), "{args:?} --help failed");
    String::from_utf8(out.stdout).unwrap()
}

/// The subcommand names a help page lists under `Commands:` (`help` aside).
fn subcommands(page: &str) -> Vec<String> {
    page.lines()
        .skip_while(|l| !l.starts_with("Commands:"))
        .skip(1)
        .take_while(|l| !l.trim().is_empty())
        .filter(|l| l.starts_with("  ") && !l.starts_with("   "))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|c| *c != "help")
        .map(str::to_string)
        .collect()
}

/// The first spec citation or ticket id `text` carries: `§`, `dsl 0.`,
/// `dsl 20…`, `Appendix`, or a whole-word `T3-26` / `ML-L15` /
/// `D1-quarantined`.
fn citation_in(text: &str) -> Option<&str> {
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

#[test]
fn help_pages_cite_no_spec_or_ticket() {
    let root = help(&[]);
    let subs = subcommands(&root);
    assert!(subs.len() >= 10, "{subs:?}");
    let mut pages = vec![(String::new(), root)];
    for sub in &subs {
        let page = help(&[sub]);
        for nested in subcommands(&page) {
            pages.push((format!("{sub} {nested}"), help(&[sub, &nested])));
        }
        pages.push((sub.clone(), page));
    }
    for (cmd, page) in &pages {
        if let Some(hit) = citation_in(page) {
            let line = page.lines().find(|l| l.contains(hit)).unwrap_or(hit);
            panic!("`lute {cmd} --help` cites `{hit}`: {line}");
        }
    }
}
