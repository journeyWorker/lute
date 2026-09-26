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

use serde_json::Value as Json;

/// The command fields a line head never shows: identity, text and the
/// role (rendered as its flag).
const HEAD_SKIP: [&str; 9] = [
    "addr",
    "kind",
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
        Some("monologue") => Some("mono"),
        Some("voiceover") => Some("vo"),
        Some("offscreen") => Some("os"),
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
/// `transcriptContains` miss quotes. A line whose bare form contains the
/// needle's (its attributes differ) wins; then lines of the needle's
/// speaker; then any line. `None` when nothing was said.
pub fn nearest<'a>(said: &'a str, needle: &str) -> Option<&'a str> {
    let (bare_needle, _) = split(needle.trim().lines().next().unwrap_or(""));
    let speaker_of = |bare: &str| -> Option<String> {
        let r = bare.strip_prefix('@')?;
        r.split_once(':').map(|(s, _)| s.to_string())
    };
    let want_speaker = speaker_of(&bare_needle);
    let candidates: Vec<(&str, String)> = said
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| (l, split(l).0))
        .collect();
    if let Some((l, _)) = candidates.iter().find(|(_, b)| b.contains(&bare_needle)) {
        return Some(l);
    }
    let dist = |bare: &str| {
        // A needle is a substring, so the length difference is free: a
        // short needle is not penalized for the rest of a long line.
        let d = lute_manifest::suggest::levenshtein(bare, &bare_needle);
        let shorter = bare.chars().count().abs_diff(bare_needle.chars().count());
        d.saturating_sub(shorter)
    };
    let same: Vec<&(&str, String)> = candidates
        .iter()
        .filter(|(_, b)| want_speaker.is_some() && speaker_of(b) == want_speaker)
        .collect();
    let pool: Vec<&(&str, String)> = if same.is_empty() {
        candidates.iter().collect()
    } else {
        same
    };
    pool.into_iter()
        .min_by_key(|(_, b)| dist(b))
        .map(|(l, _)| *l)
}

/// Judge one `transcriptContains` (`want_present`) / `transcriptLacks`
/// needle against `said`: `None` when it holds, else what the transcript
/// shows — `absent (nearest line: "…")` quoting a real line, or `present
/// (line: "…")` quoting the line(s) that matched, never the needle.
pub fn judge(said: &str, needle: &str, want_present: bool) -> Option<String> {
    let hit = find(said, needle);
    match (hit, want_present) {
        (Some(_), true) | (None, false) => None,
        (None, true) => Some(match nearest(said, needle) {
            Some(line) => format!("absent (nearest line: {line:?})"),
            None => "absent".to_string(),
        }),
        (Some(lines), false) => Some(format!("present (line: {:?})", lines.join("\n"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAID: &str =
        "@sol{emotion=\"happy\"}: Vega, Deneb, Altair.\n@wren{mono}: Quiet.\n@narrator: Night.\n";

    #[test]
    fn the_head_is_the_role_flag_then_the_attributes() {
        let cmd = json!({"kind": "line", "addr": "a", "role": "monologue", "speaker": "wren",
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
            judge(SAID, "@sol{emotion=\"sad\"}: Vega, Deneb, Altair.", true).as_deref(),
            Some("absent (nearest line: \"@sol{emotion=\\\"happy\\\"}: Vega, Deneb, Altair.\")")
        );
        assert_eq!(
            judge(SAID, "@sol{emotion=\"sad\"}: Vega, Deneb, Altair.", false),
            None
        );
        assert_eq!(
            judge(SAID, "@sol: Vega", false).as_deref(),
            Some("present (line: \"@sol{emotion=\\\"happy\\\"}: Vega, Deneb, Altair.\")")
        );
        // Same speaker first.
        assert_eq!(nearest(SAID, "@wren: Loud."), Some("@wren{mono}: Quiet."));
    }
}
