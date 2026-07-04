# FEAT-1: `property=` Timeline Tracks — complete + validate + document

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Goal:** Finish the `property=` timeline-track feature (dsl §11.4) — splitting one subject across property-scoped concurrent tracks (e.g. animate `bianca`'s position and opacity on separate tracks). The parse + conflict semantics already exist (`TrackKey::Property`, `E-DUP-TRACK`, `E-CLIP-OVERLAP`, `E-WRITE-CONFLICT` all handle it); this task adds the ONE missing validation (a track with no usable key — including `property=` without `subject=` — is silently accepted today), a committed fixture demonstrating property tracks, and removes the stale "deferred" note from the spec.

**Architecture:** Add `E-TRACK-KEY` in `resolve_timeline` (lute-check/src/timeline.rs) for any track whose canonical key is empty (parser collapses a keyless / property-without-subject track to `TrackKey::Subject("")`). Add a fixture + unit tests. Update the DSL spec.

**Tech Stack:** Rust; `lute-check` timeline resolver; `docs/proposals/scenario-dsl/0.0.1.md`.

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt -p lute-check` + `cargo clippy -p lute-check --all-targets -- -D warnings`.
- Determinism (sorted output), never-panic. capabilityVersion / tree-sitter untouched (no snapshot/grammar change).
- New diagnostic code: `E-TRACK-KEY` (dsl §7.4/§11.4).

## Background (grounding — already implemented, do NOT rebuild)
- Parser (`crates/lute-syntax/src/parser/blocks.rs:256-267`): `subject`+`property` → `TrackKey::Property`; `subject` alone → `Subject`; `channel` → `Channel`; else → `TrackKey::Subject(String::new())` with comment "missing key: checker validates."
- `resolve_timeline` (`crates/lute-check/src/timeline.rs:70`): `E-DUP-TRACK` (canon key, incl. property `"{subject}.{property}"`), `E-CLIP-OVERLAP`, cross-track `E-WRITE-CONFLICT` on resolved write targets, `W-TIMELINE-*`. `canon_key`/`subject_of` handle `Property`. Tests `two_writer_tracks`/`no_conflict_when_different_properties` already prove property tracks (same subject, distinct property keys → no false conflict; duplicate property key → E-DUP-TRACK).
- **Gap:** no check flags an EMPTY track key. A keyless track OR `property=` without `subject=` both collapse to `TrackKey::Subject("")` and are silently accepted.

---

## Task 1: `E-TRACK-KEY` validation + property-track fixture + spec

**Files:**
- Modify: `crates/lute-check/src/timeline.rs` (emit `E-TRACK-KEY`; unit tests)
- Create: `docs/examples/property-tracks.lute` (fixture)
- Modify: `docs/proposals/scenario-dsl/0.0.1.md` (remove Appendix B "deferred" bullet; note support)

**Interfaces:**
- Consumes: `TrackKey`, `canon_key(&TrackKey) -> String` (existing, timeline.rs:219).
- Produces: `E-TRACK-KEY` diagnostic on a track whose `canon_key` is empty.

- [ ] **Step 1: Write failing unit tests in `timeline.rs`**

Add to the `#[cfg(test)] mod tests` in `crates/lute-check/src/timeline.rs`:
```rust
#[test]
fn keyless_track_errors() {
    // A <track> with no subject/channel/property collapses to Subject("") -> E-TRACK-KEY.
    let tl = Timeline {
        duration: None,
        span: span(),
        tracks: vec![Track {
            key: TrackKey::Subject(String::new()),
            clips: vec![],
            span: span(),
        }],
    };
    let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
    assert!(diags.iter().any(|d| d.code == "E-TRACK-KEY"), "got {:?}", diags.iter().map(|d| &d.code).collect::<Vec<_>>());
}

#[test]
fn property_track_pair_is_clean() {
    // Two property tracks on the SAME subject, DISTINCT properties -> no E-TRACK-KEY, no E-DUP-TRACK.
    let tl = Timeline {
        duration: None,
        span: span(),
        tracks: vec![
            Track { key: TrackKey::Property { subject: "bianca".into(), property: "pos".into() }, clips: vec![], span: span() },
            Track { key: TrackKey::Property { subject: "bianca".into(), property: "opacity".into() }, clips: vec![], span: span() },
        ],
    };
    let (_r, diags) = resolve_timeline(&tl, &ctx(), &lute_manifest::core::load_core_snapshot());
    assert!(!diags.iter().any(|d| d.code == "E-TRACK-KEY" || d.code == "E-DUP-TRACK"), "got {:?}", diags.iter().map(|d| &d.code).collect::<Vec<_>>());
}
```
(Use the existing test helpers `span()`, `ctx()`, and the `Track`/`Timeline`/`TrackKey` imports already in the test module — check the module's existing helpers and imports and mirror them.)

- [ ] **Step 2: Run tests to verify `keyless_track_errors` fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo test -p lute-check --lib timeline::tests::keyless_track_errors`
Expected: FAIL (no E-TRACK-KEY emitted yet). `property_track_pair_is_clean` should already pass.

- [ ] **Step 3: Emit `E-TRACK-KEY` in `resolve_timeline`**

In the existing per-track loop that computes `canon_key` for the duplicate-key check (timeline.rs ~101-112), add an empty-key check. Emit once per offending track, BEFORE the dup-key insert (an empty key should report E-TRACK-KEY, not E-DUP-TRACK, and an empty key must not swallow a second empty key into E-DUP-TRACK — skip the dup-insert for empty keys):
```rust
for track in &tl.tracks {
    let canon = canon_key(&track.key);
    if canon.is_empty() {
        diags.push(diag(
            "E-TRACK-KEY",
            Severity::Error,
            "a <track> requires `subject`, `channel`, or `subject`+`property` (dsl §7.4)".to_string(),
            track.span,
        ));
        continue; // don't also flag empty keys as duplicates
    }
    if !seen_keys.insert(canon.clone()) {
        diags.push(diag("E-DUP-TRACK", Severity::Error, format!("duplicate track key `{canon}` in timeline"), track.span));
    }
}
```
(Adjust to the exact existing loop shape; preserve the dup-track behavior for non-empty keys.)

- [ ] **Step 4: Run timeline tests**

Run: `cargo test -p lute-check --lib timeline`
Expected: `keyless_track_errors` + `property_track_pair_is_clean` pass; ALL existing timeline tests still pass (dup-track, overlap, write-conflict, property tests unchanged).

- [ ] **Step 5: Create the fixture `docs/examples/property-tracks.lute`**

A minimal valid scene whose timeline uses TWO property tracks on one subject (demonstrating split-subject animation), plus a normal subject track, that checks exit 0. Use ONLY core staging directives valid inside `<track>` (inspect `crates/lute-manifest/assets/lute.core/directives/staging.yaml` for the available staging directive names + their attrs, and study `docs/examples/*.lute` for timeline/track syntax). Example shape (adapt directive/attr names to the real core vocabulary):
```
---
character: demo
season: 1
episode: 1
---

## Shot 1.

<timeline>
  <track subject="bianca" property="pos">
    ::<coreStagingDirective ...>{ at=0.0 ... }
  </track>
  <track subject="bianca" property="opacity">
    ::<coreStagingDirective ...>{ at=0.0 ... }
  </track>
</timeline>
```
Then: `cargo build -p lute-cli && ./target/debug/lute check docs/examples/property-tracks.lute; echo exit=$?` — MUST be exit 0. If a directive/attr is invalid, fix the fixture against the real core vocabulary until it is genuinely clean (do NOT suppress errors).

- [ ] **Step 6: Update the spec**

In `docs/proposals/scenario-dsl/0.0.1.md` Appendix B (Open items, ~line 530), REMOVE the bullet `- `property=` timeline tracks (split one subject across property-scoped tracks) — deferred.` (property tracks are now implemented + validated; §11.4 already describes them normatively). If helpful, add one sentence to §11.4 noting a `property` track requires a `subject` and that `<subject>.<property>` is the track key.

- [ ] **Step 7: fmt + clippy + commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
git add crates/lute-check/src/timeline.rs docs/examples/property-tracks.lute docs/proposals/scenario-dsl/0.0.1.md
git commit -m "feat(check,examples): validate track keys (E-TRACK-KEY) + property-track fixture (dsl §11.4)"
```

## Verification (controller, after review)
```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test -p lute-manifest --test tree_sitter_stamp   # unchanged
./target/debug/lute check docs/examples/property-tracks.lute   # exit 0
```

## Self-Review
- `E-TRACK-KEY` fires for keyless AND property-without-subject (both collapse to empty key); non-empty keys unaffected (dup-track preserved).
- Property tracks (distinct properties on one subject) are clean; duplicate property key still `E-DUP-TRACK`; cross-property write conflict still `E-WRITE-CONFLICT` (already tested).
- Fixture genuinely exit 0; spec no longer lists property tracks as deferred.
- Deferred (documented, minor): highlighting the track-key attribute in semantic tokens (needs AST attr spans not currently tracked); ambiguous multi-key tracks (e.g. subject+channel) — parser picks one, not flagged.
