//! `::assert{…}`/`::retract{…}` leaf node parsing (dsl 0.3.0 §5, Appendix C).

use lute_syntax::ast::Node;
use lute_syntax::datalog::FactTerm;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

fn body(nodes_src: &str) -> Vec<Node> {
    let (doc, diags) = lute_syntax::parse(&format!("{HDR}{nodes_src}\n"));
    assert!(
        diags.iter().all(|d| d.severity != lute_core_span::Severity::Error),
        "unexpected: {diags:?}"
    );
    doc.shots.into_iter().next().unwrap().body
}

#[test]
fn parses_assert_leaf() {
    let b = body("::assert{ inParty(shadowheart) }");
    let Node::Assert(a) = &b[0] else { panic!("expected Assert, got {:?}", b[0]) };
    assert_eq!(a.pattern.relation, "inParty");
    assert_eq!(a.pattern.args.len(), 1);
    assert_eq!(a.raw, "inParty(shadowheart)");
}

#[test]
fn parses_retract_with_wildcard() {
    let b = body("::retract{ atLocation(shadowheart, _) }");
    let Node::Retract(r) = &b[0] else { panic!() };
    assert_eq!(r.pattern.args[1].term, FactTerm::Wildcard);
}

#[test]
fn malformed_payload_emits_datalog_parse_and_sentinel() {
    let (doc, diags) = lute_syntax::parse(&format!("{HDR}::assert{{ not a fact }}\n"));
    assert!(diags.iter().any(|d| d.code == "E-DATALOG-PARSE"), "{diags:?}");
    let Node::Assert(a) = &doc.shots[0].body[0] else { panic!() };
    assert!(a.pattern.relation.is_empty(), "sentinel");
}

#[test]
fn function_term_payload_emits_datalog_function() {
    let (_, diags) = lute_syntax::parse(&format!("{HDR}::assert{{ rel(f(x)) }}\n"));
    assert!(diags.iter().any(|d| d.code == "E-DATALOG-FUNCTION"), "{diags:?}");
}

#[test]
fn bare_assert_without_brace_stays_generic_directive() {
    let b = body("::assert");
    assert!(matches!(&b[0], Node::Directive(d) if d.tag == "assert"));
}
