//! The #173 synthetic project generator (`lute-bench generate-vocab`, tier
//! `vocab`).
//!
//! A byte-for-byte port of the issue's `gen.py`: `scenes` scene documents,
//! `quests` quest documents, `paths` int state paths declared
//! `per: kind{i % kinds}`, and `kinds` closed entity kinds of `members`
//! members each, so `paths × members` per-member state slots. Every quest reads
//! per-member state in `start` and an objective `done`, and chains on the
//! previous quest. `scripts/vocab-grid.py` measures `lute` over these projects
//! (spec 0.49.0 §9.8); keep the output identical to `gen.py`, since the issue's
//! numbers and the gate thresholds are stated against it.
//!
//! One flag is not `gen.py`'s: `-E` (default 0, so the default output stays
//! `gen.py`'s) turns the last `endings` scenes into ending beats (`on:`,
//! `once:`, a `when` over a per-member path the scenes write, and `::end`),
//! so `scenario reach --endings` reports a row for each (spec 0.53.0 §3.7,
//! tier `endings`).

use std::fmt;
use std::path::Path;

/// The generator's parameters, named after `gen.py`'s flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VocabParams {
    /// `-S`: scene documents `scenes/sNNN.lute`.
    pub(crate) scenes: usize,
    /// `-N`: quest documents `quests/qNNNN.lute`.
    pub(crate) quests: usize,
    /// `-M`: `user.sI` int paths declared `per: kind{I % K}`.
    pub(crate) paths: usize,
    /// `-K`: closed entity kinds `kind0..kind{K-1}`.
    pub(crate) kinds: usize,
    /// `-P`: members per kind.
    pub(crate) members: usize,
    /// `-E` (not `gen.py`'s): the last `endings` scenes are ending beats.
    pub(crate) endings: usize,
}

impl Default for VocabParams {
    /// `gen.py`'s defaults.
    fn default() -> Self {
        Self {
            scenes: 20,
            quests: 100,
            paths: 10,
            kinds: 5,
            members: 15,
            endings: 0,
        }
    }
}

impl VocabParams {
    /// The benchmark tier `vocab` (spec §9.8): 20 scenes + 200 quests,
    /// 160 paths × 15 members = 2 400 per-member slots.
    pub(crate) const TIER: Self = Self {
        scenes: 20,
        quests: 200,
        paths: 160,
        kinds: 5,
        members: 15,
        endings: 0,
    };

    /// Parses `gen.py`'s flags (`-N -M -K -P -S`, each taking a value) and
    /// `-E`.
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut params = Self::default();
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            let field = match flag.as_str() {
                "-S" => &mut params.scenes,
                "-N" => &mut params.quests,
                "-M" => &mut params.paths,
                "-K" => &mut params.kinds,
                "-P" => &mut params.members,
                "-E" => &mut params.endings,
                other => return Err(format!("unknown generate-vocab argument `{other}`")),
            };
            let value = args
                .next()
                .ok_or_else(|| format!("{flag} requires a value"))?;
            *field = value
                .parse()
                .map_err(|error| format!("{flag} must be a non-negative integer: {error}"))?;
        }
        Ok(params)
    }
}

/// Why a project could not be generated.
#[derive(Debug)]
pub(crate) enum GenerateError {
    /// `paths > 0` with `kinds == 0`: no kind for the `per:` declarations.
    NoKinds,
    /// `paths > 0` and `quests > 0` with `members == 0`: quests read a member
    /// of an empty kind (`gen.py` fails on it with `ZeroDivisionError`).
    NoMembers,
    /// Creating a directory or writing a file failed.
    Io(std::io::Error),
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoKinds => f.write_str("K must be >= 1 when M > 0"),
            Self::NoMembers => f.write_str("P must be >= 1 when M > 0 and N > 0"),
            Self::Io(error) => write!(f, "cannot write the project: {error}"),
        }
    }
}

