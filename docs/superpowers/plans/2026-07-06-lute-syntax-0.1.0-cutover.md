# lute-syntax 0.1.0 Cutover Implementation Plan (Plan A of 6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut the parser over to the DSL 0.1.0 surface — `:speaker` content lines, `//` line comments, truly-opaque `Text`, `{{…}}` interpolation scanning, `<hub>` blocks, enforced shot headings, `<otherwise>` attr rejection, string-escape validation — with the whole workspace staying green.

**Architecture:** All changes live in `crates/lute-syntax` (hand-written line classifier, NOT tree-sitter — tree-sitter is Plan D). Downstream crates get minimal `Node::Hub` match arms that emit a transitional `E-HUB-UNSUPPORTED` check error (sound under the D6 clean-check gate; replaced by real hub checking in Plan B). All in-repo `.lute` fixtures/examples migrate in the final tasks so `cargo test --workspace` passes at plan end.

**Tech Stack:** Rust (workspace `cargo test`), spec = `docs/proposals/scenario-dsl/0.1.0.md` (cited as "dsl §N").

## Global Constraints

- Spec source of truth: `docs/proposals/scenario-dsl/0.1.0.md`; design contract `docs/superpowers/specs/2026-07-06-lute-dsl-0.1.0-design.md`.
- Diagnostic code spellings verbatim from dsl 0.1.0 Appendix D: `E-SHOT-HEADING`, `E-STRING-ESCAPE`, `E-INTERP-UNTERMINATED`. Transitional (this plan only, removed in Plan B): `E-HUB-UNSUPPORTED`.
- Clean cutover: `:line[` MUST NOT parse; it gets a fix-it diagnostic (dsl §7.1).
- SPAN-FIDELITY contract (parser.rs header): comment stripping stays length/newline-preserving; every `Span` is an original-source offset.
- No tree-sitter, LSP-feature, checker-semantics (beyond the transitional arm), or compiler work — Plans B–D.
- Run only the tests you add/modify per task; full `cargo test --workspace` gates only Tasks 9–10.

---

### Task 1: AST — `Hub`, `Interp`, new `Line` fields

**Files:**
- Modify: `crates/lute-syntax/src/ast.rs`

**Interfaces:**
- Produces (later tasks + Plans B/C consume):
  - `enum Node { …, Hub(Hub) }`
  - `pub struct Hub { pub attrs: Vec<Attr>, pub choices: Vec<Choice>, pub span: Span }`
  - `pub struct Interp { pub kind: InterpKind, pub raw: String, pub span: Span }`
  - `pub enum InterpKind { Path, Ref, Reserved }`
  - `Line` gains `pub interps: Vec<Interp>`

- [ ] **Step 1: Add types** — in `ast.rs`, after `Match` (line ~83) add:

```rust
/// `<hub id> HubChoice+ </hub>` (dsl §7.3.2). Choices reuse [`Choice`];
/// the `once` / `exit` flags arrive as bare attrs on each choice.
#[derive(Clone, Debug)]
pub struct Hub {
    pub attrs: Vec<Attr>,
    pub choices: Vec<Choice>,
    pub span: Span,
}

/// One `{{…}}` interpolation inside content `Text` (dsl §7.6).
#[derive(Clone, Debug)]
pub struct Interp {
    pub kind: InterpKind,
    /// Interior text, trimmed (e.g. `run.coins`, `@fond`, `userName`).
    pub raw: String,
    /// Span of the whole `{{…}}` in the original source.
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpKind {
    /// `scene.…` / `run.…` / `user.…` / `app.…` state path.
    Path,
    /// `@def` / `@fn(args)`.
    Ref,
    /// Reserved token (`userName`).
    Reserved,
}
```

Add `Hub(Hub)` to `enum Node`, and `pub interps: Vec<Interp>` to `struct Line`.

- [ ] **Step 2: Compile the crate only**

Run: `cargo check -p lute-syntax`
Expected: errors ONLY at `Line { … }` construction (parser.rs:424) and `Node` matches in this crate — fix by adding `interps: Vec::new()` at the construction site and a `Node::Hub(h) => h.span.byte_end` arm to `node_end` (parser.rs:503). Downstream crates stay untouched until Task 8.

- [ ] **Step 3: Commit**

