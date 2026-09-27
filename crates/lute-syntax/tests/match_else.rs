//! A fallback arm spelled `<else>` / `<default>` inside a `<match>` is one
//! error naming `<otherwise>`; its body and close tag add nothing more.

const HDR: &str = "---\nkind: scene\nid: s\n---\n## Shot 1.\n";

fn errors(body: &str) -> Vec<lute_core_span::Diagnostic> {
    let (_, diags) = lute_syntax::parse(&format!("{HDR}{body}"));
    diags
        .into_iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .collect()
}

#[test]
fn else_in_a_match_is_one_error_naming_otherwise() {
    for tag in ["else", "default"] {
        let body = format!(
            "<match on=\"run.mood\">\n  <when is=\"calm\">\n    @narrator: Calm.\n  </when>\n  \
             <{tag}>\n    @narrator: Other.\n  </{tag}>\n</match>\n@narrator: After.\n"
        );
        let errs = errors(&body);
        assert_eq!(errs.len(), 1, "{tag}: {errs:?}");
        assert!(
            errs[0].message.contains("did you mean `<otherwise>`?"),
            "{}",
            errs[0].message
        );
    }
    // A one-line form skips only its own line.
    let errs = errors(
        "<match on=\"run.mood\">\n  <else>@narrator: Other.</else>\n  <otherwise>\n    \
         @narrator: Fine.\n  </otherwise>\n</match>\n",
    );
    assert_eq!(errs.len(), 1, "{errs:?}");
}
