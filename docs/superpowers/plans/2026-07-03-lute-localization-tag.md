# Lute Localization `lute tag` Pass (§12) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add the `lute tag` localization pass (§12): assign a stable, persisted `code` to every content `:line` that lacks one, so each line carries a stable `textUnitId` for the external id-keyed translation sidecar. Existing codes are NEVER recomputed; the pass is idempotent.

**Architecture:** A pure core `tag_document(text) -> TagOutcome` in a new `lute-check/src/tag.rs` (no file I/O): it parses the document, collects every `:line` in document order, and back-fills a `code` into each untagged line by splicing the source text (back-to-front to keep byte offsets valid). The `lute tag <file>` CLI subcommand is a thin shell: read file → `tag_document` → write back → report. No engine/runtime coupling; translation tables are external tooling (out of scope).

**Tech Stack:** Rust (workspace, rustc 1.96.1). Reuses `lute_syntax::parse` (`Document`/`Shot`/`Node::Line`/`Line { speaker, attrs, text, text_span, span }`) and clap (already the CLI arg parser). No new deps.

## Global Constraints

- **rustup stable 1.96.1** via `~/.cargo/bin`. Every fresh shell: `export PATH="$HOME/.cargo/bin:$PATH"`. NEVER `brew install rust`.
- **Worktree authoritative.** Work in `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` on `feat/lute-lsp-rust` (== `main` at plan authoring, HEAD `88ea47b`). **HARNESS QUIRK:** `write`/`edit` resolve RELATIVE paths against the MAIN workspace (`~/Workspace/lute`), NOT the worktree — always use ABSOLUTE worktree paths; after every commit verify `git status` clean in BOTH trees (`?? .worktrees/` in main is expected).
- **TDD, tester-first.** Failing test first, confirm the exact failure, minimal impl, green. Own-crate `cargo test -p <crate>` during a task; full-workspace gate at plan end.
- **Format touched crates** (`cargo fmt -p <crate>`); keep `cargo fmt --check` clean. **Clippy `-D warnings`** per touched crate before each commit.
- **Never mangle the document.** `tag_document` MUST preserve every byte outside the insertions: it only INSERTS `code="…"` into untagged `:line` headers, never reorders/removes/reformats. `check()` over the tagged output MUST yield the SAME diagnostics as over the input (modulo the now-present codes) — i.e. a valid doc stays valid; the added `code` attr is itself valid.
- **Idempotent + stable:** running `tag` twice is a no-op on the second run; an already-`code`d line is NEVER touched; new codes never collide with existing ones. Inserting a new untagged line and re-tagging assigns it a fresh code without changing any existing code.
- **Determinism / never-panic:** `tag_document` is total (a malformed/parse-error doc yields the text unchanged + a report of 0 tagged, never a panic); code assignment is deterministic.
- **Snapshot-is-SoT unaffected:** this touches no capability/grammar field → `capabilityVersion` UNCHANGED → tree-sitter drift guard stays green, NO re-stamp.

## Spec source-of-truth

- DSL §12 Localization (`docs/proposals/scenario-dsl/0.0.1.md:482-499`): every content line is a `:line`; localization keys on a stable per-line `textUnitId` derived from its `code`; a `:line` with no author `code` is assigned one by a `lute tag` pass, generated **once and persisted into `code`** (the Yarn-Spinner `#line:` model), stable across edits/insertions; a `textUnitId` is assigned once, never recomputed (not positional, not a content hash). `<choice>` labels derive their textUnitId structurally from `<branchId>.<choiceId>` (NO tagging needed — out of this pass).
- `:line` grammar (`crates/lute-syntax/src/parser.rs:385-431`): `":line[" Speaker "]" ("{" Attrs "}")? WS? ":" WS Text`.

## Spec decisions (READ FIRST)

