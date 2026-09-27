//! Pointers for body lines written in another interactive-fiction language.
//!
//! Authors porting from Ink or Yarn Spinner write that language's shapes out
//! of habit — `-> knot`, `~ x = 1`, `* [choice]`, `<<set $x to 1>>`, prose
//! with no speaker. Each is still the residual `E-UNCLASSIFIED` (round-6
//! T3-60), but the message names the shape and what Lute writes instead, so
//! the author is not left guessing from "unrecognized line".

/// Longest line echoed back verbatim; longer lines are cut with `…`.
const ECHO_MAX: usize = 40;

/// How a Lute choice is written — shared by the Ink choice and the Yarn
/// option pointers.
const LUTE_CHOICES: &str = "Lute choices are `<choice id=\"…\" label=\"…\">` blocks inside a \
     `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice)";

/// What Lute writes instead of `line` (trimmed), when `line` has the shape of
/// an Ink or Yarn construct; `None` otherwise. Speakerless prose is
/// [`speakerless`] — the caller asks it only after ruling out a line wrapped
/// from the one above ([`continues`]).
pub(super) fn foreign_line(line: &str) -> Option<String> {
    if line == "->->" {
        return Some(
            "`->->` returns from an Ink tunnel; a Lute component returns on its own when its \
             lines end, so the scene goes on after its `::use{component=\"…\"}`"
                .to_string(),
        );
    }
    if let Some(rest) = line.strip_prefix("->") {
        return Some(divert(rest));
    }
    if line == "<>" {
        return Some(
            "`<>` is Ink glue; Lute joins nothing: write the whole sentence on one content line"
                .to_string(),
        );
    }
    if line.starts_with("<-") {
        return Some(format!(
            "`{}` is an Ink thread; Lute has no threads: write the choices it would gather \
             into this scene's own `<branch>` or `<hub>`",
            echo(line)
        ));
    }
    if let Some(rest) = line.strip_prefix("=>") {
        return Some(format!(
            "`{}` is a Yarn line group; Lute makes no random choice: guard each line \
             (`@narrator{{when=\"…\"}}: {}`) or choose between lines with a `<match>` on a path \
             the engine sets",
            echo(line),
            echo(rest.trim())
        ));
    }
    if let Some(rest) = line.strip_prefix("TODO:") {
        return Some(format!(
            "Ink's `TODO:` line is a note for the writer; a Lute comment is `// TODO: {}` on a \
             line of its own",
            echo(rest.trim())
        ));
    }
    if let Some(rest) = line
        .strip_prefix("INCLUDE")
        .filter(|r| r.starts_with([' ', '\t']))
    {
        return Some(format!(
            "`INCLUDE {}` pulls in another Ink file; a Lute project is every `.lute` file under \
             `lute.project.yaml`, with shared state in schemas imported with `uses:`",
            echo(rest.trim())
        ));
    }
    if line
        .strip_prefix("EXTERNAL")
        .is_some_and(|r| r.starts_with([' ', '\t']))
    {
        return Some(
            "`EXTERNAL` declares an Ink game function; Lute asks the engine to do something with \
             a directive a plugin declares, written `::name{key=\"value\"}` on its own line, and \
             names a computed value with a def under `defs:`"
                .to_string(),
        );
    }
    if line.starts_with("<<") {
        return Some(yarn_command(line));
    }
    if let Some(rest) = line.strip_prefix("[[") {
        let inner = rest.split("]]").next().unwrap_or(rest);
        let target = inner.rsplit('|').next().unwrap_or(inner).trim();
        return Some(format!(
            "`{}` is a Yarn link; {}",
            echo(line),
            no_diverts(target)
        ));
    }
    if let Some(rest) = line.strip_prefix('~') {
        return Some(ink_logic(line, rest.trim()));
    }
    for kw in ["VAR", "CONST", "LIST"] {
        if let Some(rest) = line.strip_prefix(kw).filter(|r| r.starts_with([' ', '\t'])) {
            return Some(ink_declaration(kw, rest.trim()));
        }
    }
    let mut chars = line.chars();
    let (first, second) = (chars.next(), chars.next());
    if matches!(first, Some('*' | '+')) && matches!(second, Some(' ' | '\t' | '[')) {
        return Some(ink_choice(line));
    }
    if first == Some('-') && matches!(second, None | Some(' ' | '\t')) {
        return Some(format!(
            "`{}` is an Ink gather; Lute has no gathers: after a `<branch>` or `<hub>` closes, \
             the lines below it run whichever choice was taken, so write the gathered text \
             there as an ordinary line (`@narrator: …`)",
            echo(line)
        ));
    }
    if line.starts_with("==") {
        let name = line.trim_matches(|c: char| c == '=' || c.is_whitespace());
        if let Some(sig) = name.strip_prefix("function ").map(str::trim) {
            return Some(ink_function(line, sig));
        }
        return Some(if name.is_empty() {
            format!(
                "`{}` ends a Yarn node; a Lute scene is its own `.lute` file and needs no end \
                 marker",
                echo(line)
            )
        } else {
            format!(
                "`{}` is an Ink knot; a Lute scene is its own `.lute` file (`kind: scene` and \
                 `id: {name}` in its frontmatter), and a section inside a scene is a `## {name}` \
                 heading",
                echo(line)
            )
        });
    }
    if let Some(name) = line
        .strip_prefix("= ")
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        return Some(format!(
            "`{}` is an Ink stitch; a section inside a scene is a `## {name}` heading",
            echo(line)
        ));
    }
    if line.starts_with('{') && !line.starts_with("{{") {
        return Some(format!(
            "`{}` is Ink inline logic; Lute has no inline conditional text: guard a whole line \
             with `@narrator{{when=\"…\"}}: …` or choose between lines with `<match on=\"…\">`",
            echo(line)
        ));
    }
    if let Some(key) = ["title", "tags", "position", "colorID"]
        .into_iter()
        .find(|k| {
            line.strip_prefix(k)
                .and_then(|r| r.strip_prefix(':'))
                .is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t']))
        })
    {
        return Some(format!(
            "`{key}:` is a Yarn node header; a Lute scene is its own `.lute` file whose metadata \
             is the YAML frontmatter between `---` lines at the top (`kind: scene`, `id: …`)"
        ));
    }
    None
}

