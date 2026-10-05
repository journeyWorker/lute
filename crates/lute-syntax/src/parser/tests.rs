use super::*;
use crate::ast::Node;

#[test]
fn classifies_set_before_generic_directive() {
    let (doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\n::set{scene.a = 1}\n");
    assert!(diags.is_empty(), "{diags:?}");
    let body = &doc.shots[0].body;
    assert!(
        matches!(body[0], Node::Set(_)),
        "::set must classify as Set, not Directive"
    );
}

fn only_set(src_body: &str) -> Set {
    let src = format!("---\ncharacter: x\n---\n## Shot 1.\n{src_body}\n");
    let (doc, diags) = parse(&src);
    assert!(diags.is_empty(), "{diags:?}");
    match &doc.shots[0].body[0] {
        Node::Set(s) => {
            // Slot spans index the source text verbatim.
            assert_eq!(
                &src[s.expr.span.byte_start..s.expr.span.byte_end],
                s.expr.raw
            );
            if let Some(w) = &s.when {
                assert_eq!(&src[w.span.byte_start..w.span.byte_end], w.raw);
            }
            s.clone()
        }
        other => panic!("expected Set, got {other:?}"),
    }
}

/// dsl 0.24.0 §1: a trailing `when="…"` is the write's guard, split off
/// the expression into a Condition slot.
#[test]
fn set_trailing_when_is_a_condition_slot() {
    let s = only_set("::set{run.aff.wren += 1 when=\"run.warmed.wren < run.day\"}");
    assert_eq!((s.path.as_str(), s.op.as_str()), ("run.aff.wren", "+="));
    assert_eq!(s.expr.raw, "1");
    let w = s.when.expect("guard parsed");
    assert_eq!(w.raw, "run.warmed.wren < run.day");
    assert_eq!(w.kind, CelKind::Condition);

    // Unguarded stays byte-identical.
    let s = only_set("::set{run.route = 'sol'}");
    assert_eq!(s.expr.raw, "'sol'");
    assert!(s.when.is_none());
}

/// A `when=` inside a string literal, or one not at the end, is still
/// expression text (the CEL parse reports the latter).
#[test]
fn set_when_inside_a_string_or_not_trailing_stays_expression() {
    let s = only_set("::set{run.note = \"a when=\\\"b\\\"\"}");
    assert!(s.when.is_none(), "{s:?}");
    assert_eq!(s.expr.raw, "\"a when=\\\"b\\\"\"");
    let s = only_set("::set{run.n = 1 when=\"run.n > 0\" + 2}");
    assert!(s.when.is_none(), "{s:?}");
    let s = only_set("::set{run.note = 'x' when=\"run.note == 'y'\"}");
    assert_eq!(s.expr.raw, "'x'");
    assert_eq!(s.when.expect("guard").raw, "run.note == 'y'");
}

/// A guarded write is logic; a `<track>` holds none (§7.4).
#[test]
fn set_when_in_a_track_clip_is_timeline_content() {
    let (doc, diags) = parse(
        "---\ncharacter: x\n---\n## Shot 1.\n<timeline duration=\"1\">\n<track subject=\"a\">\n\
         ::set{scene.e = 5 when=\"scene.f\"}\n</track>\n</timeline>\n",
    );
    assert!(
        diags.iter().any(|d| d.code == E_TIMELINE_CONTENT),
        "{diags:?}"
    );
    let Node::Timeline(t) = &doc.shots[0].body[0] else {
        panic!("expected timeline");
    };
    let crate::ast::ClipNode::Set(s) = &t.tracks[0].clips[0].node else {
        panic!("expected set clip");
    };
    assert!(s.when.is_none(), "a clip set never carries a guard");
}

#[test]
fn line_text_is_opaque_to_eol() {
    let (doc, _) = parse("---\ncharacter: x\n---\n## Shot 1.\n@narrator: (a) <b> : c\n");
    if let Node::Line(l) = &doc.shots[0].body[0] {
        assert_eq!(l.text, "(a) <b> : c");
        assert_eq!(l.speaker, "narrator");
    } else {
        panic!("expected Line");
    }
}

#[test]
fn unrecognized_line_is_error() {
    let (_doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\ngarbage prose\n");
    assert!(diags.iter().any(|d| d.code == "E-UNCLASSIFIED"));
}

// RC1: a top-level stray `</quest>` is not content, so it must never be
// diagnosed as `E-CONTENT-OUTSIDE-SHOT` — it belongs on the same
// `E-UNCLOSED-TAG` path as an in-shot stray close.
#[test]
fn top_level_stray_close_is_unclosed_tag_not_content_outside_shot() {
    let (_doc, diags) = parse("</quest>\n## Shot 1.\n@narrator: hi.\n");
    assert!(
        diags.iter().any(|d| d.code == E_UNCLOSED_TAG),
        "stray top-level close must be E-UNCLOSED-TAG: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == E_CONTENT_OUTSIDE_SHOT),
        "stray top-level close must NOT be misdiagnosed as E-CONTENT-OUTSIDE-SHOT: {diags:?}"
    );
}

/// lamplight F23: an `<entry>` pasted inside another names the open one
/// and its line — ONE diagnostic, no `## heading` advice, no stray-close
/// cascade — and both entries still parse, the outer keeping its tail.
#[test]
fn nested_entry_names_the_open_entry_once() {
    let src = "---\nkind: lore\ntitle: T\n---\n\n<entry id=\"first\" title=\"First\">\n  @narrator: One.\n\
               <entry id=\"second\" title=\"Second\">\n  @narrator: Two.\n</entry>\n  ::assert{x(a)}\n</entry>\n";
    let (doc, diags) = parse(src);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, E_UNCLOSED_TAG);
    assert_eq!(diags[0].span.line, 8, "{diags:?}");
    assert!(
        diags[0].message.starts_with(
            "entries cannot nest; `first` opened at line 6 is still open — close it with `</entry>`"
        ),
        "{}",
        diags[0].message
    );
    let ids: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["first", "second"]);
    assert_eq!(doc.entries[0].body.len(), 2, "the tail stays in `first`");
}