1. **ID scheme — monotonic numeric counter (documented deviation from Yarn-random, with rationale).** The spec cites the Yarn `#line:` *random* model and says "not positional, not a content hash." The load-bearing property is **stability**: a code, once assigned + persisted, is NEVER recomputed (so translations never break). This plan assigns each untagged `:line`, in document order, the next code ABOVE the highest existing numeric `code` in the document, stepping by 10 and zero-padded to ≥4 digits (e.g. existing max `0080` ⇒ next `0090`, `0100`, …; no existing numeric code ⇒ start `0010`). Rationale: (a) STABILITY is preserved (existing codes untouched; new codes only ever added above the max, so re-tagging after an insert never renumbers an existing line); (b) DETERMINISTIC (no RNG → testable by value, reproducible builds) and dep-free (no `rand`); (c) matches the repo's own convention — `docs/examples/bianca-s01ep02.lute` uses `code="0010"`, `"0020"`, … (real Idola content-catalog numbering). The spec's "not positional" concern (insertion breaking ids) is satisfied because assignment is one-shot-and-persisted, never positional-recompute. **If the reviewer/user prefer strict Yarn-random ids, it is a localized swap of the id-generator fn.**
2. **Only `:line` nodes are tagged.** `<choice>` labels key structurally (`<branchId>.<choiceId>`) and are NOT given a `code`. Directives/set/match/timeline are not content and are untouched.
3. **Insertion point (source surgery).** For each untagged line, using the ORIGINAL-source byte spans from the parse: the header is `text[line.span.byte_start .. text_span.byte_start]`. If the header contains a `{` (an attr block), insert `code="<id>" ` immediately after that `{` (a trailing space only when the block is non-empty). Otherwise insert `{code="<id>"}` immediately after the `]` that closes the speaker. Splice untagged lines **back-to-front** (highest byte offset first) so earlier offsets stay valid. Never touch a line that already has a `code` attr.
4. **Idempotency & totality.** A line has a `code` iff `line.attrs.iter().any(|a| a.key == "code")`. If a parse produced structural errors, `tag_document` returns the text unchanged with `added: 0` (do not rewrite a broken doc).

## File Structure

- `crates/lute-check/src/tag.rs` — NEW. `TagOutcome { text: String, added: usize }` + `pub fn tag_document(text: &str) -> TagOutcome`. `pub mod tag;` + `pub use tag::{tag_document, TagOutcome};` in `lib.rs`.
- `crates/lute-cli/src/main.rs` — add `Command::Tag { file }` + `run_tag(&file)`.
- `crates/lute-cli/tests/tag.rs` — spawn-the-binary acceptance.
- `docs/examples/` — reuse existing examples; a temp-copy CLI test avoids mutating committed fixtures.

---

# Task L1: pure `tag_document` core

**Files:**
- Create: `crates/lute-check/src/tag.rs`.
- Modify: `crates/lute-check/src/lib.rs` (`pub mod tag;` + re-export).
- Test: `crates/lute-check/src/tag.rs` `#[cfg(test)] mod tests`.

**Interfaces:**
```rust
//! `lute tag` localization pass (dsl §12): back-fill a stable `code` into every
//! untagged content `:line`. Pure + total; the CLI wraps this with file I/O.
use lute_syntax::ast::{Line, Node};

/// The result of tagging: the (possibly rewritten) document text and how many
/// `:line`s received a new `code`.
#[derive(Clone, Debug, PartialEq)]
pub struct TagOutcome {
    pub text: String,
    pub added: usize,
}

/// Back-fill a `code` attribute into every `:line` that lacks one (dsl §12).
/// Existing codes are never touched; new codes step above the document's highest
/// existing numeric code. Idempotent, deterministic, total (a structurally
/// broken doc is returned unchanged with `added: 0`).
pub fn tag_document(text: &str) -> TagOutcome;
```
- Implementation:
  - `let (doc, diags) = lute_syntax::parse(text);` If `diags` contains any structural/error diagnostic that would corrupt the node stream (reuse the same STRUCTURAL sense as check.rs — simplest: if `diags.iter().any(|d| d.severity == Severity::Error)`, return `TagOutcome { text: text.into(), added: 0 }`). (Import `lute_core_span::Severity`.)
  - Recursively collect every `Node::Line(&Line)` in document order (walk `doc.shots[*].body`, descending into `Node::Branch` choices' bodies and `Node::Match` arms' bodies — mirror the node kinds `check.rs::Walker::walk` visits). A small `fn collect_lines<'a>(nodes: &'a [Node], out: &mut Vec<&'a Line>)`.
  - Compute `max_code`: the max over `line.attrs` `code` values that parse as a `u32` (`a.value` is `AttrValue::Str(s)` ⇒ `s.trim().parse::<u32>().ok()`); default 0 when none.
  - Collect the untagged lines (`!attrs.any(|a| a.key == "code")`) in document order; assign codes `max_code + 10, +20, …`, formatted `format!("{n:04}")`.
  - Build the new text by splicing back-to-front (sort insertions by descending byte offset). For each untagged line: find the header slice `&text[line.span.byte_start .. line.text_span.byte_start]`; locate the first `{` byte (attr block) within it → insert point = `line.span.byte_start + rel_brace + 1`, inserted string = `format!("code=\"{code}\" ")` if the char after `{` is not `}` else `format!("code=\"{code}\"")`; else locate the first `]` byte → insert point = `line.span.byte_start + rel_bracket + 1`, inserted string = `format!("{{code=\"{code}\"}}")`. Use byte search on the ASCII `{`/`]` (both are ASCII; the speaker id has no `]`).
  - Return `TagOutcome { text: rewritten, added: untagged.len() }`.

