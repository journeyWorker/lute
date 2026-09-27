//! Round-6 T1-24 — Ink/Yarn inline markup in line text and choice labels
//! shipped silently (`ok: silent.lute (0 warning(s))`). Single braces, a
//! ` // note`, a trailing `# tag` and a `[bracketed]` label are literal, so
//! each now warns naming what Lute writes; `{{…}}` interpolation, attribute
//! braces and a URL's `//` stay quiet. Also T3-7: `::greet{}` for a
//! component names `::use`, and T3-60: `::goto` names `::next`.
use lute_check::{check, CheckInput, ComponentSet, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;
use lute_test_vocab::vocab_snapshot;

fn check_body(body: &str) -> Vec<Diagnostic> {
    let text = format!(
        "---\nkind: scene\nid: lamp\nstate:\n  run.oil: {{ type: number, default: 1 }}\n  \
         run.knowsName: {{ type: bool, default: false }}\n---\n\n## Lamp Room\n\n{body}\n"
    );
    check(&CheckInput {
        text,
        uri: "scene".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: ComponentSet::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

fn warnings<'a>(diags: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    diags.iter().filter(|d| d.code == code).collect()
}

#[test]
fn single_brace_shapes_warn_with_the_lute_form() {
    for (line, want) in [
        ("@narrator: You have {run.oil} cans.", "`{{run.oil}}`"),
        ("@narrator: Oil: {$oil}.", "`{{run.oil}}`"),
        (
            "@narrator: {run.oil > 0: A can of oil sits by the lens.}",
            "`@narrator{when=\"run.oil > 0\"}: …`",
        ),
        (
            "@narrator: {run.knowsName: You know his name.|You do not.}",
            "no inline conditional text",
        ),
        (
            "@narrator: {~Fog|Mist|Rain} rolls in.",
            "no inline alternatives",
        ),
        // Yarn's string-table placeholder is filled by nothing in Lute.
        ("@narrator: You have {0} cans.", "`{{run.…}}`"),
        // A backslash escapes nothing in line text: it ships beside the
        // braces, so the escaped group still warns.
        (
            "@narrator: Escaped \\{run.oil\\} braces.",
            "the braces and the backslashes",
        ),
    ] {
        let diags = check_body(line);
        let hits = warnings(&diags, "W-TEXT-SINGLE-BRACE");
        assert_eq!(hits.len(), 1, "{line}: {diags:#?}");
        assert!(
            hits[0].message.contains(want),
            "{line}: {}",
            hits[0].message
        );
    }
}

// The warning sits on the brace group itself, not the whole line.
#[test]
fn single_brace_warning_is_anchored_at_the_group() {
    let diags = check_body("@narrator: You have {run.oil} cans.");
    let d = warnings(&diags, "W-TEXT-SINGLE-BRACE")[0];
    // `@narrator: You have ` is 20 characters; the group starts at column 21.
    assert_eq!((d.span.line, d.span.column), (11, 21), "{d:#?}");
    assert_eq!(d.span.byte_end - d.span.byte_start, "{run.oil}".len());
}

#[test]
fn comment_and_tag_tails_warn() {
    let diags = check_body("@narrator: You have {{run.oil}} cans. // TODO fix");
    let hits = warnings(&diags, "W-TEXT-COMMENT-LIKE");
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert!(
        hits[0].message.contains("`// TODO fix`"),
        "{}",
        hits[0].message
    );

    let diags = check_body("@narrator: The lamp room smells of cold brass. # mood:cold");
    let hits = warnings(&diags, "W-TEXT-COMMENT-LIKE");
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert!(
        hits[0].message.contains("no line tags"),
        "{}",
        hits[0].message
    );
}

#[test]
fn bracket_label_warns() {
    let diags = check_body(
        "<branch id=\"door\">\n  <choice id=\"in\" label=\"[Go inside]\">\n    @narrator: In.\n  \
         </choice>\n  <choice id=\"out\" label=\"Stay\">\n    @narrator: Out.\n  </choice>\n\
         </branch>",
    );
    let hits = warnings(&diags, "W-TEXT-BRACKET-LABEL");
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert!(
        hits[0].message.contains("`label=\"Go inside\"`"),
        "{}",
        hits[0].message
    );
    // Anchored at the label's value, not at `<choice`: line 12 is the
    // choice, and `  <choice id="in" label="` is 25 characters.
    assert_eq!(
        (hits[0].span.line, hits[0].span.column),
        (12, 26),
        "{:#?}",
        hits[0]
    );
}

// A leading CRPG tag followed by more text is a label meant to show; only a
// label wholly in brackets is Ink's suppression habit.
#[test]
fn tagged_label_does_not_warn() {
    let diags = check_body(
        "<branch id=\"door\">\n  <choice id=\"in\" label=\"[Persuasion] Step into the light.\">\n    \
         @narrator: In.\n  </choice>\n  <choice id=\"out\" label=\"Stay\">\n    @narrator: Out.\n  \
         </choice>\n</branch>",
    );
    assert!(
        warnings(&diags, "W-TEXT-BRACKET-LABEL").is_empty(),
        "{diags:#?}"
    );
}

// A single-brace group in a label is located inside the label value.
#[test]
fn label_brace_is_anchored_in_the_label() {
    let diags = check_body(
        "<branch id=\"door\">\n  <choice id=\"in\" label=\"Oil {run.oil}\">\n    @narrator: In.\n  \
         </choice>\n  <choice id=\"out\" label=\"Stay\">\n    @narrator: Out.\n  </choice>\n\
         </branch>",
    );
    let d = warnings(&diags, "W-TEXT-SINGLE-BRACE")[0];
    assert!(
        d.message.contains("literal text in a choice label"),
        "{}",
        d.message
    );
    assert_eq!((d.span.line, d.span.column), (12, 30), "{d:#?}");
}

#[test]
fn ink_glue_warns() {
    let diags = check_body("@narrator: The lamp room smells of <>");
    let hits = warnings(&diags, "W-TEXT-GLUE");
    assert_eq!(hits.len(), 1, "{diags:#?}");
    assert_eq!(
        (hits[0].span.line, hits[0].span.column),
        (11, 36),
        "{:#?}",
        hits[0]
    );
}

// The brace scan walks bytes; text with a multi-byte character before or
// after a brace (`—`, Hangul) panicked with "not a char boundary".
#[test]
fn multibyte_text_is_scanned_without_panicking() {
    let diags = check_body(
        "@narrator: Wait — {{run.oil}} cans — {run.oil} left.\n@narrator: 기름 {run.oil}통 — 끝.",
    );
    assert_eq!(
        warnings(&diags, "W-TEXT-SINGLE-BRACE").len(),
        2,
        "{diags:#?}"
    );
}

// Interpolation, attribute braces, a URL, a `#1`, a stage direction in
// braces, and a `::set` stay quiet.
#[test]
fn lute_syntax_and_plain_text_do_not_warn() {
    let diags = check_body(
        "@narrator{when=\"run.oil > 0\"}: You have {{run.oil}} cans.\n\
         @narrator: See http://example.com/a // b for more.\n\
         @narrator: We're #1 {sighs} and \\{{ braces.\n\
         ::set{run.oil = run.oil + 1}",
    );
    let text_warnings: Vec<_> = diags
        .iter()
        .filter(|d| d.code.starts_with("W-TEXT-") && !d.message.contains("`// b for more.`"))
        .collect();
    assert!(text_warnings.is_empty(), "{text_warnings:#?}");
    // ` // ` standing alone IS a comment, even after a URL.
    assert_eq!(
        warnings(&diags, "W-TEXT-COMMENT-LIKE").len(),
        1,
        "{diags:#?}"
    );
}

#[test]
fn goto_names_next() {
    let diags = check_body("::goto{to=\"x\"}\n::mark{id=\"x\"}\n@narrator: hi");
    let d = diags
        .iter()
        .find(|d| d.code == "E-UNKNOWN-DIRECTIVE")
        .unwrap_or_else(|| panic!("{diags:#?}"));
    assert!(
        d.message.contains("did you mean `::next{to=\"…\"}`?"),
        "{}",
        d.message
    );
}