#[test]
fn an_unclosed_entry_before_siblings_is_reported_once_plus_its_close() {
    let src = "---\nkind: lore\ntitle: T\n---\n<entry id=\"a\">\n  @narrator: a.\n\
               <entry id=\"b\">\n  @narrator: b.\n</entry>\n<entry id=\"c\">\n  @narrator: c.\n</entry>\n";
    let (doc, diags) = parse(src);
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, [E_UNCLOSED_TAG, E_UNCLOSED_TAG], "{diags:?}");
    assert!(
        diags
            .iter()
            .any(|d| d.message.starts_with("entries cannot nest; `a`")),
        "{diags:?}"
    );
    assert!(
        diags.iter().any(|d| d
            .message
            .starts_with("`<entry>` from line 5 is never closed")),
        "{diags:?}"
    );
    assert_eq!(doc.entries.len(), 3);
}

#[test]
fn lore_content_outside_an_entry_gets_no_heading_advice() {
    let (_doc, diags) = parse("---\nkind: lore\ntitle: T\n---\n@narrator: stray.\n");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, E_CONTENT_OUTSIDE_SHOT);
    assert!(!diags[0].message.contains("##"), "{}", diags[0].message);
    assert!(
        diags[0].message.contains("`<entry>`"),
        "{}",
        diags[0].message
    );
}

#[test]
fn free_shot_headings_parse_clean() {
    // dsl 0.6.0 §3.1: the `Shot|Scene <int>.`/bookend grammar is gone — any
    // non-empty `## ` text is a valid opaque title. Every input that was an
    // E-SHOT-HEADING error pre-0.6.0 now parses clean as a titled shot.
    for good in [
        "## Chapter 1.",
        "## Shot .",
        "## Shot 3",
        "## Prolog",
        "## Shot1.",
        "## Shot 99999999999999999999.",
        "## Shot 1.Title",
    ] {
        let (doc, diags) = parse(&format!("{good}\n@narrator: hi.\n"));
        assert!(diags.is_empty(), "{good}: {diags:?}");
        assert_eq!(doc.shots.len(), 1, "{good}: one shot");
        assert_eq!(
            doc.shots[0].heading,
            good.strip_prefix("## ").unwrap(),
            "heading kept opaque: {good}"
        );
    }
}

// -- Task 9e: E-TITLE-PLACEMENT for misplaced/duplicate `# ` title (§6.2/I1) --

#[test]
fn title_after_first_shot_is_placement_error() {
    // §6.2: a `# ` title after the first shot is E-TITLE-PLACEMENT, not
    // the generic E-UNCLASSIFIED.
    let (_doc, diags) = parse("## Shot 1.\n@narrator: hi.\n# Late Title\n");
    assert!(
        diags.iter().any(|d| d.code == E_TITLE_PLACEMENT),
        "late title must be E-TITLE-PLACEMENT: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == E_UNCLASSIFIED),
        "late title must not fall through to E-UNCLASSIFIED: {diags:?}"
    );
}

#[test]
fn second_title_before_shot_is_placement_error() {
    // §6.2: at most one `# ` title; the SECOND is E-TITLE-PLACEMENT.
    let (doc, diags) = parse("# First\n# Second\n## Shot 1.\n@narrator: hi.\n");
    assert_eq!(
        doc.title.as_ref().map(|(t, _)| t.as_str()),
        Some("First"),
        "the first title is accepted"
    );
    let placement: Vec<_> = diags
        .iter()
        .filter(|d| d.code == E_TITLE_PLACEMENT)
        .collect();
    assert_eq!(
        placement.len(),
        1,
        "exactly one E-TITLE-PLACEMENT: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == E_UNCLASSIFIED),
        "second title must not fall through to E-UNCLASSIFIED: {diags:?}"
    );
}

#[test]
fn single_title_before_shot_is_clean() {
    // Regression guard: the normal case (one `# ` title before the first
    // shot) produces no diagnostic.
    let (doc, diags) = parse("# The Title\n## Shot 1.\n@narrator: hi.\n");
    assert!(
        diags.is_empty(),
        "well-placed title must be clean: {diags:?}"
    );
    assert_eq!(
        doc.title.as_ref().map(|(t, _)| t.as_str()),
        Some("The Title")
    );
}

#[test]
fn free_form_headings_parse() {
    // dsl 0.6.0 §3.1: a shot heading is opaque free text. The old keyword
    // forms still parse (they are just ordinary titles now), alongside
    // arbitrary prose — including non-Latin scripts.
    for good in [
        "## Shot 1.",
        "## Scene 2. Title",
        "## Prologue",
        "## The Interrogation",
        "## 골목길에서",
        "## 프롤로그",
        "## 1",
    ] {
        let (doc, diags) = parse(&format!("{good}\n@narrator: hi.\n"));
        assert!(diags.is_empty(), "{good}: {diags:?}");
        assert_eq!(
            doc.shots[0].heading,
            good.strip_prefix("## ").unwrap(),
            "heading kept opaque: {good}"
        );
    }
}

#[test]
fn otherwise_attrs_are_retained_not_a_parse_error() {
    // dsl 0.10.0 §4 (D-J): the parser retains the attribute and says
    // nothing; the checker's closure rule reports it as `E-UNKNOWN-ATTR` at
    // the attribute's own column, uniformly with every other logic tag.
    let src = "## Shot 1.\n<match on=\"app.rating\">\n<when test=\"$ == 'teen'\">\n@narrator: a.\n</when>\n<otherwise foo=\"bar\">\n@narrator: b.\n</otherwise>\n</match>\n";
    let (doc, diags) = parse(src);
    assert!(
        !diags.iter().any(|d| d.code == "E-LOGIC-CONTENT"),
        "the attribute arm is the checker's now: {diags:?}"
    );
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!()
    };
    let Arm::Otherwise { attrs, .. } = &m.arms[1] else {
        panic!()
    };
    assert_eq!(
        attrs.iter().map(|a| a.key.as_str()).collect::<Vec<_>>(),
        vec!["foo"]
    );
}