```bash
git add crates/lute-syntax/src/ast.rs crates/lute-syntax/src/parser.rs
git commit -m "feat(syntax): AST types for hub + interpolation (dsl 0.1.0)"
```

---

### Task 2: Content-line cutover — `:speaker{…}: text`

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (`next_node` ~261-297, `parse_line` ~387-431, crate doc header ~11-14)
- Test: `crates/lute-syntax/src/parser.rs` `mod tests`

**Interfaces:**
- Produces: `parse_line` handles `":" Ident Attrs? ":" WS Text` (dsl §7.1). `Line.speaker`/`attrs`/`text`/`text_span` semantics unchanged.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn content_line_short_form() {
    let (doc, diags) = parse("## Shot 1.\n:bianca{code=\"0010\"}: Hello!\n:narrator: Quiet.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let body = &doc.shots[0].body;
    let Node::Line(l) = &body[0] else { panic!() };
    assert_eq!(l.speaker, "bianca");
    assert_eq!(l.text, "Hello!");
    let Node::Line(n) = &body[1] else { panic!() };
    assert_eq!(n.speaker, "narrator");
}

#[test]
fn legacy_line_bracket_form_is_rejected_with_fixit() {
    let (_, diags) = parse("## Shot 1.\n:line[bianca]{code=\"0010\"}: Hello!\n");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, "E-UNCLASSIFIED");
    assert!(diags[0].message.contains("0.1.0"), "fix-it hint: {}", diags[0].message);
}

