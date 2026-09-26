//! dsl 0.27.0 §2 (T1-1): a `::set{ path … }` without an assignment operator
//! is `E-SET-SHAPE` naming `=` / `+=` / `-=` — never a silent `=` that eats the
//! author's operator (round 5: `::set{ run.clues - 1 }` compiled to
//! `run.clues = 1` and checked clean).

use lute_core_span::Severity;

const HDR: &str = "---\nkind: scene\nid: s\n---\n## One\n";

fn shape_errors(set: &str) -> Vec<String> {
    let (_, diags) = lute_syntax::parse(&format!("{HDR}{set}\n"));
    diags
        .into_iter()
        .filter(|d| d.code == "E-SET-SHAPE")
        .inspect(|d| assert_eq!(d.severity, Severity::Error))
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_missing_operator_is_an_error_with_the_meant_operator() {
    for (set, meant) in [
        ("::set{ run.clues - 1 }", "`run.clues -= 1`"),
        ("::set{ run.clues + 1 }", "`run.clues += 1`"),
        ("::set{ run.clues 2 }", "`run.clues = 2`"),
        ("::set{ run.clues == 2 }", "`run.clues = 2`"),
        // FS-F3: the YAML habit, `++`, `=+` and an unspaced `-`.
        ("::set{ run.clues: 4 }", "`run.clues = 4`"),
        ("::set{ run.clues++ }", "`run.clues += 1`"),
        ("::set{ run.clues-- }", "`run.clues -= 1`"),
        ("::set{ run.clues =+ 1 }", "`run.clues += 1`"),
        ("::set{ run.clues-1 }", "`run.clues -= 1`"),
    ] {
        let (_, diags) = lute_syntax::parse(&format!("{HDR}{set}\n"));
        // One error: the shape. The recovered write takes the meant
        // operator, so nothing cascades (no `E-CEL-PARSE` on `: 4` / `+`,
        // no `E-PATH-IDENT` on `run.clues-1`).
        assert_eq!(diags.len(), 1, "{set}: {diags:?}");
        let errs = shape_errors(set);
        assert_eq!(errs.len(), 1, "{set}: {errs:?}");
        assert!(
            ["`=`", "`+=`", "`-=`", "`*=`"]
                .iter()
                .all(|op| errs[0].contains(op)),
            "{set}: names the operators: {}",
            errs[0]
        );
        assert!(errs[0].contains(meant), "{set}: {}", errs[0]);
    }
}

#[test]
fn an_unspaced_minus_assign_is_the_operator() {
    let (doc, diags) = lute_syntax::parse(&format!("{HDR}::set{{ run.clues-=1 }}\n"));
    assert!(diags.is_empty(), "{diags:?}");
    let set = doc.shots[0]
        .body
        .iter()
        .find_map(|n| match n {
            lute_syntax::ast::Node::Set(s) => Some(s),
            _ => None,
        })
        .expect("a Set node");
    assert_eq!(
        (set.path.as_str(), set.op.as_str(), set.expr.raw.as_str()),
        ("run.clues", "-=", "1")
    );
}

#[test]
fn a_set_with_no_value_is_an_error() {
    let errs = shape_errors("::set{ run.clues }");
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].contains("found nothing — write `run.clues = <value>`"),
        "{}",
        errs[0]
    );
    let errs = shape_errors("::set{ run.clues + }");
    assert!(
        errs[0].contains("found `+` with no value — write `run.clues += <value>`"),
        "{}",
        errs[0]
    );
}

#[test]
fn every_assignment_operator_parses_clean() {
    for set in [
        "::set{ run.clues = 1 }",
        "::set{ run.clues = -1 }",
        "::set{run.clues+=1}",
        "::set{ run.clues -= 1 }",
        "::set{ run.clues *= 2 }",
        "::set{ run.clues = 1 when=\"run.flag\" }",
    ] {
        assert!(shape_errors(set).is_empty(), "{set}");
    }
}

#[test]
fn a_param_as_a_dotted_segment_names_the_indexed_form() {
    let (doc, diags) = lute_syntax::parse(&format!("{HDR}::set{{run.aff.@who += 1}}\n"));
    let errs: Vec<_> = diags.iter().filter(|d| d.code == "E-SET-SHAPE").collect();
    assert_eq!(errs.len(), 1, "{diags:?}");
    assert!(
        errs[0]
            .message
            .contains("index the family: `run.aff[@who]`"),
        "{}",
        errs[0].message
    );
    // The node recovers as the indexed path, so nothing cascades.
    let set = doc.shots[0]
        .body
        .iter()
        .find_map(|n| match n {
            lute_syntax::ast::Node::Set(s) => Some(s),
            _ => None,
        })
        .expect("a Set node");
    assert_eq!(
        (set.path.as_str(), set.op.as_str()),
        ("run.aff[@who]", "+=")
    );
}