#[test]
fn match_when_otherwise_retain_residual_attrs() {
    // dsl 0.10.0 §4 / D-J's precondition: the three logic tags that used to
    // DROP their residual attributes now carry them. The parser still says
    // nothing about them — the checker's closure rule (Task 3) is the reader.
    let src = "## Shot 1.\n<match on=\"app.rating\" bogus=\"x\">\n\
               <when test=\"true\" nonsense=\"y\">\n@narrator: a.\n</when>\n\
               <otherwise junk=\"z\">\n@narrator: b.\n</otherwise>\n\
               </match>\n";
    let (doc, _) = parse(src);
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!("expected a <match>: {:?}", doc.shots[0].body[0]);
    };
    assert_eq!(
        m.attrs.iter().map(|a| a.key.as_str()).collect::<Vec<_>>(),
        vec!["bogus"],
        "`on` is extracted into `subject`; every other key stays in `attrs`"
    );
    let Arm::When { attrs, .. } = &m.arms[0] else {
        panic!("expected a <when> arm: {:?}", m.arms[0]);
    };
    assert_eq!(
        attrs.iter().map(|a| a.key.as_str()).collect::<Vec<_>>(),
        vec!["nonsense"],
        "`is`/`test` are extracted; every other key stays in `attrs`"
    );
    let Arm::Otherwise { attrs, .. } = &m.arms[1] else {
        panic!("expected an <otherwise> arm: {:?}", m.arms[1]);
    };
    assert_eq!(
        attrs.iter().map(|a| a.key.as_str()).collect::<Vec<_>>(),
        vec!["junk"],
        "<otherwise> extracts nothing, so every key stays in `attrs`"
    );
}

#[test]
fn attr_quote_protects_structural_chars() {
    let (doc, _) =
        parse("---\ncharacter: x\n---\n## Shot 1.\n::sfx{sound=\"a } b\" name=\"n\"}\n");
    if let Node::Directive(d) = &doc.shots[0].body[0] {
        assert_eq!(d.attrs.len(), 2);
        assert_eq!(d.attrs[0].key, "sound");
    } else {
        panic!();
    }
}

// -- span-fidelity: positions map through a multi-line comment ------------

#[test]
fn span_maps_to_original_source_through_comment() {
    // A 3-line block comment precedes the shot; the error must report the
    // ORIGINAL line, not a comment-shifted one.
    let src = "---\ncharacter: x\n---\n/*\n c\n*/\n## Shot 1.\ngarbage\n";
    let (_doc, diags) = parse(src);
    let d = diags
        .iter()
        .find(|d| d.code == "E-UNCLASSIFIED")
        .expect("unclassified diag");
    // `garbage` is line 8 of the original file (1-based).
    assert_eq!(d.span.line, 8, "diag should point at the original line 8");
    assert_eq!(&src[d.span.byte_start..d.span.byte_end], "garbage");
}