#[test]
fn content_line_missing_second_colon_is_error() {
    let (_, diags) = parse("## Shot 1.\n:bianca no colon here\n");
    assert_eq!(diags[0].code, "E-UNCLASSIFIED");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-syntax content_line_short_form` → FAIL (`:bianca` currently → `E-UNCLASSIFIED`).

- [ ] **Step 3: Implement.** In `next_node` (parser.rs:269) replace the `:line[` rule:

```rust
// dsl §4.3 rule 5: `:` ident — content line. (`::` rules already matched above.)
if trimmed.starts_with(':')
    && trimmed.as_bytes().get(1).is_some_and(|b| b.is_ascii_alphabetic())
{
    return self.parse_line();
}
```

Rewrite `parse_line` (returns `Option<Node>` now; update the call site to `return self.parse_line();` with the `Option` flowing through):

```rust
/// `Line ::= ":" Speaker Attrs? ":" WS Text` (dsl §7.1). Text is opaque to
/// EOL except `{{…}}` (§4.4, §7.6). Layer = Content.
fn parse_line(&mut self) -> Option<Node> {
    let i = self.cursor;
    let (s, e) = self.lines[i];
    let cstart = s + leading_ws(&self.body[s..e]);
    let line_end = s + self.body[s..e].trim_end().len();
    let b = self.body.as_bytes();
    let mut j = cstart + 1; // past ':'
    let sp_start = j;
    while j < e && is_ident_byte(b[j]) {
        j += 1;
    }
    let speaker = self.body[sp_start..j].to_string();
    // Migration fix-it (dsl §7.1): the removed 0.0.1 bracket form.
    if speaker == "line" && j < e && b[j] == b'[' {
        self.emit_line(
            E_UNCLASSIFIED,
            "`:line[speaker]` was removed in 0.1.0 — write `:speaker{…}: text`",
            i, Layer::Content,
        );
        self.cursor += 1;
        return None;
    }
    let mut attrs = Vec::new();
    if j < e && b[j] == b'{' {
        let (a, after) = self.scan_attrs(j + 1, b'}');
        attrs = a;
        j = after;
    }
    if !(j < e && b[j] == b':') {
        self.emit_line(
            E_UNCLASSIFIED,
            "content line needs a second `:` before its text (dsl §7.1)",
            i, Layer::Content,
        );
        self.cursor += 1;
        return None;
    }
    j += 1; // past second ':'
    while j < e && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    let text_start = j;
    let text_raw = self.body[text_start..line_end.max(text_start)].trim_end();
    let text_end = text_start + text_raw.len();
    let text_span = self.span(text_start, text_end);
    let span = self.span(cstart, line_end);
    self.cursor += 1;
    Some(Node::Line(Line {
        speaker,
        attrs,
        text: text_raw.to_string(),
        text_span,
        interps: Vec::new(), // filled in Task 7
        span,
    }))
}
```

Update the crate doc header (line 12) precedence comment to `` `## ` → `# ` → `::set{` → `::` → `:`ident → `<`tag → error ``.

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` (this crate's suite; pre-existing `:line[` fixtures in THIS crate's tests: rewrite them to the new form in the same change). Expected: PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(syntax)!: content line :speaker{…}: text, remove :line[ (dsl 0.1.0 §7.1)"`

---

### Task 3: Comment engine — `//` line comments + truly-opaque `Text`

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (`strip_comments_checked` + `find_unterminated_comment` ~514-587 + their `:line`-shape helpers `line_text_start_blanked` / `text_start_for_line`)
- Test: `crates/lute-syntax/src/parser.rs` `mod tests`

**Interfaces:**
- Produces: `content_text_start(line: &str) -> Option<usize>` — byte offset (line-relative) just past the second colon of a 0.1.0 content line, `None` if the line isn't one. Both comment scans use it as the opacity boundary.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn line_comment_leading_is_trivia() {
    let (doc, diags) = parse("## Shot 1.\n// a note\n:bianca: Hi.\n");
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(doc.shots[0].body.len(), 1);
}

#[test]
fn line_comment_mid_line_is_not_a_comment() {
    // dsl §4.2: `//` only at line start; inside Text it is literal.
    let (doc, _) = parse("## Shot 1.\n:bianca: see https://example.com // really\n");
    let Node::Line(l) = &doc.shots[0].body[0] else { panic!() };
    assert!(l.text.contains("https://example.com // really"));
}

#[test]
fn block_comment_not_recognized_inside_text() {
    // dsl §4.2 exclusion 2: Text is truly opaque after the second colon.
    let (doc, diags) = parse("## Shot 1.\n:bianca: I love /* this */ you.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else { panic!() };
    assert_eq!(l.text, "I love /* this */ you.");
}

#[test]
fn unterminated_block_comment_inside_text_is_fine() {
    let (_, diags) = parse("## Shot 1.\n:bianca: half /* open\n:narrator: next line intact\n");
    assert!(diags.is_empty(), "{diags:?}");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-syntax line_comment_leading_is_trivia` → FAIL.

- [ ] **Step 3: Implement.** Add the shared boundary helper (free function, parser.rs):

```rust
/// Offset (line-relative) just past the second `:` of a 0.1.0 content line
/// (`":" Ident Attrs? ":"`), i.e. where opaque `Text` begins (dsl §4.2, §4.4).
/// `None` if the trimmed line is not a content line. Quote-aware inside the
/// optional `{…}` attr list.
pub(crate) fn content_text_start(line: &str) -> Option<usize> {
    let ws = line.len() - line.trim_start().len();
    let b = line.as_bytes();
    let mut j = ws;
    if b.get(j) != Some(&b':') {
        return None;
    }
    j += 1;
    if j >= b.len() || b[j] == b':' || !b[j].is_ascii_alphabetic() {
        return None; // `::` directive or not an ident — not a content line
    }
    while j < b.len() && is_ident_byte(b[j]) {
        j += 1;
    }
    if b.get(j) == Some(&b'{') {
        let mut in_str = false;
        j += 1;
        while j < b.len() {
            match b[j] {
                b'"' if !in_str => in_str = true,
                b'"' if in_str && b[j - 1] != b'\\' => in_str = false,
                b'}' if !in_str => break,
                _ => {}
            }
            j += 1;
        }
        j += 1; // past '}' (or EOL — caller degrades safely)
    }
    (b.get(j) == Some(&b':')).then_some(j + 1)
}
```

In `strip_comments_checked` and `find_unterminated_comment`: replace the `:line[`-based `text_start_for_line` / `line_text_start_blanked` internals with `content_text_start`, keeping the existing "recompute boundary from the blanked view after every terminated comment" discipline documented at parser.rs:517-523. Add `//` handling to the strip scan: at each line start (after leading WS, outside any open block comment, outside quotes, and only when `content_text_start` for that line is `None` **or** the `//` sits before the computed text start), blank to EOL (length/newline-preserving — same blanking technique as block comments).

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS (existing comment tests updated where they asserted 0.0.1 in-Text stripping).

- [ ] **Step 5: Commit** — `git commit -am "feat(syntax)!: // line comments; Text opaque past second colon (dsl 0.1.0 §4.2)"`

---

### Task 4: Shot-heading enforcement + `<otherwise>` attr rejection

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (`parse_shot` ~210-228, `parse_shot_number` ~469-478, new const)
- Modify: `crates/lute-syntax/src/parser/blocks.rs` (`parse_otherwise` ~203-210)
- Test: both files' `mod tests`

**Interfaces:**
- Produces: `pub const E_SHOT_HEADING: &str = "E-SHOT-HEADING";` (exported like the other five codes).

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn bad_shot_heading_is_diagnosed() {
    for bad in ["## Chapter 1.", "## Shot .", "## Shot 3", "## Prolog"] {
        let (_, diags) = parse(&format!("{bad}\n:narrator: hi.\n"));
        assert!(diags.iter().any(|d| d.code == "E-SHOT-HEADING"), "{bad}");
    }
}

#[test]
fn all_four_heading_keywords_parse() {
    for good in ["## Shot 1.", "## Scene 2. Title", "## Prologue", "## Epilogue tail", "## 프롤로그", "## 에필로그"] {
        let (_, diags) = parse(&format!("{good}\n:narrator: hi.\n"));
        assert!(diags.is_empty(), "{good}: {diags:?}");
    }
}

#[test]
fn otherwise_with_attrs_is_parse_error() {
    let src = "## Shot 1.\n<match on=\"app.rating\">\n<when test=\"$ == 'teen'\">\n:narrator: a.\n</when>\n<otherwise foo=\"bar\">\n:narrator: b.\n</otherwise>\n</match>\n";
    let (_, diags) = parse(src);
    assert!(diags.iter().any(|d| d.code == "E-LOGIC-CONTENT" && d.message.contains("otherwise")));
}
```

- [ ] **Step 2: Verify failure** — `cargo test -p lute-syntax bad_shot_heading_is_diagnosed` → FAIL (silently `number: None` today, parser.rs:470-478).

- [ ] **Step 3: Implement.** Replace `parse_shot_number` with a validator (dsl §6.3):

```rust
enum HeadingKind {
    Numbered(i64),
    Bookend, // Prologue / Epilogue / 프롤로그 / 에필로그
    Invalid,
}

/// Enforce `ShotHeading` (dsl §6.3, E-SHOT-HEADING): `Shot|Scene <int>.` or a
/// bookend keyword (`Prologue|Epilogue|프롤로그|에필로그`), each + optional trailing Text.
fn classify_heading(heading: &str) -> HeadingKind {
    for kw in ["Shot", "Scene"] {
        if let Some(rest) = heading.strip_prefix(kw) {
            let rest = rest.trim_start();
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            let after = &rest[digits.len()..];
            if !digits.is_empty() && after.starts_with('.') {
                return HeadingKind::Numbered(digits.parse().unwrap());
            }
            return HeadingKind::Invalid;
        }
    }
    for kw in ["Prologue", "Epilogue", "프롤로그", "에필로그"] {
        if let Some(rest) = heading.strip_prefix(kw) {
            if rest.is_empty() || rest.starts_with(' ') {
                return HeadingKind::Bookend;
            }
        }
    }
    HeadingKind::Invalid
}
```

In `parse_shot`: `Invalid` → `self.emit_line(E_SHOT_HEADING, "shot heading must be `Shot N.`/`Scene N.` or Prologue/Epilogue (dsl §6.3)", i, Layer::Content)` and keep `number: None` (best-effort AST). In `blocks.rs::parse_otherwise`: after `parse_open_tag()`, if `!open.attrs.is_empty()` emit `E_LOGIC_CONTENT` with message `"<otherwise> takes no attributes (dsl §7.3)"`.

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): enforce ShotHeading (E-SHOT-HEADING) + reject <otherwise> attrs (dsl 0.1.0 §6.3, §7.3)"`

---

### Task 5: `<hub>` block parsing

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (`next_node` tag dispatch ~272-288)
- Modify: `crates/lute-syntax/src/parser/blocks.rs` (new `parse_hub`, mirror of `parse_branch` ~73-111)
- Test: `crates/lute-syntax/src/parser/blocks.rs` `mod tests`

**Interfaces:**
- Produces: `Node::Hub(Hub)` with `choices: Vec<Choice>`; `once`/`exit` arrive as `AttrValue::BoolTrue` attrs on each `Choice.attrs` (no new Choice fields — Plan B reads the flags).

- [ ] **Step 1: Failing test**

```rust
#[test]
fn hub_parses_choices_with_flags() {
    let src = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"a\" label=\"Ask\" once>\n:bianca: Sure.\n</choice>\n<choice id=\"leave\" label=\"Go\" exit>\n:fixer: Bye.\n</choice>\n</hub>\n";
    let (doc, diags) = parse(src);
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Hub(h) = &doc.shots[0].body[0] else { panic!() };
    assert_eq!(h.choices.len(), 2);
    assert!(h.choices[0].attrs.iter().any(|a| a.key == "once"));
    assert!(h.choices[1].attrs.iter().any(|a| a.key == "exit"));
}

#[test]
fn hub_rejects_non_choice_children() {
    let src = "## Shot 1.\n<hub id=\"chat\">\n:narrator: stray\n</hub>\n";
    let (_, diags) = parse(src);
    assert!(diags.iter().any(|d| d.code == "E-LOGIC-CONTENT"));
}
```

- [ ] **Step 2: Verify failure** — `cargo test -p lute-syntax hub_parses_choices_with_flags` → FAIL (`unexpected block here`).

- [ ] **Step 3: Implement.** `blocks.rs`: `parse_hub` is `parse_branch` with the tag name `"hub"` and `Hub { attrs, choices, span }` (copy the branch loop verbatim — same `<choice>`-only body rule, same `E_LOGIC_CONTENT` on strays, same `consume_close`). `parser.rs` dispatch: add `Some("hub") => return Some(Node::Hub(self.parse_hub())),`.

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): parse <hub> revisit blocks (dsl 0.1.0 §7.3.2)"`

---

### Task 6: String-escape validation (`E-STRING-ESCAPE`)

**Files:**
- Modify: `crates/lute-syntax/src/parser/attrs.rs` (`scan_attrs` quoted-value loop, ~25-140), `crates/lute-syntax/src/parser.rs` (new const)
- Test: `crates/lute-syntax/src/parser/attrs.rs` `mod tests`

- [ ] **Step 1: Failing test**

```rust
#[test]
fn unknown_string_escape_is_diagnosed() {
    let (_, diags) = parse("## Shot 1.\n::sfx{sound=\"a\\qb\"}\n");
    assert!(diags.iter().any(|d| d.code == "E-STRING-ESCAPE"));
}

#[test]
fn defined_escapes_pass() {
    let (_, diags) = parse("## Shot 1.\n::sfx{sound=\"a\\\"b\\\\c\\nd\\te\"}\n");
    assert!(diags.iter().all(|d| d.code != "E-STRING-ESCAPE"), "{diags:?}");
}
```

- [ ] **Step 2: Verify failure** — `cargo test -p lute-syntax unknown_string_escape_is_diagnosed` → FAIL.

- [ ] **Step 3: Implement.** `pub const E_STRING_ESCAPE: &str = "E-STRING-ESCAPE";` in parser.rs. In `scan_attrs`'s quoted-value scan, on each `\` peek the next byte: if not one of `" \\ n t` emit `E_STRING_ESCAPE` (`"only \\\" \\\\ \\n \\t are defined escapes (dsl §4.4)"`) spanning the two bytes; scanning continues (`\` still consumes the next byte, degrading safely). NOTE: `scan_attrs` is `&self` today — thread diagnostics by changing it to `&mut self` (mechanical; its three call sites in parser.rs/blocks.rs already hold `&mut self`).

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): E-STRING-ESCAPE for undefined escapes (dsl 0.1.0 §4.4)"`

---

### Task 7: Interpolation scan (`{{…}}`, `\{{`, `E-INTERP-UNTERMINATED`)

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (new free fn + wire into `parse_line`; new const)
- Test: `crates/lute-syntax/src/parser.rs` `mod tests`

**Interfaces:**
- Produces: `Line.interps` populated; Plan B validates referents, Plan C emits `placeholders`.

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn interps_are_scanned_and_classified() {
    let (doc, diags) = parse("## Shot 1.\n:bianca: Hi {{userName}}, you have {{run.coins}} and {{@fond}}.\n");
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Line(l) = &doc.shots[0].body[0] else { panic!() };
    let kinds: Vec<_> = l.interps.iter().map(|p| (p.kind, p.raw.as_str())).collect();
    assert_eq!(kinds, [
        (InterpKind::Reserved, "userName"),
        (InterpKind::Path, "run.coins"),
        (InterpKind::Ref, "@fond"),
    ]);
}

#[test]
fn escaped_and_unterminated_interp() {
    let (doc, diags) = parse("## Shot 1.\n:bianca: literal \\{{ stays.\n:fixer: broken {{run.coins\n");
    let Node::Line(l) = &doc.shots[0].body[0] else { panic!() };
    assert!(l.interps.is_empty());
    assert!(diags.iter().any(|d| d.code == "E-INTERP-UNTERMINATED"));
}
```

- [ ] **Step 2: Verify failure** — `cargo test -p lute-syntax interps_are_scanned_and_classified` → FAIL.

- [ ] **Step 3: Implement.** `pub const E_INTERP_UNTERMINATED: &str = "E-INTERP-UNTERMINATED";` plus:

```rust
/// Scan `{{…}}` interpolations in a content line's `Text` (dsl §7.6).
/// `text_start_body` is the body-relative offset of `text`'s first byte.
/// `\{{` escapes a literal `{{`; an unclosed `{{` before EOL is
/// E-INTERP-UNTERMINATED. Classification: `@…` → Ref; contains `.` and starts
/// with a state tier → Path; `userName` → Reserved; anything else is kept as
/// Path (the checker rejects undeclared referents, Plan B).
fn scan_interps(&mut self, text: &str, text_start_body: usize) -> Vec<Interp> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut j = 0;
    while j + 1 < b.len() {
        if b[j] == b'\\' && text[j + 1..].starts_with("{{") {
            j += 3; // literal `{{`
            continue;
        }
        if b[j] == b'{' && b[j + 1] == b'{' {
            match text[j + 2..].find("}}") {
                None => {
                    let (s, e) = (text_start_body + j, text_start_body + text.len());
                    self.emit_o(
                        E_INTERP_UNTERMINATED,
                        "`{{` has no closing `}}` before end of line (dsl §7.6)".into(),
                        self.orig(s), self.orig(e), Layer::Content,
                    );
                    break;
                }
                Some(rel) => {
                    let inner = text[j + 2..j + 2 + rel].trim().to_string();
                    let kind = if inner.starts_with('@') {
                        InterpKind::Ref
                    } else if inner == "userName" {
                        InterpKind::Reserved
                    } else {
                        InterpKind::Path
                    };
                    let (s, e) = (text_start_body + j, text_start_body + j + 2 + rel + 2);
                    out.push(Interp { kind, raw: inner, span: self.span(s, e) });
                    j = j + 2 + rel + 2;
                    continue;
                }
            }
        }
        j += 1;
    }
    out
}
```

Wire into `parse_line`: `let interps = self.scan_interps(text_raw, text_start);` before constructing `Line`.

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): scan {{…}} interpolations into Line.interps (dsl 0.1.0 §7.6)"`

---

### Task 8: Downstream `Node::Hub` arms (transitional)

**Files:**
- Modify: every non-exhaustive-match break in `crates/lute-check`, `crates/lute-compile`, `crates/lute-lsp` — find with `cargo check --workspace` after Task 5; typical sites: AST walkers in `lute-check/src/{check,defassign,match_check,inject}.rs`, `lute-compile/src/{normalize,lower}.rs`, LSP traversal.
- Test: `crates/lute-check/tests/` (or its inline test module, matching existing layout)

**Interfaces:**
- Produces: `pub const E_HUB_UNSUPPORTED: &str = "E-HUB-UNSUPPORTED";` in `lute-check` (transitional; Plan B replaces it with real hub semantics and deletes the constant).

- [ ] **Step 1: Failing build** — `cargo check --workspace` → every non-exhaustive `match` on `Node` errors. List them.
- [ ] **Step 2: Implement.** In `lute-check`: one arm in the main node walk emits `E-HUB-UNSUPPORTED` (`"<hub> checking lands in the 0.1.0 checker cutover (Plan B); document cannot pass check yet"`, Severity::Error) — this keeps the D6 clean-check gate sound (hub docs cannot compile). All other walkers (defassign, inject, compile normalize/lower, LSP) treat `Node::Hub` conservatively: recurse into `choices[].body` where the walker recurses into branch choices, else no-op. `lute-compile` may `unreachable!("gated by E-HUB-UNSUPPORTED")` in `lower.rs` with a comment citing D6.
- [ ] **Step 3: Test**

```rust
#[test]
fn hub_is_rejected_until_plan_b() {
    let out = check_str("---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<hub id=\"h\">\n<choice id=\"a\" label=\"A\" exit>\n:narrator: hi.\n</choice>\n</hub>\n");
    assert!(out.diagnostics.iter().any(|d| d.code == "E-HUB-UNSUPPORTED"));
}
```

(Adapt the harness call to the crate's existing test helper — see neighboring tests in `lute-check` for the exact fixture entry point.)

- [ ] **Step 4: Run** — `cargo test -p lute-check` → PASS; `cargo check --workspace` → clean.
- [ ] **Step 5: Commit** — `git commit -am "feat(check): transitional E-HUB-UNSUPPORTED gate for Node::Hub (Plan B replaces)"`

---

### Task 9: Migrate in-repo fixtures & examples

**Files:**
- Modify: every `.lute` under `docs/examples/` (incl. `showcase/`, component/schema files' bodies) and every inline `.lute` string fixture in `crates/*/src/**` + `crates/*/tests/**` that uses removed syntax.

- [ ] **Step 1: Mechanical rewrite** — `:line[X]` → `:X` (regex `(?m)^(\s*):line\[([A-Za-z][A-Za-z0-9_-]*)\]` → `$1:$2`); `<choice … as="` → `into="` (choice lines only — do NOT touch `:speaker{as="…"}` label overrides). Grep afterwards: `grep -rn ':line\[' docs crates` → 0 matches outside frozen spec docs (`0.0.1.md`, migration/errata appendices).
- [ ] **Step 2: Heading sweep** — verify all example headings already satisfy §6.3 (E-SHOT-HEADING from Task 4 will catch strays in Step 4).
- [ ] **Step 3: Note** — do NOT add hubs/interpolation to `docs/examples/` yet (checker rejects hubs until Plan B; showcase examples must stay check-green).
- [ ] **Step 4: Run** — `cargo test --workspace` → PASS; `cargo run -q -p lute-cli -- check docs/examples/showcase/episode01.lute --project docs/examples/showcase` → exit 0.
- [ ] **Step 5: Commit** — `git commit -am "chore!: migrate examples + fixtures to 0.1.0 content-line syntax"`

---

### Task 10: Docs & diagnostics registry sweep

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (module doc), `README.md` (syntax examples, if any), `editors/README.md` (defer grammar changes to Plan D with a note).

- [ ] **Step 1:** Update crate/module docs to cite dsl 0.1.0 sections; ensure the five old + three new (`E-SHOT-HEADING`, `E-STRING-ESCAPE`, `E-INTERP-UNTERMINATED`) constants have doc comments citing their sections; README `.lute` snippets use `:speaker` form.
- [ ] **Step 2:** `cargo test --workspace` + `cargo doc -p lute-syntax --no-deps` → both clean.
- [ ] **Step 3: Commit** — `git commit -am "docs(syntax): 0.1.0 references + diagnostics registry sweep"`

---

## Self-Review (done at authoring)

1. **Spec coverage:** dsl 0.1.0 parser-owned deltas — §4.2 (T3), §4.3/§7.1 (T2), §4.4 escapes (T6), §6.3 (T4), §7.3 otherwise (T4), §7.3.2 (T5), §7.6 scanning (T7), Appendix D migration (T2 fix-it, T9). Checker-owned deltas (`E-PATH-IDENT`, `E-WHEN-PATTERN`, `E-HUB-NO-EXIT`, `E-BRANCH-ALL-GUARDED`, `E-AT-CONTEXT`, is-coverage, interp referents) → Plan B by design.
2. **Placeholders:** none — every code step carries code; Task 8's site list is discovered by `cargo check` (exact sites are compiler-enumerated, not guessable ahead).
3. **Type consistency:** `Interp`/`InterpKind`/`Hub` (T1) match uses in T5/T7; `scan_attrs` `&mut self` change (T6) precedes no earlier `&self` use.
