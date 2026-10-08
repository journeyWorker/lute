//! The one transcript form every runtime's content lines are matched in
//! (`docs/design/runtime-unification.md` §3.6, S5): a `line` record as
//! `@speaker{delivery}: text`, where the delivery block is exactly the head
//! `lute play` prints — the role flag the line was written with (`mono` /
//! `vo` / `os`) and its authored attributes (`as=`, `emotion=`,
//! `variant=`, plugin stamp attributes), never the compiler's identity
//! fields. `lute play`, `lute test` (scene walks and play files) and the
//! differential harness all build their `said` text with [`said_line`] and
//! judge `transcriptContains` / `transcriptLacks` with [`find`].
//!
//! A needle line WITH an attribute block matches a line carrying those
//! attributes (every one it names, the line may carry more); a needle line
//! without one matches a line whatever its attributes (round-5 T1-11).

use std::collections::BTreeSet;

use serde_json::Value as Json;

/// The command fields a line head never shows: identity, text and the
/// role (rendered as its flag).
const HEAD_SKIP: [&str; 10] = [
    "position",
    "kind",
    "family",
    "text",
    "speaker",
    "lineId",
    "voiceKey",
    "role",
    "placeholders",
    "texts",
];

/// A record's (or command's) string field `key`, `""` when absent.
pub fn str_of<'a>(rec: &'a Json, key: &str) -> &'a str {
    rec.get(key).and_then(Json::as_str).unwrap_or("")
}

/// A command's scalar fields as `k="v"` / `k=true` / `k=3`, key order, minus
/// `skip`.
pub fn render_attrs(cmd: &Json, skip: &[&str]) -> String {
    let Json::Object(map) = cmd else {
        return String::new();
    };
    let mut parts = Vec::new();
    for (k, v) in map {
        if skip.contains(&k.as_str()) {
            continue;
        }
        match v {
            Json::String(s) => parts.push(format!("{k}=\"{s}\"")),
            Json::Bool(b) => parts.push(format!("{k}={b}")),
            Json::Number(n) => parts.push(format!("{k}={n}")),
            _ => {}
        }
    }
    parts.join(" ")
}

/// A line's source head: `@speaker` plus its delivery — the role flag it
/// was written with (`mono`/`vo`/`os`) and its attributes — read from the
/// artifact command `cmd` the line record came from.
pub fn line_head(speaker: &str, cmd: Option<&Json>) -> String {
    let flag = match cmd.and_then(|c| c.get("role")).and_then(Json::as_str) {
        Some(flag @ ("mono" | "vo" | "os")) => Some(flag),
        _ => None,
    };
    let attrs = cmd.map(|c| render_attrs(c, &HEAD_SKIP)).unwrap_or_default();
    let inner: Vec<&str> = flag
        .into_iter()
        .chain((!attrs.is_empty()).then_some(attrs.as_str()))
        .collect();
    if inner.is_empty() {
        format!("@{speaker}")
    } else {
        format!("@{speaker}{{{}}}", inner.join(" "))
    }
}

/// The canonical transcript line of one `line` record: [`line_head`] over
/// `cmd` (the artifact command at the record's `addr`), `: `, the record's
/// interpolated text.
pub fn said_line(rec: &Json, cmd: Option<&Json>) -> String {
    let field = |k: &str| rec.get(k).and_then(Json::as_str).unwrap_or("");
    format!("{}: {}", line_head(field("speaker"), cmd), field("text"))
}

/// One attribute of a head block: `key` with its unquoted value (`None`
/// for a flag).
type Attr = (String, Option<String>);