/// The pointer for a line of text with no `@speaker:` head — prose, or a
/// script-style `Name: text` line — or `None` when `line` does not read as
/// text (a stray token, punctuation).
pub(super) fn speakerless(line: &str) -> Option<String> {
    if let Some((name, text)) = line.split_once(':') {
        let is_name = name.chars().next().is_some_and(char::is_alphabetic)
            && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        if is_name && text.starts_with([' ', '\t']) && !text.trim().is_empty() {
            return Some(format!(
                "a content line starts with its speaker's `@`: `@{name}: …` (narration is \
                 `@narrator: …`)"
            ));
        }
    }
    let starts_as_text = line
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || "\"'“‘(…".contains(c))
        || line.starts_with("{{");
    let reads_as_text = line.contains(char::is_whitespace) || line.ends_with(['.', '!', '?', '…']);
    (starts_as_text && reads_as_text).then(|| {
        "a content line needs a speaker: narration is `@narrator: …`, dialogue \
         `@<speaker>: …`"
            .to_string()
    })
}

/// `cur` reads as the wrapped tail of `prev` (both trimmed): `prev` is a
/// `<tag` opener with no closing `>`, or a content line and `cur` picks the
/// sentence up mid-way (a lowercase start, or `prev` ending on `,`/`;`/a
/// dash). A capitalized sentence after a finished line is not a wrap — it is
/// a line with no speaker ([`speakerless`]).
pub(super) fn continues(prev: &str, cur: &str) -> bool {
    if prev.starts_with('<') && !prev.starts_with("</") {
        return !prev.ends_with('>');
    }
    if !super::is_line_head(prev) {
        return false;
    }
    cur.chars().next().is_some_and(char::is_lowercase)
        || prev.ends_with([',', ';', '-', '—', '–', '('])
}

