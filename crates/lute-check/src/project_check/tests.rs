use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use lute_core_span::{Severity, Span};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;
use super::*;
mod tests {
    use super::*;
    use lute_syntax::ast::{Meta, Quest};

    fn project(docs: Vec<(PathBuf, Document)>) -> crate::ProjectDocs {
        crate::ProjectDocs::parse(docs, &CapabilitySnapshot::default())
    }

    fn span(line: u32) -> Span {
        Span {
            byte_start: (line as usize) * 10,
            byte_end: (line as usize) * 10 + 1,
            line,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn quest(id: &str, id_line: u32) -> Quest {
        Quest {
            id: id.to_string(),
            id_span: span(id_line),
            title: None,
            start: None,
            fail: None,
            follows: None,
            follows_span: span(id_line),
            tier: None,
            activate: None,
            complete: None,
            accept: None,
            rearm: None,
            attrs: Vec::new(),
            body: Vec::new(),
            rewards: Vec::new(),
            span: span(id_line),
        }
    }

    fn doc(quests: Vec<Quest>) -> Document {
        Document {
            meta: Meta {
                raw_yaml: String::new(),
                span: span(0),
            },
            title: None,
            sections: Vec::new(),
            quests,
            entries: Vec::new(),
            beats: Vec::new(),
            span: span(0),
        }
    }

    #[test]
    fn no_docs_yields_no_diagnostics() {
        assert!(check_project_quest_ids(&[]).is_empty());
    }

    #[test]
    fn distinct_ids_across_files_do_not_collide() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("alpha", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("beta", 1)])),
        ];
        assert!(check_project_quest_ids(&project(docs).views()).is_empty());
    }

    #[test]
    fn empty_id_never_collides_here() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("", 1)])),
        ];
        assert!(
            check_project_quest_ids(&project(docs).views()).is_empty(),
            "an empty quest id is E-QUEST-ID-MISSING's problem, not this pass's"
        );
    }

    #[test]
    fn same_file_repeat_is_reported_without_naming_a_second_file() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![quest("q", 1), quest("q", 5)]),
        )];
        let out = check_project_quest_ids(&project(docs).views());
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("a.lute"));
        assert_eq!(d.code, "E-QUEST-ID-DUP");
        assert_eq!(d.span.line, 5, "anchored at the SECOND occurrence");
        assert!(
            !d.message.contains("across project files"),
            "an in-document repeat must not claim a cross-file collision: {}",
            d.message
        );
    }

    #[test]
    fn cross_file_collision_names_both_files_and_anchors_the_second() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("q", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("q", 2)])),
        ];
        let out = check_project_quest_ids(&project(docs).views());
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("b.lute"), "anchored in the SECOND file");
        assert_eq!(d.span.line, 2);
        assert!(d.message.contains("a.lute"), "{}", d.message);
        assert!(d.message.contains("b.lute"), "{}", d.message);
    }

    #[test]
    fn three_occurrences_flag_every_repeat_past_the_first() {
        // File A declares `q` twice (an in-document repeat); file B declares it
        // once more. Every occurrence PAST the first is flagged: A's 2nd (line
        // 5, same-file) and B's 1st (line 1, cross-file vs A).
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![quest("q", 1), quest("q", 5)]),
            ),
            (PathBuf::from("b.lute"), doc(vec![quest("q", 1)])),
        ];
        let out = check_project_quest_ids(&project(docs).views());
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[0].0, Path::new("a.lute"));
        assert_eq!(out[0].1.span.line, 5);
        assert!(!out[0].1.message.contains("across project files"));
        assert_eq!(out[1].0, Path::new("b.lute"));
        assert_eq!(out[1].1.span.line, 1);
        assert!(out[1].1.message.contains("across project files"));
    }

    #[test]
    fn distinct_ids_are_independent_of_each_other() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![quest("alpha", 1), quest("beta", 2)]),
            ),
            (
                PathBuf::from("b.lute"),
                doc(vec![quest("alpha", 1), quest("gamma", 2)]),
            ),
        ];
        let out = check_project_quest_ids(&project(docs).views());
        assert_eq!(out.len(), 1, "only `alpha` collides: {out:?}");
        assert_eq!(out[0].0, Path::new("b.lute"));
    }

    #[test]
    fn colliding_occurrences_empty_when_no_docs_collide() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("alpha", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("beta", 1)])),
        ];
        assert!(colliding_occurrences(&project(docs.clone()).views()).is_empty(), "{docs:?}");
    }

    #[test]
    fn colliding_occurrences_includes_the_groups_first_member_too() {
        // `check_project_quest_ids` never emits a diagnostic for the group's
        // FIRST occurrence (a.lute's) -- `colliding_occurrences` still must
        // report it as a member, since the caller needs to recognize a
        // per-file diagnostic anchored on EITHER file as covered.
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("q", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("q", 2)])),
        ];
        let out = colliding_occurrences(&project(docs).views());
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out.contains(&(PathBuf::from("a.lute"), span(1))), "{out:?}");
        assert!(out.contains(&(PathBuf::from("b.lute"), span(2))), "{out:?}");
    }

    #[test]
    fn colliding_occurrences_ignores_empty_ids() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("", 1)])),
        ];
        assert!(colliding_occurrences(&project(docs.clone()).views()).is_empty(), "{docs:?}");
    }

    // --- `check_project_quest_refs` (dsl 0.5.1 §1.4) ------------------------

    fn parsed(text: &str) -> Document {
        let (doc, diags) = lute_syntax::parse(text);
        assert!(diags.is_empty(), "fixture must parse clean: {diags:?}");
        doc
    }

    fn quest_doc(quest_id: &str, objective_id: &str) -> Document {
        parsed(&format!(
            "---\nkind: quest\n---\n<quest id=\"{quest_id}\">\n\
             <objective id=\"{objective_id}\" done=\"true\"/>\n</quest>\n"
        ))
    }

    fn scene_doc_matching(subject: &str) -> Document {
        parsed(&format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
             <match subject=\"{subject}\">\n<when is=\"true\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n"
        ))
    }

    #[test]
    fn quest_refs_no_docs_yields_no_diagnostics() {
        assert!(check_project_quest_refs(&project(vec![]).views(), false).is_empty());
    }

    #[test]
    fn quest_refs_known_quest_and_objective_yield_no_warning() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heist.state"),
            ),
        ];
        assert!(
            check_project_quest_refs(&project(docs.clone()).views(), false).is_empty(),
            "{docs:?}"
        );
    }

    #[test]
    fn quest_refs_known_objective_under_known_quest_yields_no_warning() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heist.objectives.steal.done"),
            ),
        ];
        assert!(
            check_project_quest_refs(&project(docs.clone()).views(), false).is_empty(),
            "{docs:?}"
        );
    }

    #[test]
    fn quest_refs_flags_typo_d_quest_id() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heits.state"),
            ),
        ];
        let out = check_project_quest_refs(&project(docs).views(), false);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("scene.lute"), "names the referencing doc");
        assert_eq!(d.code, "W-QUEST-REF-UNKNOWN");
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.message.contains("quest.heits.state"), "{}", d.message);
        assert!(d.message.contains("heits"), "{}", d.message);
    }

    /// Over a whole project a read no quest answers can never be true: an
    /// error, and a quest DOCUMENT id written where its quest id belongs is
    /// named as such.
    #[test]
    fn quest_refs_over_a_whole_project_are_errors_naming_the_doc_id_confusion() {
        let quest = parsed(
            "---\nkind: quest\nid: quest.lamp\n---\n<quest id=\"lampOut\">\n\
             <objective id=\"o\" done=\"run.d\"/>\n</quest>\n",
        );
        let docs = vec![
            (PathBuf::from("q.lute"), quest),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.lamp.state"),
            ),
        ];
        let out = check_project_quest_refs(&project(docs).views(), true);
        assert_eq!(out.len(), 1, "{out:?}");
        let d = &out[0].1;
        assert_eq!(d.code, "E-QUEST-REF-UNKNOWN");
        assert_eq!(d.severity, Severity::Error);
        assert!(
            d.message.contains("its quest is `lampOut`"),
            "{}",
            d.message
        );
        assert!(
            !d.message.contains("outside this walked directory"),
            "{}",
            d.message
        );
    }

    #[test]
    fn quest_refs_flags_unknown_objective_under_a_known_quest() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heist.objectives.bogus.done"),
            ),
        ];
        let out = check_project_quest_refs(&project(docs).views(), false);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("scene.lute"));
        assert_eq!(d.code, "W-QUEST-REF-UNKNOWN");
        assert_eq!(d.severity, Severity::Warning);
        assert!(
            d.message.contains("quest.heist.objectives.bogus.done"),
            "{}",
            d.message
        );
        assert!(d.message.contains("bogus"), "{}", d.message);
    }

    #[test]
    fn quest_refs_deduplicates_repeated_reads_in_one_document() {
        let scene = parsed(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
             <match subject=\"quest.heits.state\">\n\
             <when is=\"active\" test=\"quest.heits.state\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n",
        );
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (PathBuf::from("scene.lute"), scene),
        ];
        let out = check_project_quest_refs(&project(docs).views(), false);
        assert_eq!(out.len(), 1, "one path read twice is one warning: {out:?}");
    }

    #[test]
    fn quest_refs_ignores_ordinary_declared_paths() {
        let scene = parsed(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
             state:\n  run.flag: { type: bool, default: false }\n---\n## Shot 1.\n\
             <match subject=\"run.flag\">\n<when is=\"true\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n",
        );
        let docs = vec![(PathBuf::from("scene.lute"), scene)];
        assert!(
            check_project_quest_refs(&project(docs.clone()).views(), false).is_empty(),
            "{docs:?}"
        );
    }

    /// 0.10.0 §11.1: the reading set is the domain-typed attribute slots in the
    /// RESOLVED snapshot, not a fixed list — a plugin directive declaring
    /// `{ domain: reason }` makes `reason` read and the warning stops.
    #[test]
    fn reading_set_is_the_snapshots_domain_typed_slots() {
        let snap = lute_manifest::core::load_core_snapshot();
        let read = domain_reading_set(&snap);
        for name in [
            "action",
            "anchor",
            "emotion",
            "mood",
            "musicPlayback",
            "vfxType",
            "volume",
        ] {
            assert!(read.contains(name), "core reads `{name}`; got {read:?}");
        }
        assert!(
            !read.contains("reason"),
            "nothing core-declared reads a `reason` domain; got {read:?}"
        );
    }

    /// §11.1: a declared domain no active construct reads is `W-DOMAIN-UNREAD`.
    #[test]
    fn an_unread_declared_domain_warns() {
        let use_a = crate::check::DomainUse {
            declared: ["emotion".to_string(), "reason".to_string()]
                .into_iter()
                .collect(),
            read: ["emotion".to_string()].into_iter().collect(),
            at: span(1),
            homes: Default::default(),
        };
        let out = check_project_domain_reads(&[(PathBuf::from("a.lute"), &use_a)]);
        assert_eq!(
            out.len(),
            1,
            "one unread domain, one diagnostic; got {out:?}"
        );
        assert_eq!(out[0].1.code, "W-DOMAIN-UNREAD");
        assert_eq!(out[0].1.severity, Severity::Warning);
        assert!(
            out[0].1.message.contains("reason"),
            "the message must name the domain; got {}",
            out[0].1.message
        );
    }

    /// **D-V**, and it is the whole reason this pass is project-wide: a domain
    /// declared in a shared schema is read by SOME document. Warning on the
    /// scene that happens not to read it would be a false positive on the most
    /// common layout there is.
    #[test]
    fn a_domain_read_by_another_document_does_not_warn() {
        let declarer = crate::check::DomainUse {
            declared: ["action".to_string()].into_iter().collect(),
            read: Default::default(),
            at: span(1),
            homes: Default::default(),
        };
        let reader = crate::check::DomainUse {
            declared: ["action".to_string()].into_iter().collect(),
            read: ["action".to_string()].into_iter().collect(),
            at: span(1),
            homes: Default::default(),
        };
        let out = check_project_domain_reads(&[
            (PathBuf::from("a.lute"), &declarer),
            (PathBuf::from("b.lute"), &reader),
        ]);
        assert!(out.is_empty(), "the union is read; got {out:?}");
    }

    /// One diagnostic per unread DOMAIN, not per declaring document, anchored at
    /// the first declarer in byte-sorted path order.
    #[test]
    fn one_diagnostic_per_domain_at_the_first_declarer() {
        let u = crate::check::DomainUse {
            declared: ["reason".to_string()].into_iter().collect(),
            read: Default::default(),
            at: span(1),
            homes: Default::default(),
        };
        let out = check_project_domain_reads(&[
            (PathBuf::from("b.lute"), &u),
            (PathBuf::from("a.lute"), &u),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].0,
            PathBuf::from("a.lute"),
            "byte-sorted first declarer"
        );
    }

    /// §11.1's "domain-typed attribute slots" is not the whole reading set: a
    /// `relations:` entry's `args:` closed-checks every atom against that
    /// domain's membership, which is as active a read as a directive attr.
    /// Omitting it fired `W-DOMAIN-UNREAD` six times on `docs/examples` —
    /// `character`, `clue`, `crew`, `location`, `suspect`, `topic`, every one an
    /// `entities:` domain read only by a relation signature.
    #[test]
    fn relation_argument_positions_read_their_domain() {
        let mut vocab = crate::rel_schema::RelVocab::default();
        vocab.relations.insert(
            "knows".to_string(),
            lute_manifest::relations::RelationDecl {
                args: vec!["crew".to_string(), "topic".to_string(), String::new()],
                ..Default::default()
            },
        );
        let read = domain_reads_from_relations(&vocab);
        assert!(read.contains("crew"), "got {read:?}");
        assert!(read.contains("topic"), "got {read:?}");
        assert!(
            !read.contains(""),
            "a non-string YAML arg is preserved as \"\" and is not a domain; got {read:?}"
        );
    }

    // --- `check_project_quest_tree` (dsl 2026-08-31 §4 subquest design) ---

    fn objective(
        id: &str,
        quest: Option<&str>,
        optional: bool,
        line: u32,
    ) -> lute_syntax::ast::Objective {
        use lute_syntax::ast::{CelKind, CelSlot};
        lute_syntax::ast::Objective {
            id: id.to_string(),
            id_span: span(line),
            // A subquest objective's synthesized `done` predicate is written
            // downstream (`lute-compile`); the AST-level slot is empty raw
            // text in the `quest=Some` case, exactly as the parser lands it.
            done: CelSlot::raw(
                CelKind::Condition,
                if quest.is_none() {
                    "true".to_string()
                } else {
                    String::new()
                },
                span(line),
            ),
            quest: quest.map(str::to_string),
            quest_span: span(line),
            visible_when: None,
            title: None,
            optional,
            on: None,
            by: None,
            target: None,
            until: None,
            attrs: Vec::new(),
            body: Vec::new(),
            rewards: Vec::new(),
            span: span(line),
        }
    }

    fn quest_with(id: &str, id_line: u32, objectives: Vec<lute_syntax::ast::Objective>) -> Quest {
        let mut q = quest(id, id_line);
        q.body = objectives
            .into_iter()
            .map(lute_syntax::ast::Node::Objective)
            .collect();
        q
    }

    #[test]
    fn quest_tree_no_docs_yields_no_diagnostics() {
        assert!(check_project_quest_tree(&[]).is_empty());
    }

    #[test]
    fn quest_tree_unknown_child_flags_e_quest_ref_unknown() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![quest_with(
                "parent",
                1,
                vec![objective("goal", Some("ghost"), false, 5)],
            )]),
        )];
        let out = check_project_quest_tree(&project(docs).views());
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("a.lute"));
        assert_eq!(d.code, E_QUEST_REF_UNKNOWN);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span.line, 5, "anchored at the objective's quest_span");
        assert!(d.message.contains("ghost"), "{}", d.message);
        assert!(d.message.contains("parent"), "{}", d.message);
    }

    #[test]
    fn quest_tree_defined_child_across_files_does_not_flag_ref_unknown() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![quest_with(
                    "parent",
                    1,
                    vec![objective("goal", Some("child"), false, 5)],
                )]),
            ),
            (PathBuf::from("b.lute"), doc(vec![quest("child", 1)])),
        ];
        let out = check_project_quest_tree(&project(docs).views());
        assert!(
            out.is_empty(),
            "cross-file resolution must succeed: {out:?}"
        );
    }

    #[test]
    fn quest_tree_multi_parent_flags_every_edge_past_the_first() {
        // `child` is referenced by both `parentA` and `parentB`. `parentA`
        // is first in `docs` order, so only `parentB`'s edge earns the
        // diagnostic, anchored at its own `quest_span`.
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![
                    quest_with(
                        "parentA",
                        1,
                        vec![objective("goalA", Some("child"), false, 5)],
                    ),
                    quest("child", 10),
                ]),
            ),
            (
                PathBuf::from("b.lute"),
                doc(vec![quest_with(
                    "parentB",
                    1,
                    vec![objective("goalB", Some("child"), false, 7)],
                )]),
            ),
        ];
        let out = check_project_quest_tree(&project(docs).views());
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("b.lute"));
        assert_eq!(d.code, E_QUEST_MULTI_PARENT);
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span.line, 7);
        assert!(d.message.contains("parentA"), "{}", d.message);
        assert!(d.message.contains("parentB"), "{}", d.message);
        assert!(d.message.contains("child"), "{}", d.message);
    }

    #[test]
    fn quest_tree_two_objectives_in_same_parent_referencing_same_child_are_not_multi_parent() {
        // Distinct-parent count is 1, so no MULTI-PARENT diagnostic; the
        // parent's own in-quest duplicate is that quest's problem, not this
        // pass's.
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![
                quest_with(
                    "parent",
                    1,
                    vec![
                        objective("goal1", Some("child"), false, 5),
                        objective("goal2", Some("child"), false, 6),
                    ],
                ),
                quest("child", 10),
            ]),
        )];
        let out = check_project_quest_tree(&project(docs).views());
        assert!(
            !out.iter().any(|(_, d)| d.code == E_QUEST_MULTI_PARENT),
            "same-parent duplicate must not read as multi-parent: {out:?}"
        );
    }

    #[test]
    fn quest_tree_cycle_flags_e_quest_tree_cycle_at_the_closing_edge() {
        // parent -> mid -> parent forms a 2-cycle; the back-edge is
        // `mid`'s `<objective quest="parent">` at line 8.
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![
                quest_with("parent", 1, vec![objective("goal", Some("mid"), false, 5)]),
                quest_with("mid", 7, vec![objective("back", Some("parent"), false, 8)]),
            ]),
        )];
        let out = check_project_quest_tree(&project(docs).views());
        let cycles: Vec<_> = out
            .iter()
            .filter(|(_, d)| d.code == E_QUEST_TREE_CYCLE)
            .collect();
        assert_eq!(cycles.len(), 1, "exactly one cycle: {out:?}");
        let (_, d) = &cycles[0];
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span.line, 8, "anchored at the back-edge");
        assert!(d.message.contains("parent"), "{}", d.message);
        assert!(d.message.contains("mid"), "{}", d.message);
    }

    #[test]
    fn quest_tree_self_reference_flags_a_length_one_cycle() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![quest_with(
                "loop",
                1,
                vec![objective("self", Some("loop"), false, 4)],
            )]),
        )];
        let out = check_project_quest_tree(&project(docs).views());
        let cycles: Vec<_> = out
            .iter()
            .filter(|(_, d)| d.code == E_QUEST_TREE_CYCLE)
            .collect();
        assert_eq!(cycles.len(), 1, "one self-cycle: {out:?}");
        let (_, d) = &cycles[0];
        assert!(
            d.message.contains("self-reference"),
            "self-cycle must announce itself as such: {}",
            d.message
        );
        assert_eq!(d.span.line, 4);
    }

    #[test]
    fn quest_tree_cycle_is_reported_once_regardless_of_dfs_start() {
        // Three-node cycle A -> B -> C -> A. Whichever node the DFS starts
        // from, the cycle is one diagnostic.
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![
                quest_with("A", 1, vec![objective("oa", Some("B"), false, 2)]),
                quest_with("B", 3, vec![objective("ob", Some("C"), false, 4)]),
                quest_with("C", 5, vec![objective("oc", Some("A"), false, 6)]),
            ]),
        )];
        let out = check_project_quest_tree(&project(docs).views());
        let cycles: Vec<_> = out
            .iter()
            .filter(|(_, d)| d.code == E_QUEST_TREE_CYCLE)
            .collect();
        assert_eq!(cycles.len(), 1, "one ring, one diagnostic: {out:?}");
    }

    #[test]
    fn quest_tree_edge_with_unknown_child_is_excluded_from_cycle_graph() {
        // The only edge points to `ghost`, which is undefined; that edge
        // already earns E-QUEST-REF-UNKNOWN. It contributes nothing to the
        // cycle graph, so no bogus E-QUEST-TREE-CYCLE fires.
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![quest_with(
                "parent",
                1,
                vec![objective("goal", Some("ghost"), false, 5)],
            )]),
        )];
        let out = check_project_quest_tree(&project(docs).views());
        assert!(
            !out.iter().any(|(_, d)| d.code == E_QUEST_TREE_CYCLE),
            "unknown-child edges must not fabricate cycles: {out:?}"
        );
    }

    // --- `check_project_subquest_unsatisfiable` (dsl 2026-08-31 §4 ext) ---

    #[test]
    fn subquest_unsat_required_child_unreachable_flags_the_parent_objective() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![
                quest_with(
                    "parent",
                    1,
                    vec![objective("goal", Some("child"), false, 5)],
                ),
                quest("child", 10),
            ]),
        )];
        let mut unreachable = BTreeSet::new();
        unreachable.insert("child".to_string());
        let out = check_project_subquest_unsatisfiable(&project(docs).views(), &unreachable);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("a.lute"));
        assert_eq!(d.code, "E-OBJECTIVE-UNSATISFIABLE");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.span.line, 5, "anchored at the referencing objective");
        assert!(d.message.contains("child"), "{}", d.message);
        assert!(
            d.message.contains("E-QUEST-UNREACHABLE"),
            "the message must name the propagating cause: {}",
            d.message
        );
    }

    #[test]
    fn subquest_unsat_optional_child_never_gates_the_parent() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![
                quest_with("parent", 1, vec![objective("goal", Some("child"), true, 5)]),
                quest("child", 10),
            ]),
        )];
        let mut unreachable = BTreeSet::new();
        unreachable.insert("child".to_string());
        let out = check_project_subquest_unsatisfiable(&project(docs).views(), &unreachable);
        assert!(
            out.is_empty(),
            "an optional child does not gate parent completion (§2.1): {out:?}"
        );
    }

    #[test]
    fn subquest_unsat_reachable_child_does_not_flag() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![
                quest_with(
                    "parent",
                    1,
                    vec![objective("goal", Some("child"), false, 5)],
                ),
                quest("child", 10),
            ]),
        )];
        let unreachable: BTreeSet<String> = BTreeSet::new();
        let out = check_project_subquest_unsatisfiable(&project(docs).views(), &unreachable);
        assert!(out.is_empty(), "{out:?}");
    }

    fn run_tier(mut q: Quest) -> Quest {
        q.tier = Some(("run".to_string(), span(q.span.line)));
        q
    }

    #[test]
    fn tier_mix_across_documents_is_check_projects_and_same_document_is_checks() {
        // lamplight N1: a run-tier parent with a user-tier child, the child in
        // another file — project-wide, anchored at the `quest=` objective.
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![run_tier(quest_with(
                    "case",
                    1,
                    vec![objective("inq", Some("inq"), false, 5)],
                ))]),
            ),
            (PathBuf::from("b.lute"), doc(vec![quest("inq", 1)])),
        ];
        let out: Vec<_> = check_project_quest_tree(&project(docs).views())
            .into_iter()
            .filter(|(_, d)| d.code == E_QUEST_TIER_MIX)
            .collect();
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("a.lute"));
        assert_eq!(d.span.line, 5);
        assert_eq!(d.severity, Severity::Error);
        for needle in ["`inq` is tier `user`", "`case` is tier `run`"] {
            assert!(d.message.contains(needle), "{}", d.message);
        }

        // Both in one document: the per-file check reports it (the other
        // direction, a user-tier parent), and check-project does not repeat it.
        let same = doc(vec![
            quest_with("case", 1, vec![objective("acc", Some("acc"), false, 5)]),
            run_tier(quest("acc", 10)),
        ]);
        let per_file = check_doc_quest_tiers(&same);
        assert_eq!(per_file.len(), 1, "{per_file:?}");
        assert!(
            per_file[0].message.contains("`acc` resets to `unset`"),
            "{}",
            per_file[0].message
        );
        let project = check_project_quest_tree(&project(vec![(PathBuf::from("a.lute"), same)]).views());
        assert!(
            !project.iter().any(|(_, d)| d.code == E_QUEST_TIER_MIX),
            "{project:?}"
        );
    }

    #[test]
    fn matching_tiers_are_clean() {
        let same = doc(vec![
            run_tier(quest_with(
                "case",
                1,
                vec![objective("acc", Some("acc"), false, 5)],
            )),
            run_tier(quest("acc", 10)),
        ]);
        assert!(check_doc_quest_tiers(&same).is_empty());
        let user = doc(vec![
            quest_with("case", 1, vec![objective("inq", Some("inq"), false, 5)]),
            quest("inq", 10),
        ]);
        assert!(check_doc_quest_tiers(&user).is_empty());
    }
}