- [ ] **Step 1: failing tests** (`tag.rs` tests):
```rust
const NO_ATTRS: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[narrator]: hi there\n";
const WITH_ATTRS: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[fixer]{delivery=\"thought\"}: hmm\n";
const ALREADY: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[fixer]{code=\"0010\"}: kept\n";

#[test]
fn tags_line_without_attrs() {
    let out = tag_document(NO_ATTRS);
    assert_eq!(out.added, 1);
    assert!(out.text.contains(":line[narrator]{code=\"0010\"}: hi there"), "got:\n{}", out.text);
}
#[test]
fn tags_line_with_existing_attrs() {
    let out = tag_document(WITH_ATTRS);
    assert_eq!(out.added, 1);
    assert!(out.text.contains("code=\"0010\""), "got:\n{}", out.text);
    assert!(out.text.contains("delivery=\"thought\""), "existing attr preserved:\n{}", out.text);
}
#[test]
fn already_tagged_is_untouched_and_idempotent() {
    let out = tag_document(ALREADY);
    assert_eq!(out.added, 0);
    assert_eq!(out.text, ALREADY);
}
#[test]
fn new_codes_step_above_max_existing() {
    // one tagged 0050 + one untagged -> untagged gets 0060 (above max), tagged kept
    let src = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[a]{code=\"0050\"}: one\n:line[b]: two\n";
    let out = tag_document(src);
    assert_eq!(out.added, 1);
    assert!(out.text.contains(":line[a]{code=\"0050\"}: one"));
    assert!(out.text.contains("code=\"0060\""), "got:\n{}", out.text);
}
#[test]
fn tagging_output_is_idempotent() {
    let once = tag_document(NO_ATTRS).text;
    let twice = tag_document(&once);
    assert_eq!(twice.added, 0);
    assert_eq!(twice.text, once);
}
#[test]
fn tagged_output_still_parses_clean() {
    // the rewritten doc must have no NEW parse errors (the inserted code attr is valid)
    let out = tag_document(NO_ATTRS);
    let (_doc, diags) = lute_syntax::parse(&out.text);
    assert!(!diags.iter().any(|d| d.severity == lute_core_span::Severity::Error), "{diags:?}");
}
```
- [ ] **Step 2: RED** — `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p lute-check --lib tag` → FAIL (`tag_document`/`TagOutcome` missing).
- [ ] **Step 3: implement** `tag.rs` per the interface; `pub mod tag;` + re-export in `lib.rs`.
- [ ] **Step 4: GREEN + build** — `cargo test -p lute-check --lib tag` (6/6) + `cargo build -p lute-check -p lute-lsp -p lute-cli` clean.
- [ ] **Step 5: regression + fmt + clippy + commit**
  - `cargo test -p lute-check && cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings`.
  ```bash
  git add crates/lute-check/src/tag.rs crates/lute-check/src/lib.rs
  git commit -m "feat(check): tag_document localization pass — back-fill :line code (dsl §12)"
  ```

# Task L2: `lute tag` CLI subcommand + gate

**Files:**
- Modify: `crates/lute-cli/src/main.rs` (add `Command::Tag { file }` + `run_tag`).
- Test: NEW `crates/lute-cli/tests/tag.rs` (spawn the binary on a temp copy).