impl std::error::Error for GenerateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoKinds | Self::NoMembers => None,
            Self::Io(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for GenerateError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

const PROJECT: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";
const USES: &str = "uses: ../schema/world.schema.yaml";

/// Writes the project under `out`, creating directories as needed and
/// overwriting generated files (as `gen.py` does; other files are kept).
pub(crate) fn generate(out: &Path, params: VocabParams) -> Result<(), GenerateError> {
    if params.paths > 0 && params.kinds < 1 {
        return Err(GenerateError::NoKinds);
    }
    if params.paths > 0 && params.quests > 0 && params.members < 1 {
        return Err(GenerateError::NoMembers);
    }
    for dir in ["schema", "scenes", "quests"] {
        std::fs::create_dir_all(out.join(dir))?;
    }
    std::fs::write(out.join("lute.project.yaml"), PROJECT)?;
    std::fs::write(out.join("schema/world.schema.yaml"), schema(params))?;
    for s in 0..params.scenes {
        std::fs::write(out.join(format!("scenes/s{s:03}.lute")), scene(params, s))?;
    }
    for n in 1..=params.quests {
        std::fs::write(out.join(format!("quests/q{n:04}.lute")), quest(params, n))?;
    }
    Ok(())
}

fn schema(params: VocabParams) -> String {
    let kinds = params.kinds.max(1);
    let mut lines = vec!["state:".to_string()];
    if params.paths > 0 {
        for i in 0..params.paths {
            lines.push(format!(
                "  user.s{i}: {{ type: int, default: 0, per: kind{} }}",
                i % kinds
            ));
        }
    } else {
        lines.push("  user.base: { type: int, default: 0 }".to_string());
    }
    lines.push("entities:".to_string());
    for k in 0..params.kinds {
        let members = (0..params.members)
            .map(|j| format!("k{k}m{j}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("  kind{k}: {{ members: [{members}] }}"));
    }
    lines.join("\n") + "\n"
}

fn scene(params: VocabParams, s: usize) -> String {
    let (set, read) = if params.paths > 0 {
        ("::set{user.s0.k0m0 += 1}", "user.s0.k0m0 >= 1")
    } else {
        ("::set{user.base += 1}", "user.base >= 1")
    };
    let id = format!("id: scene.s{s:03}");
    let room = format!("@narrator: A quiet room, number {s}.");
    let mut lines = vec!["---", "kind: scene", &id];
    let when = format!("when: \"{read}\"");
    let ending = params.endings > 0 && s + params.endings >= params.scenes;
    if ending {
        lines.extend(["on: day_end", "once: run", &when]);
    }
    lines.extend([
        USES,
        "---",
        "",
        "## Opening",
        "",
        &room,
        "@narrator: Something stirs.",
        set,
    ]);
    if ending {
        lines.push("::end");
    }
    lines.push("");
    lines.join("\n") + "\n"
}

/// Per-member state path number `i`, member `j` of its kind.
fn slot(params: VocabParams, i: usize, j: usize) -> String {
    let path = i % params.paths;
    let kind = path % params.kinds.max(1);
    format!("user.s{path}.k{kind}m{}", j % params.members)
}

fn quest(params: VocabParams, n: usize) -> String {
    let (mut start, done1, done2) = if params.paths > 0 {
        let s = |i, j| slot(params, i, j);
        (
            format!("{} >= 0 && {} >= 0", s(n, n), s(n + 1, n + 2)),
            format!("{} >= 1", s(n + 2, n + 3)),
            format!("{} >= 2 || {} >= 1", s(n + 3, n + 5), s(n + 4, n)),
        )
    } else {
        (
            "user.base >= 0".to_string(),
            "user.base >= 1".to_string(),
            "user.base >= 2".to_string(),
        )
    };
    if n > 1 {
        start.push_str(&format!(" && quest.q{:04}.state == 'complete'", n - 1));
    }
    let mut doc = vec![
        "---".to_string(),
        "kind: quest".to_string(),
        USES.to_string(),
        "---".to_string(),
        String::new(),
        format!("<quest id=\"q{n:04}\" title=\"Quest {n}\" start=\"{start}\">"),
        format!("  <objective id=\"a\" title=\"Step A\" done=\"{done1}\"/>"),
        format!("  <objective id=\"b\" title=\"Step B\" done=\"{done2}\"/>"),
        "  <on event=\"questComplete\">".to_string(),
    ];
    if params.paths > 0 {
        doc.push(format!("    ::set{{{} += 1}}", slot(params, n + 5, n + 1)));
        doc.push(format!("    ::set{{{} += 1}}", slot(params, n + 6, n + 2)));
    } else {
        doc.push("    ::set{user.base += 1}".to_string());
    }
    doc.extend(["  </on>".to_string(), "</quest>".to_string(), String::new()]);
    doc.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: VocabParams = VocabParams {
        scenes: 1,
        quests: 2,
        paths: 3,
        kinds: 2,
        members: 2,
        endings: 0,
    };

    // Expected bytes are `gen.py`'s output for `-S 1 -N 2 -M 3 -K 2 -P 2`
    // (issue #173), copied verbatim.
    #[test]
    fn schema_matches_gen_py() {
        assert_eq!(
            schema(SMALL),
            "state:\n\
             \x20 user.s0: { type: int, default: 0, per: kind0 }\n\
             \x20 user.s1: { type: int, default: 0, per: kind1 }\n\
             \x20 user.s2: { type: int, default: 0, per: kind0 }\n\
             entities:\n\
             \x20 kind0: { members: [k0m0, k0m1] }\n\
             \x20 kind1: { members: [k1m0, k1m1] }\n"
        );
    }

    #[test]
    fn scene_matches_gen_py() {
        assert_eq!(
            scene(SMALL, 0),
            "---\nkind: scene\nid: scene.s000\nuses: ../schema/world.schema.yaml\n---\n\n\
             ## Opening\n\n@narrator: A quiet room, number 0.\n@narrator: Something stirs.\n\
             ::set{user.s0.k0m0 += 1}\n\n"
        );
    }

    #[test]
    fn quests_match_gen_py() {
        assert_eq!(
            quest(SMALL, 1),
            "---\nkind: quest\nuses: ../schema/world.schema.yaml\n---\n\n\
             <quest id=\"q0001\" title=\"Quest 1\" start=\"user.s1.k1m1 >= 0 && user.s2.k0m1 >= 0\">\n\
             \x20 <objective id=\"a\" title=\"Step A\" done=\"user.s0.k0m0 >= 1\"/>\n\
             \x20 <objective id=\"b\" title=\"Step B\" done=\"user.s1.k1m0 >= 2 || user.s2.k0m1 >= 1\"/>\n\
             \x20 <on event=\"questComplete\">\n\
             \x20   ::set{user.s0.k0m0 += 1}\n\
             \x20   ::set{user.s1.k1m1 += 1}\n\
             \x20 </on>\n\
             </quest>\n"
        );
        assert!(quest(SMALL, 2).contains(" && quest.q0001.state == 'complete'\">"));
    }

    #[test]
    fn no_state_variant_matches_gen_py() {
        let params = VocabParams {
            paths: 0,
            kinds: 0,
            ..SMALL
        };
        assert_eq!(
            schema(params),
            "state:\n  user.base: { type: int, default: 0 }\nentities:\n"
        );
        assert!(scene(params, 7).ends_with("Something stirs.\n::set{user.base += 1}\n\n"));
        assert!(quest(params, 1).contains("start=\"user.base >= 0\">"));
    }

    /// `-E`: the last scenes are ending beats; the others stay `gen.py`'s.
    #[test]
    fn ending_scenes_are_beats_that_end() {
        let params = VocabParams {
            scenes: 3,
            endings: 1,
            ..SMALL
        };
        assert_eq!(scene(params, 1), scene(SMALL, 1));
        assert_eq!(
            scene(params, 2),
            "---\nkind: scene\nid: scene.s002\non: day_end\nonce: run\n\
             when: \"user.s0.k0m0 >= 1\"\nuses: ../schema/world.schema.yaml\n---\n\n\
             ## Opening\n\n@narrator: A quiet room, number 2.\n@narrator: Something stirs.\n\
             ::set{user.s0.k0m0 += 1}\n::end\n\n"
        );
    }

    #[test]
    fn rejects_parameters_gen_py_cannot_generate() {
        let out = std::env::temp_dir().join("lute-bench-vocab-never-written");
        let no_kinds = VocabParams { kinds: 0, ..SMALL };
        assert!(matches!(
            generate(&out, no_kinds),
            Err(GenerateError::NoKinds)
        ));
        let no_members = VocabParams {
            members: 0,
            ..SMALL
        };
        assert!(matches!(
            generate(&out, no_members),
            Err(GenerateError::NoMembers)
        ));
        assert!(!out.exists());
    }

    #[test]
    fn parses_gen_py_flags_over_its_defaults() {
        let args = ["-S", "100", "-N", "0", "-M", "40", "-E", "5"].map(String::from);
        assert_eq!(
            VocabParams::parse(&args),
            Ok(VocabParams {
                scenes: 100,
                quests: 0,
                paths: 40,
                endings: 5,
                ..VocabParams::default()
            })
        );
        VocabParams::parse(&["-X".to_string()]).unwrap_err();
        VocabParams::parse(&["-N".to_string()]).unwrap_err();
        VocabParams::parse(&["-N".to_string(), "-1".to_string()]).unwrap_err();
    }
}