/// A transcript or needle line split into its bare form (`@speaker: text`,
/// or the line itself when it is not a speaker line) and the attributes its
/// block named (`None` without a block).
fn split(line: &str) -> (String, Option<Vec<Attr>>) {
    let Some(rest) = line.strip_prefix('@') else {
        return (line.to_string(), None);
    };
    let name_end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    let (name, after) = rest.split_at(name_end);
    let Some(block) = after.strip_prefix('{') else {
        return (line.to_string(), None);
    };
    // The block ends at the first `}` outside quotes.
    let mut quote = None;
    let close = block.char_indices().find_map(|(i, c)| match (quote, c) {
        (None, '"' | '\'') => {
            quote = Some(c);
            None
        }
        (Some(q), c) if c == q => {
            quote = None;
            None
        }
        (None, '}') => Some(i),
        _ => None,
    });
    match close {
        Some(i) => (
            format!("@{name}{}", &block[i + 1..]),
            Some(parse_attrs(&block[..i])),
        ),
        None => (line.to_string(), None),
    }
}

/// `mono emotion="sad" variant=0` -> `[(mono, None), (emotion, sad),
/// (variant, 0)]`: whitespace-separated, a value quoted or bare.
fn parse_attrs(block: &str) -> Vec<Attr> {
    let mut out = Vec::new();
    let mut chars = block.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut key = String::new();
        while let Some(c) = chars.next_if(|c| !c.is_whitespace() && *c != '=') {
            key.push(c);
        }
        if key.is_empty() {
            if chars.next().is_none() {
                return out;
            }
            continue;
        }
        if chars.next_if_eq(&'=').is_none() {
            out.push((key, None));
            continue;
        }
        let mut value = String::new();
        match chars.next_if(|c| *c == '"' || *c == '\'') {
            Some(q) => {
                for c in chars.by_ref() {
                    if c == q {
                        break;
                    }
                    value.push(c);
                }
            }
            None => {
                while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                    value.push(c);
                }
            }
        }
        out.push((key, Some(value)));
    }
}

/// Does a line carrying `have` satisfy a needle naming `want`?
fn attrs_match(want: &[Attr], have: &[Attr]) -> bool {
    want.iter().all(|w| have.contains(w))
}

/// Where `needle` occurs in the transcript `said` (one canonical line per
/// `\n`): the transcript lines the first match spans, in order, or `None`.
/// The needle is a substring over the lines' bare `@speaker: text` form, as
/// before; each needle line that carries an attribute block must start at a
/// transcript line whose attributes include the ones it names.
pub fn find<'a>(said: &'a str, needle: &str) -> Option<Vec<&'a str>> {
    let lines: Vec<&str> = said.lines().collect();
    let parsed: Vec<(String, Vec<Attr>)> = lines
        .iter()
        .map(|l| {
            let (bare, attrs) = split(l);
            (bare, attrs.unwrap_or_default())
        })
        .collect();
    let mut bare = String::new();
    let mut starts = Vec::with_capacity(parsed.len());
    for (b, _) in &parsed {
        starts.push(bare.len());
        bare.push_str(b);
        bare.push('\n');
    }
    let wanted: Vec<(String, Option<Vec<Attr>>)> = needle.split('\n').map(split).collect();
    let bare_needle = wanted
        .iter()
        .map(|(b, _)| b.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut from = 0;
    if bare_needle.is_empty() {
        return Some(Vec::new());
    }
    while let Some(hit) = bare[from..].find(&bare_needle).map(|i| i + from) {
        let first = starts.partition_point(|s| *s <= hit) - 1;
        let at_start = starts[first] == hit;
        let end = hit + bare_needle.len();
        let last = starts.partition_point(|s| *s < end).max(first + 1) - 1;
        let ok = wanted.iter().enumerate().all(|(j, (_, want))| match want {
            None => true,
            Some(want) => {
                (j > 0 || at_start)
                    && parsed
                        .get(first + j)
                        .is_some_and(|(_, have)| attrs_match(want, have))
            }
        });
        if ok {
            return Some(lines[first..=last.min(lines.len() - 1)].to_vec());
        }
        from = hit + bare[hit..].chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// The transcript line nearest a needle that matched none — what a
/// `transcriptContains` miss quotes (round-5 T3-16). The miss is about the
/// needle's first line that no transcript line contains; when every needle
/// line is said somewhere (with other attributes, or not in this order) the
/// line saying its first one is quoted. Otherwise lines rank by the missing
/// needle line's speaker first, then the step where the needle's other lines
/// were said, then how many edits the needle line needs to occur in the line
/// (the rest of a long line is free; a short line is not). `steps` holds the
/// index of each step's first transcript line, ascending (empty: one step).
/// `None` when nothing was said.
pub fn nearest<'a>(said: &'a str, steps: &[usize], needle: &str) -> Option<&'a str> {
    let lines: Vec<(&str, String)> = said.lines().map(|l| (l, split(l).0)).collect();
    let wanted: Vec<String> = needle
        .trim()
        .split('\n')
        .map(|l| split(l).0)
        .filter(|b| !b.trim().is_empty())
        .collect();
    let saying = |w: &str| -> Vec<usize> {
        (0..lines.len())
            .filter(|&i| lines[i].1.contains(w))
            .collect()
    };
    let Some(k) = wanted.iter().position(|w| saying(w).is_empty()) else {
        let first = saying(wanted.first()?).into_iter().next()?;
        return Some(lines[first].0);
    };
    let step_of = |i: usize| steps.partition_point(|&s| s <= i);
    let anchored: BTreeSet<usize> = wanted.iter().flat_map(|w| saying(w)).map(step_of).collect();
    let missing = &wanted[k];
    let speaker = speaker_of(missing);
    (0..lines.len())
        .filter(|&i| !lines[i].0.trim().is_empty())
        .min_by_key(|&i| {
            (
                speaker.is_some() && speaker_of(&lines[i].1) != speaker,
                !anchored.contains(&step_of(i)),
                substring_distance(missing, &lines[i].1),
            )
        })
        .map(|i| lines[i].0)
}

