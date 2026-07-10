# lute-syntax 0.2.0 Implementation Plan (Plan A of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Teach the hand-written parser the three new 0.2.0 closed-grammar constructs — `<quest>` (top-level), `<on>` (ECA trigger), `<objective>` (with self-closing `<objective/>`) — kind-agnostically, and extend the CEL-slot walk to cover them, while `cargo test --workspace` stays green via transitional downstream arms.

**Architecture:** All grammar lives in `crates/lute-syntax` (line-classification parser, NOT tree-sitter — that is Plan E). The parser NEVER reads frontmatter `kind:`; it structurally admits `<quest>`/`<on>`/`<objective>` unconditionally, exactly as it already admits `<hub>`/`<timeline>`. `Document` gains a sibling `quests: Vec<Quest>` field; `Node` gains `Objective(Objective)` and `On(On)` variants. All per-kind / grammar admission (`E-KIND-MISSING`, `E-UNKNOWN-KIND`, `E-GRAMMAR-NOT-ADMITTED`) is deferred to lute-check (Plan C). Downstream crates (lute-check, lute-compile) get minimal transitional `Node::On|Objective` arms so the workspace compiles; Plans C/D replace them with real semantics.

**Tech Stack:** Rust (workspace `cargo test`), spec = `docs/proposals/scenario-dsl/0.2.0.md` (cited "dsl 0.2.0 §N"), design contract `docs/superpowers/specs/2026-07-07-lute-dsl-0.2.0-design.md`.

## Global Constraints

- Spec source of truth: `docs/proposals/scenario-dsl/0.2.0.md`; kernel unchanged from `0.1.0.md`.
- Parser is kind-agnostic — NO YAML parse, NO `kind:` read. Zero new `pub const E_*` diagnostics in lute-syntax for 0.2.0 (every new diagnostic is a checker-layer concern — proven: `parse_match`/`parse_when` already synthesize empty CEL slots for missing `on=`/`test=` without a parse error).
- SPAN-FIDELITY contract (parser.rs header): comment stripping stays length/newline-preserving; every `Span` is an original-source offset.
- Grammar shapes (dsl 0.2.0 §4.1, §6.3, §6.4):
  - `On ::= "<on" Attrs ">" Node* "</on>"` — attr `event` (String), `when` (CEL, optional).
  - `QuestDecl ::= "<quest" Attrs ">" QuestBody "</quest>"` — attrs `id` (req), `title`, `start` (CEL), `fail` (CEL).
  - `Objective ::= "<objective" Attrs ">" Node* "</objective>" | "<objective" Attrs "/>"` — attrs `id` (req), `done` (CEL, req), `when` (CEL), `title`, `optional` (bare bool).
- `<quest>` is TOP-LEVEL ONLY (never a `Node`, never nests); `<on>`/`<objective>` are `Node`s. Nested `<quest>` must fall through to the existing `E-UNCLASSIFIED` "unexpected block here" path (no dedicated code).
- walk.rs immutable + mutable walks MUST stay structurally byte-identical (the StableId sequence rides on the visit order).
- Run only the tests you add/modify per task; full `cargo test --workspace` gates only the final task.
- Work in the worktree `~/Workspace/lute/.worktrees/lute-0.2.0` on branch `feat/lute-0.2.0`.

---

### Task 1: AST — `Quest`, `Objective`, `On` types + `Document.quests`

**Files:**
- Modify: `crates/lute-syntax/src/ast.rs` (Document ~4-9, after Hub ~94)

**Interfaces:**
- Produces (all later tasks + Plans C/D consume):
  - `Document` gains `pub quests: Vec<Quest>`
  - `enum Node { …, Objective(Objective), On(On) }`
  - `pub struct Quest { pub id: String, pub id_span: Span, pub title: Option<String>, pub start: Option<CelSlot>, pub fail: Option<CelSlot>, pub attrs: Vec<Attr>, pub body: Vec<Node>, pub span: Span }`
  - `pub struct Objective { pub id: String, pub id_span: Span, pub done: CelSlot, pub when: Option<CelSlot>, pub title: Option<String>, pub optional: bool, pub attrs: Vec<Attr>, pub body: Vec<Node>, pub span: Span }`
  - `pub struct On { pub event: String, pub event_span: Span, pub when: Option<CelSlot>, pub attrs: Vec<Attr>, pub body: Vec<Node>, pub span: Span }`