**Interfaces:**
- `Command::Tag { file: PathBuf }` (doc-comment: "Back-fill a stable `code` into every untagged `:line` (dsl §12), rewriting the file in place.").
- `fn run_tag(file: &Path) -> ExitCode`: read the file (I/O err → `ExitCode::from(2)` + stderr, like `run_check`); `let out = lute_check::tag_document(&text);` if `out.added > 0` write `out.text` back to `file`; print `lute: tagged {added} line(s)` (or `already tagged`); exit `0`. (Never partial-writes: only writes when `added > 0`.)

- [ ] **Step 1: failing test** — `crates/lute-cli/tests/tag.rs` (mirror `tests/cli.rs` harness: `const BIN = env!("CARGO_BIN_EXE_lute")` + a `temp_dir` helper):
```rust
#[test]
fn tag_backfills_code_and_is_idempotent() {
    let dir = temp_dir("tag");
    let f = dir.join("scene.lute");
    std::fs::write(&f, "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:line[narrator]: hi\n").unwrap();
    let out = std::process::Command::new(BIN).args(["tag", f.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let after = std::fs::read_to_string(&f).unwrap();
    assert!(after.contains("code=\"0010\""), "code back-filled:\n{after}");
    // idempotent: second run changes nothing
    let out2 = std::process::Command::new(BIN).args(["tag", f.to_str().unwrap()]).output().unwrap();
    assert!(out2.status.success());
    assert_eq!(std::fs::read_to_string(&f).unwrap(), after, "second tag run must be a no-op");
}
```
- [ ] **Step 2: RED** — `cargo build -p lute-cli && cargo test -p lute-cli --test tag` → FAIL (no `tag` subcommand — clap errors / non-zero).
- [ ] **Step 3: implement** the subcommand + `run_tag` + the `main` match arm.
- [ ] **Step 4: GREEN + build** — `cargo test -p lute-cli --test tag` + `cargo build -p lute-check -p lute-lsp -p lute-cli` clean.
- [ ] **Step 5: full gate**
  - `cargo test --workspace` green · `cargo clippy --workspace --all-targets -- -D warnings` 0 · `cargo fmt --check` clean · `(cd tree-sitter-lute && npx --yes tree-sitter-cli@latest test)` 25/25 (capabilityVersion UNCHANGED).
  - Acceptances unchanged: bianca 0 · date-minigame core-only 1 · `--project` 0 · idola-portrait `--project` 0 · carry-ep 0. NEW: `lute tag` on a temp copy of bianca is a no-op (bianca is fully coded) — `./target/debug/lute tag <temp-copy-of-bianca>` prints "already tagged"/adds 0 and leaves the file byte-identical (ad-hoc check).
- [ ] **Step 6: fmt + clippy + commit**
  ```bash
  cargo fmt -p lute-cli && cargo clippy -p lute-cli --all-targets -- -D warnings
  git add crates/lute-cli/src/main.rs crates/lute-cli/tests/tag.rs
  git commit -m "feat(cli): lute tag subcommand — persist :line codes (dsl §12)"
  ```

---

# Final gate (after L1 + L2)

- [ ] `cargo test --workspace` all green · `cargo clippy --workspace --all-targets -- -D warnings` 0 · `cargo fmt --check` clean · tree-sitter 25/25.
- [ ] Acceptances: bianca 0 / dm-core 1 / dm-proj 0 / portrait 0 / carry-ep 0; `lute tag` on a fully-coded doc is a no-op; on an untagged doc back-fills `code`s and is idempotent.
- [ ] Both trees clean; whole-branch review (most-capable) → Ready to merge; ff-merge to `main`.

## Self-Review

- **Spec coverage:** §12 `lute tag` back-fill of a persisted, stable per-`:line` `code` (L1 core + L2 CLI); `<choice>` structural keying is out-of-pass (documented); translation tables external (out of scope, documented).
- **Placeholder scan:** concrete signatures (`tag_document`/`TagOutcome`, `Command::Tag`/`run_tag`) + full test bodies; no TBD.
- **Type consistency:** `TagOutcome { text, added }` and `tag_document(&str) -> TagOutcome` stable across L1/L2.
- **ID-scheme decision** is called out (counter vs Yarn-random) with rationale + a note that it's a one-fn swap if the reviewer/user prefer random.
- **Risk:** the source-surgery insertion is the delicate part — the back-to-front splice keeps offsets valid; the "still parses clean" test guards against a malformed insertion; idempotency + already-tagged tests guard stability.
