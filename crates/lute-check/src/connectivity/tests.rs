
    use super::*;
    use std::path::PathBuf;
    use lute_core_span::Span;
    use lute_syntax::ast::{Document, Meta};

    fn span(line: u32) -> Span {
        Span {
            byte_start: (line as usize) * 10,
            byte_end: (line as usize) * 10 + 1,
            line,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn doc(raw_yaml: &str) -> Document {
        Document {
            meta: Meta {
                raw_yaml: raw_yaml.to_string(),
                span: span(0),
            },
            sections: Vec::new(),
            quests: Vec::new(),
            entries: Vec::new(),
            beats: Vec::new(),
            span: span(0),
        }
    }
    fn views(docs: Vec<(PathBuf, Document)>) -> crate::ProjectDocs {
        crate::ProjectDocs::parse(docs, &lute_manifest::core::load_core_snapshot())
    }

    #[test]
    fn identical_pair_in_same_root_is_dup() {
        let raw = "kind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n";
        let docs = vec![
            (PathBuf::from("a.lute"), doc(raw)),
            (PathBuf::from("b.lute"), doc(raw)),
        ];
        let out = check_conn_episode_dup(&views(docs).views());
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1.code, "E-CONN-EPISODE-ID-DUP");
    }

    #[test]
    fn distinct_keys_do_not_collide() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc("kind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n"),
            ),
        ];
        assert!(check_conn_episode_dup(&views(docs).views()).is_empty());
    }

    #[test]
    fn cross_pair_join_collision_is_caught() {
        // character="a", episodeId="b.c"  vs  character="a.b", episodeId="c"  → same "a.b.c"
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc("kind: scene\ncharacter: a\nseason: 1\nepisode: 1\nepisodeId: b.c\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\ncharacter: a.b\nseason: 1\nepisode: 1\nepisodeId: c\n"),
            ),
        ];
        assert_eq!(check_conn_episode_dup(&views(docs).views()).len(), 1);
    }

    /// Regression (review note): a scene doc missing/mistyping
    /// `character`/`season`/`episode` must never fall back to a degenerate
    /// key (e.g. `.s00ep00`) — two such malformed docs must NOT collide.
    /// That doc's own missing-key problem is `E-META-MISSING`'s job, from
    /// the normal per-file `check()`, not this project-wide pass.
    #[test]
    fn missing_identity_keys_never_fabricate_a_dup() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc("kind: scene\nseason: 1\nepisode: 1\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\nseason: 1\nepisode: 1\n"),
            ),
        ];
        assert!(check_conn_episode_dup(&views(docs).views()).is_empty());
    }