/// The speaker of a bare `@speaker: text` line.
fn speaker_of(bare: &str) -> Option<&str> {
    bare.strip_prefix('@')?.split_once(':').map(|(s, _)| s)
}

/// How many edits `needle` needs to occur somewhere in `line` (approximate
/// substring matching): the text of `line` around the match is free, the
/// part of the needle a short line lacks is not.
fn substring_distance(needle: &str, line: &str) -> usize {
    let hay: Vec<char> = line.chars().collect();
    // Row `i` holds, per end position in `line`, the edits the needle's
    // first `i` chars need to end there; row 0 is free anywhere.
    let mut prev = vec![0usize; hay.len() + 1];
    let mut cur = vec![0usize; hay.len() + 1];
    for (i, n) in needle.chars().enumerate() {
        cur[0] = i + 1;
        for (j, h) in hay.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(n != *h))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev.into_iter().min().unwrap_or(0)
}

/// Judge one `transcriptContains` (`want_present`) / `transcriptLacks`
/// needle against `said`: `None` when it holds, else what the transcript
/// shows — `absent (nearest line: "…")` quoting a real line ([`nearest`],
/// over `steps`), or `present (line: "…")` quoting the line(s) that
/// matched, never the needle.
pub fn judge(said: &str, steps: &[usize], needle: &str, want_present: bool) -> Option<String> {
    let hit = find(said, needle);
    match (hit, want_present) {
        (Some(_), true) | (None, false) => None,
        (None, true) => Some(match nearest(said, steps, needle) {
            Some(line) => format!("absent (nearest line: {line:?})"),
            None => "absent".to_string(),
        }),
        (Some(lines), false) => Some(format!("present (line: {:?})", lines.join("\n"))),
    }
}

/// The delivery flags a line head shows for its role ([`line_head`]).
const HEAD_FLAGS: [&str; 3] = ["mono", "os", "vo"];


/// Authored content-line attributes a line head never shows: `code` feeds
/// the line's identity and `id` is a `::jump` label.
const HEAD_SKIP_AUTHORED: [&str; 2] = ["code", "id"];