- [ ] **Step 1: Add types** — in `ast.rs`, add `Objective(Objective)` and `On(On)` to `enum Node` (after `Hub(Hub)`), add `pub quests: Vec<Quest>` to `struct Document`, and after `struct Hub` (line ~94) add:

```rust
/// `<quest id …> QuestBody </quest>` (dsl 0.2.0 §6.3). A TOP-LEVEL declaration
/// (never a [`Node`]); `body` reuses the shared `Node` stream (only the arms
/// admitted by dsl 0.2.0 §6.7 are legal — enforced in lute-check, not here).
/// `start`/`fail` are optional CEL guards; `title` is a localizable String
/// captured raw (interps recovered on demand via `scan_label_interps`).
#[derive(Clone, Debug)]
pub struct Quest {
    pub id: String,
    pub id_span: Span,
    pub title: Option<String>,
    pub start: Option<CelSlot>,
    pub fail: Option<CelSlot>,
    /// Residual (post-extraction) attrs, mirroring [`Branch`]; normally empty.
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// `<objective id done …> Node* </objective>` or self-closing
/// `<objective … />` (dsl 0.2.0 §6.4). `done` is the required completion
/// predicate; `when` gates visibility; `optional` is a bare boolean flag.
#[derive(Clone, Debug)]
pub struct Objective {
    pub id: String,
    pub id_span: Span,
    pub done: CelSlot,
    pub when: Option<CelSlot>,
    pub title: Option<String>,
    pub optional: bool,
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}

/// `<on event … [when …]> Node* </on>` (dsl 0.2.0 §4). The ECA trigger:
/// `event` names a built-in lifecycle or capability world event (a plain
/// String, NOT CEL); `when` is an optional CEL guard.
#[derive(Clone, Debug)]
pub struct On {
    pub event: String,
    pub event_span: Span,
    pub when: Option<CelSlot>,
    pub attrs: Vec<Attr>,
    pub body: Vec<Node>,
    pub span: Span,
}
```

- [ ] **Step 2: Compile the crate only** — `cargo check -p lute-syntax`. Expected errors ONLY in `lute-syntax`: (a) `Document { … }` construction in `parser.rs::parse` (~97-105) missing `quests` — add `quests` (populated in Task 4; for now the field is wired there); (b) `node_end` (parser.rs ~690) non-exhaustive; (c) `walk.rs` `node`/`node_mut` non-exhaustive. These are fixed in Tasks 4 & 5. To make THIS task compile in isolation, temporarily add `quests: Vec::new()` at the `Document` literal and `Node::Objective(o) => o.span.byte_end, Node::On(o) => o.span.byte_end` to `node_end`, and `Node::Objective(_) | Node::On(_) => {}` to walk.rs's two matches (Task 5 replaces the walk stubs). Prefer to leave those precise edits to their owning tasks if executing in order; if so, this step's `cargo check` is expected to fail with exactly those non-exhaustive/missing-field errors.

- [ ] **Step 3: Commit** — `git commit -am "feat(syntax): AST types for quest/objective/on (dsl 0.2.0)"`

---

### Task 2: `take_bool` attr helper

**Files:**
- Modify: `crates/lute-syntax/src/parser/attrs.rs` (~216, alongside `take_str`)
- Test: covered end-to-end by Task 3's `objective_optional_flag_parses` (a bare `optional` flag round-trips through a real parse — less brittle than hand-building an `Attr`).

**Interfaces:**
- Produces: `pub(super) fn take_bool(attrs: &mut Vec<Attr>, key: &str) -> bool` — removes the first attr named `key` and returns `true` when it was present as a bare boolean-true attr. GROUND TRUTH from `attrs.rs`: a bare (no `=`) attr parses to `AttrValue::BoolTrue` (confirmed: `take_cel` matches `AttrValue::{Str, Ref, BoolTrue}`; `Attr { key, value, value_span }`). `take_str`/`take_str_spanned`/`take_cel` are `pub(super)` FREE functions in `attrs.rs` (NOT methods).