/// `line` for a message: verbatim up to [`ECHO_MAX`] characters.
fn echo(line: &str) -> String {
    if line.chars().count() <= ECHO_MAX {
        line.to_string()
    } else {
        let cut: String = line.chars().take(ECHO_MAX - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

/// `true` for an Ink/Yarn node name: `ledger`, `lamp_room`, `knot.stitch`.
fn is_node_name(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

/// What replaces a jump to `target` (an Ink divert, a Yarn `<<jump>>` or
/// link): Ink's `END` is the schema's `terminal:`, `DONE` is `::end`,
/// otherwise the three ways Lute moves on.
fn no_diverts(target: &str) -> String {
    match target {
        "END" => {
            return "it ends the whole story, which in Lute is the schema's `terminal:` \
                    condition: a scene makes it hold with an ordinary `::set{…}` (`::end` is \
                    Ink's `-> DONE`: it ends only this scene)"
                .to_string();
        }
        "DONE" => return "a scene ends with `::end`, or simply at its last line".to_string(),
        _ => {}
    }
    let name = if is_node_name(target) { target } else { "…" };
    format!(
        "Lute has no diverts: `::next{{to=\"{name}\"}}` jumps forward to a \
         `::mark{{id=\"{name}\"}}` later in this document, a `<hub>` repeats its choices until \
         an `exit` choice, and another scene is reached through the occasion it answers (`on:` \
         in its frontmatter)"
    )
}

/// `-> rest`: an Ink divert (`-> knot`, `-> END`), a tunnel `-> knot ->`
/// or, when `rest` is text rather than a name, a Yarn option.
fn divert(rest: &str) -> String {
    let rest = rest.trim();
    let tunnel = rest.len() > 2 && rest.ends_with("->");
    let target = rest
        .trim_start_matches("->")
        .trim()
        .trim_end_matches("->")
        .trim();
    if tunnel && is_node_name(target) {
        return format!(
            "`-> {target} ->` is an Ink tunnel; Lute's nearest is a component: \
             `::use{{component=\"{target}\"}}` plays the component's lines here, and the scene \
             goes on after them"
        );
    }
    if target.is_empty() || is_node_name(target) {
        let shown = if target.is_empty() {
            "->".to_string()
        } else {
            format!("-> {target}")
        };
        return format!("`{shown}` is an Ink divert; {}", no_diverts(target));
    }
    format!("`-> {}` is a Yarn option; {LUTE_CHOICES}", echo(rest))
}

/// `=== function name(params) ===`: an Ink function, whose Lute form is a
/// def with params.
fn ink_function(line: &str, sig: &str) -> String {
    let (name, params) = sig
        .split_once('(')
        .map_or((sig, ""), |(n, p)| (n.trim(), p.trim_end_matches(')')));
    let name = if is_node_name(name) { name } else { "f" };
    let params: Vec<&str> = params
        .split(',')
        .map(str::trim)
        .filter(|p| is_node_name(p))
        .collect();
    let (decl, call) = if params.is_empty() {
        (String::new(), String::new())
    } else {
        (
            format!(
                "params: {{ {} }}, ",
                params
                    .iter()
                    .map(|p| format!("{p}: …"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "(…)".to_string(),
        )
    };
    format!(
        "`{}` is an Ink function; Lute computes a value with a def under `defs:` in the \
         frontmatter (`{name}: {{ type: …, {decl}cel: \"…\" }}`), read as `@{name}{call}`",
        echo(line)
    )
}

/// `* [label]` / `+ [label]`: an Ink choice (`*` once-only, `+` sticky).
fn ink_choice(line: &str) -> String {
    let flag = if line.starts_with('*') {
        "Ink's once-only `*` in a loop is a `<hub>` choice with the `once` flag"
    } else {
        "Ink's sticky `+` is a plain `<hub>` choice"
    };
    format!("`{}` is an Ink choice; {LUTE_CHOICES}; {flag}", echo(line))
}

/// `~ rest`: an Ink logic line. An assignment maps onto `::set{…}` as written.
fn ink_logic(line: &str, rest: &str) -> String {
    let body = rest.strip_prefix("temp ").map_or(rest, str::trim);
    let lhs_len = body
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(body.len());
    let (lhs, op) = (&body[..lhs_len], body[lhs_len..].trim_start());
    let assigns = !lhs.is_empty()
        && (op.starts_with("+=")
            || op.starts_with("-=")
            || (op.starts_with('=') && !op.starts_with("==")));
    if assigns {
        let tier = if lhs.contains('.') {
            String::new()
        } else {
            format!(" (a state path starts with its tier, e.g. `run.{lhs}`)")
        };
        return format!(
            "`{}` is Ink logic; Lute writes state with `::set{{{body}}}`{tier}, and the path is \
             declared under `state:` in the frontmatter",
            echo(line)
        );
    }
    format!(
        "`{}` is Ink logic; Lute writes state with `::set{{run.x = …}}` and names computed values \
         under `defs:` in the frontmatter",
        echo(line)
    )
}

/// `VAR`/`CONST`/`LIST name = value`: an Ink global declaration.
fn ink_declaration(kw: &str, rest: &str) -> String {
    let (name, value) = rest
        .split_once('=')
        .map_or((rest, ""), |(n, v)| (n.trim(), v.trim()));
    let name = if is_node_name(name) { name } else { "x" };
    match kw {
        "CONST" => format!(
            "`CONST` declares an Ink constant; Lute names a fixed value under `defs:` in the \
             frontmatter (`{name}: \"…\"`) and reads it as `@{name}`"
        ),
        "LIST" => format!(
            "`LIST` declares an Ink list; Lute uses a state path typed by an enum, declared under \
             `state:` in the frontmatter (`run.{name}: {{ type: {{ enum: [{}] }} }}`)",
            if value.is_empty() { "…" } else { value }
        ),
        _ => state_decl("`VAR` declares an Ink global", name, value),
    }
}

/// The `state:` entry an Ink `VAR` / Yarn `<<declare>>` of `name = value`
/// becomes, typed from the literal when it is one.
fn state_decl(what: &str, name: &str, value: &str) -> String {
    let ty = if value.parse::<f64>().is_ok() {
        "number"
    } else if matches!(value, "true" | "false") {
        "bool"
    } else if value.starts_with('"') {
        "string"
    } else {
        "…"
    };
    let default = if value.is_empty() || ty == "…" {
        "…"
    } else {
        value
    };
    format!(
        "{what}; Lute declares state under `state:` in the frontmatter (`run.{name}: {{ type: \
         {ty}, default: {default} }}`) and writes it with `::set{{…}}`"
    )
}

/// A Yarn `<<command …>>` line.
fn yarn_command(line: &str) -> String {
    let inner = line.trim_start_matches("<<");
    let inner = inner.split(">>").next().unwrap_or(inner).trim();
    let (cmd, args) = inner
        .split_once(char::is_whitespace)
        .map_or((inner, ""), |(c, a)| (c, a.trim()));
    let shown = echo(line);
    match cmd {
        "set" | "declare" => {
            let (var, value) = args
                .split_once(" to ")
                .or_else(|| args.split_once('='))
                .map_or((args, ""), |(v, x)| (v.trim(), x.trim()));
            let name = var.trim_start_matches('$');
            let name = if is_node_name(name) { name } else { "x" };
            if cmd == "declare" {
                return state_decl(&format!("`{shown}` is a Yarn declaration"), name, value);
            }
            let value = if value.is_empty() {
                "…".to_string()
            } else {
                yarn_vars(value)
            };
            format!(
                "`{shown}` is a Yarn command; Lute writes state with `::set{{run.{name} = \
                 {value}}}`, and the path is declared under `state:` in the frontmatter"
            )
        }
        "if" | "elseif" | "else" | "endif" => {
            let cond = if args.is_empty() {
                "…".to_string()
            } else {
                yarn_vars(args)
            };
            format!(
                "`{shown}` is a Yarn conditional; Lute chooses between lines with `<match \
                 on=\"…\">` and its `<when is=\"…\">`/`<when test=\"…\">` arms, or guards one \
                 line: `@narrator{{when=\"{cond}\"}}: …`"
            )
        }
        "jump" => format!("`{shown}` is a Yarn jump; {}", no_diverts(args)),
        "stop" => format!("`{shown}` is a Yarn command; a scene ends with `::end`"),
        "once" | "endonce" => format!(
            "`{shown}` is a Yarn once block; Lute counts in a number path it `::set`s (`scene.*` \
             for this presentation, `run.*` for the run) and chooses the lines with a `<match>` \
             on it; a menu option offered once is a hub `<choice … once>`"
        ),
        _ => format!(
            "`{shown}` is a Yarn command; a Lute directive is written `::name{{key=\"value\"}}` \
             on its own line"
        ),
    }
}

/// Yarn's `$name` variables rewritten as `run.name` state paths, for an
/// example Lute condition or value.
fn yarn_vars(expr: &str) -> String {
    let mut out = String::with_capacity(expr.len() + 8);
    let mut rest = expr;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        if after.starts_with(|c: char| c.is_alphabetic() || c == '_') {
            out.push_str("run.");
        } else {
            out.push('$');
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hint(line: &str) -> String {
        foreign_line(line).unwrap_or_else(|| panic!("no pointer for {line:?}"))
    }

    #[test]
    fn ink_diverts_point_at_next_and_end() {
        assert!(hint("-> ledger").contains("`::next{to=\"ledger\"}`"));
        // `-> END` ends the story (the schema's `terminal:`); `-> DONE` ends
        // the scene (`::end`). Pointing END at `::end` taught the mistake of
        // a game that goes on to its next chapter.
        let end = hint("-> END");
        assert!(end.contains("`terminal:`"), "{end}");
        assert!(!end.contains("a scene ends with `::end`"), "{end}");
        assert!(hint("-> DONE").contains("a scene ends with `::end`"));
        // A Yarn option (text, not a name) is a choice, not a divert.
        assert!(hint("-> Wait for dark").contains("`<choice"));
    }

    // Each Ink/Yarn construct names the Lute form the guide maps it to, not
    // a neighbouring one (a tunnel is not a divert, `INCLUDE` is not prose).
    #[test]
    fn other_ink_and_yarn_shapes_name_their_lute_form() {
        for (line, want) in [
            ("-> lamp_room ->", "`::use{component=\"lamp_room\"}`"),
            ("->->", "component returns on its own"),
            ("<- whispers", "Ink thread"),
            ("<>", "Ink glue"),
            (
                "=== function lower(x) ===",
                "`lower: { type: …, params: { x: … }, cel: \"…\" }`",
            ),
            ("INCLUDE ledger.ink", "`lute.project.yaml`"),
            ("EXTERNAL playSound(name)", "a plugin declares"),
            ("TODO: fix this", "`// TODO: fix this`"),
            ("=> Fog rolls in.", "`@narrator{when=\"…\"}: Fog rolls in.`"),
            ("<<once>>", "`<match>`"),
        ] {
            let h = hint(line);
            assert!(h.contains(want), "{line}: {h}");
        }
    }

    #[test]
    fn ink_logic_and_globals_point_at_set_and_state() {
        assert!(hint("~ run.oil = run.oil + 2").contains("`::set{run.oil = run.oil + 2}`"));
        assert!(hint("~ oil += 1").contains("`run.oil`"));
        assert!(hint("VAR x = 1").contains("`run.x: { type: number, default: 1 }`"));
        assert!(hint("VAR met = false").contains("type: bool"));
        assert!(hint("CONST MAX = 3").contains("`defs:`"));
    }

    #[test]
    fn ink_choices_gathers_and_knots() {
        let once = hint("* [Read the ledger]");
        assert!(
            once.contains("`<hub>`") && once.contains("`once`"),
            "{once}"
        );
        assert!(hint("+ [Wait for dark]").contains("sticky"));
        assert!(hint("* * [Nested]").contains("Ink choice"));
        assert!(hint("- gather").contains("no gathers"));
        assert!(hint("=== ledger ===").contains("`## ledger`"));
        assert!(hint("= stitch").contains("`## stitch`"));
    }

    #[test]
    fn yarn_commands_are_rewritten_with_state_paths() {
        assert!(hint("<<set $oil to 3>>").contains("`::set{run.oil = 3}`"));
        assert!(hint("<<if $oil > 2>>").contains("`@narrator{when=\"run.oil > 2\"}: …`"));
        assert!(hint("<<jump Lamp_Room>>").contains("`::next{to=\"Lamp_Room\"}`"));
        assert!(hint("<<declare $oil = 1>>").contains("type: number"));
        assert!(hint("title: Lamp_Room").contains("Yarn node header"));
    }

    // Shapes a Lute author writes on purpose, or plain text, get no foreign
    // pointer: `--`/`**` are not Ink sigils, `{{…}}` is interpolation.
    #[test]
    fn ordinary_shapes_are_not_foreign() {
        for line in [
            "**bold**",
            "---",
            "+1 point",
            "{{run.oil}} cans",
            "Plain prose.",
        ] {
            assert_eq!(foreign_line(line), None, "{line}");
        }
    }

    #[test]
    fn prose_and_wraps_are_told_apart() {
        assert!(speakerless("Plain prose line with no speaker.").is_some());
        assert!(speakerless("Keeper: Hello there.").is_some_and(|h| h.contains("`@Keeper: …`")));
        assert_eq!(speakerless("garbage"), None);
        // A lowercase pick-up, or a line cut on a comma, is a wrap…
        assert!(continues("@mira: hello", "garbage next"));
        assert!(continues("@mira: When the tide turns,", "We leave."));
        // …a new capitalized sentence after a finished line is not.
        assert!(!continues(
            "@narrator: hi",
            "Plain prose line with no speaker."
        ));
        assert!(!continues("::bg{location=\"x\"}", "more text"));
    }
}