/// The speaker a needle line's head names — `@name` right before its `:`
/// or attribute block — or `None` when the line is no speaker head.
fn head_speaker(line: &str) -> Option<&str> {
    let rest = line.strip_prefix('@')?;
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    let (name, after) = rest.split_at(end);
    (!name.is_empty() && (after.starts_with(':') || after.starts_with('{'))).then_some(name)
}

/// dsl 0.28.0 (T1-25): what to assert instead when a needle line has the
/// shape of a report record the transcript prints around content — a quest
/// transition (`quest wire -> failed`), an objective's (`wire.sent failed
/// (by)`), a write (`set run.x = 1`), a selection or section line (`✓ h`,
/// `→ h`, `── end`). No content line has that shape, so a
/// `transcriptLacks` of it would hold vacuously. `None` for anything else.
fn record_shape(line: &str) -> Option<&'static str> {
    let ident = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            && !s.starts_with('.')
            && !s.ends_with('.')
    };
    let words: Vec<&str> = line.split_whitespace().collect();
    const QUEST_STATES: [&str; 4] = ["unset", "active", "complete", "failed"];
    match words.as_slice() {
        ["quest", id, "->", state, ..] if ident(id) && QUEST_STATES.contains(state) => {
            Some("assert the quest with `quests: { <id>: <state> }`")
        }
        [obj, "failed" | "done", ..] if ident(obj) && obj.contains('.') => Some(
            "assert the objective with `state: { quest.<id>.objectives.<o>.done: … }` (or \
             `.failed`)",
        ),
        ["set", path, "=", ..] if ident(path) && path.contains('.') => {
            Some("assert the value with `state: { <path>: … }`")
        }
        ["✓" | "✗" | "→" | "──", ..] => {
            Some("assert what was presented with a step's `expect: { winner / presented }`")
        }
        _ => None,
    }
}

/// Build a vocabulary from one checked document.
pub fn needle_vocab(
    input: &lute_check::CheckInput,
    meta: &lute_check::TypedMeta,
) -> lute_runtime::NeedleVocab {
    let nowhere = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    let (domains, _) = lute_check::schema_import::merge_domains(
        &input.snapshot,
        &input.imports,
        meta,
        nowhere,
    );
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &meta.cast);
    lute_runtime::NeedleVocab {
        members: lute_check::content_line::CONTENT_LINE_DOMAIN_SLOTS
            .iter()
            .map(|slot| {
                let members = domains
                    .get(*slot)
                    .filter(|domain| !domain.open)
                    .map(|domain| domain.members.iter().cloned().collect());
                (slot.to_string(), members)
            })
            .collect(),
        stamps: input.snapshot.stamp_attrs.keys().cloned().collect(),
        speakers: (!cast.is_empty()).then(|| {
            cast.into_keys()
                .chain(std::iter::once("narrator".to_string()))
                .collect()
        }),
        seen: true,
    }
}