- [ ] **Step 1: Implement** — add to attrs.rs near `take_str` (~216), mirroring `take_str`'s `position`+`remove` idiom:

```rust
/// Remove the first attr named `key` and report whether it was present as a
/// bare boolean-true flag (dsl 0.2.0 §6.4 `optional`). A bare `key` with no
/// `=` parses to `AttrValue::BoolTrue`; a `key="…"` value is still consumed but
/// reported `false` (it is not a bare flag).
pub(super) fn take_bool(attrs: &mut Vec<Attr>, key: &str) -> bool {
    if let Some(pos) = attrs.iter().position(|a| a.key == key) {
        return matches!(attrs.remove(pos).value, AttrValue::BoolTrue);
    }
    false
}
```

- [ ] **Step 2: Compile** — `cargo check -p lute-syntax` → clean (helper unused until Task 3; add `#[allow(dead_code)]` ONLY if the unused-warning gate blocks — Task 3 uses it immediately, so prefer committing Tasks 2+3 together if the crate denies warnings).

- [ ] **Step 3: Commit** — `git commit -am "feat(syntax): take_bool attr helper for objective optional flag"` (or fold into Task 3's commit if warnings-as-errors blocks a standalone unused helper).

---

### Task 3: `blocks.rs` — self-closing detection + `parse_quest`/`parse_objective`/`parse_on`

**Files:**
- Modify: `crates/lute-syntax/src/parser/blocks.rs` (`OpenTag` ~18-24, `parse_open_tag` ~28-46, new parse fns near `parse_branch`/`parse_when`)
- Test: `crates/lute-syntax/src/parser/blocks.rs` `mod tests` (~427)

**Interfaces:**
- Consumes: `Quest`/`Objective`/`On` (Task 1), `take_bool` (Task 2), existing `parse_open_tag`/`at_close`/`consume_close`/`parse_block_body`/`take_str`/`take_str_spanned`/`take_cel`, `CelKind::Condition`.
- Produces: `pub(super) fn parse_quest(&mut self) -> Quest`, `pub(super) fn parse_objective(&mut self) -> Objective`, `pub(super) fn parse_on(&mut self) -> On`; `OpenTag` gains `pub self_closing: bool`.

- [ ] **Step 1: Write failing tests** — in blocks.rs `mod tests` (alongside the existing `hub_*` tests, which show the `crate::parse("## Shot 1.\n<hub…")` idiom):

```rust
#[test]
fn on_parses_event_when_and_body() {
    let (doc, diags) = crate::parse(
        "## Shot 1.\n<on event=\"combatEnd\" when=\"run.dead\">\n:narrator: silence.\n</on>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    let Node::On(on) = &doc.shots[0].body[0] else { panic!("{:?}", doc.shots[0].body) };
    assert_eq!(on.event, "combatEnd");
    assert!(on.when.is_some());
    assert_eq!(on.body.len(), 1);
}

#[test]
fn objective_self_closing_has_empty_body() {
    let (doc, diags) = crate::parse(
        "## Shot 1.\n<objective id=\"reach\" title=\"Reach\" done=\"run.here\"/>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Objective(o) = &doc.shots[0].body[0] else { panic!() };
    assert_eq!(o.id, "reach");
    assert_eq!(o.title.as_deref(), Some("Reach"));
    assert!(o.done.raw.contains("run.here"));
    assert!(o.body.is_empty());
    assert!(!o.optional);
}

#[test]
fn objective_optional_flag_parses() {
    let (doc, _) = crate::parse(
        "## Shot 1.\n<objective id=\"x\" done=\"a\" optional/>\n",
    );
    let Node::Objective(o) = &doc.shots[0].body[0] else { panic!() };
    assert!(o.optional);
}

#[test]
fn objective_long_form_body_emits() {
    let (doc, diags) = crate::parse(
        "## Shot 1.\n<objective id=\"x\" done=\"a\">\n::set{run.x = 1}\n</objective>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    let Node::Objective(o) = &doc.shots[0].body[0] else { panic!() };
    assert_eq!(o.body.len(), 1);
}
```

> `CelSlot`'s raw-text field name comes from `ast.rs` (`CelSlot { raw, kind, ast?, span }` — confirm; use the actual field for the `done.raw` assertion, or assert on `o.done.kind`).

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-syntax on_parses_event` → FAIL (no `parse_on`; `<on>` currently hits `E-UNCLASSIFIED`).

- [ ] **Step 3a: Self-closing detection.** In `OpenTag` (blocks.rs ~18-24) add `pub self_closing: bool`. In `parse_open_tag` (~28-46), right after `let (attrs, after) = self.scan_attrs(j, b'>');`, compute the flag from the byte immediately before the consumed `>` terminator:

```rust
// dsl 0.2.0 §6.4 self-closing `<tag/>`: the `>` was preceded by `/`. The
// attr scanner tolerates the lone `/` (skips it as an unparseable token),
// so detect it from the raw byte just before the consumed terminator.
let self_closing = after >= 2 && self.body.as_bytes()[after - 2] == b'/';
```

Include `self_closing` in the returned `OpenTag { … }` literal. (Every existing `OpenTag { … }` construction — only `parse_open_tag` builds it — gets the field.)

- [ ] **Step 3b: parse fns.** Add near `parse_branch`/`parse_when` (mirror their `parse_open_tag` → extract → `parse_block_body`/`consume_close` → wrap shape). Use the crate's actual helper names (`take_str`, `take_str_spanned` for the id span, `take_cel(&mut attrs, key, CelKind::Condition)`):

```rust
// blocks.rs already imports `take_cel, take_str, take_str_spanned` (line ~10);
// add `take_bool` to that `use super::attrs::{…}` line.
pub(super) fn parse_on(&mut self) -> On {
    let open = self.parse_open_tag();
    let mut attrs = open.attrs.clone();
    let (event, event_span) = take_str_spanned(&mut attrs, "event")
        .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
    let when = take_cel(&mut attrs, "when", CelKind::Condition);
    let (body, end_o) = self.parse_block_body("on", &open);
    On { event, event_span, when, attrs, body, span: self.span_o(open.start_o, end_o) }
}

pub(super) fn parse_quest(&mut self) -> Quest {
    let open = self.parse_open_tag();
    let mut attrs = open.attrs.clone();
    let (id, id_span) = take_str_spanned(&mut attrs, "id")
        .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
    let title = take_str(&mut attrs, "title");
    let start = take_cel(&mut attrs, "start", CelKind::Condition);
    let fail = take_cel(&mut attrs, "fail", CelKind::Condition);
    let (body, end_o) = self.parse_block_body("quest", &open);
    Quest { id, id_span, title, start, fail, attrs, body, span: self.span_o(open.start_o, end_o) }
}

pub(super) fn parse_objective(&mut self) -> Objective {
    let open = self.parse_open_tag();
    let mut attrs = open.attrs.clone();
    let (id, id_span) = take_str_spanned(&mut attrs, "id")
        .unwrap_or_else(|| (String::new(), self.span_o(open.start_o, open.end_o)));
    // `done` is required but a MISSING `done` still yields a valid AST (empty CEL
    // slot) — E-OBJECTIVE-MISSING-DONE is a Plan C checker diagnostic, NOT a parse
    // error. Mirror parse_when/parse_match's empty-slot idiom exactly.
    let done = take_cel(&mut attrs, "done", CelKind::Condition).unwrap_or_else(|| {
        CelSlot::raw(CelKind::Condition, String::new(), self.span_o(open.start_o, open.end_o))
    });
    let when = take_cel(&mut attrs, "when", CelKind::Condition);
    let title = take_str(&mut attrs, "title");
    let optional = take_bool(&mut attrs, "optional");
    let (body, end_o) = if open.self_closing {
        (Vec::new(), open.end_o)
    } else {
        self.parse_block_body("objective", &open)
    };
    Objective { id, id_span, done, when, title, optional, attrs, body, span: self.span_o(open.start_o, end_o) }
}
```

> GROUND TRUTH (verified against blocks.rs/attrs.rs — do NOT deviate):
> - `take_str(attrs,key) -> Option<String>`, `take_str_spanned(attrs,key) -> Option<(String,Span)>`, `take_cel(attrs,key,CelKind) -> Option<CelSlot>` are `pub(super)` FREE fns in `attrs.rs`; `take_bool` is Task 2's. Call them bare (NOT `self.`).
> - Empty/absent CEL slot = `CelSlot::raw(CelKind::Condition, String::new(), span)` — the SAME constructor parse_when/parse_match use (blocks.rs ~176-182 / ~234-240).
> - `open.attrs` is CLONED because `&open` is borrowed later by `parse_block_body`; `open.start_o`/`open.end_o` are `usize`, `open.self_closing` is `bool` (all Copy). Matches every sibling parse fn (`let mut attrs = open.attrs.clone();`).
> - `parse_block_body(name, &open) -> (Vec<Node>, usize)` consumes the matching close (E_UNCLOSED_TAG if absent). For a self-closing objective, SKIP it — use `open.end_o`.
> - `span_o(start_o, end_o) -> Span` converts original-text offsets (sibling fns identical).

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS (all four new tests; existing tests unaffected).

- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): parse <quest>/<on>/<objective> blocks + self-closing (dsl 0.2.0)"`

---

### Task 4: `parser.rs` — wire top-level `<quest>`, `<on>`/`<objective>` dispatch, `node_end`, `Document.quests`

**Files:**
- Modify: `crates/lute-syntax/src/parser.rs` (`parse` ~95-105, `parse_document_inner` ~188-223, `next_node` open-tag match ~316-320, `node_end` ~690)
- Test: `crates/lute-syntax/src/parser.rs` `mod tests` (~771)

**Interfaces:**
- Consumes: `parse_quest`/`parse_on`/`parse_objective` (Task 3), `Quest` (Task 1).
- Produces: top-level `<quest>` collected into `Document.quests`; `<on>`/`<objective>` classified as `Node::On`/`Node::Objective` inside any body; `node_end` handles the two new variants.

- [ ] **Step 1: Write failing tests** — in parser.rs `mod tests`:

```rust
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
         <on event=\"questComplete\">\n:x: hi\n</on>\n\
         </quest>\n",
    );
    assert!(diags.is_empty(), "{diags:?}");
    assert!(matches!(doc.quests[0].body[0], Node::On(_)));
}

#[test]
fn nested_quest_is_unclassified() {
    // <quest> is top-level only; nested it must fall through to the error path.
    let (_, diags) = parse("<quest id=\"q\">\n<quest id=\"inner\"></quest>\n</quest>\n");
    assert!(diags.iter().any(|d| d.code == "E-UNCLASSIFIED"), "{diags:?}");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-syntax quest_doc_collects` → FAIL (top-level `<quest>` currently → `E-UNCLASSIFIED`; `doc.quests` may not compile yet if Task 1's temporary stub differs).

- [ ] **Step 3a: `parse_document_inner`** (parser.rs ~188). Change the signature to also return quests, e.g. `-> (Option<(String, Span)>, Vec<Shot>, Vec<Quest>)`, and add a `<quest>` branch BEFORE the final `else`:

```rust
} else if trimmed.starts_with('<') && open_tag_name(&trimmed).as_deref() == Some("quest") {
    quests.push(self.parse_quest());
} else {
```

Declare `let mut quests = Vec::new();` at the top and return it. (A stray top-level `<on>`/`<branch>`/etc. still correctly falls to the existing `E-UNCLASSIFIED` path — no new branch needed there.)

- [ ] **Step 3b: `parse`** (parser.rs ~95). Capture `let (title, shots, quests) = p.parse_document_inner();` and add `quests` to the `Document { … }` literal (~97-105).

- [ ] **Step 3c: `next_node`** open-tag match (parser.rs ~316-320). Add two arms alongside `branch`/`match`/`timeline`/`hub`:

```rust
Some("on") => return Some(Node::On(self.parse_on())),
Some("objective") => return Some(Node::Objective(self.parse_objective())),
```

Deliberately do NOT add `Some("quest")` — a nested `<quest>` falls through to the `_ =>` "unexpected block here" `E-UNCLASSIFIED` arm (dsl 0.2.0 §6.7).

- [ ] **Step 3d: `node_end`** (parser.rs ~690). Add:

```rust
Node::Objective(o) => o.span.byte_end,
Node::On(o) => o.span.byte_end,
```

(Confirm the field name `span.byte_end` from the sibling arms.)

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS (new + existing).

- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): top-level <quest> + <on>/<objective> node dispatch (dsl 0.2.0)"`

---

### Task 5: `walk.rs` — CEL-slot traversal for quest/objective/on

**Files:**
- Modify: `crates/lute-syntax/src/walk.rs` (imports ~27-29, `for_each_cel_slot` ~35-39, `node` ~55-65, `for_each_cel_slot_mut` ~121-125, `node_mut` ~141-151, new free fns)
- Test: `crates/lute-syntax/src/walk.rs` `mod tests` (~202)

**Interfaces:**
- Consumes: `Quest`/`Objective`/`On` (Task 1).
- Produces: `for_each_cel_slot(_mut)` visit `doc.quests` after `doc.shots`; `Node::Objective`/`Node::On` arms in `node`/`node_mut`; canonical order documented below.
- **Canonical CEL-slot pre-order** (immutable + mutable MUST match exactly):
  - `doc`: every `shot.body` (unchanged), THEN every `quest` in `doc.quests`.
  - `Quest`: `start` (if any), `fail` (if any), residual `attrs` refs, then `body`.
  - `Node::Objective`: `done`, `when` (if any), residual `attrs` refs, then `body`.
  - `Node::On`: `when` (if any), residual `attrs` refs, then `body`.

- [ ] **Step 1: Write the failing test** — in walk.rs `mod tests`:

```rust
#[test]
fn quest_slots_visited_in_canonical_order() {
    let (doc, _) = crate::parse(
        "<quest id=\"q\" start=\"run.s\" fail=\"run.f\">\n\
         <objective id=\"o\" done=\"run.d\" when=\"run.w\"/>\n\
         <on event=\"questComplete\" when=\"run.g\">\n:x: hi\n</on>\n\
         </quest>\n",
    );
    let mut raws: Vec<String> = Vec::new();
    super::for_each_cel_slot(&doc, &mut |s| raws.push(s.raw.clone()));
    // start, fail, objective.done, objective.when, on.when — in this order.
    assert_eq!(raws, vec!["run.s", "run.f", "run.d", "run.w", "run.g"]);
}
```

> Confirm `CelSlot`'s text field name (`raw`) from ast.rs; adapt the assertion if it differs.

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-syntax quest_slots_visited` → FAIL.

- [ ] **Step 3: Implement.** Update imports (add `On, Objective, Quest`). In `for_each_cel_slot` (~35) after the shots loop add `for q in &doc.quests { quest(q, f); }`. Add `node` arms:

```rust
Node::Objective(o) => objective(o, f),
Node::On(o) => on(o, f),
```

Add free fns (mirroring `branch`/`hub`):

```rust
fn quest<'a>(q: &'a Quest, f: &mut impl FnMut(&'a CelSlot)) {
    if let Some(s) = &q.start { f(s); }
    if let Some(fl) = &q.fail { f(fl); }
    attrs(&q.attrs, f);
    body(&q.body, f);
}
fn objective<'a>(o: &'a Objective, f: &mut impl FnMut(&'a CelSlot)) {
    f(&o.done);
    if let Some(w) = &o.when { f(w); }
    attrs(&o.attrs, f);
    body(&o.body, f);
}
fn on<'a>(o: &'a On, f: &mut impl FnMut(&'a CelSlot)) {
    if let Some(w) = &o.when { f(w); }
    attrs(&o.attrs, f);
    body(&o.body, f);
}
```

Mirror EXACTLY on the mutable side: `for_each_cel_slot_mut` adds `for q in &mut doc.quests { quest_mut(q, f); }`; `node_mut` adds `Node::Objective(o) => objective_mut(o, f), Node::On(o) => on_mut(o, f)`; add `quest_mut`/`objective_mut`/`on_mut` with the identical visit order over `&mut` slots.

- [ ] **Step 4: Run** — `cargo test -p lute-syntax` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(syntax): walk quest/objective/on CEL slots in canonical order (dsl 0.2.0)"`

---

### Task 6: Downstream transitional arms + workspace green

**Files:**
- Modify (transitional `Node::On|Objective` arms — minimal, replaced by Plans C/D):
  - `crates/lute-check/src/check.rs` — every exhaustive `match node` site: `Walker::walk` (~526), `walk_component_body` (~1117), `collect_use_targets` (~1210), `fold_branches_nodes` (~1538), `fold_slots_nodes` (~1598), `fold_injections` (~1748), `node_summary` (~1868)
  - `crates/lute-check/src/defassign.rs` — `walk_nodes` (~90)
  - `crates/lute-compile/src/normalize.rs` — `normalize_nodes` (~64, has wildcard), `bind_params` (~223, exhaustive)
  - `crates/lute-compile/src/expand.rs` — `expand_nodes` (~41, exhaustive)
  - `crates/lute-compile/src/stage.rs` — `walk_seq` (~44, exhaustive)
- Test: none new (existing workspace suite is the gate).

**Interfaces:**
- Produces: workspace compiles + all existing tests pass with the two new `Node` variants present. Transitional behavior: `<on>`/`<objective>` appearing in a document (only possible via authored input) is REJECTED by the checker with a transitional `E-QUEST-UNSUPPORTED` (replaced by real semantics + admission in Plan C); compile is unreachable on such docs (D6 gate). No existing fixture uses these constructs, so all current tests stay green.

- [ ] **Step 1: Add the transitional check gate.** In `check.rs`, add `const E_QUEST_UNSUPPORTED: &str = "E-QUEST-UNSUPPORTED";` and in `Walker::walk`'s `match node` (~526) add an arm that pushes an Error diagnostic at the node's span for `Node::On`/`Node::Objective` (mirror the shape the 0.1.0 plan used for the transitional `E-HUB-UNSUPPORTED` — read git history commit `2b576d8` if needed). In `node_summary` (~1868, no wildcard) add `Node::On(_) => "<on>".into(), Node::Objective(_) => "<objective>".into()`. In the recursion-only match sites (`collect_use_targets`, `fold_branches_nodes`, `fold_slots_nodes`, `fold_injections`) add `Node::On(_) | Node::Objective(_) => {}` arms (or fold into their existing `_ => {}` wildcard where present — `fold_branches_nodes`/`fold_slots_nodes`/`fold_injections` already have wildcards; only the exhaustive ones need explicit arms). `walk_component_body` (~1117): add `Node::On(_) | Node::Objective(_) => { /* E-COMPONENT-BODY */ }` denying them like Set/Branch (they can never appear in a component body).

- [ ] **Step 2: defassign.** In `walk_nodes` (~90) add `Node::On(_) | Node::Objective(_) => {}` (transitional no-op; real may-write handling is Plan C).

- [ ] **Step 3: compile transitional arms.** In `normalize_nodes` (~64) the wildcard `_ => {}` already absorbs them — leave as-is (Plan D adds real recursion). In `bind_params` (~223, exhaustive), `expand_nodes` (~41, exhaustive), `walk_seq` (~44, exhaustive) add `Node::On(_) | Node::Objective(_) => {}` arms. These are unreachable in practice (compile only runs on a clean check, and Plan A's checker rejects on/objective docs), so a no-op is sound transitionally.

- [ ] **Step 4: Run the full workspace** — `cargo test --workspace`. Expected: PASS (same counts as baseline; note the pre-existing `lute-compile` e2e parallelism flake — re-run `cargo test -p lute-compile --test e2e` in isolation to confirm 6/6 if the workspace run shows an e2e failure). Fix any real compile error by adding the missing arm at the site the compiler names.

- [ ] **Step 5: Commit** — `git commit -am "chore(check,compile): transitional Node::On/Objective arms (Plan A; Plans C/D replace)"`

---

## Self-Review checklist (run before executing)

1. **Spec coverage:** `<quest>`/`<on>`/`<objective>` grammar (dsl 0.2.0 §4.1/§6.3/§6.4) → Tasks 1,3,4. Self-closing (§6.4) → Task 3. CEL-slot walk determinism → Task 5. Kind-agnostic parser (design D3) → enforced throughout (no `kind:` read). Admission/diagnostics deferred to Plan C — explicitly out of scope here.
2. **Placeholder scan:** none — every step has code or a precise verify-against-source instruction.
3. **Type consistency:** `Quest`/`Objective`/`On` field names identical across Tasks 1/3/4/5; `take_bool`/`take_cel`/`take_str_spanned` names to be confirmed against `attrs.rs`/`blocks.rs` at implementation time (flagged in each task).