#[test]
fn unterminated_comment_is_diagnosed() {
    let (_doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\n/* never ends\n");
    assert!(diags.iter().any(|d| d.code == "E-COMMENT-UNTERMINATED"));
}

#[test]
fn no_unterminated_from_block_comment_inside_content_text() {
    // §4.2 exclusion 2 + blanked-view recompute: after the leading `/* p */`
    // is blanked, the boundary is recomputed to recognize the content line,
    // so its `Text` is opaque and the body `/* boom` is literal — NOT an
    // unterminated block comment. `find_unterminated_comment` reports none.
    let body = "/* p */ @marina: a /* boom";
    assert_eq!(find_unterminated_comment(body), 0);
}

#[test]
fn no_unterminated_diag_from_block_comment_inside_content_text() {
    // End-to-end: a `/*` inside a content line's opaque `Text` is literal
    // (§4.2 exclusion 2), so no E-COMMENT-UNTERMINATED is raised and the
    // `Text` keeps the `/*` verbatim. The leading `/* p */` is still blanked.
    let src = "---\ncharacter: x\n---\n## Shot 1.\n/* p */ @marina: a /* boom\n";
    let (doc, diags) = parse(src);
    assert!(
        !diags.iter().any(|d| d.code == E_COMMENT_UNTERMINATED),
        "opaque Text must not raise E-COMMENT-UNTERMINATED: {diags:?}"
    );
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!("expected Line")
    };
    assert!(
        l.text.contains("a /* boom"),
        "Text lost the literal `/*`: {l:?}"
    );
}

#[test]
fn escaped_backslash_attr_value_text_stays_opaque() {
    // Regression (content_text_start escape state): an attr value ending in
    // an escaped backslash must not spill the string state past `}` `:`, or a
    // `/*` in the opaque `Text` would be wrongly stripped / flagged
    // (§4.2 exclusion 2). The `Text` keeps its `/* … */` verbatim.
    let (doc, diags) = parse("## Shot 1.\n@marina{u=\"\\\\\"}: keep /* literal */\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!("expected Line")
    };
    assert_eq!(l.text, "keep /* literal */");
}

#[test]
fn attr_derived_celslot_span_bounds_raw() {
    // Regression (T2.3 review Critical): attr-derived CEL slots must have
    // span == the inner value bytes, so src[slot.span] == slot.raw (matching
    // Set slots). Otherwise Phase-3 CEL sub-diagnostics drift by key.len()+2.
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n<when test=\"$ == 'gold'\">\n@narrator: hi\n</when>\n<otherwise>\n@narrator: bye\n</otherwise>\n</match>\n";
    let (doc, diags) = parse(src);
    assert!(diags.is_empty(), "{diags:?}");
    let slot_ok = |s: &CelSlot| {
        let got = &src[s.span.byte_start..s.span.byte_end];
        assert_eq!(got, s.raw, "src[span] must equal raw for kind {:?}", s.kind);
    };
    if let Node::Match(m) = &doc.shots[0].body[0] {
        assert_eq!(m.subject.raw, "scene.choices.number");
        slot_ok(&m.subject);
        if let Arm::When { test, .. } = &m.arms[0] {
            assert_eq!(test.raw, "$ == 'gold'");
            slot_ok(test);
        } else {
            panic!("expected When arm");
        }
    } else {
        panic!("expected Match");
    }
}

#[test]
fn when_is_pattern_preserved_without_test() {
    // dsl §7.3.1: `<when is="…">` is the 0.1.0 headline construct. The literal
    // pattern MUST be preserved on the arm, distinct from `test` (which stays
    // an empty synthesized CelSlot when absent).
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.choices.x\">\n<when is=\"soft | curt\">\n@narrator: hi\n</when>\n</match>\n";
    let (doc, _diags) = parse(src);
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!("expected Match")
    };
    let Arm::When { is, test, .. } = &m.arms[0] else {
        panic!("expected When arm")
    };
    let is = is.as_ref().expect("is pattern must be preserved");
    assert_eq!(is.raw, "soft | curt");
    assert_eq!(test.raw, "", "test stays empty when only `is` is given");
}

#[test]
fn when_is_and_test_both_preserved() {
    // A `<when>` may carry both a literal `is` pattern and a `test` guard;
    // neither clobbers the other.
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.choices.x\">\n<when is=\"gold\" test=\"$ != 'x'\">\n@narrator: hi\n</when>\n</match>\n";
    let (doc, _diags) = parse(src);
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!("expected Match")
    };
    let Arm::When { is, test, .. } = &m.arms[0] else {
        panic!("expected When arm")
    };
    assert_eq!(is.as_ref().expect("is preserved").raw, "gold");
    assert_eq!(test.raw, "$ != 'x'");
}

#[test]
fn when_without_is_has_none() {
    // A test-only `<when>` carries no `is` pattern.
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.choices.x\">\n<when test=\"$ == 1\">\n@narrator: hi\n</when>\n</match>\n";
    let (doc, _diags) = parse(src);
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!("expected Match")
    };
    let Arm::When { is, test, .. } = &m.arms[0] else {
        panic!("expected When arm")
    };
    assert!(is.is_none(), "no `is` attr => None");
    assert_eq!(test.raw, "$ == 1");
}

#[test]
fn match_with_is_arm_and_otherwise_preserves_is() {
    // Final-review fixture: a full <match> whose single guarded arm uses the
    // literal `is` pattern (no `test`) must parse with the `is` value intact —
    // the pattern is not dropped at the parse layer (dsl §7.3.1).
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.choices.x\">\n<when is=\"soft\">\n@narrator: soft\n</when>\n<otherwise>\n@narrator: else\n</otherwise>\n</match>\n";
    let (doc, _diags) = parse(src);
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!("expected Match")
    };
    assert_eq!(m.arms.len(), 2);
    let Arm::When { is, .. } = &m.arms[0] else {
        panic!("expected When arm")
    };
    assert_eq!(is.as_ref().expect("is preserved").raw, "soft");
    assert!(matches!(&m.arms[1], Arm::Otherwise { .. }));
}

#[test]
fn stray_line_under_branch_is_diagnosed() {
    // §7.3: a <branch> body admits only <choice> children. A direct content line is
    // invalid structure and MUST be reported (not silently dropped), mirroring
    // the <track>/E-TIMELINE-CONTENT rule.
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<branch id=\"b\">\n@narrator: stray\n<choice id=\"c\" label=\"L\">\n@narrator: ok\n</choice>\n</branch>\n";
    let (_doc, diags) = parse(src);
    assert!(
        diags.iter().any(|d| d.code == E_LOGIC_CONTENT),
        "stray content line under <branch> must be diagnosed, got {diags:?}"
    );
}

#[test]
fn stray_directive_under_match_is_diagnosed() {
    // §7.3: a <match> body admits only <when>/<otherwise>. A direct ::set is
    // invalid structure and MUST be reported, not silently skipped.
    let src = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.x\">\n::set{scene.x = 1}\n<otherwise>\n@narrator: ok\n</otherwise>\n</match>\n";
    let (_doc, diags) = parse(src);
    assert!(
        diags.iter().any(|d| d.code == E_LOGIC_CONTENT),
        "stray ::set under <match> must be diagnosed, got {diags:?}"
    );
}

#[test]
fn content_line_short_form() {
    let (doc, diags) = parse("## Shot 1.\n@marina{code=\"0010\"}: Hello!\n@narrator: Quiet.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let body = &doc.shots[0].body;
    let Node::Line(l) = &body[0] else { panic!() };
    assert_eq!(l.speaker, "marina");
    assert_eq!(l.text, "Hello!");
    let Node::Line(n) = &body[1] else { panic!() };
    assert_eq!(n.speaker, "narrator");
}

#[test]
fn legacy_line_bracket_form_is_rejected_with_fixit() {
    let src = "## Shot 1.\n:line[marina]{code=\"0010\"}: Hello!\n";
    let (_, diags) = parse(src);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E-UNCLASSIFIED");
    assert!(
        diags[0].message.contains("0.2.2"),
        "fix-it hint: {}",
        diags[0].message
    );
    // 0.2.2 migration (dsl §7.1, foundation C1): the diagnostic carries a
    // `migrate` fix-it whose single TextEdit rewrites the removed
    // `:line[speaker]` bracket form to `@speaker` — the CURRENT sigil.
    // `lute fix` (Task C3/D5) applies it (phase 1).
    assert_eq!(diags[0].fixits.len(), 1, "expected one migrate fix-it");
    let fx = &diags[0].fixits[0];
    assert_eq!(fx.kind, "migrate", "fix-it kind: {}", fx.kind);
    assert_eq!(fx.edit.len(), 1);
    assert_eq!(fx.edit[0].new_text, "@marina");
    // The edit span covers exactly `:line[marina]` (`:` through the `]`).
    let sp = fx.edit[0].span;
    assert_eq!(&src[sp.byte_start..sp.byte_end], ":line[marina]");
}

#[test]
fn content_line_missing_second_colon_is_error() {
    let (_, diags) = parse("## Shot 1.\n@marina no colon here\n");
    assert_eq!(diags[0].code, "E-UNCLASSIFIED");
}

// -- Task 3: `//` line comments + truly-opaque Text (dsl §4.2) -----------

#[test]
fn line_comment_leading_is_trivia() {
    let (doc, diags) = parse("## Shot 1.\n// a note\n@marina: Hi.\n");
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(doc.shots[0].body.len(), 1);
}

#[test]
fn line_comment_mid_line_is_not_a_comment() {
    // dsl §4.2: `//` only at line start; inside Text it is literal.
    let (doc, _) = parse("## Shot 1.\n@marina: see https://example.com // really\n");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    assert!(l.text.contains("https://example.com // really"));
}

#[test]
fn block_comment_not_recognized_inside_text() {
    // dsl §4.2 exclusion 2: Text is truly opaque after the second colon.
    let (doc, diags) = parse("## Shot 1.\n@marina: I love /* this */ you.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    assert_eq!(l.text, "I love /* this */ you.");
}

#[test]
fn unterminated_block_comment_inside_text_is_fine() {
    let (_, diags) = parse("## Shot 1.\n@marina: half /* open\n@narrator: next line intact\n");
    assert!(diags.is_empty(), "{diags:?}");
}

// -- Task 7: `{{…}}` interpolation scan (dsl §7.6) ----------------------

#[test]
fn interps_are_scanned_and_classified() {
    let (doc, diags) =
        parse("## Shot 1.\n@marina: Hi {{userName}}, you have {{run.coins}} and {{@fond}}.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    let kinds: Vec<_> = l.interps.iter().map(|p| (p.kind, p.raw.as_str())).collect();
    assert_eq!(
        kinds,
        [
            (InterpKind::Reserved, "userName"),
            (InterpKind::Path, "run.coins"),
            (InterpKind::Ref, "@fond"),
        ]
    );
}

#[test]
fn escaped_and_unterminated_interp() {
    let (doc, diags) =
        parse("## Shot 1.\n@marina: literal \\{{ stays.\n@fixer: broken {{run.coins\n");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    assert!(l.interps.is_empty());
    assert!(diags.iter().any(|d| d.code == "E-INTERP-UNTERMINATED"));
}

#[test]
fn interp_inner_whitespace_is_trimmed() {
    let (doc, diags) = parse("## Shot 1.\n@marina: You have {{ run.coins }} left.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    assert_eq!(l.interps.len(), 1);
    assert_eq!(l.interps[0].kind, InterpKind::Path);
    assert_eq!(l.interps[0].raw, "run.coins");
}

#[test]
fn empty_interp_is_scanned_as_empty_path() {
    // Parser stays dumb: `{{}}` is a well-formed (if useless) interpolation
    // with empty `raw`; the checker rejects the empty referent (Plan B).
    let (doc, diags) = parse("## Shot 1.\n@marina: nothing here {{}} really.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    assert_eq!(l.interps.len(), 1);
    assert_eq!(l.interps[0].kind, InterpKind::Path);
    assert_eq!(l.interps[0].raw, "");
}

#[test]
fn escaped_then_real_interp_same_line() {
    // `\{{` is a literal (no interp); a later unescaped `{{later}}` still scans.
    let (doc, diags) = parse("## Shot 1.\n@marina: braces \\{{ then {{later}}.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    let kinds: Vec<_> = l.interps.iter().map(|p| (p.kind, p.raw.as_str())).collect();
    assert_eq!(kinds, [(InterpKind::Path, "later")]);
}

#[test]
fn interp_format_hint_is_split_off_the_referent() {
    // dsl 0.24.0 §4: `{{x:hint}}` — `raw` is the referent alone, so every
    // referent consumer (checker, trace, compile) is unchanged by a hint;
    // an unknown hint is still scanned (the checker rejects it).
    let (doc, diags) = parse(
        "## Shot 1.\n@marina: {{user.deaths:ordinal}} {{ @nth(run.a ? 1 : 2) : ordinal }} {{run.n:plural}} {{run.n}}\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    let got: Vec<_> = l
        .interps
        .iter()
        .map(|p| (p.kind, p.raw.as_str(), p.format.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            (InterpKind::Path, "user.deaths", Some("ordinal")),
            (InterpKind::Ref, "@nth(run.a ? 1 : 2)", Some("ordinal")),
            (InterpKind::Path, "run.n", Some("plural")),
            (InterpKind::Path, "run.n", None),
        ]
    );
    // The span still covers the whole marker, hint included.
    let s = &l.interps[0].span;
    assert_eq!(
        &l.text[(s.byte_start - l.text_span.byte_start)..(s.byte_end - l.text_span.byte_start)],
        "{{user.deaths:ordinal}}"
    );
}

#[test]
fn a_colon_inside_a_call_or_before_a_non_identifier_is_not_a_hint() {
    let (doc, _) = parse("## Shot 1.\n@marina: {{@f(a ? b : c)}} {{run.n:}} {{run.n:1}}\n");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    let got: Vec<_> = l
        .interps
        .iter()
        .map(|p| (p.raw.as_str(), p.format.is_some()))
        .collect();
    assert_eq!(
        got,
        [
            ("@f(a ? b : c)", false),
            ("run.n:", false),
            ("run.n:1", false)
        ]
    );
}

#[test]
fn interp_span_after_multibyte_is_utf8_safe() {
    // A multi-byte prefix must not throw off the byte offsets: slicing the
    // ORIGINAL source by the interp span lands on char boundaries and covers
    // exactly the `{{…}}`.
    let src = "## Shot 1.\n@marina: 안녕 {{userName}}!\n";
    let (doc, diags) = parse(src);
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else {
        panic!()
    };
    assert_eq!(l.interps.len(), 1);
    let sp = l.interps[0].span;
    assert_eq!(&src[sp.byte_start..sp.byte_end], "{{userName}}");
    assert_eq!(l.interps[0].raw, "userName");
}

#[test]
fn quest_doc_collects_top_level_quests() {
    // A quest doc: NO `## ` headings, one or more top-level <quest> blocks.
    let (doc, diags) = parse(
        "<quest id=\"q1\" title=\"One\" start=\"run.a\">\n\
         <objective id=\"o1\" done=\"run.b\"/>\n\
         </quest>\n\
         <quest id=\"q2\">\n\
         <objective id=\"o2\" done=\"run.c\"/>\n\
         </quest>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(doc.quests.len(), 2);
    assert_eq!(doc.quests[0].id, "q1");
    assert_eq!(doc.quests[0].body.len(), 1); // one <objective> Node
    assert!(doc.shots.is_empty());
}

#[test]
fn on_and_objective_are_nodes_in_a_body() {
    let (doc, diags) = parse(
        "<quest id=\"q\">\n\
         <on event=\"questComplete\">\n@x: hi\n</on>\n\
         </quest>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    assert!(matches!(doc.quests[0].body[0], Node::On(_)));
}

#[test]
fn nested_quest_names_the_open_quest() {
    // <quest> is top-level only; nested, it reports the open quest
    // (lamplight F23) and parses as its sibling.
    let (doc, diags) = parse("<quest id=\"q\">\n<quest id=\"inner\">\n</quest>\n</quest>\n");
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(diags[0].code, E_UNCLOSED_TAG);
    assert!(
        diags[0]
            .message
            .starts_with("quests cannot nest; `q` opened at line 1"),
        "{diags:?}"
    );
    assert_eq!(doc.quests.len(), 2);
}

// -- dsl 0.19.0: top-level <entry> (lore documents) --

/// The source text a span covers.
fn spanned<'s>(src: &'s str, sp: &Span) -> &'s str {
    &src[sp.byte_start..sp.byte_end]
}

#[test]
fn entry_with_every_attr_parses_into_fields() {
    let src = "---\nkind: lore\n---\n\
               <entry id=\"scientistLog1\" target=\"item.torn_note_1\" category=\"note\" \
               series=\"scientistLog\" order=\"1\" title=\"Research log, day 3\" \
               on=\"inspect\" priority=\"-5\" when=\"run.labOpen\">\n\
               @scientist: Day three.\n\
               ::assert{knows(vesna, project_lumen)}\n\
               </entry>\n";
    let (doc, diags) = parse(src);
    assert!(diags.is_empty(), "{diags:?}");
    assert!(doc.shots.is_empty() && doc.quests.is_empty());
    assert_eq!(doc.entries.len(), 1);
    let e = &doc.entries[0];
    assert_eq!(e.id, "scientistLog1");
    assert_eq!(spanned(src, &e.id_span), "scientistLog1");
    for (field, want) in [
        (&e.target, "item.torn_note_1"),
        (&e.category, "note"),
        (&e.series, "scientistLog"),
        (&e.order, "1"),
        (&e.title, "Research log, day 3"),
        (&e.on, "inspect"),
        (&e.priority, "-5"),
    ] {
        let (value, sp) = field.as_ref().expect("attr extracted");
        assert_eq!(value, want);
        assert_eq!(
            spanned(src, sp),
            want,
            "value span anchors the attribute value"
        );
    }
    let when = e.when.as_ref().expect("when slot");
    assert_eq!(when.kind, CelKind::Condition);
    assert_eq!(when.raw, "run.labOpen");
    assert_eq!(spanned(src, &when.span), "run.labOpen");
    assert!(
        e.attrs.is_empty(),
        "every known attr is extracted: {:?}",
        e.attrs
    );
    assert!(matches!(e.body[..], [Node::Line(_), Node::Assert(_)]));
    assert!(spanned(src, &e.span).starts_with("<entry id="));
    assert!(spanned(src, &e.span).ends_with("</entry>"));
}

#[test]
fn entry_without_attrs_leaves_optionals_empty() {
    let (doc, diags) = parse("<entry id=\"k\">\n@narrator: A key.\n</entry>\n");
    assert!(diags.is_empty(), "{diags:?}");
    let e = &doc.entries[0];
    assert!(e.target.is_none() && e.category.is_none() && e.title.is_none());
    assert!(e.series.is_none() && e.order.is_none() && e.when.is_none());
    assert!(e.on.is_none() && e.priority.is_none());
}

#[test]
fn entry_missing_id_is_empty_and_anchored_at_open_tag() {
    let src = "<entry category=\"item\">\n@narrator: A key.\n</entry>\n";
    let (doc, diags) = parse(src);
    assert!(
        diags.is_empty(),
        "missing id is the checker's E-ENTRY-ATTR: {diags:?}"
    );
    let e = &doc.entries[0];
    assert_eq!(e.id, "");
    assert_eq!(spanned(src, &e.id_span), "<entry category=\"item\">");
    assert_eq!(e.body.len(), 1);
}

#[test]
fn entry_non_string_and_unknown_attrs_stay_residual() {
    // A bare `id` flag is not a string id (checker: E-ENTRY-ATTR); an
    // invented key is left for the per-tag closure (E-UNKNOWN-ATTR).
    let (doc, diags) = parse("<entry id speaker=\"x\">\n@narrator: hi\n</entry>\n");
    assert!(diags.is_empty(), "{diags:?}");
    let e = &doc.entries[0];
    assert_eq!(e.id, "");
    let keys: Vec<&str> = e.attrs.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, ["id", "speaker"]);
}

#[test]
fn entries_collect_in_document_order_beside_quests() {
    let (doc, diags) = parse(
        "<entry id=\"a\">\n@x: one\n</entry>\n\
         <quest id=\"q\">\n</quest>\n\
         <entry id=\"b\">\n@x: two\n</entry>\n\
         <entry id=\"c\">\n@x: three\n</entry>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    let ids: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["a", "b", "c"]);
    assert_eq!(doc.quests.len(), 1);
}

#[test]
fn entry_body_is_the_ordinary_node_stream() {
    let (doc, diags) = parse(
        "<entry id=\"rustyKey\" target=\"item.rusty_key\">\n\
         <match on=\"run.labBurned\">\n\
         <when is=\"true\">\n@narrator: A scorched key.\n</when>\n\
         <otherwise>\n@narrator: A rusty key.\n</otherwise>\n\
         </match>\n\
         ::set{run.keySeen = true}\n\
         ::assert{knows(vesna, project_lumen)}\n\
         ::retract{suspects(vesna, _)}\n\
         @narrator: It is cold.\n\
         </entry>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    let body = &doc.entries[0].body;
    assert!(
        matches!(
            body[..],
            [
                Node::Match(_),
                Node::Set(_),
                Node::Assert(_),
                Node::Retract(_),
                Node::Line(_)
            ]
        ),
        "{body:?}"
    );
    let Node::Match(m) = &body[0] else {
        unreachable!()
    };
    assert_eq!(m.subject.raw, "run.labBurned");
    assert_eq!(m.arms.len(), 2);
}

#[test]
fn entry_body_admission_is_not_the_parsers() {
    // Directives / <branch> parse as ordinary nodes; rejecting them in an
    // entry body is the checker's E-GRAMMAR-NOT-ADMITTED.
    let (doc, diags) = parse(
        "<entry id=\"e\">\n::bg{id=\"lab\"}\n<branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\">\n@x: hi\n</choice>\n</branch>\n</entry>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    assert!(matches!(
        doc.entries[0].body[..],
        [Node::Directive(_), Node::Branch(_)]
    ));
}

#[test]
fn nested_entry_behaves_like_nested_quest() {
    // <entry> is top-level only, like <quest>: nested, it reports the
    // open block the same way.
    let codes = |tag: &str| {
        let (_, diags) = parse(&format!(
            "<{tag} id=\"o\">\n<{tag} id=\"inner\">\n@x: hi\n</{tag}>\n</{tag}>\n"
        ));
        diags
            .iter()
            .map(|d| (d.code.clone(), d.span.byte_start, d.span.byte_end))
            .collect::<Vec<_>>()
    };
    let entry = codes("entry");
    assert_eq!(entry.len(), 1, "{entry:?}");
    assert_eq!(entry[0].0, E_UNCLOSED_TAG);
    assert_eq!(entry, codes("quest"));
    // A mixed nesting names both tags.
    let (doc, diags) = parse("<beat id=\"b\">\n<entry id=\"inner\">\n</entry>\n</beat>\n");
    assert!(
        diags[0]
            .message
            .starts_with("a `<entry>` cannot sit inside a `<beat>`; `b` opened at line 1"),
        "{diags:?}"
    );
    assert_eq!((doc.beats.len(), doc.entries.len()), (1, 1));
}

#[test]
fn unclosed_entry_is_unclosed_tag() {
    let (doc, diags) = parse("<entry id=\"e\">\n@x: hi\n");
    assert!(diags.iter().any(|d| d.code == E_UNCLOSED_TAG), "{diags:?}");
    assert_eq!(doc.entries[0].body.len(), 1);
}

// -- dsl 0.5.0 §2.1: E-UNCLASSIFIED split into named causes --

#[test]
fn content_before_first_heading_is_content_outside_shot() {
    let (_, diags) = parse("@narrator: hello before any shot\n");
    assert!(
        diags.iter().any(|d| d.code == "E-CONTENT-OUTSIDE-SHOT"),
        "{diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLASSIFIED"),
        "must not fall through to the residual E-UNCLASSIFIED: {diags:?}"
    );
}

#[test]
fn directive_and_tag_before_first_heading_are_also_content_outside_shot() {
    for src in [
        "::set{scene.a = 1}\n",
        "<branch id=\"b\"><choice id=\"c\" label=\"x\"/></branch>\n",
    ] {
        let (_, diags) = parse(src);
        assert!(
            diags.iter().any(|d| d.code == "E-CONTENT-OUTSIDE-SHOT"),
            "{src}: {diags:?}"
        );
    }
}

#[test]
fn genuinely_unrecognized_line_outside_shot_stays_unclassified() {
    // No sigil/tag shape at all -> the residual catch-all, not the new split code.
    let (_, diags) = parse("1234 not a valid construct\n");
    assert!(
        diags.iter().any(|d| d.code == "E-UNCLASSIFIED"),
        "{diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-CONTENT-OUTSIDE-SHOT"),
        "{diags:?}"
    );
}

#[test]
fn content_line_bracket_form_is_named() {
    // §7.1: content-line attributes are `{…}`, not `[…]`.
    let (_, diags) = parse("## Shot 1.\n@mira[emotion=\"x\"]: hi\n");
    assert!(
        diags.iter().any(|d| d.code == "E-CONTENT-LINE-BRACKET"),
        "{diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLASSIFIED"),
        "must not fall through to the missing-second-colon E-UNCLASSIFIED: {diags:?}"
    );
}

#[test]
fn wrapped_tag_attribute_is_tag_not_one_line() {
    // §2.3: a <tag>'s attributes must all be on the tag's own physical
    // line; wrapping is E-TAG-NOT-ONE-LINE, naming the one-line rule,
    // not a misleading E-UNCLOSED-TAG/E-UNCLASSIFIED.
    let (_, diags) = parse(
        "## Shot 1.\n\
         <on event=\"x\"\n\
         when=\"run.a\">\n\
         </on>\n",
    );
    let d = diags
        .iter()
        .find(|d| d.code == "E-TAG-NOT-ONE-LINE")
        .unwrap_or_else(|| panic!("expected E-TAG-NOT-ONE-LINE: {diags:?}"));
    assert!(
        d.message.contains("one physical line"),
        "message must name the one-physical-line rule: {}",
        d.message
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLOSED-TAG"),
        "the properly-closed </on> must not ALSO trip a misleading unclosed-tag error: {diags:?}"
    );
}

// RC2: a quoted attribute value containing an embedded PHYSICAL newline
// must not let `scan_attrs` read across the line looking for the closing
// quote + `>` on a LATER line — that silently satisfies the "found `>`"
// check and desyncs the cursor (the later line's bytes get consumed
// twice: once mis-scanned into the attribute, once re-parsed as the next
// node).
#[test]
fn newline_inside_quoted_attr_value_is_tag_not_one_line() {
    let (doc, diags) = parse(
        "## Shot 1.\n\
         <on event=\"a\n\
         b\" when=\"run.x\">\n\
         </on>\n",
    );
    assert!(
        diags.iter().any(|d| d.code == "E-TAG-NOT-ONE-LINE"),
        "embedded newline inside a quoted value must still be E-TAG-NOT-ONE-LINE: {diags:?}"
    );
    // No desync: the mis-scanned `>` on the LATER physical line must not
    // be accepted as this tag's terminator, so the real `</on>` is still
    // seen as the block's (only) close — never a spurious unclosed-tag.
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLOSED-TAG"),
        "the real </on> close must still be found (no cursor desync): {diags:?}"
    );
    // No bytes reparsed: the shot body holds exactly the one `On` node —
    // a desync would either drop it or fabricate extra nodes from the
    // wrongly-consumed/re-consumed line.
    assert_eq!(
        doc.shots[0].body.len(),
        1,
        "exactly one On node, no duplicate/dropped nodes from a desync: {:?}",
        doc.shots[0].body
    );
    assert!(
        matches!(doc.shots[0].body[0], Node::On(_)),
        "expected an On node: {:?}",
        doc.shots[0].body
    );
}

#[test]
fn inline_tag_body_is_named() {
    // §2.3: an element with children uses the BLOCK form — children on
    // their own lines. A single-line `<tag>…</tag>` must name exactly that,
    // not a misleading E-UNCLOSED-TAG (the close is right there on the
    // line) or E-UNCLASSIFIED (the following arm is well-formed).
    let (doc, diags) = parse(
        "## Shot 1.\n\
         <match on=\"run.mood\">\n\
         <when is=\"calm\"> @fixer{mono}: Steady. </when>\n\
         <otherwise> @fixer{mono}: Not steady. </otherwise>\n\
         </match>\n",
    );
    assert_eq!(
        diags
            .iter()
            .filter(|d| d.code == "E-TAG-INLINE-BODY")
            .count(),
        2,
        "one diagnostic per offending line: {diags:?}"
    );
    let d = diags
        .iter()
        .find(|d| d.code == "E-TAG-INLINE-BODY")
        .unwrap_or_else(|| panic!("expected E-TAG-INLINE-BODY: {diags:?}"));
    assert!(
        d.message.contains("own line") && d.message.contains("§2.3"),
        "message must name the own-line rule and cite §2.3: {}",
        d.message
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLOSED-TAG"),
        "the inline `</when>` IS the close — no unclosed-tag misdirection: {diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLASSIFIED"),
        "a well-formed `<otherwise>` is never `unexpected block here`: {diags:?}"
    );
    // The element IS complete on its line, so recovery keeps the arm
    // stream intact: both arms still reach the checker, which is what
    // denies the spurious exhaustiveness verdict a basis to fire.
    let Node::Match(m) = &doc.shots[0].body[0] else {
        panic!("expected a Match node: {:?}", doc.shots[0].body);
    };
    assert_eq!(m.arms.len(), 2, "both arms recovered: {:?}", m.arms);
}

#[test]
fn block_form_tag_body_stays_clean() {
    // Regression guard for the fix above: the SUPPORTED block form of the
    // same content must keep parsing with zero diagnostics.
    let (_, diags) = parse(
        "## Shot 1.\n\
         <match on=\"run.mood\">\n\
         <when is=\"calm\">\n\
         @fixer{mono}: Steady.\n\
         </when>\n\
         <otherwise>\n\
         @fixer{mono}: Not steady.\n\
         </otherwise>\n\
         </match>\n",
    );
    assert!(diags.is_empty(), "block form must parse clean: {diags:?}");
}

#[test]
fn inline_tag_body_named_on_sibling_block_elements() {
    // The mistake is available on every block element, and every one of
    // them opens through the SAME `parse_open_tag`, so `<on>` reports it
    // exactly as `<when>`/`<otherwise>` do — no unclosed-tag cascade.
    let (_, diags) = parse("## Shot 1.\n<on event=\"x\"> @fixer: hi. </on>\n");
    assert_eq!(
        diags
            .iter()
            .filter(|d| d.code == "E-TAG-INLINE-BODY")
            .count(),
        1,
        "{diags:?}"
    );
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLOSED-TAG"),
        "{diags:?}"
    );
}

#[test]
fn unclassified_after_content_line_gets_continuation_hint() {
    let (_, diags) = parse("## Shot 1.\n@mira: hello\ngarbage next\n");
    let d = diags
        .iter()
        .find(|d| d.code == "E-UNCLASSIFIED")
        .unwrap_or_else(|| panic!("garbage line must still error: {diags:?}"));
    assert!(
        d.message.contains("cannot span multiple physical lines"),
        "{}",
        d.message
    );
}

// Round-6 T3-60: an Ink/Yarn line (or speakerless prose) after a finished
// content line was told it "cannot span multiple physical lines". It now
// names the Lute form, and the wrap note is kept for real wraps.
#[test]
fn foreign_lines_after_a_content_line_name_the_lute_form() {
    for (line, want) in [
        ("-> ledger", "`::next{to=\"ledger\"}`"),
        ("-> END", "`terminal:`"),
        ("~ run.oil = run.oil + 2", "`::set{run.oil = run.oil + 2}`"),
        ("VAR x = 1", "`state:`"),
        ("* [Read the ledger]", "`once`"),
        ("+ [Wait for dark]", "`<hub>`"),
        ("- gather", "no gathers"),
        ("=== ledger ===", "`## ledger`"),
        ("Plain prose line with no speaker.", "`@narrator: …`"),
        ("<<set $oil to 3>>", "`::set{run.oil = 3}`"),
        ("<<if $oil > 2>>", "`<match"),
    ] {
        let (_, diags) = parse(&format!("## Shot 1.\n@narrator: hi\n{line}\n"));
        let d = diags
            .iter()
            .find(|d| d.code == "E-UNCLASSIFIED")
            .unwrap_or_else(|| panic!("{line}: {diags:?}"));
        assert!(
            d.message.contains(want) && !d.message.contains("physical lines"),
            "{line}: {}",
            d.message
        );
    }
    let (_, diags) = parse("## Shot 1.\n@narrator: hi\n# mood:cold\n");
    assert!(
        diags
            .iter()
            .any(|d| d.code == "E-TITLE-PLACEMENT" && d.message.contains("`// …`")),
        "{diags:?}"
    );
}

#[test]
fn legacy_sigil_diagnostic_mentions_lute_fix() {
    let (_, diags) = parse("## Shot 1.\n:mira: hi\n");
    let d = diags
        .iter()
        .find(|d| d.code == "E-LEGACY-CONTENT-SIGIL" && d.message.contains("sigil"))
        .unwrap_or_else(|| panic!("expected legacy sigil diagnostic: {diags:?}"));
    assert!(
        d.message.contains("lute fix"),
        "must mention `lute fix` applies the migration automatically: {}",
        d.message
    );
    // FL1 (dsl 0.5.0 §2.1): the legacy `:` sigil is a precisely-recognized
    // deprecated shape — it must never fall through to the residual
    // E-UNCLASSIFIED catch-all.
    assert!(
        !diags.iter().any(|d| d.code == "E-UNCLASSIFIED"),
        "legacy sigil must not ALSO/instead report E-UNCLASSIFIED: {diags:?}"
    );
}