/// Why `needle` can never match a presented line, as a usage error with a
/// did-you-mean — `None` when every head names a speaker of the project
/// (its cast or `narrator`; any id while speakers are shape-only), every
/// key is one a line head shows and every value is one it can carry (0.27
/// prerelease OT-F-2, OT N-2). A needle without a head is never refused.
pub fn needle_problem(needle: &str, vocab: &lute_runtime::NeedleVocab) -> Option<String> {
    let near = |s: &str, known: &[&str]| {
        lute_manifest::suggest::nearest(s, known.iter().copied(), 2)
            .map(|k| format!(" — did you mean `{k}`?"))
            .unwrap_or_default()
    };
    let keys = vocab.keys();
    for line in needle.split('\n') {
        if let Some(instead) = record_shape(line.trim()) {
            return Some(format!(
                "needle {needle:?} is the shape of a line the engine's report prints, not of a \
                 content line — needles judge only what is said, so this one can never match; \
                 {instead}"
            ));
        }
        if let (Some(who), Some(speakers)) = (head_speaker(line), &vocab.speakers) {
            if !speakers.contains(who) {
                let known: Vec<&str> = speakers.iter().map(String::as_str).collect();
                return Some(format!(
                    "needle {needle:?}: `@{who}` is not a speaker of this project{} (speakers: \
                     {})",
                    near(who, &known),
                    known.join(", ")
                ));
            }
        }
        let (_, Some(attrs)) = split(line) else {
            continue;
        };
        for (key, value) in attrs {
            let why = if !keys.contains(&key.as_str()) {
                let never = if HEAD_SKIP_AUTHORED.contains(&key.as_str()) {
                    format!(" (`{key}` is never shown on a transcript line)")
                } else if key == "when" {
                    " (a `when=` line that plays is shown without it; one that is skipped is \
                     not in the transcript)"
                        .to_string()
                } else {
                    String::new()
                };
                format!(
                    "`{key}` is not an attribute a transcript line shows{}{never} (a line shows: \
                     {})",
                    near(&key, &keys),
                    keys.join(", ")
                )
            } else if HEAD_FLAGS.contains(&key.as_str()) {
                match value {
                    None => continue,
                    Some(_) => format!("`{key}` is a bare flag — write `{key}`, not `{key}=…`"),
                }
            } else {
                let Some(value) = value else {
                    return Some(format!(
                        "needle {needle:?}: `{key}` takes a value — write `{key}=\"…\"`"
                    ));
                };
                match (key.as_str(), vocab.members.get(&key)) {
                    ("variant", _) if value.parse::<i64>().is_err() => {
                        format!("`variant={value}` is not a number")
                    }
                    (_, Some(Some(members))) if !members.contains(&value) => {
                        let known: Vec<&str> = members.iter().map(String::as_str).collect();
                        format!(
                            "`{value}` is not a member of `{key}`{} (members: {})",
                            near(&value, &known),
                            known.join(", ")
                        )
                    }
                    _ => continue,
                }
            };
            return Some(format!("needle {needle:?}: {why}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{find, judge, needle_problem, nearest, said_line};
    use std::collections::{BTreeMap, BTreeSet};
    use lute_runtime::NeedleVocab;
    use serde_json::json;

    const SAID: &str =
        "@sol{emotion=\"happy\"}: Vega, Deneb, Altair.\n@wren{mono}: Quiet.\n@narrator: Night.\n";

    #[test]
    fn the_head_is_the_role_flag_then_the_attributes() {
        let cmd = json!({"kind": "line", "position": "a", "role": "mono", "speaker": "wren",
                         "text": "x", "emotion": "sad", "variant": 0, "lineId": "l"});
        let rec = json!({"kind": "line", "speaker": "wren", "text": "Quiet."});
        assert_eq!(
            said_line(&rec, Some(&cmd)),
            "@wren{mono emotion=\"sad\" variant=0}: Quiet."
        );
        assert_eq!(said_line(&rec, None), "@wren: Quiet.");
    }

    #[test]
    fn a_needle_without_attributes_matches_any_attributes() {
        assert!(find(SAID, "@sol: Vega").is_some());
        assert!(find(SAID, "Deneb, Altair.\n@wren: Qui").is_some());
    }

    #[test]
    fn a_needle_with_attributes_matches_only_those() {
        assert!(find(SAID, "@sol{emotion=\"happy\"}: Vega").is_some());
        assert!(find(SAID, "@sol{emotion='happy'}: Vega").is_some());
        assert!(find(SAID, "@sol{emotion=\"sad\"}: Vega").is_none());
        assert!(find(SAID, "@wren{mono}: Quiet.").is_some());
        assert!(find(SAID, "@narrator{mono}: Night.").is_none());
        // A block on a later needle line binds that line.
        assert!(find(SAID, "Altair.\n@wren{mono}: Quiet.").is_some());
        assert!(find(SAID, "Altair.\n@wren{vo}: Quiet.").is_none());
    }

    #[test]
    fn misses_quote_real_lines_never_the_needle() {
        assert_eq!(
            judge(
                SAID,
                &[],
                "@sol{emotion=\"sad\"}: Vega, Deneb, Altair.",
                true
            )
            .as_deref(),
            Some("absent (nearest line: \"@sol{emotion=\\\"happy\\\"}: Vega, Deneb, Altair.\")")
        );
        assert_eq!(
            judge(
                SAID,
                &[],
                "@sol{emotion=\"sad\"}: Vega, Deneb, Altair.",
                false
            ),
            None
        );
        assert_eq!(
            judge(SAID, &[], "@sol: Vega", false).as_deref(),
            Some("present (line: \"@sol{emotion=\\\"happy\\\"}: Vega, Deneb, Altair.\")")
        );
        // Same speaker first.
        assert_eq!(
            nearest(SAID, &[], "@wren: Loud."),
            Some("@wren{mono}: Quiet.")
        );
    }

    /// Round-5 T3-16 (SG-F15): a short line of the speaker is not "near" a
    /// long needle just because it has little text to differ in.
    #[test]
    fn a_short_line_is_not_nearest_to_a_long_needle() {
        let said = "@pim: Wish well!\n\
                    @pim: Comets have all fallen, friend. Missions are closed.\n\
                    @narrator: The moon's set on the missions.\n";
        assert_eq!(
            nearest(
                said,
                &[],
                "@pim: The moon's set on those missions, friend. There's always the next festival."
            ),
            Some("@pim: Comets have all fallen, friend. Missions are closed.")
        );
    }

    /// Round-5 T3-16: the speaker outranks the step, the step where the
    /// needle's other lines were said outranks the text.
    #[test]
    fn nearest_prefers_the_speaker_then_the_step_then_the_text() {
        let said = "@sol: Morning.\n@wren: The stars are out.\n\
                    @sol: Evening.\n@narrator: The stars are cut.\n@wren: The stars are gone.\n";
        let steps = [0, 2];
        let needle = "@sol: Evening.\n@wren: The stars are cut.";
        assert_eq!(
            nearest(said, &steps, needle),
            Some("@wren: The stars are gone.")
        );
        // As one step, the text decides among the speaker's lines.
        assert_eq!(
            nearest(said, &[], needle),
            Some("@wren: The stars are out.")
        );
    }

    /// OT-F-2: a needle attribute no line head can show, or a value its
    /// domain lacks, is refused with a did-you-mean — it could only ever make
    /// `transcriptLacks` hold vacuously.
    #[test]
    fn a_needle_naming_what_no_line_can_show_is_refused() {
        let vocab = NeedleVocab {
            members: BTreeMap::from([
                (
                    "emotion".to_string(),
                    Some(BTreeSet::from(["sad".to_string(), "happy".to_string()])),
                ),
                ("action".to_string(), None),
            ]),
            stamps: BTreeSet::from(["take".to_string()]),
            speakers: None,
            seen: true,
        };
        let problem = |n: &str| needle_problem(n, &vocab);
        for ok in [
            "@soren{emotion=\"sad\"}: We ran out.",
            "@soren{mono}: We ran out.",
            "@soren{variant=1 as=\"The Smith\" take=\"b\" action=\"wave\"}: x",
            "@soren: We ran out.",
            "no speaker at all",
        ] {
            assert_eq!(problem(ok), None, "{ok}");
        }
        let e = problem("@soren{emotoin=\"sad\"}: We ran out.").unwrap();
        assert!(
            e.contains("`emotoin`") && e.contains("did you mean `emotion`?"),
            "{e}"
        );
        let e = problem("@soren{emotion=\"sadd\"}: We ran out.").unwrap();
        assert!(e.contains("did you mean `sad`?"), "{e}");
        let e = problem("@soren{emotion=\"sad\" when=\"true\"}: x").unwrap();
        assert!(e.contains("`when`"), "{e}");
        for bad in [
            "@soren{mono=true}: x",
            "@soren{emotion}: x",
            "@soren{variant=two}: x",
            "@soren{code=\"a1\"}: x",
            "@a: fine\n@soren{sadd}: x",
        ] {
            assert!(problem(bad).is_some(), "{bad}");
        }
    }
}
